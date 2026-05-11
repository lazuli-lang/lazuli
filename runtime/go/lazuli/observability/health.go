// Health probe wiring. The language declares
// `app.runtime <unit>.{healthcheck,readiness}` paths; this package
// mounts them onto an http.ServeMux. Replaces the hardcoded
// `GET /healthz` mount in `runtime/go/lazuli/http.go:28-30` once the
// runtime team threads `HealthProbes` through the boot path.
//
// Doctor's `health_probe_path_invalid_diagnostics` validates the
// path shape at compile time; the runtime trusts the input.
//
// See `docs/proposals/bucket-observability-cycle.md` §6.3.

package observability

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
)

// HealthProbeSet is the lowered set of runtime-unit probes from
// `app.lzi`. Codegen emits a `var HealthProbes = observability.HealthProbeSet{...}`
// per app, one entry per `unit api` that declared `healthcheck` /
// `readiness`.
type HealthProbeSet struct {
	// Liveness is the path mounted for liveness checks (e.g.
	// `/healthz`). The handler returns 200 unconditionally — its
	// purpose is to prove the process is responsive, not that the
	// app is functional.
	Liveness string
	// Readiness is the path mounted for readiness checks (e.g.
	// `/readyz`). The handler returns 200 only when every registered
	// `ReadinessProbe` reports healthy; 503 otherwise.
	Readiness string
}

// ReadinessProbe is a single dependency check (database pool,
// event-bus connection, etc.). Generated code registers one probe
// per `requires integration <slot>` declared in the app.
type ReadinessProbe interface {
	// Name returns a short identifier for the probe (used in the
	// 503 response body).
	Name() string
	// Check returns nil when the dependency is healthy, or an error
	// describing why it is not.
	Check(ctx context.Context) error
}

// Typed errors.
var (
	// ErrReadinessUnhealthy is the sentinel returned by the readiness
	// handler when one or more probes fail. The handler converts it
	// to a 503 envelope listing the failing probe names.
	ErrReadinessUnhealthy = errors.New("lazuli/observability: readiness_unhealthy")
)

// RegisterProbes mounts the declared paths onto the provided mux.
// Stub: full implementation (probe registry threading, slog
// integration, configurable per-probe timeout) lands with the
// runtime team.
//
// Signature is committed; the boot path may call without waiting
// for the body.
func RegisterProbes(mux *http.ServeMux, probes HealthProbeSet, readiness []ReadinessProbe) {
	if probes.Liveness != "" {
		mux.HandleFunc("GET "+probes.Liveness, LivenessHandler)
	}
	if probes.Readiness != "" {
		mux.HandleFunc("GET "+probes.Readiness, ReadinessHandlerFn(readiness))
	}
}

// LivenessHandler is the canonical liveness handler. Returns
// `{"status":"ok"}` with HTTP 200 unconditionally.
func LivenessHandler(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
}

// ReadinessHandlerFn returns an http.HandlerFunc that walks the
// provided probes. Returns 503 with `{"status":"unready","failing":[...]}`
// when any probe fails.
//
// Stub: the runtime team owns timeouts (per-probe + total) and
// concurrent probe execution.
func ReadinessHandlerFn(probes []ReadinessProbe) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var failing []string
		for _, probe := range probes {
			if err := probe.Check(r.Context()); err != nil {
				failing = append(failing, probe.Name())
				// TODO(runtime): log err with the resolved logger.
				_ = err
			}
		}
		w.Header().Set("Content-Type", "application/json")
		if len(failing) == 0 {
			w.WriteHeader(http.StatusOK)
			_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
			return
		}
		w.WriteHeader(http.StatusServiceUnavailable)
		_ = json.NewEncoder(w).Encode(map[string]any{
			"status":  "unready",
			"failing": failing,
		})
	}
}
