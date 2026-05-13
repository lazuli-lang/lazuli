package lazuli

import (
	"errors"
	"net/http/httptest"
	"strings"
	"testing"
)

const staticAssetTestDigest = "d53333ec4d6aa21b09c10e37b22c507c2ac8b7c558194d9888a05e95c1d98727"

func TestStaticAssetFingerprintManifestLookupAndEntries(t *testing.T) {
	source := []StaticAssetFingerprintEntry{
		{
			LogicalName: "/z.js",
			Path:        "/assets//z.d4c3b2a1.js",
		},
		{
			LogicalName: "/app.css",
			Path:        "/assets//app.a1b2c3d4.css",
			Digest:      "SHA256:" + strings.ToUpper(staticAssetTestDigest),
		},
	}

	manifest, err := NewStaticAssetFingerprintManifest(source)
	if err != nil {
		t.Fatalf("NewStaticAssetFingerprintManifest returned error: %v", err)
	}
	source[1].Path = "assets/app.changed.css"

	entry, ok := manifest.Lookup("app.css")
	if !ok {
		t.Fatal("Lookup(app.css) did not find entry")
	}
	if entry.LogicalName != "app.css" {
		t.Fatalf("entry.LogicalName = %q, want app.css", entry.LogicalName)
	}
	if entry.Path != "assets/app.a1b2c3d4.css" {
		t.Fatalf("entry.Path = %q, want assets/app.a1b2c3d4.css", entry.Path)
	}
	if entry.Digest != StaticAssetDigestSHA256Prefix+staticAssetTestDigest {
		t.Fatalf("entry.Digest = %q, want canonical sha256 digest", entry.Digest)
	}

	entries := manifest.Entries()
	if len(entries) != 2 {
		t.Fatalf("Entries length = %d, want 2", len(entries))
	}
	if entries[0].LogicalName != "app.css" || entries[1].LogicalName != "z.js" {
		t.Fatalf("Entries sorted logical names = %q, %q; want app.css, z.js", entries[0].LogicalName, entries[1].LogicalName)
	}
	entries[0].Path = "assets/app.mutated.css"

	entry, ok = manifest.Lookup("/app.css")
	if !ok {
		t.Fatal("Lookup(/app.css) did not find entry")
	}
	if entry.Path != "assets/app.a1b2c3d4.css" {
		t.Fatalf("entry.Path after Entries mutation = %q, want original", entry.Path)
	}
}

func TestStaticAssetFingerprintManifestResolveKnownSafePaths(t *testing.T) {
	manifest, err := NewStaticAssetFingerprintManifest([]StaticAssetFingerprintEntry{
		{
			LogicalName: "app.css",
			Path:        "assets/app.a1b2c3d4.css",
		},
	})
	if err != nil {
		t.Fatalf("NewStaticAssetFingerprintManifest returned error: %v", err)
	}

	for _, name := range []string{"app.css", "/assets//app.a1b2c3d4.css"} {
		entry, ok := manifest.Resolve(name)
		if !ok {
			t.Fatalf("Resolve(%q) did not find entry", name)
		}
		if entry.Path != "assets/app.a1b2c3d4.css" {
			t.Fatalf("Resolve(%q).Path = %q, want generated path", name, entry.Path)
		}

		path, ok := manifest.ResolvePath(name)
		if !ok {
			t.Fatalf("ResolvePath(%q) did not find path", name)
		}
		if path != "assets/app.a1b2c3d4.css" {
			t.Fatalf("ResolvePath(%q) = %q, want generated path", name, path)
		}
	}

	for _, name := range []string{"../secret.txt", "/%2e%2e/secret.txt", "assets/unknown.a1b2c3d4.css"} {
		if _, ok := manifest.Resolve(name); ok {
			t.Fatalf("Resolve(%q) ok = true, want false", name)
		}
		if _, ok := manifest.ResolvePath(name); ok {
			t.Fatalf("ResolvePath(%q) ok = true, want false", name)
		}
	}
}

func TestStaticAssetFingerprintManifestAssetPathAndCacheMetadata(t *testing.T) {
	manifest, err := NewStaticAssetFingerprintManifest([]StaticAssetFingerprintEntry{
		{
			LogicalName: "app.css",
			Path:        "assets/app.a1b2c3d4.css",
			Digest:      StaticAssetDigestSHA256Prefix + staticAssetTestDigest,
		},
	})
	if err != nil {
		t.Fatalf("NewStaticAssetFingerprintManifest returned error: %v", err)
	}

	path, ok := manifest.AssetPath("/app.css")
	if !ok {
		t.Fatal("AssetPath(/app.css) did not find path")
	}
	if path != "assets/app.a1b2c3d4.css" {
		t.Fatalf("AssetPath(/app.css) = %q, want generated path", path)
	}

	metadata, ok := manifest.CacheMetadata("/assets/app.a1b2c3d4.css")
	if !ok {
		t.Fatal("CacheMetadata(fingerprinted path) did not find metadata")
	}
	if !metadata.Immutable {
		t.Fatal("metadata.Immutable = false, want true")
	}
	if metadata.CacheControl != immutableStaticCacheControl {
		t.Fatalf("metadata.CacheControl = %q, want %q", metadata.CacheControl, immutableStaticCacheControl)
	}
	if metadata.ETag != `"`+StaticAssetDigestSHA256Prefix+staticAssetTestDigest+`"` {
		t.Fatalf("metadata.ETag = %q, want digest ETag", metadata.ETag)
	}

	rec := httptest.NewRecorder()
	metadata.Headers().Apply(rec)
	if got := rec.Header().Get("Cache-Control"); got != immutableStaticCacheControl {
		t.Fatalf("Cache-Control = %q, want %q", got, immutableStaticCacheControl)
	}
	if got := rec.Header().Get("ETag"); got != metadata.ETag {
		t.Fatalf("ETag = %q, want %q", got, metadata.ETag)
	}
}

