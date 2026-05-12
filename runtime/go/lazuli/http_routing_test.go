package lazuli

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

type httpRoutingInput struct {
	Name string `json:"name"`
}

type httpRoutingRow struct {
	ID ID `db:"id" json:"id"`
}

func TestMountAllMountsRegisteredCommandAndQueryRoutes(t *testing.T) {
	withIsolatedHTTPRegistry(t)

	RegisterCommand(&Command[httpRoutingInput, httpRoutingRow]{Name: "hostpoint.create"})
	RegisterQuery(&Query[httpRoutingInput, httpRoutingRow]{
		Name: "hostpoint.query.list",
		Kind: QueryList,
	})

	mux := http.NewServeMux()
	MountAll(mux)

	for _, tc := range []struct {
		name   string
		path   string
		method string
		status int
	}{
		{
			name:   "command route dispatches",
			path:   "/api/v1/c/hostpoint.create",
			method: http.MethodPost,
			status: http.StatusBadRequest,
		},
		{
			name:   "query route dispatches",
			path:   "/api/v1/q/hostpoint.query.list",
			method: http.MethodPost,
			status: http.StatusBadRequest,
		},
		{
			name:   "command rejects wrong method",
			path:   "/api/v1/c/hostpoint.create",
			method: http.MethodGet,
			status: http.StatusMethodNotAllowed,
		},
		{
			name:   "query rejects wrong method",
			path:   "/api/v1/q/hostpoint.query.list",
			method: http.MethodDelete,
			status: http.StatusMethodNotAllowed,
		},
		{
			name:   "unknown route stays not found",
			path:   "/api/v1/c/hostpoint.missing",
			method: http.MethodPost,
			status: http.StatusNotFound,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			rec := httptest.NewRecorder()
			req := httptest.NewRequest(tc.method, tc.path, strings.NewReader("{"))

			mux.ServeHTTP(rec, req)

			if rec.Code != tc.status {
				t.Fatalf("status = %d, want %d; body = %s", rec.Code, tc.status, rec.Body.String())
			}
		})
	}
}

func TestMuxSupportsGeneratedMountAllCall(t *testing.T) {
	withIsolatedHTTPRegistry(t)

	RegisterCommand(&Command[httpRoutingInput, httpRoutingRow]{Name: "hostpoint.create"})

	mux := Mux()
	MountAll(mux)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/hostpoint.create", strings.NewReader("{"))

	mux.ServeHTTP(rec, req)

	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d; body = %s", rec.Code, http.StatusBadRequest, rec.Body.String())
	}
}

func TestMountAllMountsRegisteredAPIMetadataRoute(t *testing.T) {
	withIsolatedHTTPRegistry(t)

	GlobalRegistry.RegisterApi(apiRegistration{
		Name:    "hostpoint.report",
		Feature: "hostpoint",
		Path:    "/api/hostpoint/reports/:id",
	})

	mux := http.NewServeMux()
	MountAll(mux)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/hostpoint/reports/42", nil)

	mux.ServeHTTP(rec, req)

	if rec.Code != http.StatusNotImplemented {
		t.Fatalf("status = %d, want %d; body = %s", rec.Code, http.StatusNotImplemented, rec.Body.String())
	}
}

func TestMountApiHandlesMethodAndPathParameters(t *testing.T) {
	type summaryArgs struct {
		ID ID `json:"id"`
	}
	type summaryOut struct {
		ID    ID    `json:"id"`
		Actor Actor `json:"actor"`
	}

	api := &Api[summaryArgs, summaryOut]{
		Name:   "customer_summary",
		Method: MethodGet,
		Path:   "/api/customer/{id}/summary",
		Policy: Policy{
			Name:  "@scope.public",
			Atoms: []PolicyAtom{{Namespace: "scope", Name: "public"}},
		},
		Handler: func(ctx *Ctx, input summaryArgs) (summaryOut, error) {
			return summaryOut{ID: input.ID, Actor: ctx.Actor}, nil
		},
	}
	mux := http.NewServeMux()
	MountApi(mux, api)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/customer/42/summary", nil)

	mux.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body = %s", rec.Code, http.StatusOK, rec.Body.String())
	}
	var out summaryOut
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("unmarshal response: %v", err)
	}
	if out.ID != 42 || out.Actor != ActorAnonymous {
		t.Fatalf("response = %+v, want id 42 actor anonymous", out)
	}

	rec = httptest.NewRecorder()
	req = httptest.NewRequest(http.MethodPost, "/api/customer/42/summary", nil)
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusMethodNotAllowed {
		t.Fatalf("POST status = %d, want %d", rec.Code, http.StatusMethodNotAllowed)
	}

	rec = httptest.NewRecorder()
	req = httptest.NewRequest(http.MethodGet, "/api/customer/not-an-id/summary", nil)
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("bad id status = %d, want %d", rec.Code, http.StatusBadRequest)
	}
}

func withIsolatedHTTPRegistry(t *testing.T) {
	t.Helper()

	registry.Lock()
	oldResources := registry.resources
	oldCommands := registry.commands
	oldCommandHandlers := registry.commandHandlers
	oldQueries := registry.queries
	oldQueryHandlers := registry.queryHandlers
	registry.resources = map[string]*resourceErased{}
	registry.commands = map[string]*commandErased{}
	registry.commandHandlers = map[string]commandHandler{}
	registry.queries = map[string]*queryErased{}
	registry.queryHandlers = map[string]queryHandler{}
	registry.Unlock()

	GlobalRegistry.mu.Lock()
	oldGlobalResources := GlobalRegistry.resources
	oldGlobalCommands := GlobalRegistry.commands
	oldGlobalQueries := GlobalRegistry.queries
	oldGlobalAPIs := GlobalRegistry.apis
	GlobalRegistry.resources = map[string]*resourceErased{}
	GlobalRegistry.commands = map[string]commandRegistration{}
	GlobalRegistry.queries = map[string]queryRegistration{}
	GlobalRegistry.apis = map[string]apiRegistration{}
	GlobalRegistry.mu.Unlock()

	t.Cleanup(func() {
		registry.Lock()
		registry.resources = oldResources
		registry.commands = oldCommands
		registry.commandHandlers = oldCommandHandlers
		registry.queries = oldQueries
		registry.queryHandlers = oldQueryHandlers
		registry.Unlock()

		GlobalRegistry.mu.Lock()
		GlobalRegistry.resources = oldGlobalResources
		GlobalRegistry.commands = oldGlobalCommands
		GlobalRegistry.queries = oldGlobalQueries
		GlobalRegistry.apis = oldGlobalAPIs
		GlobalRegistry.mu.Unlock()
	})
}
