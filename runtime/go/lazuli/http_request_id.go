package lazuli

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"net/http"
)

const requestIDHeader = "X-Request-Id"

type requestIDKey struct{}

// RequestIDMiddleware reads X-Request-Id from the inbound request or mints a
// fresh one, stashes it in the context, and echoes it back in the response
// header. Downstream handlers retrieve it via RequestID(ctx).
func RequestIDMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		id := r.Header.Get(requestIDHeader)
		if id == "" {
			var err error
			id, err = mintRequestID()
			if err != nil {
				http.Error(w, http.StatusText(http.StatusInternalServerError), http.StatusInternalServerError)
				return
			}
		}

		w.Header().Set(requestIDHeader, id)
		ctx := context.WithValue(r.Context(), requestIDKey{}, id)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

// RequestID returns the correlation id stashed by the middleware, or "" when
// not present.
func RequestID(ctx context.Context) string {
	if ctx == nil {
		return ""
	}
	id, _ := ctx.Value(requestIDKey{}).(string)
	return id
}

func mintRequestID() (string, error) {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(b[:]), nil
}
