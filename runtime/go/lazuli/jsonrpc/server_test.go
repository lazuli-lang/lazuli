package jsonrpc

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestServerHandlesSingleRequest(t *testing.T) {
	server := NewServer(nil)
	server.Register("echo", func(ctx context.Context, req Request) (any, error) {
		if ctx == nil {
			t.Fatal("ctx = nil")
		}
		var params struct {
			Message string `json:"message"`
		}
		if err := json.Unmarshal(req.Params, &params); err != nil {
			return nil, NewError(CodeInvalidParams, "bad params")
		}
		return map[string]string{"message": params.Message}, nil
	})

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{
		"jsonrpc":"2.0",
		"method":"echo",
		"params":{"message":"hello"},
		"id":1
	}`)))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Content-Type"); got != ContentType {
		t.Fatalf("Content-Type = %q, want %q", got, ContentType)
	}

	response := decodeResponse(t, rec.Body.String())
	if response.JSONRPC != Version {
		t.Fatalf("jsonrpc = %q, want %q", response.JSONRPC, Version)
	}
	if response.Error != nil {
		t.Fatalf("error = %#v, want nil", response.Error)
	}
	if string(response.ID) != "1" {
		t.Fatalf("id = %s, want 1", response.ID)
	}

	var result map[string]string
	if err := json.Unmarshal(response.Result, &result); err != nil {
		t.Fatalf("result decode error = %v", err)
	}
	if result["message"] != "hello" {
		t.Fatalf("result message = %q, want hello", result["message"])
	}
}

func TestServerReturnsTypedError(t *testing.T) {
	server := NewServer(nil)
	server.Register("fail", func(context.Context, Request) (any, error) {
		return nil, NewErrorWithData(CodeInvalidParams, "missing name", map[string]string{"field": "name"})
	})

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{
		"jsonrpc":"2.0",
		"method":"fail",
		"id":"abc"
	}`)))

	response := decodeResponse(t, rec.Body.String())
	if response.Error == nil {
		t.Fatal("error = nil, want JSON-RPC error")
	}
	if response.Error.Code != CodeInvalidParams {
		t.Fatalf("code = %d, want %d", response.Error.Code, CodeInvalidParams)
	}
	if response.Error.Message != "missing name" {
		t.Fatalf("message = %q, want missing name", response.Error.Message)
	}
	if string(response.ID) != `"abc"` {
		t.Fatalf("id = %s, want %q", response.ID, `"abc"`)
	}
}

func TestServerMapsUnknownErrorsToInternalError(t *testing.T) {
	server := NewServer(nil)
	server.Register("fail", func(context.Context, Request) (any, error) {
		return nil, errors.New("database password leaked here")
	})

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{
		"jsonrpc":"2.0",
		"method":"fail",
		"id":1
	}`)))

	response := decodeResponse(t, rec.Body.String())
	if response.Error == nil {
		t.Fatal("error = nil, want JSON-RPC error")
	}
	if response.Error.Code != CodeInternalError {
		t.Fatalf("code = %d, want %d", response.Error.Code, CodeInternalError)
	}
	if response.Error.Message != CodeInternalError.DefaultMessage() {
		t.Fatalf("message = %q, want %q", response.Error.Message, CodeInternalError.DefaultMessage())
	}
}

func TestServerReturnsMethodNotFound(t *testing.T) {
	server := NewServer(nil)

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{
		"jsonrpc":"2.0",
		"method":"missing",
		"id":1
	}`)))

	response := decodeResponse(t, rec.Body.String())
	if response.Error == nil {
		t.Fatal("error = nil, want JSON-RPC error")
	}
	if response.Error.Code != CodeMethodNotFound {
		t.Fatalf("code = %d, want %d", response.Error.Code, CodeMethodNotFound)
	}
	if string(response.ID) != "1" {
		t.Fatalf("id = %s, want 1", response.ID)
	}
}

func TestServerHandlesBatchAndNotifications(t *testing.T) {
	server := NewServer(nil)
	calls := 0
	server.Register("ok", func(context.Context, Request) (any, error) {
		calls++
		return "done", nil
	})

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`[
		{"jsonrpc":"2.0","method":"ok","id":1},
		{"jsonrpc":"2.0","method":"ok"},
		{"jsonrpc":"2.0","method":"missing","id":2}
	]`)))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if calls != 2 {
		t.Fatalf("calls = %d, want 2", calls)
	}

	var responses []wireResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &responses); err != nil {
		t.Fatalf("response decode error = %v; body = %q", err, rec.Body.String())
	}
	if len(responses) != 2 {
		t.Fatalf("responses length = %d, want 2", len(responses))
	}
	if string(responses[0].ID) != "1" {
		t.Fatalf("first id = %s, want 1", responses[0].ID)
	}
	if string(responses[0].Result) != `"done"` {
		t.Fatalf("first result = %s, want %q", responses[0].Result, `"done"`)
	}
	if string(responses[1].ID) != "2" {
		t.Fatalf("second id = %s, want 2", responses[1].ID)
	}
	if responses[1].Error == nil || responses[1].Error.Code != CodeMethodNotFound {
		t.Fatalf("second error = %#v, want method not found", responses[1].Error)
	}
}

