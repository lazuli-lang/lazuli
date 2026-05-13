package lazuli

import (
	"bytes"
	"errors"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
)

type requestBindingInput struct {
	Name   string   `json:"name" form:"name"`
	Age    int      `json:"age" form:"age"`
	Active bool     `json:"active" form:"active"`
	Tags   []string `json:"tags" form:"tags"`
	Token  string   `json:"token"`
}

func TestBindJSONRequestDecodesBody(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/users", strings.NewReader(`{"name":"Ada","age":37,"active":true,"tags":["admin"]}`))
	req.Header.Set("Content-Type", "application/vnd.lazuli+json; charset=utf-8")

	var input requestBindingInput
	if err := BindJSONRequest(req, &input); err != nil {
		t.Fatalf("BindJSONRequest error = %v", err)
	}

	if input.Name != "Ada" || input.Age != 37 || !input.Active {
		t.Fatalf("input = %#v, want decoded scalar fields", input)
	}
	if len(input.Tags) != 1 || input.Tags[0] != "admin" {
		t.Fatalf("tags = %#v, want [admin]", input.Tags)
	}
}

func TestBindJSONRequestRejectsUnknownFieldWhenConfigured(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/users", strings.NewReader(`{"name":"Ada","extra":true}`))
	req.Header.Set("Content-Type", "application/json")

	var input requestBindingInput
	err := BindJSONRequest(req, &input, WithRequestBindingDisallowUnknownJSONFields())
	if err == nil {
		t.Fatal("BindJSONRequest error = nil, want unknown field error")
	}
	if !errors.Is(err, ErrRequestBindingUnknownField) {
		t.Fatalf("error = %v, want ErrRequestBindingUnknownField", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusBadRequest {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusBadRequest)
	}
	if problem.Extensions["code"] != CodeRequestBindingUnknownField {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeRequestBindingUnknownField)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["field"] != "extra" {
		t.Fatalf("field = %v, want extra", data["field"])
	}
}

func TestBindJSONRequestRejectsUnsupportedContentType(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/users", strings.NewReader(`{"name":"Ada"}`))
	req.Header.Set("Content-Type", "text/plain")

	var input requestBindingInput
	err := BindJSONRequest(req, &input)
	if err == nil {
		t.Fatal("BindJSONRequest error = nil, want content type error")
	}
	if !errors.Is(err, ErrRequestBindingUnsupportedMediaType) {
		t.Fatalf("error = %v, want ErrRequestBindingUnsupportedMediaType", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusUnsupportedMediaType {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusUnsupportedMediaType)
	}
	if problem.Extensions["code"] != CodeRequestBindingUnsupportedMediaType {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeRequestBindingUnsupportedMediaType)
	}
}

func TestBindJSONRequestHonorsMaxBytes(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/users", strings.NewReader(`{"name":"Ada"}`))
	req.Header.Set("Content-Type", "application/json")

	var input requestBindingInput
	err := BindJSONRequest(req, &input, WithRequestBindingMaxBytes(4))
	if err == nil {
		t.Fatal("BindJSONRequest error = nil, want body too large error")
	}
	if !errors.Is(err, ErrRequestBindingRequestTooLarge) {
		t.Fatalf("error = %v, want ErrRequestBindingRequestTooLarge", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusRequestEntityTooLarge {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusRequestEntityTooLarge)
	}
	if problem.Extensions["code"] != CodeRequestBindingRequestTooLarge {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeRequestBindingRequestTooLarge)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["max_bytes"] != int64(4) {
		t.Fatalf("max_bytes = %v, want 4", data["max_bytes"])
	}
}

func TestBindURLEncodedRequestBindsFormTags(t *testing.T) {
	form := url.Values{
		"name":   {"Ada"},
		"age":    {"37"},
		"active": {"true"},
		"tags":   {"admin", "owner"},
		"token":  {"json-fallback"},
	}
	req := httptest.NewRequest(http.MethodPost, "/users", strings.NewReader(form.Encode()))
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded; charset=utf-8")

	var input requestBindingInput
	if err := BindURLEncodedRequest(req, &input); err != nil {
		t.Fatalf("BindURLEncodedRequest error = %v", err)
	}

	if input.Name != "Ada" || input.Age != 37 || !input.Active || input.Token != "json-fallback" {
		t.Fatalf("input = %#v, want decoded urlencoded fields", input)
	}
	if len(input.Tags) != 2 || input.Tags[0] != "admin" || input.Tags[1] != "owner" {
		t.Fatalf("tags = %#v, want [admin owner]", input.Tags)
	}
}

func TestBindURLEncodedRequestReportsInvalidField(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/users", strings.NewReader("age=bad"))
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")

	var input requestBindingInput
	err := BindURLEncodedRequest(req, &input)
	if err == nil {
		t.Fatal("BindURLEncodedRequest error = nil, want invalid field error")
	}
	if !errors.Is(err, ErrRequestBindingInvalid) {
		t.Fatalf("error = %v, want ErrRequestBindingInvalid", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusBadRequest {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusBadRequest)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["field"] != "age" {
		t.Fatalf("field = %v, want age", data["field"])
	}
}

func TestBindFormRequestBindsMultipartFormFields(t *testing.T) {
	req := newRequestBindingMultipartRequest(t, map[string][]string{
		"name":   {"Ada"},
		"age":    {"37"},
		"active": {"true"},
		"tags":   {"admin", "owner"},
		"token":  {"json-fallback"},
	})

	var input requestBindingInput
	if err := BindFormRequest(req, &input); err != nil {
		t.Fatalf("BindFormRequest error = %v", err)
	}

	if input.Name != "Ada" || input.Age != 37 || !input.Active || input.Token != "json-fallback" {
		t.Fatalf("input = %#v, want decoded multipart fields", input)
	}
	if len(input.Tags) != 2 || input.Tags[0] != "admin" || input.Tags[1] != "owner" {
		t.Fatalf("tags = %#v, want [admin owner]", input.Tags)
	}
}

func TestBindURLEncodedRequestCanBindValuesMap(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/users", strings.NewReader("tag=a&tag=b"))
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")

	var values url.Values
	if err := BindURLEncodedRequest(req, &values); err != nil {
		t.Fatalf("BindURLEncodedRequest error = %v", err)
	}
	if got := values["tag"]; len(got) != 2 || got[0] != "a" || got[1] != "b" {
		t.Fatalf("values[tag] = %#v, want [a b]", got)
	}
}

func newRequestBindingMultipartRequest(t *testing.T, fields map[string][]string) *http.Request {
	t.Helper()

	var body bytes.Buffer
	writer := multipart.NewWriter(&body)
	for name, values := range fields {
		for _, value := range values {
			if err := writer.WriteField(name, value); err != nil {
				t.Fatalf("WriteField error = %v", err)
			}
		}
	}
	if err := writer.Close(); err != nil {
		t.Fatalf("multipart writer close error = %v", err)
	}

	req := httptest.NewRequest(http.MethodPost, "/users", &body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	return req
}