func TestStaticAssetFingerprintManifestFromAssetManifest(t *testing.T) {
	assets, err := NewAssetManifest(map[string]string{
		"app.css": "assets/app.a1b2c3d4.css",
	})
	if err != nil {
		t.Fatalf("NewAssetManifest returned error: %v", err)
	}

	manifest, err := NewStaticAssetFingerprintManifestFromAssetManifest(assets)
	if err != nil {
		t.Fatalf("NewStaticAssetFingerprintManifestFromAssetManifest returned error: %v", err)
	}

	path, ok := manifest.AssetPath("app.css")
	if !ok {
		t.Fatal("AssetPath(app.css) did not find path")
	}
	if path != "assets/app.a1b2c3d4.css" {
		t.Fatalf("AssetPath(app.css) = %q, want assets/app.a1b2c3d4.css", path)
	}
}

func TestStaticAssetDigestHelpers(t *testing.T) {
	if got := StaticAssetSHA256Digest([]byte("hello lazuli\n")); got != StaticAssetDigestSHA256Prefix+staticAssetTestDigest {
		t.Fatalf("StaticAssetSHA256Digest = %q, want sha256 digest", got)
	}

	got, err := StaticAssetSHA256DigestFromReader(strings.NewReader("hello lazuli\n"))
	if err != nil {
		t.Fatalf("StaticAssetSHA256DigestFromReader returned error: %v", err)
	}
	if got != StaticAssetDigestSHA256Prefix+staticAssetTestDigest {
		t.Fatalf("StaticAssetSHA256DigestFromReader = %q, want sha256 digest", got)
	}

	if got := StaticAssetDigestETag("SHA256:" + strings.ToUpper(staticAssetTestDigest)); got != `"`+StaticAssetDigestSHA256Prefix+staticAssetTestDigest+`"` {
		t.Fatalf("StaticAssetDigestETag = %q, want digest ETag", got)
	}
	if got := StaticAssetDigestETag("sha1:" + staticAssetTestDigest); got != "" {
		t.Fatalf("StaticAssetDigestETag(invalid) = %q, want empty", got)
	}

	_, err = StaticAssetSHA256DigestFromReader(nil)
	if !errors.Is(err, ErrInvalidAssetManifest) {
		t.Fatalf("StaticAssetSHA256DigestFromReader(nil) error = %v, want ErrInvalidAssetManifest", err)
	}
}

func TestStaticAssetFingerprintEntryUsesPathETagFallback(t *testing.T) {
	entry := StaticAssetFingerprintEntry{
		LogicalName: "app.css",
		Path:        "assets/app.a1b2c3d4.css",
	}

	if got := entry.ETag(); got != `"asset:assets/app.a1b2c3d4.css"` {
		t.Fatalf("entry.ETag() = %q, want path ETag fallback", got)
	}

	metadata := entry.CacheMetadata()
	if metadata.ETag != `"asset:assets/app.a1b2c3d4.css"` {
		t.Fatalf("entry.CacheMetadata().ETag = %q, want path ETag fallback", metadata.ETag)
	}

	plain := StaticAssetFingerprintEntry{Path: "assets/app.css"}
	if metadata := plain.CacheMetadata(); metadata.Immutable {
		t.Fatalf("plain CacheMetadata().Immutable = true, want false")
	}
}

func TestStaticAssetFingerprintManifestRejectsInvalidEntries(t *testing.T) {
	tests := []struct {
		name    string
		entries []StaticAssetFingerprintEntry
	}{
		{
			name: "unsafe logical path",
			entries: []StaticAssetFingerprintEntry{
				{LogicalName: "../app.css", Path: "assets/app.a1b2c3d4.css"},
			},
		},
		{
			name: "unsafe generated path",
			entries: []StaticAssetFingerprintEntry{
				{LogicalName: "app.css", Path: `assets\app.a1b2c3d4.css`},
			},
		},
		{
			name: "generated path not fingerprinted",
			entries: []StaticAssetFingerprintEntry{
				{LogicalName: "app.css", Path: "assets/app.css"},
			},
		},
		{
			name: "invalid digest",
			entries: []StaticAssetFingerprintEntry{
				{LogicalName: "app.css", Path: "assets/app.a1b2c3d4.css", Digest: "sha256:not-hex"},
			},
		},
		{
			name: "duplicate logical path after normalization",
			entries: []StaticAssetFingerprintEntry{
				{LogicalName: "/app.css", Path: "assets/app.a1b2c3d4.css"},
				{LogicalName: "app.css", Path: "assets/app.d4c3b2a1.css"},
			},
		},
		{
			name: "duplicate generated path after normalization",
			entries: []StaticAssetFingerprintEntry{
				{LogicalName: "app.css", Path: "/assets/app.a1b2c3d4.css"},
				{LogicalName: "legacy.css", Path: "assets//app.a1b2c3d4.css"},
			},
		},
		{
			name: "logical path conflicts with another generated path",
			entries: []StaticAssetFingerprintEntry{
				{LogicalName: "app.css", Path: "assets/app.a1b2c3d4.css"},
				{LogicalName: "assets/app.a1b2c3d4.css", Path: "assets/other.d4c3b2a1.css"},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewStaticAssetFingerprintManifest(tt.entries)
			if !errors.Is(err, ErrInvalidAssetManifest) {
				t.Fatalf("NewStaticAssetFingerprintManifest error = %v, want ErrInvalidAssetManifest", err)
			}
		})
	}
}
