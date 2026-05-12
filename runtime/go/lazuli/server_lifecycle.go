package lazuli

import (
	"context"
	"errors"
	"net"
	"net/http"
	"sync/atomic"
	"time"
)

const defaultServerShutdownTimeout = 30 * time.Second

var (
	errNilHTTPServer = errors.New("lazuli: nil http server")
	errNilListener   = errors.New("lazuli: nil listener")
)

// RunServerOptions configures RunServer.
type RunServerOptions struct {
	// ShutdownTimeout limits how long graceful shutdown waits for active
	// requests to finish. Values less than or equal to zero use the default.
	ShutdownTimeout time.Duration
	// Readiness is marked ready after the listener is bound and unready before
	// graceful shutdown begins. It is optional.
	Readiness *ReadinessState
}

// RunServer starts srv and gracefully shuts it down when ctx is canceled.
//
// RunServer does not install signal handlers; callers should cancel ctx from
// their own signal handling or process lifecycle wiring. http.ErrServerClosed
// is treated as a normal shutdown result.
func RunServer(ctx context.Context, srv *http.Server, opts RunServerOptions) error {
	if srv == nil {
		return errNilHTTPServer
	}
	if ctx == nil {
		ctx = context.Background()
	}

	select {
	case <-ctx.Done():
		return nil
	default:
	}

	addr := srv.Addr
	if addr == "" {
		addr = ":http"
	}
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return err
	}

	return serveServer(ctx, srv, ln, opts)
}

func serveServer(ctx context.Context, srv *http.Server, ln net.Listener, opts RunServerOptions) error {
	if srv == nil {
		if ln != nil {
			_ = ln.Close()
		}
		return errNilHTTPServer
	}
	if ln == nil {
		return errNilListener
	}
	if ctx == nil {
		ctx = context.Background()
	}

	select {
	case <-ctx.Done():
		_ = ln.Close()
		return nil
	default:
	}

	readiness := opts.Readiness
	if readiness != nil {
		readiness.SetReady(true)
		defer readiness.SetReady(false)
	}

	serveErr := make(chan error, 1)
	go func() {
		serveErr <- srv.Serve(ln)
	}()

	select {
	case err := <-serveErr:
		return ignoreServerClosed(err)
	case <-ctx.Done():
	}

	if readiness != nil {
		readiness.SetReady(false)
	}

	shutdownCtx, cancel := context.WithTimeout(context.Background(), shutdownTimeoutOrDefault(opts.ShutdownTimeout))
	defer cancel()

	if err := srv.Shutdown(shutdownCtx); err != nil && !errors.Is(err, http.ErrServerClosed) {
		return err
	}

	return ignoreServerClosed(<-serveErr)
}

func shutdownTimeoutOrDefault(timeout time.Duration) time.Duration {
	if timeout <= 0 {
		return defaultServerShutdownTimeout
	}
	return timeout
}

func ignoreServerClosed(err error) error {
	if errors.Is(err, http.ErrServerClosed) {
		return nil
	}
	return err
}

// ReadinessState is an atomic readiness flag for HTTP readiness endpoints.
//
// The zero value is unready. A nil *ReadinessState is also treated as unready.
type ReadinessState struct {
	ready atomic.Bool
}

// NewReadinessState returns a readiness state initialized to the given value.
func NewReadinessState(ready bool) *ReadinessState {
	state := &ReadinessState{}
	state.ready.Store(ready)
	return state
}

// Ready reports whether the process should receive traffic.
func (s *ReadinessState) Ready() bool {
	if s == nil {
		return false
	}
	return s.ready.Load()
}

// SetReady updates whether the process should receive traffic.
func (s *ReadinessState) SetReady(ready bool) {
	if s == nil {
		return
	}
	s.ready.Store(ready)
}

// Handler returns an HTTP readiness handler for the current state.
func (s *ReadinessState) Handler() http.Handler {
	return http.HandlerFunc(s.ServeHTTP)
}

// ServeHTTP writes 200 when ready and 503 when unready.
func (s *ReadinessState) ServeHTTP(w http.ResponseWriter, _ *http.Request) {
	if s.Ready() {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ready"})
		return
	}
	writeJSON(w, http.StatusServiceUnavailable, map[string]string{"status": "unready"})
}
