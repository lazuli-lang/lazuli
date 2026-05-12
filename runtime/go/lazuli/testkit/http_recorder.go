package testkit

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strconv"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli"
)

// HTTPRecord captures one served HTTP exchange for generated handler tests.
type HTTPRecord struct {
	// Request is a defensive copy of the request before the handler consumed it.
	Request *http.Request
	// Response is the response returned by the handler. Its Body is replayable.
	Response *http.Response
	// Body is a defensive copy of the response body.
	Body []byte
}

// ProblemExpectation describes fields AssertJSONProblem checks. Zero values are
// ignored, except Extensions entries which must be present and equal.
type ProblemExpectation struct {
	Status     int
	Type       string
	Title      string
	Detail     string
	Code       string
	Extensions map[string]any
}

// NewHTTPRequest builds a httptest request and encodes ctx using the same
// dev-mode headers consumed by the Lazuli HTTP runtime.
func NewHTTPRequest(t testing.TB, method, target string, body any, ctx *lazuli.Ctx) *http.Request {
	t.Helper()

	if method == "" {
		method = http.MethodGet
	}
	if target == "" {
		target = "/"
	}

	reader, jsonBody := httpRequestBody(t, body)
	req := httptest.NewRequest(method, target, reader)
	if jsonBody {
		req.Header.Set("Content-Type", "application/json")
	}
	applyLazuliContext(req, ctx)
	return req
}

// RecordHTTP serves req through handler and captures the request and response.
func RecordHTTP(t testing.TB, handler http.Handler, req *http.Request) *HTTPRecord {
	t.Helper()

	if handler == nil {
		t.Fatal("testkit: nil HTTP handler")
	}
	requestRecord, err := recordRequest(req)
	if err != nil {
		t.Fatalf("testkit: record request: %v", err)
	}

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)

	res := rec.Result()
	body, err := io.ReadAll(res.Body)
	if err != nil {
		t.Fatalf("testkit: read response body: %v", err)
	}
	_ = res.Body.Close()
	res.Body = io.NopCloser(bytes.NewReader(body))
	res.ContentLength = int64(len(body))

	return &HTTPRecord{
		Request:  cloneRecordedRequest(requestRecord),
		Response: res,
		Body:     cloneBytes(body),
	}
}

// AssertStatus fails the test when the captured response status differs.
func (r *HTTPRecord) AssertStatus(t testing.TB, want int) {
	t.Helper()
	if r == nil || r.Response == nil {
		t.Fatal("testkit: nil HTTP record response")
	}
	if want == 0 {
		want = http.StatusOK
	}
	if r.Response.StatusCode != want {
		t.Fatalf("status = %d %s, want %d %s",
			r.Response.StatusCode, http.StatusText(r.Response.StatusCode),
			want, http.StatusText(want))
	}
}

// AssertHeader fails the test when the response header value differs.
func (r *HTTPRecord) AssertHeader(t testing.TB, name, want string) {
	t.Helper()
	if r == nil || r.Response == nil {
		t.Fatal("testkit: nil HTTP record response")
	}
	if got := r.Response.Header.Get(name); got != want {
		t.Fatalf("response header %q = %q, want %q", name, got, want)
	}
}

// AssertJSONProblem decodes and checks an application/problem+json response.
// It returns the decoded problem for any extra test-specific checks.
func (r *HTTPRecord) AssertJSONProblem(t testing.TB, want ProblemExpectation) lazuli.Problem {
	t.Helper()
	if r == nil || r.Response == nil {
		t.Fatal("testkit: nil HTTP record response")
	}

	r.AssertHeader(t, "Content-Type", "application/problem+json")
	if want.Status != 0 {
		r.AssertStatus(t, want.Status)
	}

	var problem lazuli.Problem
	if err := json.Unmarshal(r.Body, &problem); err != nil {
		t.Fatalf("problem JSON decode error: %v; body = %q", err, string(r.Body))
	}

	if problem.Status != r.Response.StatusCode {
		t.Fatalf("problem status = %d, want response status %d", problem.Status, r.Response.StatusCode)
	}
	if want.Type != "" && problem.Type != want.Type {
		t.Fatalf("problem type = %q, want %q", problem.Type, want.Type)
	}
	if want.Title != "" && problem.Title != want.Title {
		t.Fatalf("problem title = %q, want %q", problem.Title, want.Title)
	}
	if want.Detail != "" && problem.Detail != want.Detail {
		t.Fatalf("problem detail = %q, want %q", problem.Detail, want.Detail)
	}
	if want.Code != "" {
		got, _ := problem.Extensions["code"].(string)
		if got != want.Code {
			t.Fatalf("problem code = %q, want %q", got, want.Code)
		}
	}
	for name, wantValue := range want.Extensions {
		gotValue, ok := problem.Extensions[name]
		if !ok {
			t.Fatalf("problem extension %q missing", name)
		}
		wantValue = normalizeProblemExtension(t, wantValue)
		if !reflect.DeepEqual(gotValue, wantValue) {
			t.Fatalf("problem extension %q = %#v, want %#v", name, gotValue, wantValue)
		}
	}
	return problem
}

func httpRequestBody(t testing.TB, body any) (io.Reader, bool) {
	t.Helper()

	switch body := body.(type) {
	case nil:
		return nil, false
	case json.RawMessage:
		return bytes.NewReader(body), true
	case []byte:
		return bytes.NewReader(body), false
	case string:
		return strings.NewReader(body), false
	case io.Reader:
		return body, false
	default:
		data, err := json.Marshal(body)
		if err != nil {
			t.Fatalf("request body JSON marshal error: %v", err)
		}
		return bytes.NewReader(data), true
	}
}

func applyLazuliContext(req *http.Request, ctx *lazuli.Ctx) {
	if ctx == nil {
		return
	}
	if ctx.Context != nil {
		*req = *req.WithContext(ctx.Context)
	}
	if ctx.User == nil && ctx.Actor != "" {
		req.Header.Set("X-Lazuli-Actor", string(ctx.Actor))
	}
	if ctx.User != nil {
		req.Header.Set("X-Lazuli-User-ID", strconv.FormatInt(ctx.User.ID, 10))
		if ctx.User.Email != "" {
			req.Header.Set("X-Lazuli-Email", ctx.User.Email)
		}
		if len(ctx.User.Roles) > 0 {
			req.Header.Set("X-Lazuli-Roles", strings.Join(ctx.User.Roles, ","))
		}
		if ctx.User.OrgID != 0 {
			req.Header.Set("X-Lazuli-Org-ID", strconv.FormatInt(ctx.User.OrgID, 10))
		}
	}
	if ctx.Tenant != nil {
		req.Header.Set("X-Lazuli-Org-ID", strconv.FormatInt(ctx.Tenant.OrgID, 10))
	}
	if ctx.RequestID != "" {
		req.Header.Set("X-Request-ID", ctx.RequestID)
	}
	if ctx.TraceID != "" {
		req.Header.Set("X-Trace-ID", ctx.TraceID)
	}
}

func normalizeProblemExtension(t testing.TB, value any) any {
	t.Helper()

	data, err := json.Marshal(value)
	if err != nil {
		t.Fatalf("problem extension JSON marshal error: %v", err)
	}
	var normalized any
	if err := json.Unmarshal(data, &normalized); err != nil {
		t.Fatalf("problem extension JSON decode error: %v", err)
	}
	return normalized
}
