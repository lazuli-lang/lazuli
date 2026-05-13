package lazuli

import (
	"context"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
)

const (
	defaultHTTPProcessAddrEnv       = "PORT"
	defaultHTTPProcessReadinessPath = "/readyz"
)

// LookupEnvFunc reads an environment variable.
type LookupEnvFunc func(string) (string, bool)

// HTTPProcessServerOptions configures NewHTTPProcessServer.
type HTTPProcessServerOptions struct {
	// Addr overrides environment and default address resolution.
	Addr string
	// AddrEnv is the environment variable used for the listen address. It
	// defaults to PORT. Numeric values are treated as TCP ports.
	AddrEnv string
	// DefaultAddr is used when Addr and AddrEnv are empty. It defaults to
	// Lazuli's server address default.
	DefaultAddr string
	// LookupEnv reads environment variables. It defaults to os.LookupEnv.
	LookupEnv LookupEnvFunc
	// ServerConfig customizes NewServer defaults after address resolution.
	ServerConfig ServerConfig
	// RunOptions customizes RunServer.
	RunOptions RunServerOptions
	// MountReadiness mounts Readiness at ReadinessPath on the process mux.
	MountReadiness bool
	// ReadinessPath is the readiness endpoint path. It defaults to /readyz when
	// MountReadiness is true.
	ReadinessPath string
	// Readiness is the state used by the readiness endpoint and RunServer. When
	// nil and readiness is mounted, a new unready state is created.
	Readiness *ReadinessState
}

// HTTPProcessServerPlan is an executable HTTP process plan assembled from
// Mux, NewServer, and RunServer.
type HTTPProcessServerPlan struct {
	Addr      string
	Mux       *http.ServeMux
	Server    *http.Server
	Readiness *ReadinessState
	RunOpts   RunServerOptions
}

// NewHTTPProcessServer returns a process-friendly server plan for Lazuli's
// registered HTTP routes. It does not start listening.
func NewHTTPProcessServer(opts HTTPProcessServerOptions) HTTPProcessServerPlan {
	addr := ResolveHTTPProcessAddr(opts)
	mux := Mux()
	readiness := opts.Readiness

	if opts.MountReadiness || strings.TrimSpace(opts.ReadinessPath) != "" {
		if readiness == nil {
			readiness = NewReadinessState(false)
		}
		path := strings.TrimSpace(opts.ReadinessPath)
		if path == "" {
			path = defaultHTTPProcessReadinessPath
		}
		mux.Handle("GET "+normalizeHTTPPathPattern(path), readiness.Handler())
	}

	runOpts := opts.RunOptions
	if readiness != nil && runOpts.Readiness == nil {
		runOpts.Readiness = readiness
	}

	cfg := opts.ServerConfig
	cfg.Addr = addr

	server := NewServer(mux, cfg)
	return HTTPProcessServerPlan{
		Addr:      addr,
		Mux:       mux,
		Server:    server,
		Readiness: readiness,
		RunOpts:   runOpts,
	}
}

// ResolveHTTPProcessAddr resolves the process listen address from explicit
// options, environment, then Lazuli defaults.
func ResolveHTTPProcessAddr(opts HTTPProcessServerOptions) string {
	if addr := normalizeHTTPProcessAddr(opts.Addr); addr != "" {
		return addr
	}

	lookup := opts.LookupEnv
	if lookup == nil {
		lookup = os.LookupEnv
	}
	env := strings.TrimSpace(opts.AddrEnv)
	if env == "" {
		env = defaultHTTPProcessAddrEnv
	}
	if raw, ok := lookup(env); ok {
		if addr := normalizeHTTPProcessAddr(raw); addr != "" {
			return addr
		}
	}

	if addr := normalizeHTTPProcessAddr(opts.DefaultAddr); addr != "" {
		return addr
	}
	return defaultServerAddr
}

// Run starts the plan's server and gracefully shuts it down when ctx is
// canceled.
func (p HTTPProcessServerPlan) Run(ctx context.Context) error {
	return RunServer(ctx, p.Server, p.RunOpts)
}

// Serve starts the plan's server on ln. It is useful for tests and callers
// that bind their own listener.
func (p HTTPProcessServerPlan) Serve(ctx context.Context, ln net.Listener) error {
	return serveServer(ctx, p.Server, ln, p.RunOpts)
}

// RunHTTPProcessServer builds and runs a Lazuli HTTP process server.
func RunHTTPProcessServer(ctx context.Context, opts HTTPProcessServerOptions) error {
	return NewHTTPProcessServer(opts).Run(ctx)
}

// HTTPProcessSignalContext returns a context canceled by process signals.
// When no signals are provided, os.Interrupt and SIGTERM are used.
func HTTPProcessSignalContext(parent context.Context, signals ...os.Signal) (context.Context, context.CancelFunc) {
	if parent == nil {
		parent = context.Background()
	}
	if len(signals) == 0 {
		signals = []os.Signal{os.Interrupt, syscall.SIGTERM}
	}
	return signal.NotifyContext(parent, signals...)
}

func normalizeHTTPProcessAddr(addr string) string {
	addr = strings.TrimSpace(addr)
	if addr == "" {
		return ""
	}
	if isDecimalPort(addr) {
		return ":" + addr
	}
	return addr
}

func isDecimalPort(value string) bool {
	for _, ch := range value {
		if ch < '0' || ch > '9' {
			return false
		}
	}
	return value != ""
}
