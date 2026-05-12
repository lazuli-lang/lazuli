package testkit_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/testkit"
)

type contextKey string

func TestHTTPRecorderCapturesGeneratedHandlerExchange(t *testing.T) {
	type input struct {
		ID   string `json:"id"`
		Name string `json:"name"`
	}
	type body struct {
		Name string `json:"name"`
	}
	type output struct {
		ID        string `json:"id"`
		Name      string `json:"name"`
		Actor     string `json:"actor"`
		UserID    int64  `json:"user_id"`
		OrgID     int64  `json:"org_id"`
		RequestID string `json:"request_id"`
		TraceID   string `json:"trace_id"`
		Context   string `json:"context"`
	}

	key := contextKey("marker")
	api := &lazuli.Api[input, output]{
		Name:   "widgets.show",
		Method: lazuli.MethodPost,
		Path:   "/widgets/{id}",
		Policy: lazuli.Policy{Atoms: []lazuli.PolicyAtom{{Namespace: "role", Name: "admin"}}},
		Handler: func(ctx *lazuli.Ctx, in input) (output, error) {
			return output{
				ID:        in.ID,
				Name:      in.Name,
				Actor:     string(ctx.Actor),
				UserID:    ctx.User.ID,
				OrgID:     ctx.Tenant.OrgID,
				RequestID: ctx.RequestID,
				TraceID:   ctx.TraceID,
				Context:   ctx.Value(key).(string),
			}, nil
		},
	}
	mux := http.NewServeMux()
	lazuli.MountApi(mux, api)

	ctx := &lazuli.Ctx{
		Context:   context.WithValue(context.Background(), key, "from-context"),
		Actor:     lazuli.ActorUser,
		User:      &lazuli.User{ID: 42, OrgID: 7, Email: "dev@example.test", Roles: []string{"admin", "ops"}},
		Tenant:    &lazuli.Tenant{OrgID: 7},
		RequestID: "req-123",
		TraceID:   "trace-456",
	}
	req := testkit.NewHTTPRequest(t, http.MethodPost, "/widgets/w-1", body{Name: "Ada"}, ctx)

	record := testkit.RecordHTTP(t, mux, req)
	record.AssertStatus(t, http.StatusOK)
	record.AssertHeader(t, "Content-Type", "application/json")

	requestBody, err := io.ReadAll(record.Request.Body)
	if err != nil {
		t.Fatalf("ReadAll(recorded request) error = %v", err)
	}
	if strings.TrimSpace(string(requestBody)) != `{"name":"Ada"}` {
		t.Fatalf("recorded request body = %q, want JSON name body", requestBody)
	}

	var got output
	if err := json.Unmarshal(record.Body, &got); err != nil {
		t.Fatalf("response JSON decode error: %v", err)
	}
	want := output{
		ID:        "w-1",
		Name:      "Ada",
		Actor:     "user",
		UserID:    42,
		OrgID:     7,
		RequestID: "req-123",
		TraceID:   "trace-456",
		Context:   "from-context",
	}
	if got != want {
		t.Fatalf("response = %+v, want %+v", got, want)
	}

	replayed, err := io.ReadAll(record.Response.Body)
	if err != nil {
		t.Fatalf("ReadAll(replayable response) error = %v", err)
	}
	if !strings.Contains(string(replayed), `"request_id":"req-123"`) {
		t.Fatalf("replayed response body = %q, want request_id", replayed)
	}
}

func TestHTTPRecorderAssertsJSONProblem(t *testing.T) {
	type input struct{}
	type output struct{}

	api := &lazuli.Api[input, output]{
		Name:   "widgets.create",
		Method: lazuli.MethodPost,
		Path:   "/widgets",
		Policy: lazuli.Policy{Atoms: []lazuli.PolicyAtom{{Namespace: "scope", Name: "public"}}},
		Handler: func(_ *lazuli.Ctx, _ input) (output, error) {
			return output{}, &lazuli.Error{
				Status:  http.StatusConflict,
				Code:    "widget_exists",
				Message: "widget already exists",
				Data:    map[string]any{"id": "w-1", "attempt": 2},
			}
		},
	}
	mux := http.NewServeMux()
	lazuli.MountApi(mux, api)

	req := testkit.NewHTTPRequest(t, http.MethodPost, "/widgets", input{}, nil)
	record := testkit.RecordHTTP(t, mux, req)

	problem := record.AssertJSONProblem(t, testkit.ProblemExpectation{
		Status: http.StatusConflict,
		Type:   "about:blank",
		Title:  "Conflict",
		Detail: "widget already exists",
		Code:   "widget_exists",
		Extensions: map[string]any{
			"data": map[string]any{"id": "w-1", "attempt": 2},
		},
	})
	if problem.Extensions["code"] != "widget_exists" {
		t.Fatalf("decoded problem code = %v, want widget_exists", problem.Extensions["code"])
	}
}
