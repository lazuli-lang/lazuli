package lazuli

import (
	"net/http"

	"lazuli.dev/runtime/lazuli/report"
)

// init wires the report package's auto-mount hooks (R.C.4) to the
// runtime's policy evaluator and rate-limit middleware. The report
// package keeps its own typed mirrors (`report.PolicyAtom`) so it
// never imports `lazuli` (cycle avoidance); this file installs the
// closures that bridge the two namespaces.
//
// Lifecycle: runs once at process startup, before any
// `report.Register(...)` invocation in generated `init()` blocks. Go
// guarantees `init()` ordering within a package — this file ships in
// `lazuli/`, so the runtime imports of `lazuli` (every generated
// `dist/go/.../*.gen.go`) trigger this `init` before the per-feature
// reports register.
func init() {
	report.SetPolicyChecker(func(r *http.Request, atoms []report.PolicyAtom) error {
		return EvalPolicy(newRequestCtx(r), Policy{Atoms: liftReportAtoms(atoms)})
	})
	report.SetRateLimiter(func(limit string, next http.Handler) http.Handler {
		return RateLimitMiddleware(RateLimit(limit), next)
	})
}

// liftReportAtoms copies `report.PolicyAtom` values into the canonical
// `lazuli.PolicyAtom` shape so the policy evaluator can run unchanged.
// The shapes are field-identical; this is a typed memcpy.
func liftReportAtoms(in []report.PolicyAtom) []PolicyAtom {
	out := make([]PolicyAtom, len(in))
	for i, a := range in {
		out[i] = PolicyAtom{Namespace: a.Namespace, Name: a.Name, Reason: a.Reason}
	}
	return out
}
