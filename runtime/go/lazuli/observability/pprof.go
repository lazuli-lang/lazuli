package observability

import (
	"net/http"
	"net/http/pprof"
	"strings"
)

const defaultPprofPrefix = "/debug/pprof"

// RegisterPprof mounts the standard net/http/pprof handlers on mux.
//
// Prefix is normalized to a leading slash with trailing slashes removed. An
// empty prefix mounts the handlers under /debug/pprof.
func RegisterPprof(mux *http.ServeMux, prefix string) {
	prefix = normalizePprofPrefix(prefix)

	mux.HandleFunc("GET "+prefix+"/", pprofIndexHandler(prefix))
	mux.HandleFunc("GET "+prefix+"/cmdline", pprof.Cmdline)
	mux.HandleFunc("GET "+prefix+"/profile", pprof.Profile)
	mux.HandleFunc("GET "+prefix+"/symbol", pprof.Symbol)
	mux.HandleFunc("GET "+prefix+"/trace", pprof.Trace)
}

func normalizePprofPrefix(prefix string) string {
	prefix = strings.TrimSpace(prefix)
	if prefix == "" {
		return defaultPprofPrefix
	}
	if !strings.HasPrefix(prefix, "/") {
		prefix = "/" + prefix
	}
	prefix = strings.TrimRight(prefix, "/")
	if prefix == "" {
		return defaultPprofPrefix
	}
	return prefix
}

func pprofIndexHandler(prefix string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		name := strings.TrimPrefix(r.URL.Path, prefix+"/")
		if name == "" || name == r.URL.Path {
			pprof.Index(w, r)
			return
		}
		pprof.Handler(name).ServeHTTP(w, r)
	}
}
