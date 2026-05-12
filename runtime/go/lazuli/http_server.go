package lazuli

import (
	"net/http"
	"time"
)

const (
	defaultServerAddr        = ":8080"
	defaultReadTimeout       = 15 * time.Second
	defaultReadHeaderTimeout = 5 * time.Second
	defaultWriteTimeout      = 30 * time.Second
	defaultIdleTimeout       = 120 * time.Second
	defaultMaxHeaderBytes    = 1 << 20
)

// ServerConfig configures NewServer. Zero-valued fields receive Lazuli's
// production-safe HTTP server defaults.
type ServerConfig struct {
	// Addr is the TCP address the server listens on. It defaults to ":8080".
	Addr string
	// ReadTimeout is the maximum duration for reading the entire request. It
	// defaults to 15 seconds.
	ReadTimeout time.Duration
	// ReadHeaderTimeout is the maximum duration for reading request headers. It
	// defaults to 5 seconds.
	ReadHeaderTimeout time.Duration
	// WriteTimeout is the maximum duration before timing out writes of the
	// response. It defaults to 30 seconds.
	WriteTimeout time.Duration
	// IdleTimeout is the maximum time to wait for the next request on an idle
	// keep-alive connection. It defaults to 120 seconds.
	IdleTimeout time.Duration
	// MaxHeaderBytes controls the maximum size of request headers. It defaults
	// to 1 MiB.
	MaxHeaderBytes int
}

// NewServer returns an http.Server for handler with production-safe defaults
// for address, timeouts, and maximum request header size. Pass Mux() as the
// handler to serve Lazuli's registered routes.
func NewServer(handler http.Handler, cfg ServerConfig) *http.Server {
	return &http.Server{
		Addr:              stringOr(cfg.Addr, defaultServerAddr),
		Handler:           handler,
		ReadTimeout:       durationOr(cfg.ReadTimeout, defaultReadTimeout),
		ReadHeaderTimeout: durationOr(cfg.ReadHeaderTimeout, defaultReadHeaderTimeout),
		WriteTimeout:      durationOr(cfg.WriteTimeout, defaultWriteTimeout),
		IdleTimeout:       durationOr(cfg.IdleTimeout, defaultIdleTimeout),
		MaxHeaderBytes:    intOr(cfg.MaxHeaderBytes, defaultMaxHeaderBytes),
	}
}

func durationOr(value, fallback time.Duration) time.Duration {
	if value == 0 {
		return fallback
	}
	return value
}

func intOr(value, fallback int) int {
	if value == 0 {
		return fallback
	}
	return value
}
