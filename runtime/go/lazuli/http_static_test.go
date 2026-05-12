package lazuli

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"testing/fstest"
)

func TestStaticFilesServesFS(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{
			"assets/app.css": &fstest.MapFile{Data: []byte("body{color:#111}")},
		},
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/assets/app.css", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.String() != "body{color:#111}" {
		t.Fatalf("body = %q", rec.Body.String())
	}
	if got := rec.Header().Get("Content-Type"); got != "text/css; charset=utf-8" {
		t.Fatalf("Content-Type = %q", got)
	}
}

func TestStaticFilesServesHTTPFileSystem(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "app.js"), []byte("console.log('ok')"), 0o644); err != nil {
		t.Fatal(err)
	}

	handler := StaticFiles(StaticFileConfig{FileSystem: http.Dir(root)})
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/app.js", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.String() != "console.log('ok')" {
		t.Fatalf("body = %q", rec.Body.String())
	}
}

func TestStaticFilesCleansSafePaths(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{
			"assets/app.css": &fstest.MapFile{Data: []byte("ok")},
		},
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/assets//./app.css", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.String() != "ok" {
		t.Fatalf("body = %q", rec.Body.String())
	}
}

func TestStaticFilesRejectsTraversalBeforeFallback(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{
			"index.html": &fstest.MapFile{Data: []byte("index")},
			"secret.txt": &fstest.MapFile{Data: []byte("secret")},
		},
		IndexFallback: "index.html",
	})

	for _, target := range []string{"/../secret.txt", "/%2e%2e/secret.txt", "/assets\\secret.txt"} {
		rec := httptest.NewRecorder()
		handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, target, nil))

		if rec.Code != http.StatusNotFound {
			t.Fatalf("%s status = %d, want %d", target, rec.Code, http.StatusNotFound)
		}
		if rec.Body.String() == "index" || rec.Body.String() == "secret" {
			t.Fatalf("%s served unsafe body %q", target, rec.Body.String())
		}
	}
}

func TestStaticFilesIndexFallback(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{
			"index.html": &fstest.MapFile{Data: []byte("<main></main>")},
		},
		IndexFallback: "index.html",
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/dashboard/settings", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.String() != "<main></main>" {
		t.Fatalf("body = %q", rec.Body.String())
	}
}

func TestStaticFilesNotFoundHandler(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{},
		NotFound: http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			http.Error(w, "missing asset", http.StatusTeapot)
		}),
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/missing.txt", nil))

	if rec.Code != http.StatusTeapot {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusTeapot)
	}
}

func TestStaticFilesDoesNotListDirectories(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{
			"assets/app.js": &fstest.MapFile{Data: []byte("ok")},
		},
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/assets", nil))

	if rec.Code != http.StatusNotFound {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNotFound)
	}
	if rec.Body.String() == "ok" {
		t.Fatal("served directory child for directory request")
	}
}

func TestStaticFilesImmutableCacheForFingerprintedAssets(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{
			"assets/app.a1b2c3d4.js": &fstest.MapFile{Data: []byte("hash")},
			"assets/app.js":          &fstest.MapFile{Data: []byte("plain")},
		},
		ImmutableCache: true,
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/assets/app.a1b2c3d4.js", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Cache-Control"); got != immutableStaticCacheControl {
		t.Fatalf("fingerprinted Cache-Control = %q, want %q", got, immutableStaticCacheControl)
	}

	rec = httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/assets/app.js", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Cache-Control"); got != "" {
		t.Fatalf("plain Cache-Control = %q, want empty", got)
	}
}

func TestStaticFilesHEADDoesNotWriteBody(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{
			"app.js": &fstest.MapFile{Data: []byte("console.log('ok')")},
		},
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodHead, "/app.js", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.Len() != 0 {
		t.Fatalf("body length = %d, want 0", rec.Body.Len())
	}
}

func TestStaticFilesRejectsUnsupportedMethods(t *testing.T) {
	handler := StaticFiles(StaticFileConfig{
		FS: fstest.MapFS{
			"app.js": &fstest.MapFile{Data: []byte("ok")},
		},
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/app.js", nil))

	if rec.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMethodNotAllowed)
	}
	if got := rec.Header().Get("Allow"); got != "GET, HEAD" {
		t.Fatalf("Allow = %q, want %q", got, "GET, HEAD")
	}
}

func TestStaticFilesPanicsWithoutFileSystem(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Fatal("StaticFiles did not panic")
		}
	}()

	_ = StaticFiles(StaticFileConfig{})
}

func TestStaticFilesPanicsWithUnsafeIndexFallback(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Fatal("StaticFiles did not panic")
		}
	}()

	_ = StaticFiles(StaticFileConfig{
		FS:            fstest.MapFS{},
		IndexFallback: "../index.html",
	})
}
