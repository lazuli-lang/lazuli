package lazuli

import (
	"encoding/base64"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestRequestIDMiddlewarePassesThroughExistingHeader(t *testing.T) {
	const existingID = "req-existing"

	handler := RequestIDMiddleware(http.HandlerFunc(func(_ http.ResponseWriter, r *http.Request) {
		if got := RequestID(r.Context()); got != existingID {
			t.Fatalf("RequestID(ctx) = %q, want %q", got, existingID)
		}
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set(requestIDHeader, existingID)

	handler.ServeHTTP(rec, req)

	if got := rec.Result().Header.Get(requestIDHeader); got != existingID {
		t.Fatalf("response X-Request-Id = %q, want %q", got, existingID)
	}
}

func TestRequestIDMiddlewareMintsWhenAbsent(t *testing.T) {
	var contextID string
	handler := RequestIDMiddleware(http.HandlerFunc(func(_ http.ResponseWriter, r *http.Request) {
		contextID = RequestID(r.Context())
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	responseID := rec.Result().Header.Get(requestIDHeader)
	if responseID == "" {
		t.Fatal("response X-Request-Id is empty")
	}
	if contextID != responseID {
		t.Fatalf("RequestID(ctx) = %q, want response id %q", contextID, responseID)
	}

	decoded, err := base64.RawURLEncoding.DecodeString(responseID)
	if err != nil {
		t.Fatalf("response X-Request-Id is not raw URL base64: %v", err)
	}
	if len(decoded) != 16 {
		t.Fatalf("decoded request id length = %d, want 16", len(decoded))
	}
}

func TestRequestIDReturnsEmptyWhenAbsent(t *testing.T) {
	if got := RequestID(t.Context()); got != "" {
		t.Fatalf("RequestID(ctx) = %q, want empty", got)
	}
}
