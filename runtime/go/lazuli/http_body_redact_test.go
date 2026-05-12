package lazuli

import (
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestRedactedBodyPreviewRedactsDefaultJSONFieldsAndRestoresBody(t *testing.T) {
	body := `{"username":"ada","password":"p4ss","profile":{"token":"abc"},"events":[{"secret":"hidden"},{"public":"ok"}]}`
	req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body))

	preview, truncated, err := RedactedBodyPreview(req, nil)
	if err != nil {
		t.Fatalf("RedactedBodyPreview error = %v", err)
	}
	if truncated {
		t.Fatal("truncated = true, want false")
	}

	var got map[string]any
	if err := json.Unmarshal(preview, &got); err != nil {
		t.Fatalf("preview JSON decode error: %v; preview = %q", err, preview)
	}
	if got["username"] != "ada" {
		t.Fatalf("username = %v, want ada", got["username"])
	}
	if got["password"] != redactedBodyValue {
		t.Fatalf("password = %v, want redacted", got["password"])
	}
	profile := got["profile"].(map[string]any)
	if profile["token"] != redactedBodyValue {
		t.Fatalf("profile.token = %v, want redacted", profile["token"])
	}
	events := got["events"].([]any)
	firstEvent := events[0].(map[string]any)
	if firstEvent["secret"] != redactedBodyValue {
		t.Fatalf("events[0].secret = %v, want redacted", firstEvent["secret"])
	}
	if strings.Contains(string(preview), "p4ss") || strings.Contains(string(preview), "abc") || strings.Contains(string(preview), "hidden") {
		t.Fatalf("preview leaked sensitive value: %s", preview)
	}

	restored := readAllString(t, req.Body)
	if restored != body {
		t.Fatalf("restored body = %q, want %q", restored, body)
	}
}

func TestRedactedBodyPreviewUsesConfiguredFields(t *testing.T) {
	body := `{"apiKey":"k123","password":"kept"}`
	req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body))

	preview, truncated, err := RedactedBodyPreview(req, &BodyRedactionConfig{
		MaxBytes:     128,
		RedactFields: []string{"apiKey"},
	})
	if err != nil {
		t.Fatalf("RedactedBodyPreview error = %v", err)
	}
	if truncated {
		t.Fatal("truncated = true, want false")
	}

	if !strings.Contains(string(preview), `"apiKey":"[REDACTED]"`) {
		t.Fatalf("preview did not redact configured apiKey field: %s", preview)
	}
	if !strings.Contains(string(preview), `"password":"kept"`) {
		t.Fatalf("preview redacted unconfigured password field: %s", preview)
	}
	if restored := readAllString(t, req.Body); restored != body {
		t.Fatalf("restored body = %q, want %q", restored, body)
	}
}

func TestRedactedBodyPreviewTruncatesLargeBodiesAndRestoresBody(t *testing.T) {
	body := "abcdefghijklmnopqrstuvwxyz"
	req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body))

	preview, truncated, err := RedactedBodyPreview(req, &BodyRedactionConfig{MaxBytes: 5})
	if err != nil {
		t.Fatalf("RedactedBodyPreview error = %v", err)
	}
	if !truncated {
		t.Fatal("truncated = false, want true")
	}
	if string(preview) != "abcde" {
		t.Fatalf("preview = %q, want abcde", preview)
	}
	if restored := readAllString(t, req.Body); restored != body {
		t.Fatalf("restored body = %q, want %q", restored, body)
	}
}

func TestRedactedBodyPreviewBestEffortRedactsInvalidJSON(t *testing.T) {
	body := `{"password":"p4ss","token":"abc",`
	req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body))

	preview, truncated, err := RedactedBodyPreview(req, &BodyRedactionConfig{MaxBytes: 128})
	if err != nil {
		t.Fatalf("RedactedBodyPreview error = %v", err)
	}
	if truncated {
		t.Fatal("truncated = true, want false")
	}
	if strings.Contains(string(preview), "p4ss") || strings.Contains(string(preview), "abc") {
		t.Fatalf("preview leaked sensitive value: %s", preview)
	}
	if !strings.Contains(string(preview), `"password":"[REDACTED]"`) || !strings.Contains(string(preview), `"token":"[REDACTED]"`) {
		t.Fatalf("preview did not redact invalid JSON fields: %s", preview)
	}
	if restored := readAllString(t, req.Body); restored != body {
		t.Fatalf("restored body = %q, want %q", restored, body)
	}
}

func TestRedactedBodyPreviewReturnsNilForNilBody(t *testing.T) {
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Body = nil

	preview, truncated, err := RedactedBodyPreview(req, nil)
	if err != nil {
		t.Fatalf("RedactedBodyPreview error = %v", err)
	}
	if preview != nil {
		t.Fatalf("preview = %q, want nil", preview)
	}
	if truncated {
		t.Fatal("truncated = true, want false")
	}
}

func TestRedactedBodyPreviewRestoresBodyAfterReadError(t *testing.T) {
	readErr := errors.New("read failed")
	req := httptest.NewRequest(http.MethodPost, "/", nil)
	req.Body = &failingReadCloser{
		data: []byte("partial"),
		err:  readErr,
	}

	preview, truncated, err := RedactedBodyPreview(req, &BodyRedactionConfig{MaxBytes: 64})
	if !errors.Is(err, readErr) {
		t.Fatalf("RedactedBodyPreview error = %v, want %v", err, readErr)
	}
	if preview != nil {
		t.Fatalf("preview = %q, want nil", preview)
	}
	if truncated {
		t.Fatal("truncated = true, want false")
	}
	if restored := readAllStringAllowingError(req.Body); restored != "partial" {
		t.Fatalf("restored body prefix = %q, want partial", restored)
	}
}

type failingReadCloser struct {
	data []byte
	err  error
}

func (f *failingReadCloser) Read(p []byte) (int, error) {
	if len(f.data) == 0 {
		return 0, f.err
	}
	n := copy(p, f.data)
	f.data = f.data[n:]
	return n, f.err
}

func (f *failingReadCloser) Close() error {
	return nil
}

func readAllString(t *testing.T, r io.Reader) string {
	t.Helper()
	buf, err := io.ReadAll(r)
	if err != nil {
		t.Fatalf("ReadAll error = %v", err)
	}
	return string(buf)
}

func readAllStringAllowingError(r io.Reader) string {
	buf, _ := io.ReadAll(r)
	return string(buf)
}
