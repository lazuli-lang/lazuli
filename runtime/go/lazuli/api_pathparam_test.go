package lazuli

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

// SECURITY (SEC-API-PATHARG-UNBOUND): an `api` contract with a path
// parameter (`api ... path "/x/{id}/y"`) must bind the matched route
// variable into the typed handler input. Before the fix the dispatch
// surface only decoded the JSON body, so a path-keyed endpoint received
// the zero value for `id` — every request to `/attachments/{id}/url`
// reached the handler with id=0.

type pathArgInput struct {
	ID ID `json:"id"`
}

// TestApiPathParamBindsIntoHandlerInputOverHTTP is the load-bearing
// regression: a real HTTP request to `GET /x/{id}/y` with id=42 must
// reach the handler carrying ID=42. Pre-fix this captured 0.
func TestApiPathParamBindsIntoHandlerInputOverHTTP(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	installZeroAuthoringFixture(t)
	t.Cleanup(clearRegistriesForTest())

	var seen ID = -1
	api := &Api[pathArgInput, map[string]any]{
		Name:    "attachments.path_probe",
		Feature: "attachments",
		Method:  MethodGet,
		Path:    "/x/{id}/y",
		Policy: Policy{
			Name:  "@policy.authenticated",
			Atoms: []PolicyAtom{{Namespace: "scope", Name: "authenticated"}},
		},
		Handler: func(_ *Ctx, in pathArgInput) (map[string]any, error) {
			seen = in.ID
			return map[string]any{"id": in.ID}, nil
		},
	}
	RegisterApi(api)

	handler := Mux()

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/x/42/y", nil)
	// Satisfy @policy.authenticated via the dev-session header path.
	req.Header.Set("X-Lazuli-User-ID", "7")
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("path-param api request returned %d (want 200); body: %s", rec.Code, rec.Body.String())
	}
	if seen != 42 {
		t.Fatalf("handler input.ID = %d, want 42 (path param was not bound — the SEC-API-PATHARG-UNBOUND bug)", seen)
	}
	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("response not JSON: %v; body: %s", err, rec.Body.String())
	}
	if got, _ := body["id"].(float64); got != 42 {
		t.Fatalf("response id = %v, want 42; body: %s", body["id"], rec.Body.String())
	}
}

// TestApiPathParamRejectsNonNumericForIntField proves the binder coerces
// per the field type and surfaces a 400 (not a silent zero) when a path
// segment cannot fit the declared slot.
func TestApiPathParamRejectsNonNumericForIntField(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	installZeroAuthoringFixture(t)
	t.Cleanup(clearRegistriesForTest())

	handlerRan := false
	api := &Api[pathArgInput, map[string]any]{
		Name:    "attachments.path_probe_bad",
		Feature: "attachments",
		Method:  MethodGet,
		Path:    "/z/{id}/w",
		Policy: Policy{
			Name:  "@policy.public",
			Atoms: []PolicyAtom{{Namespace: "scope", Name: "public"}},
		},
		Handler: func(_ *Ctx, _ pathArgInput) (map[string]any, error) {
			handlerRan = true
			return map[string]any{}, nil
		},
	}
	RegisterApi(api)

	handler := Mux()
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/z/not-a-number/w", nil)
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusBadRequest {
		t.Fatalf("non-numeric path param for int field returned %d (want 400); body: %s", rec.Code, rec.Body.String())
	}
	if handlerRan {
		t.Fatal("handler must not run when a path param fails to coerce")
	}
}

// --- binder unit tests (no HTTP) ---

func TestBindPathParamsInt64(t *testing.T) {
	in := &pathArgInput{}
	if err := bindPathParams(in, map[string]string{"id": "99"}); err != nil {
		t.Fatalf("bindPathParams: %v", err)
	}
	if in.ID != 99 {
		t.Fatalf("ID = %d, want 99", in.ID)
	}
}

func TestBindPathParamsString(t *testing.T) {
	type uuidInput struct {
		Token string `json:"token"`
	}
	in := &uuidInput{}
	if err := bindPathParams(in, map[string]string{"token": "abc-123"}); err != nil {
		t.Fatalf("bindPathParams: %v", err)
	}
	if in.Token != "abc-123" {
		t.Fatalf("Token = %q, want abc-123", in.Token)
	}
}

// Path param overrides a same-named body field — the path is the
// authoritative source for its declared slot.
func TestBindPathParamsOverridesBody(t *testing.T) {
	in := &pathArgInput{ID: 1} // as if decoded from body
	if err := bindPathParams(in, map[string]string{"id": "55"}); err != nil {
		t.Fatalf("bindPathParams: %v", err)
	}
	if in.ID != 55 {
		t.Fatalf("ID = %d, want 55 (path must win over body)", in.ID)
	}
}

// An unmapped path param name is skipped, not fatal — other params still
// bind.
func TestBindPathParamsSkipsUnknownName(t *testing.T) {
	in := &pathArgInput{}
	if err := bindPathParams(in, map[string]string{"id": "3", "unknown": "x"}); err != nil {
		t.Fatalf("bindPathParams: %v", err)
	}
	if in.ID != 3 {
		t.Fatalf("ID = %d, want 3", in.ID)
	}
}

func TestPathParamNames(t *testing.T) {
	cases := map[string][]string{
		"/attachments/{id}/url": {"id"},
		"/a/{x}/b/{y}":          {"x", "y"},
		"/no/params":            nil,
		"/files/{path...}":      {"path"},
	}
	for pattern, want := range cases {
		got := pathParamNames(pattern)
		if len(got) != len(want) {
			t.Fatalf("pathParamNames(%q) = %v, want %v", pattern, got, want)
		}
		for i := range want {
			if got[i] != want[i] {
				t.Fatalf("pathParamNames(%q)[%d] = %q, want %q", pattern, i, got[i], want[i])
			}
		}
	}
}
