package lazuli

import (
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestBodyLimitMiddlewareRejectsDeclaredOversizeWithoutCallingNext(t *testing.T) {
	called := false
	handler := BodyLimitMiddleware(4)(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader("abcde"))

	handler.ServeHTTP(rec, req)

	if called {
		t.Fatal("next handler was called")
	}
	if rec.Code != http.StatusRequestEntityTooLarge {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusRequestEntityTooLarge)
	}
}

func TestBodyLimitMiddlewareAllowsDeclaredBodyAtLimit(t *testing.T) {
	var got string
	handler := BodyLimitMiddleware(5)(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Fatalf("ReadAll error = %v", err)
		}
		got = string(body)
		w.WriteHeader(http.StatusAccepted)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader("abcde"))

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	if got != "abcde" {
		t.Fatalf("body = %q, want %q", got, "abcde")
	}
}

func TestBodyLimitMiddlewareWrapsUnknownLengthBody(t *testing.T) {
	var gotErr error
	handler := BodyLimitMiddleware(4)(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, gotErr = io.ReadAll(r.Body)
		if gotErr != nil {
			http.Error(w, "too large", http.StatusRequestEntityTooLarge)
			return
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader("abcde"))
	req.ContentLength = -1

	handler.ServeHTTP(rec, req)

	var maxBytesErr *http.MaxBytesError
	if !errors.As(gotErr, &maxBytesErr) {
		t.Fatalf("ReadAll error = %v, want *http.MaxBytesError", gotErr)
	}
	if maxBytesErr.Limit != 4 {
		t.Fatalf("MaxBytesError.Limit = %d, want 4", maxBytesErr.Limit)
	}
	if rec.Code != http.StatusRequestEntityTooLarge {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusRequestEntityTooLarge)
	}
}

func TestBodyLimitMiddlewareZeroOrNegativeLimitDisables(t *testing.T) {
	tests := []struct {
		name     string
		maxBytes int64
	}{
		{name: "zero", maxBytes: 0},
		{name: "negative", maxBytes: -1},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var got string
			handler := BodyLimitMiddleware(tt.maxBytes)(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				body, err := io.ReadAll(r.Body)
				if err != nil {
					t.Fatalf("ReadAll error = %v", err)
				}
				got = string(body)
				w.WriteHeader(http.StatusAccepted)
			}))

			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader("abcde"))

			handler.ServeHTTP(rec, req)

			if rec.Code != http.StatusAccepted {
				t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
			}
			if got != "abcde" {
				t.Fatalf("body = %q, want %q", got, "abcde")
			}
		})
	}
}
