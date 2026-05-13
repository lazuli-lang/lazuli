package lazuli

import (
	"bytes"
	"encoding/xml"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

type xmlBindingWidget struct {
	XMLName xml.Name `xml:"widget"`
	Name    string   `xml:"name"`
}

func TestIsXMLContentType(t *testing.T) {
	tests := []struct {
		contentType string
		want        bool
	}{
		{contentType: "application/xml", want: true},
		{contentType: "application/xml; charset=utf-8", want: true},
		{contentType: "text/xml", want: true},
		{contentType: "Application/Vnd.Lazuli.Widget+XML; Charset=UTF-8", want: true},
		{contentType: "application/json", want: false},
		{contentType: "text/plain", want: false},
		{contentType: "", want: false},
		{contentType: "application/xml; charset", want: false},
	}

	for _, tt := range tests {
		t.Run(tt.contentType, func(t *testing.T) {
			if got := IsXMLContentType(tt.contentType); got != tt.want {
				t.Fatalf("IsXMLContentType(%q) = %v, want %v", tt.contentType, got, tt.want)
			}
		})
	}
}

func TestDecodeXMLRequestDecodesStruct(t *testing.T) {
	req := newXMLBindingRequest(`<widget><name>Ada</name></widget>`, "application/xml; charset=utf-8")

	var got xmlBindingWidget
	if err := DecodeXMLRequest(httptest.NewRecorder(), req, &got); err != nil {
		t.Fatalf("DecodeXMLRequest error = %v", err)
	}

	if got.Name != "Ada" {
		t.Fatalf("Name = %q, want Ada", got.Name)
	}
	if got.XMLName.Local != "widget" {
		t.Fatalf("XMLName.Local = %q, want widget", got.XMLName.Local)
	}
}

func TestDecodeXMLRequestRejectsUnsupportedContentType(t *testing.T) {
	req := newXMLBindingRequest(`<widget><name>Ada</name></widget>`, "application/json")

	var got xmlBindingWidget
	err := DecodeXMLRequest(httptest.NewRecorder(), req, &got)
	if err == nil {
		t.Fatal("DecodeXMLRequest error = nil, want unsupported media type")
	}
	if !errors.Is(err, ErrXMLUnsupportedMediaType) {
		t.Fatalf("error = %v, want ErrXMLUnsupportedMediaType", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusUnsupportedMediaType {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusUnsupportedMediaType)
	}
	if problem.Extensions["code"] != CodeXMLUnsupportedMediaType {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeXMLUnsupportedMediaType)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["content_type"] != "application/json" {
		t.Fatalf("content_type = %v, want application/json", data["content_type"])
	}
}

func TestDecodeXMLRequestHonorsMaxBytes(t *testing.T) {
	req := newXMLBindingRequest(`<widget><name>`+strings.Repeat("x", 128)+`</name></widget>`, "application/xml")
	req.ContentLength = -1

	var got xmlBindingWidget
	err := DecodeXMLRequest(httptest.NewRecorder(), req, &got, WithXMLBindingMaxBytes(32))
	if err == nil {
		t.Fatal("DecodeXMLRequest error = nil, want request too large")
	}
	if !errors.Is(err, ErrXMLRequestTooLarge) {
		t.Fatalf("error = %v, want ErrXMLRequestTooLarge", err)
	}
	var maxBytesErr *http.MaxBytesError
	if !errors.As(err, &maxBytesErr) {
		t.Fatalf("error = %T, want *http.MaxBytesError cause", err)
	}
	if maxBytesErr.Limit != 32 {
		t.Fatalf("MaxBytesError.Limit = %d, want 32", maxBytesErr.Limit)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusRequestEntityTooLarge {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusRequestEntityTooLarge)
	}
	if problem.Extensions["code"] != CodeXMLRequestTooLarge {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeXMLRequestTooLarge)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["max_request_bytes"] != int64(32) {
		t.Fatalf("max_request_bytes = %v, want 32", data["max_request_bytes"])
	}
}

func TestDecodeXMLRequestReportsXMLNameMismatch(t *testing.T) {
	req := newXMLBindingRequest(`<customer><name>Ada</name></customer>`, "application/xml")

	var got xmlBindingWidget
	err := DecodeXMLRequest(httptest.NewRecorder(), req, &got)
	if err == nil {
		t.Fatal("DecodeXMLRequest error = nil, want XMLName mismatch")
	}
	if !errors.Is(err, ErrXMLNameMismatch) {
		t.Fatalf("error = %v, want ErrXMLNameMismatch", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusBadRequest {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusBadRequest)
	}
	if problem.Detail != "XML root element mismatch: expected <widget>, got <customer>" {
		t.Fatalf("problem detail = %q, want friendly XMLName mismatch", problem.Detail)
	}
	if problem.Extensions["code"] != CodeXMLNameMismatch {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeXMLNameMismatch)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["expected_element"] != "widget" || data["actual_element"] != "customer" {
		t.Fatalf("problem data = %#v, want expected/actual element names", data)
	}
}

func TestDecodeXMLRequestRejectsTrailingContent(t *testing.T) {
	req := newXMLBindingRequest(`<widget><name>Ada</name></widget><widget><name>Grace</name></widget>`, "application/xml")

	var got xmlBindingWidget
	err := DecodeXMLRequest(httptest.NewRecorder(), req, &got)
	if err == nil {
		t.Fatal("DecodeXMLRequest error = nil, want trailing content error")
	}
	if !errors.Is(err, ErrXMLInvalid) {
		t.Fatalf("error = %v, want ErrXMLInvalid", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusBadRequest {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusBadRequest)
	}
	if problem.Extensions["code"] != CodeXMLInvalid {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeXMLInvalid)
	}
}

func TestEncodeXMLResponseWritesXML(t *testing.T) {
	rec := httptest.NewRecorder()

	err := EncodeXMLResponse(rec, http.StatusCreated, xmlBindingWidget{Name: "Ada"})
	if err != nil {
		t.Fatalf("EncodeXMLResponse error = %v", err)
	}

	if rec.Code != http.StatusCreated {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusCreated)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/xml" {
		t.Fatalf("Content-Type = %q, want application/xml", got)
	}
	if got, want := rec.Body.String(), `<widget><name>Ada</name></widget>`; got != want {
		t.Fatalf("body = %q, want %q", got, want)
	}
}

func TestEncodeXMLResponseErrorConvertsToProblem(t *testing.T) {
	rec := httptest.NewRecorder()

	err := EncodeXMLResponse(rec, http.StatusOK, struct {
		Ch chan int `xml:"ch"`
	}{Ch: make(chan int)})
	if err == nil {
		t.Fatal("EncodeXMLResponse error = nil, want encode failure")
	}
	if !errors.Is(err, ErrXMLEncodeFailed) {
		t.Fatalf("error = %v, want ErrXMLEncodeFailed", err)
	}
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want default recorder status before write", rec.Code)
	}
	if rec.Body.Len() != 0 {
		t.Fatalf("body = %q, want empty body before write", rec.Body.String())
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusInternalServerError {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusInternalServerError)
	}
	if problem.Extensions["code"] != CodeXMLEncodeFailed {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeXMLEncodeFailed)
	}
}

func newXMLBindingRequest(body, contentType string) *http.Request {
	req := httptest.NewRequest(http.MethodPost, "/xml", bytes.NewBufferString(body))
	req.Header.Set("Content-Type", contentType)
	return req
}
