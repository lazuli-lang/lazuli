// Auto-mount HTTP routes for declared reports.
//
// `Mount(mux)` walks the registry populated by generated `init()`
// blocks and registers one `GET /api/reports/<name>.<format>` route
// per (contract × format). lazuli.Mux() calls this so the routes are
// available out of the box without main.go boilerplate.
//
// Per-route pipeline (R.C.4):
//  1. apply rate-limit gate when `Contract.RateLimit` is non-empty
//     and a `RateLimiter` hook is installed.
//  2. evaluate `Contract.Atoms` against the request context via the
//     installed `PolicyChecker` hook; deny with 403 on failure.
//  3. invoke the registered `Runner` and translate its result into
//     the HTTP response shape (302 redirect / 200 JSON envelope /
//     501 stub error).
//
// Response shape:
//   - Visibility == Signed → 302 redirect to the signed URL.
//   - Visibility == Public / Private → 200 JSON envelope `{"url":"..."}`
//     (the URL is the resolved key; the bound storage adapter handles
//     authorization on subsequent GETs).
//
// Wire-thin: no CSV/XLSX bytes, no signing — those live in
// pipeline.go + storage/. This file is the routing glue.

package report

import (
	"encoding/json"
	"net/http"
	"sort"

	"lazuli.dev/runtime/lazuli/storage"
)

// Mount registers one HTTP route per (contract × format) pair under
// `GET /api/reports/<name>.<format>`. Determinism: contracts are
// sorted by name so two equivalent registries produce equivalent
// route registrations (matters for test output and `gofmt` diffs).
func Mount(mux *http.ServeMux) {
	regs := Registered()
	sort.Slice(regs, func(i, j int) bool {
		return regs[i].Contract.Name < regs[j].Contract.Name
	})
	for _, reg := range regs {
		reg := reg
		for _, format := range reg.Contract.Formats {
			format := format
			path := "GET /api/reports/" + reg.Contract.Name + "." + format.Token()
			var handler http.Handler = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				serve(w, r, reg, format)
			})
			if reg.Contract.RateLimit != "" && rateLimiter != nil {
				handler = rateLimiter(reg.Contract.RateLimit, handler)
			}
			mux.Handle(path, handler)
		}
	}
}

// serve enforces policy, runs the registered runner and translates its
// result into the HTTP response shape (302 vs. JSON envelope).
func serve(w http.ResponseWriter, r *http.Request, reg Registration, format Format) {
	if len(reg.Contract.Atoms) > 0 {
		if policyChecker == nil {
			http.Error(w, "policy checker not installed", http.StatusForbidden)
			return
		}
		if err := policyChecker(r, reg.Contract.Atoms); err != nil {
			http.Error(w, err.Error(), http.StatusForbidden)
			return
		}
	}
	// W5 GAP-REPORT-01 — parse + validate the declared report input
	// params from the request query string and thread them to the
	// SourceFn via the request context. Missing required params fail
	// closed with HTTP 400 before the runner (and its DB cursor) opens.
	ctx := r.Context()
	if len(reg.Contract.Inputs) > 0 {
		params, perr := ParseInputs(reg.Contract.Inputs, r.URL.Query())
		if perr != nil {
			http.Error(w, perr.Error(), http.StatusBadRequest)
			return
		}
		ctx = WithParams(ctx, params)
	}
	url, err := reg.Runner(ctx, format)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotImplemented)
		return
	}
	if reg.Contract.Visibility == storage.VisibilitySigned {
		http.Redirect(w, r, url, http.StatusFound)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(map[string]string{"url": url})
}
