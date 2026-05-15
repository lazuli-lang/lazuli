// Auto-mount HTTP routes for declared reports.
//
// `Mount(mux)` walks the registry populated by generated `init()`
// blocks and registers one `GET /api/reports/<name>.<format>` route
// per (contract × format). lazuli.Mux() calls this so the routes are
// available out of the box without main.go boilerplate.
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
			mux.HandleFunc(path, func(w http.ResponseWriter, r *http.Request) {
				serve(w, r, reg, format)
			})
		}
	}
}

// serve runs the registered runner and translates its result into the
// HTTP response shape (302 vs. JSON envelope).
func serve(w http.ResponseWriter, r *http.Request, reg Registration, format Format) {
	url, err := reg.Runner(r.Context(), format)
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
