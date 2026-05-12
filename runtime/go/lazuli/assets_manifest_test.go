package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"testing/fstest"
)

func TestLoadAssetManifestLookup(t *testing.T) {
	manifest, err := LoadAssetManifest(strings.NewReader(`{
		"/app.css": "/assets/app.a1b2c3d4.css",
		"assets/app.js": "assets/app.d4c3b2a1.js"
	}`))
	if err != nil {
		t.Fatalf("LoadAssetManifest returned error: %v", err)
	}

	got, ok := manifest.Lookup("app.css")
	if !ok {
		t.Fatal("Lookup(app.css) did not find asset")
	}
	if got != "assets/app.a1b2c3d4.css" {
		t.Fatalf("Lookup(app.css) = %q, want %q", got, "assets/app.a1b2c3d4.css")
	}

	got, ok = manifest.Lookup("/assets//app.js")
	if !ok {
		t.Fatal("Lookup(/assets//app.js) did not find asset")
	}
	if got != "assets/app.d4c3b2a1.js" {
		t.Fatalf("Lookup(/assets//app.js) = %q, want %q", got, "assets/app.d4c3b2a1.js")
	}
}

func TestNewAssetManifestCopiesEntries(t *testing.T) {
	source := map[string]string{
		"app.css": "assets/app.a1b2c3d4.css",
	}
	manifest, err := NewAssetManifest(source)
	if err != nil {
		t.Fatalf("NewAssetManifest returned error: %v", err)
	}

	source["app.css"] = "assets/app.changed.css"
	entries := manifest.Entries()
	entries["app.css"] = "assets/app.changed.css"

	got, ok := manifest.Lookup("app.css")
	if !ok {
		t.Fatal("Lookup(app.css) did not find asset")
	}
	if got != "assets/app.a1b2c3d4.css" {
		t.Fatalf("Lookup(app.css) = %q, want original target", got)
	}
}

func TestLoadAssetManifestRejectsInvalidJSON(t *testing.T) {
	tests := []struct {
		name  string
		input string
	}{
		{name: "empty", input: ""},
		{name: "non object", input: `[]`},
		{name: "non string value", input: `{"app.css": 12}`},
		{name: "duplicate exact key", input: `{"app.css":"assets/app.a1b2c3d4.css","app.css":"assets/app.d4c3b2a1.css"}`},
		{name: "trailing value", input: `{"app.css":"assets/app.a1b2c3d4.css"} []`},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := LoadAssetManifest(strings.NewReader(tt.input))
			if !errors.Is(err, ErrInvalidAssetManifest) {
				t.Fatalf("LoadAssetManifest error = %v, want ErrInvalidAssetManifest", err)
			}
		})
	}
}

func TestAssetManifestRejectsUnsafePaths(t *testing.T) {
	tests := []struct {
		name    string
		entries map[string]string
	}{
		{
			name: "unsafe logical traversal",
			entries: map[string]string{
				"../app.css": "assets/app.a1b2c3d4.css",
			},
		},
		{
			name: "unsafe target traversal",
			entries: map[string]string{
				"app.css": "../assets/app.a1b2c3d4.css",
			},
		},
		{
			name: "unsafe target backslash",
			entries: map[string]string{
				"app.css": `assets\app.a1b2c3d4.css`,
			},
		},
		{
			name: "target absolute URL",
			entries: map[string]string{
				"app.css": "https://cdn.example/assets/app.a1b2c3d4.css",
			},
		},
		{
			name: "target query string",
			entries: map[string]string{
				"app.css": "assets/app.a1b2c3d4.css?v=1",
			},
		},
		{
			name: "empty logical path",
			entries: map[string]string{
				"/": "assets/app.a1b2c3d4.css",
			},
		},
		{
			name: "normalized duplicate logical paths",
			entries: map[string]string{
				"/app.css": "assets/app.a1b2c3d4.css",
				"app.css":  "assets/app.d4c3b2a1.css",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewAssetManifest(tt.entries)
			if !errors.Is(err, ErrInvalidAssetManifest) {
				t.Fatalf("NewAssetManifest error = %v, want ErrInvalidAssetManifest", err)
			}
		})
	}
}

func TestAssetManifestValidationIsDeterministic(t *testing.T) {
	_, err := NewAssetManifest(map[string]string{
		"z.css": "../assets/z.a1b2c3d4.css",
		"a.css": `assets\a.a1b2c3d4.css`,
	})
	if !errors.Is(err, ErrInvalidAssetManifest) {
		t.Fatalf("NewAssetManifest error = %v, want ErrInvalidAssetManifest", err)
	}
	if !strings.Contains(err.Error(), `asset "a.css" target`) {
		t.Fatalf("NewAssetManifest error = %v, want deterministic first key a.css", err)
	}
}

func TestStaticFilesWithManifestRedirectsLogicalAsset(t *testing.T) {
	manifest, err := NewAssetManifest(map[string]string{
		"app.css": "assets/app.a1b2c3d4.css",
	})
	if err != nil {
		t.Fatalf("NewAssetManifest returned error: %v", err)
	}

	handler := StaticFilesWithManifest(StaticFileConfig{
		FS: fstest.MapFS{
			"assets/app.a1b2c3d4.css": &fstest.MapFile{Data: []byte("body{color:#111}")},
		},
		ImmutableCache: true,
	}, manifest)

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/app.css?v=1", nil))

	if rec.Code != http.StatusTemporaryRedirect {
		t.Fatalf("logical status = %d, want %d", rec.Code, http.StatusTemporaryRedirect)
	}
	if got := rec.Header().Get("Location"); got != "/assets/app.a1b2c3d4.css?v=1" {
		t.Fatalf("Location = %q, want %q", got, "/assets/app.a1b2c3d4.css?v=1")
	}

	rec = httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/assets/app.a1b2c3d4.css", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("fingerprinted status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.String() != "body{color:#111}" {
		t.Fatalf("body = %q", rec.Body.String())
	}
	if got := rec.Header().Get("Cache-Control"); got != immutableStaticCacheControl {
		t.Fatalf("Cache-Control = %q, want %q", got, immutableStaticCacheControl)
	}
}

func TestStaticFilesWithManifestDelegatesUnsupportedMethods(t *testing.T) {
	manifest, err := NewAssetManifest(map[string]string{
		"app.css": "assets/app.a1b2c3d4.css",
	})
	if err != nil {
		t.Fatalf("NewAssetManifest returned error: %v", err)
	}

	handler := StaticFilesWithManifest(StaticFileConfig{
		FS: fstest.MapFS{
			"assets/app.a1b2c3d4.css": &fstest.MapFile{Data: []byte("ok")},
		},
	}, manifest)

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/app.css", nil))

	if rec.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMethodNotAllowed)
	}
	if got := rec.Header().Get("Allow"); got != "GET, HEAD" {
		t.Fatalf("Allow = %q, want %q", got, "GET, HEAD")
	}
}
