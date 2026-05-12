package lazuli

import (
	"bytes"
	"errors"
	"io"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestParseMultipartRequestNormalUpload(t *testing.T) {
	req := newMultipartTestRequest(t,
		map[string]string{"title": "profile"},
		[]multipartTestFile{{
			Field:    "avatar",
			Filename: "avatar.txt",
			Body:     []byte("avatar-bytes"),
		}},
	)

	form, err := ParseMultipartRequest(httptest.NewRecorder(), req, MultipartLimits{
		MaxMemory:       1024,
		MaxRequestBytes: req.ContentLength + 1024,
		MaxFiles:        1,
	})
	if err != nil {
		t.Fatalf("ParseMultipartRequest error = %v", err)
	}
	defer form.RemoveAll()

	if got := CountMultipartFiles(form); got != 1 {
		t.Fatalf("CountMultipartFiles = %d, want 1", got)
	}

	title, err := RequiredMultipartValue(form, "title")
	if err != nil {
		t.Fatalf("RequiredMultipartValue error = %v", err)
	}
	if title != "profile" {
		t.Fatalf("title = %q, want profile", title)
	}

	header, err := RequiredMultipartFile(form, "avatar")
	if err != nil {
		t.Fatalf("RequiredMultipartFile error = %v", err)
	}
	if header.Filename != "avatar.txt" {
		t.Fatalf("Filename = %q, want avatar.txt", header.Filename)
	}

	file, err := header.Open()
	if err != nil {
		t.Fatalf("Open uploaded file error = %v", err)
	}
	defer file.Close()

	body, err := io.ReadAll(file)
	if err != nil {
		t.Fatalf("Read uploaded file error = %v", err)
	}
	if string(body) != "avatar-bytes" {
		t.Fatalf("uploaded file body = %q, want avatar-bytes", body)
	}
}

func TestParseMultipartRequestRejectsOversize(t *testing.T) {
	req := newMultipartTestRequest(t, nil, []multipartTestFile{{
		Field:    "avatar",
		Filename: "avatar.txt",
		Body:     bytes.Repeat([]byte("x"), 256),
	}})
	req.ContentLength = -1

	_, err := ParseMultipartRequest(httptest.NewRecorder(), req, MultipartLimits{
		MaxMemory:       32,
		MaxRequestBytes: 64,
		MaxFiles:        1,
	})
	if err == nil {
		t.Fatal("ParseMultipartRequest error = nil, want oversize error")
	}
	if !errors.Is(err, ErrMultipartRequestTooLarge) {
		t.Fatalf("error = %v, want ErrMultipartRequestTooLarge", err)
	}

	var maxBytesErr *http.MaxBytesError
	if !errors.As(err, &maxBytesErr) {
		t.Fatalf("error = %T, want *http.MaxBytesError cause", err)
	}
	if maxBytesErr.Limit != 64 {
		t.Fatalf("MaxBytesError.Limit = %d, want 64", maxBytesErr.Limit)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusRequestEntityTooLarge {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusRequestEntityTooLarge)
	}
	if problem.Extensions["code"] != CodeMultipartRequestTooLarge {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeMultipartRequestTooLarge)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["max_request_bytes"] != int64(64) {
		t.Fatalf("max_request_bytes = %v, want 64", data["max_request_bytes"])
	}
}

func TestRequiredMultipartFileReportsMissingField(t *testing.T) {
	req := newMultipartTestRequest(t, map[string]string{"title": "profile"}, nil)

	form, err := ParseMultipartRequest(httptest.NewRecorder(), req, MultipartLimits{
		MaxMemory:       1024,
		MaxRequestBytes: req.ContentLength + 1024,
		MaxFiles:        1,
	})
	if err != nil {
		t.Fatalf("ParseMultipartRequest error = %v", err)
	}
	defer form.RemoveAll()

	_, err = RequiredMultipartFile(form, "avatar")
	if err == nil {
		t.Fatal("RequiredMultipartFile error = nil, want missing field error")
	}
	if !errors.Is(err, ErrMultipartFieldMissing) {
		t.Fatalf("error = %v, want ErrMultipartFieldMissing", err)
	}

	var multipartErr *MultipartError
	if !errors.As(err, &multipartErr) {
		t.Fatalf("error = %T, want *MultipartError", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusBadRequest {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusBadRequest)
	}
	if problem.Extensions["code"] != CodeMultipartFieldMissing {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeMultipartFieldMissing)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["field"] != "avatar" {
		t.Fatalf("field = %v, want avatar", data["field"])
	}
}

func TestParseMultipartRequestRejectsTooManyFiles(t *testing.T) {
	req := newMultipartTestRequest(t, nil, []multipartTestFile{
		{Field: "avatar", Filename: "avatar.txt", Body: []byte("avatar")},
		{Field: "cover", Filename: "cover.txt", Body: []byte("cover")},
	})

	_, err := ParseMultipartRequest(httptest.NewRecorder(), req, MultipartLimits{
		MaxMemory:       1024,
		MaxRequestBytes: req.ContentLength + 1024,
		MaxFiles:        1,
	})
	if err == nil {
		t.Fatal("ParseMultipartRequest error = nil, want too many files error")
	}
	if !errors.Is(err, ErrMultipartTooManyFiles) {
		t.Fatalf("error = %v, want ErrMultipartTooManyFiles", err)
	}

	problem := ProblemFromError(err)
	if problem.Status != http.StatusBadRequest {
		t.Fatalf("problem status = %d, want %d", problem.Status, http.StatusBadRequest)
	}
	if problem.Extensions["code"] != CodeMultipartTooManyFiles {
		t.Fatalf("problem code = %v, want %s", problem.Extensions["code"], CodeMultipartTooManyFiles)
	}
	data, ok := problem.Extensions["data"].(map[string]any)
	if !ok {
		t.Fatalf("problem data = %#v, want map", problem.Extensions["data"])
	}
	if data["max_files"] != 1 {
		t.Fatalf("max_files = %v, want 1", data["max_files"])
	}
	if data["file_count"] != 2 {
		t.Fatalf("file_count = %v, want 2", data["file_count"])
	}
}

type multipartTestFile struct {
	Field    string
	Filename string
	Body     []byte
}

func newMultipartTestRequest(t *testing.T, fields map[string]string, files []multipartTestFile) *http.Request {
	t.Helper()

	var body bytes.Buffer
	writer := multipart.NewWriter(&body)
	for field, value := range fields {
		if err := writer.WriteField(field, value); err != nil {
			t.Fatalf("WriteField error = %v", err)
		}
	}
	for _, file := range files {
		part, err := writer.CreateFormFile(file.Field, file.Filename)
		if err != nil {
			t.Fatalf("CreateFormFile error = %v", err)
		}
		if _, err := part.Write(file.Body); err != nil {
			t.Fatalf("write file part error = %v", err)
		}
	}
	if err := writer.Close(); err != nil {
		t.Fatalf("multipart writer close error = %v", err)
	}

	req := httptest.NewRequest(http.MethodPost, "/upload", &body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	return req
}
