package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestETagFormatsRawValue(t *testing.T) {
	if got := ETag("asset-v1"); got != `"asset-v1"` {
		t.Fatalf("ETag = %q, want %q", got, `"asset-v1"`)
	}
}

func TestETagPreservesExistingTags(t *testing.T) {
	for _, value := range []string{`"asset-v1"`, `W/"asset-v1"`} {
		if got := ETag(value); got != value {
			t.Fatalf("ETag(%q) = %q, want unchanged", value, got)
		}
	}
}

func TestCacheHeadersMiddlewareAppliesHeaders(t *testing.T) {
	handler := CacheHeadersMiddleware(CacheHeaders{
		CacheControl: "public, max-age=60",
		ETag:         "asset-v1",
		Vary:         []string{"Accept-Encoding"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusAccepted)
		_, _ = w.Write([]byte("accepted"))
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	header := rec.Result().Header
	if got := header.Get("Cache-Control"); got != "public, max-age=60" {
		t.Fatalf("Cache-Control = %q, want public, max-age=60", got)
	}
	if got := header.Get("ETag"); got != `"asset-v1"` {
		t.Fatalf("ETag = %q, want %q", got, `"asset-v1"`)
	}
	if got := header.Get("Vary"); got != "Accept-Encoding" {
		t.Fatalf("Vary = %q, want Accept-Encoding", got)
	}
	if got := rec.Body.String(); got != "accepted" {
		t.Fatalf("body = %q, want accepted", got)
	}
}

func TestCacheHeadersMiddlewareWritesNotModifiedForMatchingGet(t *testing.T) {
	called := false
	handler := CacheHeadersMiddleware(CacheHeaders{
		CacheControl: "public, max-age=60",
		ETag:         "asset-v1",
	})(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("If-None-Match", `"other", W/"asset-v1"`)

	handler.ServeHTTP(rec, req)

	if called {
		t.Fatal("handler was called for matching If-None-Match")
	}
	if rec.Code != http.StatusNotModified {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNotModified)
	}
	header := rec.Result().Header
	if got := header.Get("Cache-Control"); got != "public, max-age=60" {
		t.Fatalf("Cache-Control = %q, want public, max-age=60", got)
	}
	if got := header.Get("ETag"); got != `"asset-v1"` {
		t.Fatalf("ETag = %q, want %q", got, `"asset-v1"`)
	}
	if got := rec.Body.String(); got != "" {
		t.Fatalf("body = %q, want empty", got)
	}
}

func TestWriteNotModifiedIfMatchMatchesHead(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodHead, "/", nil)
	req.Header.Set("If-None-Match", `"asset-v1"`)

	if !WriteNotModifiedIfMatch(rec, req, "asset-v1") {
		t.Fatal("WriteNotModifiedIfMatch = false, want true")
	}
	if rec.Code != http.StatusNotModified {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNotModified)
	}
	if got := rec.Body.String(); got != "" {
		t.Fatalf("body = %q, want empty", got)
	}
}

func TestWriteNotModifiedIfMatchIgnoresUnsafeMethods(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/", nil)
	req.Header.Set("If-None-Match", `"asset-v1"`)

	if WriteNotModifiedIfMatch(rec, req, "asset-v1") {
		t.Fatal("WriteNotModifiedIfMatch = true, want false")
	}
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want unwritten recorder status %d", rec.Code, http.StatusOK)
	}
}

func TestWriteNotModifiedIfMatchMatchesWildcard(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("If-None-Match", "*")

	if !WriteNotModifiedIfMatch(rec, req, "asset-v1") {
		t.Fatal("WriteNotModifiedIfMatch = false, want true")
	}
	if rec.Code != http.StatusNotModified {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNotModified)
	}
}

func TestWriteNotModifiedIfMatchMiss(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("If-None-Match", `"other"`)

	if WriteNotModifiedIfMatch(rec, req, "asset-v1") {
		t.Fatal("WriteNotModifiedIfMatch = true, want false")
	}
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want unwritten recorder status %d", rec.Code, http.StatusOK)
	}
}