func TestServerReturnsNoContentForNotificationOnly(t *testing.T) {
	server := NewServer(nil)
	called := false
	server.Register("notify", func(context.Context, Request) (any, error) {
		called = true
		return nil, nil
	})

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{
		"jsonrpc":"2.0",
		"method":"notify"
	}`)))

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	if rec.Body.Len() != 0 {
		t.Fatalf("body = %q, want empty", rec.Body.String())
	}
	if !called {
		t.Fatal("notification handler was not called")
	}
}

func TestServerRejectsInvalidJSON(t *testing.T) {
	server := NewServer(nil)

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{`)))

	response := decodeResponse(t, rec.Body.String())
	if response.Error == nil {
		t.Fatal("error = nil, want parse error")
	}
	if response.Error.Code != CodeParseError {
		t.Fatalf("code = %d, want %d", response.Error.Code, CodeParseError)
	}
	if string(response.ID) != "null" {
		t.Fatalf("id = %s, want null", response.ID)
	}
}

func TestServerRejectsInvalidRequest(t *testing.T) {
	server := NewServer(nil)

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{
		"jsonrpc":"2.0",
		"id":{"bad":true}
	}`)))

	response := decodeResponse(t, rec.Body.String())
	if response.Error == nil {
		t.Fatal("error = nil, want invalid request")
	}
	if response.Error.Code != CodeInvalidRequest {
		t.Fatalf("code = %d, want %d", response.Error.Code, CodeInvalidRequest)
	}
	if string(response.ID) != "null" {
		t.Fatalf("id = %s, want null", response.ID)
	}
}

func TestServerEchoesDetectedIDForInvalidRequest(t *testing.T) {
	server := NewServer(nil)

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{
		"jsonrpc":"2.0",
		"id":7
	}`)))

	response := decodeResponse(t, rec.Body.String())
	if response.Error == nil {
		t.Fatal("error = nil, want invalid request")
	}
	if response.Error.Code != CodeInvalidRequest {
		t.Fatalf("code = %d, want %d", response.Error.Code, CodeInvalidRequest)
	}
	if string(response.ID) != "7" {
		t.Fatalf("id = %s, want 7", response.ID)
	}
}

func TestServerRejectsEmptyBatch(t *testing.T) {
	server := NewServer(nil)

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`[]`)))

	response := decodeResponse(t, rec.Body.String())
	if response.Error == nil {
		t.Fatal("error = nil, want invalid request")
	}
	if response.Error.Code != CodeInvalidRequest {
		t.Fatalf("code = %d, want %d", response.Error.Code, CodeInvalidRequest)
	}
}

func TestServerRequiresPost(t *testing.T) {
	server := NewServer(nil)

	rec := httptest.NewRecorder()
	server.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))

	if rec.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMethodNotAllowed)
	}
	if got := rec.Header().Get("Allow"); got != http.MethodPost {
		t.Fatalf("Allow = %q, want %q", got, http.MethodPost)
	}
}

func TestRegistryPanicsForInvalidRegistration(t *testing.T) {
	tests := []struct {
		name string
		run  func()
	}{
		{
			name: "empty method",
			run: func() {
				NewRegistry().Register("", func(context.Context, Request) (any, error) { return nil, nil })
			},
		},
		{
			name: "nil handler",
			run: func() {
				NewRegistry().Register("missing", nil)
			},
		},
		{
			name: "duplicate method",
			run: func() {
				registry := NewRegistry()
				registry.Register("same", func(context.Context, Request) (any, error) { return nil, nil })
				registry.Register("same", func(context.Context, Request) (any, error) { return nil, nil })
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			defer func() {
				if recover() == nil {
					t.Fatal("Register did not panic")
				}
			}()
			tt.run()
		})
	}
}

type wireResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	Result  json.RawMessage `json:"result"`
	Error   *Error          `json:"error"`
	ID      json.RawMessage `json:"id"`
}

func decodeResponse(t *testing.T, body string) wireResponse {
	t.Helper()

	var response wireResponse
	if err := json.Unmarshal([]byte(body), &response); err != nil {
		t.Fatalf("response decode error = %v; body = %q", err, body)
	}
	return response
}
