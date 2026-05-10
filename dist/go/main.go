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

	// Demo subscribers — proof that post-commit emits flow through the
	// in-process bus. In the eventual generated world, cross-feature
	// reactions (e.g. `customer_outreach.job send_welcome` triggered by
	// `customer.customer_activated`) would register equivalent
	// Subscribers from each feature's init().
	lazuli.Subscribe("customer_created", func(_ context.Context, e lazuli.Event) error {
		slog.Info("subscriber: customer_created received",
			"id", e.Payload["id"], "email", e.Payload["email"])
		return nil
	})
	lazuli.Subscribe("customer_archived", func(_ context.Context, e lazuli.Event) error {
		slog.Info("subscriber: customer_archived received",
			"customer_id", e.Payload["customer_id"], "actor_id", e.Payload["actor_id"])
		return nil
	})

	slog.Info("lazuli runtime booted",
		"resources", len(lazuli.Resources()),
		"commands", len(lazuli.Commands()),
		"queries", len(lazuli.Queries()),
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
