package lazuli

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestWriteProblemDefaultsAndExtensions(t *testing.T) {
	rec := httptest.NewRecorder()

	WriteProblem(rec, Problem{
		Status:   http.StatusTeapot,
		Detail:   "short and stout",
		Instance: "/tea/42",
		Extensions: map[string]any{
			"code":   "teapot",
			"status": "ignored",
		},
	})

	if rec.Code != http.StatusTeapot {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusTeapot)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("Content-Type = %q, want application/problem+json", got)
	}

	body := decodeProblemResponse(t, rec)
	if body["type"] != defaultProblemType {
		t.Fatalf("type = %v, want %s", body["type"], defaultProblemType)
	}
	if body["title"] != http.StatusText(http.StatusTeapot) {
		t.Fatalf("title = %v, want %s", body["title"], http.StatusText(http.StatusTeapot))
	}
	if body["status"] != float64(http.StatusTeapot) {
		t.Fatalf("status body = %v, want %d", body["status"], http.StatusTeapot)
	}
	if body["detail"] != "short and stout" {
		t.Fatalf("detail = %v, want short and stout", body["detail"])
	}
	if body["instance"] != "/tea/42" {
		t.Fatalf("instance = %v, want /tea/42", body["instance"])
	}
	if body["code"] != "teapot" {
		t.Fatalf("code = %v, want teapot", body["code"])
	}
}

func TestProblemJSONRoundTripKeepsExtensions(t *testing.T) {
	raw := []byte(`{"type":"https://example.test/problems/conflict","title":"Conflict","status":409,"detail":"version mismatch","instance":"/orders/7","code":"stale_version","meta":{"field":"version"}}`)

	var problem Problem
	if err := json.Unmarshal(raw, &problem); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	if problem.Type != "https://example.test/problems/conflict" {
		t.Fatalf("Type = %q, want example problem type", problem.Type)
	}
	if problem.Status != http.StatusConflict {
		t.Fatalf("Status = %d, want %d", problem.Status, http.StatusConflict)
	}
	if problem.Extensions["code"] != "stale_version" {
		t.Fatalf("Extensions[code] = %v, want stale_version", problem.Extensions["code"])
	}
	meta, ok := problem.Extensions["meta"].(map[string]any)
	if !ok || meta["field"] != "version" {
		t.Fatalf("Extensions[meta] = %#v, want field version", problem.Extensions["meta"])
	}

	encoded, err := json.Marshal(problem)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}
	var out map[string]any
	if err := json.Unmarshal(encoded, &out); err != nil {
		t.Fatalf("encoded JSON decode error: %v", err)
	}
	if out["code"] != "stale_version" {
		t.Fatalf("encoded code = %v, want stale_version", out["code"])
	}
	if _, ok := out["meta"].(map[string]any); !ok {
		t.Fatalf("encoded meta = %#v, want object", out["meta"])
	}
}

func TestProblemFromErrorMapsLazuliError(t *testing.T) {
	problem := ProblemFromError(&Error{
		Status:  http.StatusUnprocessableEntity,
		Code:    CodeValidationFailed,
		Message: "email is invalid",
		Data: map[string]string{
			"field": "email",
		},
	})

	if problem.Status != http.StatusUnprocessableEntity {
		t.Fatalf("Status = %d, want %d", problem.Status, http.StatusUnprocessableEntity)
	}
	if problem.Title != http.StatusText(http.StatusUnprocessableEntity) {
		t.Fatalf("Title = %q, want %q", problem.Title, http.StatusText(http.StatusUnprocessableEntity))
	}
	if problem.Detail != "email is invalid" {
		t.Fatalf("Detail = %q, want email is invalid", problem.Detail)
	}
	if problem.Extensions["code"] != CodeValidationFailed {
		t.Fatalf("Extensions[code] = %v, want %s", problem.Extensions["code"], CodeValidationFailed)
	}
	data, ok := problem.Extensions["data"].(map[string]string)
	if !ok || data["field"] != "email" {
		t.Fatalf("Extensions[data] = %#v, want field email", problem.Extensions["data"])
	}
}

func TestProblemFromErrorMapsNonLazuliError(t *testing.T) {
	problem := ProblemFromError(errors.New("database unavailable"))

	if problem.Status != http.StatusInternalServerError {
		t.Fatalf("Status = %d, want %d", problem.Status, http.StatusInternalServerError)
	}
	if problem.Detail != "database unavailable" {
		t.Fatalf("Detail = %q, want database unavailable", problem.Detail)
	}
	if problem.Extensions["code"] != CodeInternal {
		t.Fatalf("Extensions[code] = %v, want %s", problem.Extensions["code"], CodeInternal)
	}
}

func TestWriteErrorUsesProblemDetails(t *testing.T) {
	rec := httptest.NewRecorder()

	writeError(rec, &Error{
		Status:  http.StatusBadRequest,
		Code:    CodeBadRequest,
		Message: "invalid input",
	})

	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusBadRequest)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("Content-Type = %q, want application/problem+json", got)
	}
	body := decodeProblemResponse(t, rec)
	if body["status"] != float64(http.StatusBadRequest) {
		t.Fatalf("status body = %v, want %d", body["status"], http.StatusBadRequest)
	}
	if body["detail"] != "invalid input" {
		t.Fatalf("detail = %v, want invalid input", body["detail"])
	}
	if body["code"] != CodeBadRequest {
		t.Fatalf("code = %v, want %s", body["code"], CodeBadRequest)
	}
}

func decodeProblemResponse(t *testing.T, rec *httptest.ResponseRecorder) map[string]any {
	t.Helper()

	var body map[string]any
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("problem JSON decode error: %v; body = %q", err, rec.Body.String())
	}
	return body
}
