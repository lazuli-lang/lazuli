// Hand-written entrypoint for the runtime spike. In the eventual generated
// world this becomes `dist/go/main.gen.go` produced by codegen from the
// app.lzi runtime block. For the spike we wire it manually.
package main

import (
	"context"
	"log/slog"
	"net/http"
	"os"

	_ "lazuli.dev/example/full-capsule/customer" // register customer feature
	"lazuli.dev/runtime/lazuli"
)

func main() {
	ctx := context.Background()
	slog.SetDefault(slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo})))

	dbURL := os.Getenv("LAZULI_DB")
	if dbURL == "" {
		dbURL = "postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable"
	}

	if err := lazuli.Boot(ctx, dbURL); err != nil {
		slog.Error("lazuli boot failed", "error", err)
		os.Exit(1)
	}
	slog.Info("lazuli runtime booted",
		"resources", len(lazuli.Resources()),
		"commands", len(lazuli.Commands()),
	)

	addr := os.Getenv("LAZULI_ADDR")
	if addr == "" {
		addr = ":8080"
	}
	slog.Info("lazuli http listening", "addr", addr)
	if err := http.ListenAndServe(addr, lazuli.Mux()); err != nil {
		slog.Error("lazuli http server exited", "error", err)
		os.Exit(1)
	}
}
