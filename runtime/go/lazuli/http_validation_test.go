package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestRequestValidationMiddlewarePassesValidRequestsThrough(t *testing.T) {
	validatorCalled := false
	nextCalled := false
	handler := RequestValidationMiddleware(RequestValidatorFunc(func(r *http.Request) []ValidationViolation {
		validatorCalled = true
		if r.URL.Path != "/widgets" {
			t.Fatalf("path = %q, want /widgets", r.URL.Path)
		}
		return nil
	}))(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		nextCalled = true
		w.Header().Set("X-Lazuli-Test", "ok")
		w.WriteHeader(http.StatusAccepted)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/widgets", nil)

	handler.ServeHTTP(rec, req)

	if !validatorCalled {
		t.Fatal("validator was not called")
	}
	if !nextCalled {
		t.Fatal("next handler was not called")
	}
	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	if got := rec.Header().Get("X-Lazuli-Test"); got != "ok" {
		t.Fatalf("X-Lazuli-Test = %q, want ok", got)
	}
}

func TestRequestValidationMiddlewareCollectsViolations(t *testing.T) {
	nextCalled := false
	handler := RequestValidationMiddleware(
		RequestValidatorFunc(func(*http.Request) []ValidationViolation {
			return []ValidationViolation{{
				Location: "query",
				Field:    "email",
				Code:     "required",
				Message:  "email is required",
			}}
		}),
		RequestValidatorFunc(func(*http.Request) []ValidationViolation {
			return []ValidationViolation{{
				Location: "header",
				Field:    "X-Request-ID",
				Code:     "required",
				Message:  "request id is required",
			}}
		}),
	)(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		nextCalled = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/widgets", nil)

	handler.ServeHTTP(rec, req)

	if nextCalled {
		t.Fatal("next handler was called")
	}
	if rec.Code != http.StatusUnprocessableEntity {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusUnprocessableEntity)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("Content-Type = %q, want application/problem+json", got)
	}
	body := decodeProblemResponse(t, rec)
	if body["status"] != float64(http.StatusUnprocessableEntity) {
		t.Fatalf("body status = %v, want %d", body["status"], http.StatusUnprocessableEntity)
	}
	if body["code"] != CodeValidationFailed {
		t.Fatalf("body code = %v, want %s", body["code"], CodeValidationFailed)
	}
	violations, ok := body["violations"].([]any)
	if !ok {
		t.Fatalf("violations = %#v, want array", body["violations"])
	}
	if len(violations) != 2 {
		t.Fatalf("violations len = %d, want 2", len(violations))
	}
	first, ok := violations[0].(map[string]any)
	if !ok {
		t.Fatalf("first violation = %#v, want object", violations[0])
	}
	if first["location"] != "query" || first["field"] != "email" || first["code"] != "required" || first["message"] != "email is required" {
		t.Fatalf("first violation = %#v, want email required query violation", first)
	}
}

func TestRequestValidationMiddlewareUsesBadRequestForMalformedViolations(t *testing.T) {
	nextCalled := false
	handler := RequestValidationMiddleware(RequestValidatorFunc(func(*http.Request) []ValidationViolation {
		return []ValidationViolation{
			{
				Location: "body",
				Code:     "json_invalid",
				Message:  "invalid JSON body",
				Status:   http.StatusBadRequest,
			},
			{
				Location: "query",
				Field:    "page",
				Code:     "minimum",
				Message:  "page must be at least 1",
				Status:   http.StatusUnprocessableEntity,
			},
		}
	}))(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		nextCalled = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/widgets?page=0", nil)

	handler.ServeHTTP(rec, req)

	if nextCalled {
		t.Fatal("next handler was called")
	}
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusBadRequest)
	}
	body := decodeProblemResponse(t, rec)
	if body["code"] != CodeBadRequest {
		t.Fatalf("body code = %v, want %s", body["code"], CodeBadRequest)
	}
	violations, ok := body["violations"].([]any)
	if !ok || len(violations) != 2 {
		t.Fatalf("violations = %#v, want 2 item array", body["violations"])
	}
	first, ok := violations[0].(map[string]any)
	if !ok {
		t.Fatalf("first violation = %#v, want object", violations[0])
	}
	if _, ok := first["status"]; ok {
		t.Fatalf("first violation includes status: %#v", first)
	}
}
