package observability

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"

	"lazuli.dev/runtime/lazuli"
)

func TestRecoverHTTPWithTypedFieldErrorReturns400WithStructuredBody(t *testing.T) {
	ctx := lazuli.SetEnvironment(context.Background(), "dev")
	ctx = lazuli.SetObservabilityPolicy(ctx, []string{"dev", "staging"})
	req := httptest.NewRequest(http.MethodGet, "/", nil).WithContext(ctx)
	rr := httptest.NewRecorder()

	handler := RecoverHTTP(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		panic(fmt.Errorf("wrapped: %w", &lazuli.FieldError{
			Base: lazuli.ErrorBase{
				Code:    "field_invalid",
				Surface: lazuli.SurfaceUserDSL,
				Status:  http.StatusBadRequest,
				Message: "invalid email",
				Source:  "features/customer.lzi:42:8",
			},
			Field:  "email",
			Path:   "input.identity.email",
			Reason: lazuli.FieldReasonInvalidFormat,
		}))
	}))

	handler.ServeHTTP(rr, req)
	body := decodeErrorBody(t, rr)
	if rr.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want 400", rr.Code)
	}
	if body["code"] != "field_invalid" || body["field"] != "email" || body["reason"] != "invalid_format" {
		t.Fatalf("unexpected body: %#v", body)
	}
}

func TestRecoverHTTPWithLibBugEmitsInternalPanicTraceEvent(t *testing.T) {
	var events []TraceEvent
	SetTraceEventSink(func(_ context.Context, event TraceEvent) {
		events = append(events, event)
	})
	defer SetTraceEventSink(nil)

	handler := RecoverHTTP(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		panic(&lazuli.LibBugError{
			Base: lazuli.ErrorBase{
				Code:    "internal_panic",
				Surface: lazuli.SurfaceLibInternal,
				Status:  http.StatusInternalServerError,
				Message: "invariant failed",
			},
			Component: "lazuli.dev/runtime/lazuli/auth",
			IssueURL:  "https://github.com/lazuli-lang/lazuli/issues/new",
		})
	}))
	handler.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/", nil))

	for _, event := range events {
		if event.Name == "lazuli_internal_panic" {
			if event.Payload["issue_url"] != "https://github.com/lazuli-lang/lazuli/issues/new" {
				t.Fatalf("issue_url = %#v", event.Payload["issue_url"])
			}
			return
		}
	}
	t.Fatalf("lazuli_internal_panic not emitted: %#v", events)
}

func TestRecoverHTTPStripsSourceInProdEnv(t *testing.T) {
	body := recoverBodyForEnv(t, "prod")
	if _, ok := body["source"]; ok {
		t.Fatalf("source included in prod body: %#v", body)
	}
}

func TestRecoverHTTPIncludesSourceInDevEnv(t *testing.T) {
	body := recoverBodyForEnv(t, "dev")
	if body["source"] != "features/customer.lzi:42:8" {
		t.Fatalf("source = %#v", body["source"])
	}
}

func TestRecoverHTTPWithUnknownPanicValueBuildsLibBugEnvelope(t *testing.T) {
	handler := RecoverHTTP(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		panic(42)
	}))
	rr := httptest.NewRecorder()
	handler.ServeHTTP(rr, httptest.NewRequest(http.MethodGet, "/", nil))
	body := decodeErrorBody(t, rr)
	if rr.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500", rr.Code)
	}
	if body["code"] != "internal_panic" || body["surface"] != "lib_internal" {
		t.Fatalf("unexpected body: %#v", body)
	}
}

func recoverBodyForEnv(t *testing.T, env string) map[string]interface{} {
	t.Helper()
	ctx := lazuli.SetEnvironment(context.Background(), env)
	ctx = lazuli.SetObservabilityPolicy(ctx, []string{"dev", "staging"})
	req := httptest.NewRequest(http.MethodGet, "/", nil).WithContext(ctx)
	rr := httptest.NewRecorder()
	handler := RecoverHTTP(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		panic(&lazuli.Error{Base: lazuli.ErrorBase{
			Code:    "internal_panic",
			Surface: lazuli.SurfaceUserDSL,
			Status:  http.StatusInternalServerError,
			Message: "boom",
			Source:  "features/customer.lzi:42:8",
		}})
	}))
	handler.ServeHTTP(rr, req)
	return decodeErrorBody(t, rr)
}

func decodeErrorBody(t *testing.T, rr *httptest.ResponseRecorder) map[string]interface{} {
	t.Helper()
	var body map[string]map[string]interface{}
	if err := json.NewDecoder(rr.Body).Decode(&body); err != nil {
		t.Fatalf("decode body: %v", err)
	}
	return body["error"]
}
