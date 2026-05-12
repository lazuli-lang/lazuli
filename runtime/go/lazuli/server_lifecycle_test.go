package lazuli

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"
)

func TestRunServerStopsOnContextCancel(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	ready := NewReadinessState(false)
	srv := &http.Server{
		Addr:    "127.0.0.1:0",
		Handler: http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusNoContent) }),
	}
	t.Cleanup(func() { _ = srv.Close() })

	errc := make(chan error, 1)
	go func() {
		errc <- RunServer(ctx, srv, RunServerOptions{
			ShutdownTimeout: time.Second,
			Readiness:       ready,
		})
	}()

	waitFor(t, "server readiness", ready.Ready)
	cancel()

	if err := waitRunServer(t, errc); err != nil {
		t.Fatalf("RunServer returned %v, want nil", err)
	}
	if ready.Ready() {
		t.Fatal("readiness stayed ready after shutdown")
	}
}

func TestRunServerTreatsErrServerClosedAsNil(t *testing.T) {
	ready := NewReadinessState(false)
	srv := &http.Server{
		Addr:    "127.0.0.1:0",
		Handler: http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusNoContent) }),
	}
	t.Cleanup(func() { _ = srv.Close() })

	errc := make(chan error, 1)
	go func() {
		errc <- RunServer(context.Background(), srv, RunServerOptions{Readiness: ready})
	}()

	waitFor(t, "server readiness", ready.Ready)
	if err := srv.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}

	if err := waitRunServer(t, errc); err != nil {
		t.Fatalf("RunServer returned %v, want nil", err)
	}
	if ready.Ready() {
		t.Fatal("readiness stayed ready after close")
	}
}

func TestRunServerReturnsListenError(t *testing.T) {
	srv := &http.Server{Addr: "127.0.0.1:bad-port"}

	err := RunServer(context.Background(), srv, RunServerOptions{})

	if err == nil {
		t.Fatal("RunServer returned nil, want listen error")
	}
	if errors.Is(err, http.ErrServerClosed) {
		t.Fatalf("RunServer returned %v, want listen error", err)
	}
}

func TestRunServerRejectsNilServer(t *testing.T) {
	if err := RunServer(context.Background(), nil, RunServerOptions{}); !errors.Is(err, errNilHTTPServer) {
		t.Fatalf("RunServer nil server error = %v, want %v", err, errNilHTTPServer)
	}
}

func TestRunServerReturnsShutdownTimeout(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("Listen: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	entered := make(chan struct{})
	release := make(chan struct{})
	var releaseOnce sync.Once
	releaseHandler := func() {
		releaseOnce.Do(func() {
			close(release)
		})
	}
	srv := &http.Server{
		Handler: http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			close(entered)
			<-release
			_, _ = w.Write([]byte("done"))
		}),
	}
	t.Cleanup(func() {
		_ = srv.Close()
		releaseHandler()
	})

	errc := make(chan error, 1)
	go func() {
		errc <- serveServer(ctx, srv, ln, RunServerOptions{ShutdownTimeout: 10 * time.Millisecond})
	}()

	clientErr := make(chan error, 1)
	go func() {
		resp, err := http.Get("http://" + ln.Addr().String())
		if err != nil {
			clientErr <- err
			return
		}
		defer resp.Body.Close()
		_, err = io.Copy(io.Discard, resp.Body)
		clientErr <- err
	}()

	select {
	case <-entered:
	case err := <-clientErr:
		t.Fatalf("client returned before handler entered: %v", err)
	case <-time.After(2 * time.Second):
		t.Fatal("handler was not called")
	}

	cancel()

	err = waitRunServer(t, errc)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("RunServer returned %v, want %v", err, context.DeadlineExceeded)
	}

	releaseHandler()
	select {
	case err := <-clientErr:
		if err != nil {
			t.Fatalf("client returned %v, want nil", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("client did not finish")
	}
}

func TestReadinessStateHandler(t *testing.T) {
	state := NewReadinessState(false)

	rec := httptest.NewRecorder()
	state.Handler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/readyz", nil))
	assertReadinessResponse(t, rec, http.StatusServiceUnavailable, "unready")

	state.SetReady(true)

	rec = httptest.NewRecorder()
	state.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/readyz", nil))
	assertReadinessResponse(t, rec, http.StatusOK, "ready")
}

func waitFor(t *testing.T, name string, ok func() bool) {
	t.Helper()

	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if ok() {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("timed out waiting for %s", name)
}

func waitRunServer(t *testing.T, errc <-chan error) error {
	t.Helper()

	select {
	case err := <-errc:
		return err
	case <-time.After(2 * time.Second):
		t.Fatal("RunServer did not return")
		return nil
	}
}

func assertReadinessResponse(t *testing.T, rec *httptest.ResponseRecorder, wantStatus int, wantBodyStatus string) {
	t.Helper()

	if rec.Code != wantStatus {
		t.Fatalf("status = %d, want %d", rec.Code, wantStatus)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var body struct {
		Status string `json:"status"`
	}
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body.Status != wantBodyStatus {
		t.Fatalf("response status = %q, want %q", body.Status, wantBodyStatus)
	}
}
