package openapi

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"testing/fstest"
)

func TestHandlerServesBytesJSON(t *testing.T) {
	handler := Handler(Config{
		Path:         "/openapi.json",
		Bytes:        []byte(`{"openapi":"3.1.0"}`),
		CacheControl: "public, max-age=60",
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/openapi.json", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.String() != `{"openapi":"3.1.0"}` {
		t.Fatalf("body = %q", rec.Body.String())
	}
	if got := rec.Header().Get("Content-Type"); got != ContentTypeJSON {
		t.Fatalf("Content-Type = %q, want %q", got, ContentTypeJSON)
	}
	if got := rec.Header().Get("Cache-Control"); got != "public, max-age=60" {
		t.Fatalf("Cache-Control = %q, want public, max-age=60", got)
	}
	if got := rec.Header().Get("Content-Length"); got != "19" {
		t.Fatalf("Content-Length = %q, want 19", got)
	}
}

func TestHandlerServesFSYAML(t *testing.T) {
	handler := Handler(Config{
		Path: "/spec/openapi.yaml",
		FS: fstest.MapFS{
			"dist/openapi.yaml": &fstest.MapFile{Data: []byte("openapi: 3.1.0\n")},
		},
		File: "dist/openapi.yaml",
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/spec/openapi.yaml", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.String() != "openapi: 3.1.0\n" {
		t.Fatalf("body = %q", rec.Body.String())
	}
	if got := rec.Header().Get("Content-Type"); got != ContentTypeYAML {
		t.Fatalf("Content-Type = %q, want %q", got, ContentTypeYAML)
	}
}

func TestHandlerUsesFileExtensionWhenPathHasNoExtension(t *testing.T) {
	handler := Handler(Config{
		Path: "/openapi",
		FS: fstest.MapFS{
			"openapi.json": &fstest.MapFile{Data: []byte(`{"openapi":"3.1.0"}`)},
		},
		File: "openapi.json",
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/openapi", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Content-Type"); got != ContentTypeJSON {
		t.Fatalf("Content-Type = %q, want %q", got, ContentTypeJSON)
	}
}

func TestHandlerContentTypeOverride(t *testing.T) {
	handler := Handler(Config{
		Path:        "/openapi",
		Bytes:       []byte(`{"openapi":"3.1.0"}`),
		ContentType: "application/vnd.oai.openapi+json",
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/openapi", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/vnd.oai.openapi+json" {
		t.Fatalf("Content-Type = %q, want application/vnd.oai.openapi+json", got)
	}
}

func TestHandlerHEADDoesNotWriteBody(t *testing.T) {
	handler := Handler(Config{
		Path:  "/openapi.yaml",
		Bytes: []byte("openapi: 3.1.0\n"),
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodHead, "/openapi.yaml", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.Len() != 0 {
		t.Fatalf("body length = %d, want 0", rec.Body.Len())
	}
	if got := rec.Header().Get("Content-Length"); got != "15" {
		t.Fatalf("Content-Length = %q, want 15", got)
	}
}

func TestHandlerRejectsUnsupportedMethods(t *testing.T) {
	handler := Handler(Config{
		Path:  "/openapi.yaml",
		Bytes: []byte("openapi: 3.1.0\n"),
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/openapi.yaml", nil))

	if rec.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMethodNotAllowed)
	}
	if got := rec.Header().Get("Allow"); got != "GET, HEAD" {
		t.Fatalf("Allow = %q, want GET, HEAD", got)
	}
}

func TestHandlerNotFound(t *testing.T) {
	handler := Handler(Config{
		Path:  "/openapi.yaml",
		Bytes: []byte("openapi: 3.1.0\n"),
		NotFound: http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			http.Error(w, "missing spec", http.StatusTeapot)
		}),
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/missing.yaml", nil))

	if rec.Code != http.StatusTeapot {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusTeapot)
	}
	if !strings.Contains(rec.Body.String(), "missing spec") {
		t.Fatalf("body = %q, want missing spec", rec.Body.String())
	}
}

func TestHandlerRejectsUnsafeRequestPathsBeforeNotFound(t *testing.T) {
	handler := Handler(Config{
		Path:  "/openapi.yaml",
		Bytes: []byte("openapi: 3.1.0\n"),
		NotFound: http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			w.WriteHeader(http.StatusNotFound)
		}),
	})

	for _, target := range []string{"/../openapi.yaml", "/%2e%2e/openapi.yaml", "/openapi\\yaml"} {
		rec := httptest.NewRecorder()
		handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, target, nil))

		if rec.Code != http.StatusNotFound {
			t.Fatalf("%s status = %d, want %d", target, rec.Code, http.StatusNotFound)
		}
		if rec.Body.String() == "openapi: 3.1.0\n" {
			t.Fatalf("%s served artifact for unsafe path", target)
		}
	}
}

func TestHandlerUIRedirectPlaceholder(t *testing.T) {
	handler := Handler(Config{
		Path:   "/openapi.yaml",
		UIPath: "/docs",
		Bytes:  []byte("openapi: 3.1.0\n"),
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/docs", nil))

	if rec.Code != http.StatusTemporaryRedirect {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusTemporaryRedirect)
	}
	if got := rec.Header().Get("Location"); got != "/openapi.yaml" {
		t.Fatalf("Location = %q, want /openapi.yaml", got)
	}
}

func TestServeMuxMountsArtifactAndUIPaths(t *testing.T) {
	mux := ServeMux(Config{
		Path:   "/openapi.yaml",
		UIPath: "/docs",
		Bytes:  []byte("openapi: 3.1.0\n"),
	})

	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/openapi.yaml", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("artifact status = %d, want %d", rec.Code, http.StatusOK)
	}

	rec = httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/docs", nil))

	if rec.Code != http.StatusTemporaryRedirect {
		t.Fatalf("ui status = %d, want %d", rec.Code, http.StatusTemporaryRedirect)
	}

	rec = httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/missing", nil))

	if rec.Code != http.StatusNotFound {
		t.Fatalf("missing status = %d, want %d", rec.Code, http.StatusNotFound)
	}
}

func TestServeMuxUsesCustomNotFound(t *testing.T) {
	mux := ServeMux(Config{
		Path:  "/openapi.yaml",
		Bytes: []byte("openapi: 3.1.0\n"),
		NotFound: http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			http.Error(w, "missing spec", http.StatusTeapot)
		}),
	})

	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/missing", nil))

	if rec.Code != http.StatusTeapot {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusTeapot)
	}
	if !strings.Contains(rec.Body.String(), "missing spec") {
		t.Fatalf("body = %q, want missing spec", rec.Body.String())
	}
}

