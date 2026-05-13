package lazuli

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestResolveHTTPProcessAddrUsesExplicitEnvAndDefault(t *testing.T) {
	lookup := mapLookupEnv(map[string]string{
		"PORT":        "9090",
		"SERVER_ADDR": "127.0.0.1:9091",
	})

	tests := []struct {
		name string
		opts HTTPProcessServerOptions
		want string
	}{
		{
			name: "explicit addr wins",
			opts: HTTPProcessServerOptions{Addr: " 127.0.0.1:8081 ", LookupEnv: lookup},
			want: "127.0.0.1:8081",
		},
		{
			name: "default port env becomes addr",
			opts: HTTPProcessServerOptions{LookupEnv: lookup},
			want: ":9090",
		},
		{
			name: "custom env can provide full addr",
			opts: HTTPProcessServerOptions{AddrEnv: "SERVER_ADDR", LookupEnv: lookup},
			want: "127.0.0.1:9091",
		},
		{
			name: "configured default is used after empty env",
			opts: HTTPProcessServerOptions{
				DefaultAddr: "7070",
				LookupEnv:   mapLookupEnv(map[string]string{"PORT": " "}),
			},
			want: ":7070",
		},
		{
			name: "server default is final fallback",
			opts: HTTPProcessServerOptions{LookupEnv: mapLookupEnv(map[string]string{})},
			want: defaultServerAddr,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := ResolveHTTPProcessAddr(tt.opts); got != tt.want {
				t.Fatalf("ResolveHTTPProcessAddr() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestNewHTTPProcessServerMountsReadinessWithoutChangingHealthz(t *testing.T) {
	ready := NewReadinessState(true)
	plan := NewHTTPProcessServer(HTTPProcessServerOptions{
		Addr:           "127.0.0.1:0",
		MountReadiness: true,
		Readiness:      ready,
	})

	if plan.Addr != "127.0.0.1:0" {
		t.Fatalf("Addr = %q, want configured addr", plan.Addr)
	}
	if plan.Server.Addr != plan.Addr {
		t.Fatalf("Server.Addr = %q, want %q", plan.Server.Addr, plan.Addr)
	}
	if plan.Server.Handler != plan.Mux {
		t.Fatalf("Server.Handler = %v, want process mux", plan.Server.Handler)
	}
	if plan.RunOpts.Readiness != ready {
		t.Fatalf("RunOpts.Readiness = %v, want configured readiness", plan.RunOpts.Readiness)
	}

	rec := httptest.NewRecorder()
	plan.Mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/healthz", nil))
	assertStatusJSON(t, rec, http.StatusOK, "ok")

	rec = httptest.NewRecorder()
	plan.Mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/readyz", nil))
	assertStatusJSON(t, rec, http.StatusOK, "ready")
}

func TestHTTPProcessServerPlanServeUsesListenerAndReadiness(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("Listen: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	plan := NewHTTPProcessServer(HTTPProcessServerOptions{
		Addr:           "127.0.0.1:0",
		MountReadiness: true,
		RunOptions: RunServerOptions{
			ShutdownTimeout: time.Second,
		},
	})
	t.Cleanup(func() { _ = plan.Server.Close() })

	errc := make(chan error, 1)
	go func() {
		errc <- plan.Serve(ctx, ln)
	}()

	waitFor(t, "process server readiness", plan.Readiness.Ready)

	resp, err := http.Get("http://" + ln.Addr().String() + "/readyz")
	if err != nil {
		t.Fatalf("GET /readyz: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("GET /readyz status = %d, want %d", resp.StatusCode, http.StatusOK)
	}

	cancel()

	if err := waitRunServer(t, errc); err != nil {
		t.Fatalf("Serve returned %v, want nil", err)
	}
	if plan.Readiness.Ready() {
		t.Fatal("readiness stayed ready after shutdown")
	}
}

func TestHTTPProcessSignalContextCancelsWithParent(t *testing.T) {
	parent, cancelParent := context.WithCancel(context.Background())
	ctx, stop := HTTPProcessSignalContext(parent)
	defer stop()

	cancelParent()

	select {
	case <-ctx.Done():
	case <-time.After(time.Second):
		t.Fatal("signal context did not observe parent cancellation")
	}
}

func mapLookupEnv(values map[string]string) LookupEnvFunc {
	return func(key string) (string, bool) {
		value, ok := values[key]
		return value, ok
	}
}

func assertStatusJSON(t *testing.T, rec *httptest.ResponseRecorder, wantStatus int, wantBodyStatus string) {
	t.Helper()

	if rec.Code != wantStatus {
		t.Fatalf("status = %d, want %d", rec.Code, wantStatus)
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
