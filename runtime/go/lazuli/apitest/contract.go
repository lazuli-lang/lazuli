// Package apitest provides small stdlib-only helpers for HTTP API
// contract tests.
package apitest

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strconv"
	"strings"
	"testing"
)

// RequestCase describes one HTTP request and its expected response.
type RequestCase struct {
	// Name is used as the subtest name. When empty, RunCases derives
	// a name from the case index, method, and path.
	Name string

	// Method is the HTTP method. Empty defaults to GET.
	Method string

	// Path is the request target, including any query string. Empty
	// defaults to "/".
	Path string

	// Body is the request body. nil sends no body; string, []byte,
	// json.RawMessage, and io.Reader are sent directly. Any other value
	// is marshaled as JSON and gets Content-Type: application/json when
	// the case did not provide a Content-Type header.
	Body any

	// Headers are added to the request before it is served.
	Headers http.Header

	// Expected is the response contract for this request.
	Expected ExpectedResponse
}

// ExpectedResponse describes assertions RunCases applies to a response.
type ExpectedResponse struct {
	// Status is the expected status code. Zero defaults to 200 OK.
	Status int

	// Headers are expected response headers. A single expected value is
	// compared with Header.Get; multiple expected values must match the
	// response values in order.
	Headers http.Header

	// JSON, when non-nil, must match the response body after both are
	// decoded as JSON. Use json.RawMessage or []byte for raw JSON.
	JSON any

	// JSONSubset, when non-nil, must be contained in the decoded response
	// body. Object fields are matched recursively; arrays must match
	// length and order, with each element matched recursively; scalar
	// values are compared exactly.
	JSONSubset any
}

// RunCases serves each request case against handler and fails the
// corresponding subtest when the response does not match the contract.
func RunCases(t *testing.T, handler http.Handler, cases []RequestCase) {
	t.Helper()

	if handler == nil {
		t.Fatal("apitest: nil handler")
	}

	for i, tc := range cases {
		tc := tc
		t.Run(caseName(i, tc), func(t *testing.T) {
			t.Helper()

			req := newRequest(t, tc)
			rec := httptest.NewRecorder()
			handler.ServeHTTP(rec, req)

			res := rec.Result()
			defer res.Body.Close()

			assertStatus(t, res.StatusCode, tc.Expected.Status)
			assertResponseHeaders(t, res.Header, tc.Expected.Headers)

			body := rec.Body.Bytes()
			if tc.Expected.JSON != nil {
				assertJSONEqual(t, body, tc.Expected.JSON)
			}
			if tc.Expected.JSONSubset != nil {
				assertJSONSubset(t, body, tc.Expected.JSONSubset)
			}
		})
	}
}

func caseName(i int, tc RequestCase) string {
	if tc.Name != "" {
		return tc.Name
	}

	return fmt.Sprintf("%02d %s %s", i+1, requestMethod(tc), requestPath(tc))
}

func newRequest(t *testing.T, tc RequestCase) *http.Request {
	t.Helper()

	body, jsonBody := requestBody(t, tc.Body)
	req := httptest.NewRequest(requestMethod(tc), requestPath(tc), body)
	for name, values := range tc.Headers {
		for _, value := range values {
			req.Header.Add(name, value)
		}
	}
	if jsonBody && req.Header.Get("Content-Type") == "" {
		req.Header.Set("Content-Type", "application/json")
	}
	return req
}

func requestMethod(tc RequestCase) string {
	if tc.Method == "" {
		return http.MethodGet
	}
	return tc.Method
}

func requestPath(tc RequestCase) string {
	if tc.Path == "" {
		return "/"
	}
	return tc.Path
}

