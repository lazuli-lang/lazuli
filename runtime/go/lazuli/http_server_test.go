package lazuli

import (
	"net/http"
	"testing"
	"time"
)

func TestNewServerAppliesDefaults(t *testing.T) {
	handler := http.NewServeMux()

	server := NewServer(handler, ServerConfig{})

	if server.Addr != defaultServerAddr {
		t.Fatalf("Addr = %q, want %q", server.Addr, defaultServerAddr)
	}
	if server.Handler != handler {
		t.Fatalf("Handler = %v, want configured handler", server.Handler)
	}
	if server.ReadTimeout != defaultReadTimeout {
		t.Fatalf("ReadTimeout = %s, want %s", server.ReadTimeout, defaultReadTimeout)
	}
	if server.ReadHeaderTimeout != defaultReadHeaderTimeout {
		t.Fatalf("ReadHeaderTimeout = %s, want %s", server.ReadHeaderTimeout, defaultReadHeaderTimeout)
	}
	if server.WriteTimeout != defaultWriteTimeout {
		t.Fatalf("WriteTimeout = %s, want %s", server.WriteTimeout, defaultWriteTimeout)
	}
	if server.IdleTimeout != defaultIdleTimeout {
		t.Fatalf("IdleTimeout = %s, want %s", server.IdleTimeout, defaultIdleTimeout)
	}
	if server.MaxHeaderBytes != defaultMaxHeaderBytes {
		t.Fatalf("MaxHeaderBytes = %d, want %d", server.MaxHeaderBytes, defaultMaxHeaderBytes)
	}
}

func TestNewServerRespectsExplicitConfig(t *testing.T) {
	handler := http.NewServeMux()
	cfg := ServerConfig{
		Addr:              "127.0.0.1:9090",
		ReadTimeout:       time.Second,
		ReadHeaderTimeout: 2 * time.Second,
		WriteTimeout:      3 * time.Second,
		IdleTimeout:       4 * time.Second,
		MaxHeaderBytes:    4096,
	}

	server := NewServer(handler, cfg)

	if server.Addr != cfg.Addr {
		t.Fatalf("Addr = %q, want %q", server.Addr, cfg.Addr)
	}
	if server.Handler != handler {
		t.Fatalf("Handler = %v, want configured handler", server.Handler)
	}
	if server.ReadTimeout != cfg.ReadTimeout {
		t.Fatalf("ReadTimeout = %s, want %s", server.ReadTimeout, cfg.ReadTimeout)
	}
	if server.ReadHeaderTimeout != cfg.ReadHeaderTimeout {
		t.Fatalf("ReadHeaderTimeout = %s, want %s", server.ReadHeaderTimeout, cfg.ReadHeaderTimeout)
	}
	if server.WriteTimeout != cfg.WriteTimeout {
		t.Fatalf("WriteTimeout = %s, want %s", server.WriteTimeout, cfg.WriteTimeout)
	}
	if server.IdleTimeout != cfg.IdleTimeout {
		t.Fatalf("IdleTimeout = %s, want %s", server.IdleTimeout, cfg.IdleTimeout)
	}
	if server.MaxHeaderBytes != cfg.MaxHeaderBytes {
		t.Fatalf("MaxHeaderBytes = %d, want %d", server.MaxHeaderBytes, cfg.MaxHeaderBytes)
	}
}