func TestHandlerPanicsForInvalidConfig(t *testing.T) {
	tests := []struct {
		name   string
		config Config
	}{
		{
			name:   "missing source",
			config: Config{},
		},
		{
			name: "both sources",
			config: Config{
				FS: fstest.MapFS{
					"openapi.yaml": &fstest.MapFile{Data: []byte("openapi: 3.1.0\n")},
				},
				File:  "openapi.yaml",
				Bytes: []byte("openapi: 3.1.0\n"),
			},
		},
		{
			name: "unsafe path",
			config: Config{
				Path:  "../openapi.yaml",
				Bytes: []byte("openapi: 3.1.0\n"),
			},
		},
		{
			name: "root path",
			config: Config{
				Path:  "/",
				Bytes: []byte("openapi: 3.1.0\n"),
			},
		},
		{
			name: "serve mux pattern path",
			config: Config{
				Path:  "/openapi/{name}.yaml",
				Bytes: []byte("openapi: 3.1.0\n"),
			},
		},
		{
			name: "unsafe fs path",
			config: Config{
				FS: fstest.MapFS{
					"openapi.yaml": &fstest.MapFile{Data: []byte("openapi: 3.1.0\n")},
				},
				File: "../openapi.yaml",
			},
		},
		{
			name: "missing fs file",
			config: Config{
				FS:   fstest.MapFS{},
				File: "openapi.yaml",
			},
		},
		{
			name: "duplicate ui path",
			config: Config{
				Path:   "/openapi.yaml",
				UIPath: "/openapi.yaml",
				Bytes:  []byte("openapi: 3.1.0\n"),
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			defer func() {
				if recover() == nil {
					t.Fatal("Handler did not panic")
				}
			}()

			_ = Handler(tt.config)
		})
	}
}