func requestBody(t *testing.T, body any) (io.Reader, bool) {
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

func assertStatus(t *testing.T, got, want int) {
	t.Helper()

	if want == 0 {
		want = http.StatusOK
	}
	if got != want {
		t.Fatalf("status = %d %s, want %d %s", got, http.StatusText(got), want, http.StatusText(want))
	}
}

func assertResponseHeaders(t *testing.T, got, want http.Header) {
	t.Helper()

	for name, wantValues := range want {
		switch len(wantValues) {
		case 0:
			if len(got.Values(name)) == 0 {
				t.Fatalf("response header %q missing", name)
			}
		case 1:
			if gotValue := got.Get(name); gotValue != wantValues[0] {
				t.Fatalf("response header %q = %q, want %q", name, gotValue, wantValues[0])
			}
		default:
			gotValues := got.Values(name)
			if !reflect.DeepEqual(gotValues, wantValues) {
				t.Fatalf("response header %q = %q, want %q", name, gotValues, wantValues)
			}
		}
	}
}

func assertJSONEqual(t *testing.T, body []byte, want any) {
	t.Helper()

	gotJSON, err := decodeJSON(body)
	if err != nil {
		t.Fatalf("response body JSON decode error: %v; body = %q", err, string(body))
	}
	wantJSON, err := normalizeExpectedJSON(want)
	if err != nil {
		t.Fatalf("expected JSON decode error: %v", err)
	}
	if !reflect.DeepEqual(gotJSON, wantJSON) {
		t.Fatalf("JSON body mismatch\nwant:\n%s\ngot:\n%s", formatJSON(wantJSON), formatJSON(gotJSON))
	}
}

func assertJSONSubset(t *testing.T, body []byte, want any) {
	t.Helper()

	gotJSON, err := decodeJSON(body)
	if err != nil {
		t.Fatalf("response body JSON decode error: %v; body = %q", err, string(body))
	}
	wantJSON, err := normalizeExpectedJSON(want)
	if err != nil {
		t.Fatalf("expected JSON subset decode error: %v", err)
	}
	if err := compareJSONSubset(wantJSON, gotJSON, "$"); err != nil {
		t.Fatalf("JSON body subset mismatch: %v\nwant subset:\n%s\ngot:\n%s", err, formatJSON(wantJSON), formatJSON(gotJSON))
	}
}

func normalizeExpectedJSON(v any) (any, error) {
	var data []byte
	switch v := v.(type) {
	case json.RawMessage:
		data = v
	case []byte:
		data = v
	default:
		var err error
		data, err = json.Marshal(v)
		if err != nil {
			return nil, err
		}
	}
	return decodeJSON(data)
}

func decodeJSON(data []byte) (any, error) {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()

	var out any
	if err := dec.Decode(&out); err != nil {
		return nil, err
	}

	var extra any
	err := dec.Decode(&extra)
	if err == nil {
		return nil, errors.New("multiple JSON values")
	}
	if !errors.Is(err, io.EOF) {
		return nil, err
	}
	return out, nil
}

func compareJSONSubset(want, got any, path string) error {
	switch want := want.(type) {
	case map[string]any:
		got, ok := got.(map[string]any)
		if !ok {
			return fmt.Errorf("%s: got %s, want object", path, jsonType(got))
		}
		for key, wantValue := range want {
			gotValue, ok := got[key]
			if !ok {
				return fmt.Errorf("%s: missing key %q", path, key)
			}
			if err := compareJSONSubset(wantValue, gotValue, jsonObjectPath(path, key)); err != nil {
				return err
			}
		}
		return nil
	case []any:
		got, ok := got.([]any)
		if !ok {
			return fmt.Errorf("%s: got %s, want array", path, jsonType(got))
		}
		if len(got) != len(want) {
			return fmt.Errorf("%s: array length = %d, want %d", path, len(got), len(want))
		}
		for i, wantValue := range want {
			if err := compareJSONSubset(wantValue, got[i], jsonArrayPath(path, i)); err != nil {
				return err
			}
		}
		return nil
	default:
		if !reflect.DeepEqual(got, want) {
			return fmt.Errorf("%s: got %s, want %s", path, formatJSONInline(got), formatJSONInline(want))
		}
		return nil
	}
}

func jsonObjectPath(path, key string) string {
	return path + "[" + strconv.Quote(key) + "]"
}

func jsonArrayPath(path string, index int) string {
	return fmt.Sprintf("%s[%d]", path, index)
}

func jsonType(v any) string {
	switch v.(type) {
	case nil:
		return "null"
	case map[string]any:
		return "object"
	case []any:
		return "array"
	case string:
		return "string"
	case json.Number:
		return "number"
	case bool:
		return "bool"
	default:
		return fmt.Sprintf("%T", v)
	}
}

func formatJSON(v any) string {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return fmt.Sprintf("%#v", v)
	}
	return string(data)
}

func formatJSONInline(v any) string {
	data, err := json.Marshal(v)
	if err != nil {
		return fmt.Sprintf("%#v", v)
	}
	return string(data)
}
