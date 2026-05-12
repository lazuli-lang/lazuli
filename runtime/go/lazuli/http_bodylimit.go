package lazuli

import "net/http"

// BodyLimitMiddleware returns a middleware that caps inbound request bodies at
// maxBytes. Requests declaring a larger Content-Length are rejected with 413
// before the next handler runs; other bodies are wrapped with http.MaxBytesReader
// so oversized streaming or unknown-length bodies fail while being read.
//
// A maxBytes value of zero or less disables the limit and leaves requests
// untouched.
func BodyLimitMiddleware(maxBytes int64) Middleware {
	return func(next http.Handler) http.Handler {
		if maxBytes <= 0 {
			return next
		}

		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.ContentLength > maxBytes {
				http.Error(w, http.StatusText(http.StatusRequestEntityTooLarge), http.StatusRequestEntityTooLarge)
				return
			}

			if r.Body != nil {
				r.Body = http.MaxBytesReader(w, r.Body, maxBytes)
			}
			next.ServeHTTP(w, r)
		})
	}
}
