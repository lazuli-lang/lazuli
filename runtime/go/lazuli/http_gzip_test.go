package lazuli

import (
	"bytes"
	"compress/gzip"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestGzipMiddlewareCompressesAcceptedResponse(t *testing.T) {
	handler := GzipMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Length", "7")
		w.Header().Set("Content-Type", "text/plain")
		w.Header().Set("Vary", "Accept-Language")
		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte("created"))
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("Accept-Encoding", "br, gzip")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusCreated {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusCreated)
	}
	if got := rec.Header().Get("Content-Encoding"); got != "gzip" {
		t.Fatalf("Content-Encoding = %q, want gzip", got)
	}
	if got := rec.Header().Get("Content-Length"); got != "" {
		t.Fatalf("Content-Length = %q, want empty", got)
	}
	if got := rec.Header().Get("Vary"); got != "Accept-Language, Accept-Encoding" {
		t.Fatalf("Vary = %q, want %q", got, "Accept-Language, Accept-Encoding")
	}
	if got := gunzipString(t, rec.Body.Bytes()); got != "created" {
		t.Fatalf("body = %q, want created", got)
	}
}

func TestGzipMiddlewareSkipsWhenRequestDoesNotAcceptGzip(t *testing.T) {
	handler := GzipMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/plain")
		_, _ = w.Write([]byte("plain"))
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("Accept-Encoding", "br")

	handler.ServeHTTP(rec, req)

	if got := rec.Header().Get("Content-Encoding"); got != "" {
		t.Fatalf("Content-Encoding = %q, want empty", got)
	}
	if got := rec.Header().Get("Vary"); got != "Accept-Encoding" {
		t.Fatalf("Vary = %q, want Accept-Encoding", got)
	}
	if got := rec.Body.String(); got != "plain" {
		t.Fatalf("body = %q, want plain", got)
	}
}

func TestGzipMiddlewareSkipsExistingContentEncoding(t *testing.T) {
	handler := GzipMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Encoding", "br")
		w.Header().Set("Vary", "Accept-Language")
		_, _ = w.Write([]byte("encoded elsewhere"))
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("Accept-Encoding", "gzip")

	handler.ServeHTTP(rec, req)

	if got := rec.Header().Get("Content-Encoding"); got != "br" {
		t.Fatalf("Content-Encoding = %q, want br", got)
	}
	if got := rec.Header().Get("Vary"); got != "Accept-Language" {
		t.Fatalf("Vary = %q, want Accept-Language", got)
	}
	if got := rec.Body.String(); got != "encoded elsewhere" {
		t.Fatalf("body = %q, want encoded elsewhere", got)
	}
}

func TestGzipMiddlewareSkipsNoTransform(t *testing.T) {
	handler := GzipMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Cache-Control", "private, no-transform")
		_, _ = w.Write([]byte("plain"))
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("Accept-Encoding", "gzip")

	handler.ServeHTTP(rec, req)

	if got := rec.Header().Get("Content-Encoding"); got != "" {
		t.Fatalf("Content-Encoding = %q, want empty", got)
	}
	if got := rec.Header().Get("Vary"); got != "" {
		t.Fatalf("Vary = %q, want empty", got)
	}
	if got := rec.Body.String(); got != "plain" {
		t.Fatalf("body = %q, want plain", got)
	}
}

func TestGzipMiddlewareSkipsStatusCodesWithoutBodies(t *testing.T) {
	for _, status := range []int{http.StatusNoContent, http.StatusNotModified, http.StatusSwitchingProtocols} {
		t.Run(http.StatusText(status), func(t *testing.T) {
			handler := GzipMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				w.WriteHeader(status)
				_, _ = w.Write([]byte("must not compress"))
			}))
			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodGet, "/", nil)
			req.Header.Set("Accept-Encoding", "gzip")

			handler.ServeHTTP(rec, req)

			if rec.Code != status {
				t.Fatalf("status = %d, want %d", rec.Code, status)
			}
			if got := rec.Header().Get("Content-Encoding"); got != "" {
				t.Fatalf("Content-Encoding = %q, want empty", got)
			}
		})
	}
}

func TestGzipMiddlewareDoesNotDuplicateVary(t *testing.T) {
	handler := GzipMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Vary", "accept-encoding, Accept-Language")
		_, _ = w.Write([]byte("body"))
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("Accept-Encoding", "gzip")

	handler.ServeHTTP(rec, req)

	if got := rec.Header().Get("Vary"); got != "accept-encoding, Accept-Language" {
		t.Fatalf("Vary = %q, want original value", got)
	}
}

func TestGzipMiddlewareRespectsGzipQualityZero(t *testing.T) {
	handler := GzipMiddleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte("plain"))
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set("Accept-Encoding", "gzip;q=0, br")

	handler.ServeHTTP(rec, req)

	if got := rec.Header().Get("Content-Encoding"); got != "" {
		t.Fatalf("Content-Encoding = %q, want empty", got)
	}
	if got := rec.Body.String(); got != "plain" {
		t.Fatalf("body = %q, want plain", got)
	}
}

func gunzipString(t *testing.T, body []byte) string {
	t.Helper()

	zr, err := gzip.NewReader(bytes.NewReader(body))
	if err != nil {
		t.Fatalf("gzip.NewReader error = %v", err)
	}
	defer zr.Close()

	out, err := io.ReadAll(zr)
	if err != nil {
		t.Fatalf("gzip read error = %v", err)
	}
	return string(out)
}
