package lazuli

import (
	"context"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"
)

// DefaultGracePeriod is what generated main.go uses when no override.
const DefaultGracePeriod = 30 * time.Second

// WaitForShutdown blocks until SIGINT/SIGTERM, then triggers server.Shutdown
// with a configurable grace period. Returns nil on clean shutdown,
// error from Shutdown otherwise.
func WaitForShutdown(server *http.Server, gracePeriod time.Duration) error {
	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, syscall.SIGINT, syscall.SIGTERM)
	defer signal.Stop(sigs)

	return waitForShutdown(server, gracePeriod, sigs)
}

func waitForShutdown(server *http.Server, gracePeriod time.Duration, sigs <-chan os.Signal) error {
	sig := <-sigs
	slog.Info("lazuli: shutdown signal received", "signal", sig)
	ctx, cancel := context.WithTimeout(context.Background(), gracePeriod)
	defer cancel()
	return server.Shutdown(ctx)
}
