package probe

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"sync/atomic"
)

// Readiness gates whether the /readyz endpoint returns 200 or 503.
// Mounted separately from /healthz so k8s/loadbalancer can drain
// gracefully without removing the pod from health checks.
type Readiness struct {
	ready atomic.Bool
}

// NewReadiness returns a readiness gate initialized as unready.
func NewReadiness() *Readiness {
	return &Readiness{}
}

// MarkReady toggles to ready=true. Called by Boot after all init() runs.
func (r *Readiness) MarkReady() {
	if r == nil {
		return
	}
	r.ready.Store(true)
}

// MarkDraining toggles to ready=false. Called on SIGINT before
// http.Server.Shutdown so in-flight requests complete + new ones get 503.
func (r *Readiness) MarkDraining() {
	if r == nil {
		return
	}
	r.ready.Store(false)
}

// Handler returns the http.Handler for /readyz.
func (r *Readiness) Handler() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if r != nil && r.ready.Load() {
			writeJSON(w, http.StatusOK, map[string]string{"status": "ready"})
			return
		}
		writeJSON(w, http.StatusServiceUnavailable, map[string]string{"status": "unready"})
	})
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	if err := json.NewEncoder(w).Encode(v); err != nil {
		slog.Error("lazuli/probe: failed to encode response", "error", err)
	}
}
