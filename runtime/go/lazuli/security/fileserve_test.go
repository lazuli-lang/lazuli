package security

import (
	"errors"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"testing"
)

func TestCleanFileServePathCleansRelativePaths(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		path string
		want string
	}{
		{name: "plain", path: "assets/app.css", want: "assets/app.css"},
		{name: "leading slash", path: "/assets/app.css", want: "assets/app.css"},
		{name: "duplicate slash and dot", path: "/assets//./app.css", want: "assets/app.css"},
		{name: "relative dot prefix", path: "./images/logo.png", want: "images/logo.png"},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, err := CleanFileServePath(tt.path, FileServeOptions{})
			if err != nil {
				t.Fatalf("CleanFileServePath() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("CleanFileServePath() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestCleanFileServePathRejectsUnsafePaths(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		path string
	}{
		{name: "empty", path: ""},
		{name: "root", path: "/"},
		{name: "parent", path: "../secret.txt"},
		{name: "nested parent", path: "assets/../secret.txt"},
		{name: "escaped parent after slash", path: "/../secret.txt"},
		{name: "backslash", path: `assets\secret.txt`},
		{name: "drive path", path: "C:/Windows/win.ini"},
		{name: "scheme", path: "https://example.test/app.css"},
		{name: "query", path: "assets/app.css?v=1"},
		{name: "fragment", path: "assets/app.css#hash"},
		{name: "nul", path: "assets/\x00secret.txt"},
		{name: "control", path: "assets/\nsecret.txt"},
		{name: "double leading slash", path: "//assets/app.css"},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			_, err := CleanFileServePath(tt.path, FileServeOptions{Dotfiles: AllowDotfiles})
			if !errors.Is(err, ErrInvalidFileServePath) {
				t.Fatalf("CleanFileServePath() error = %v, want ErrInvalidFileServePath", err)
			}
		})
	}
}

func TestCleanFileServePathAppliesExtensionAllowlist(t *testing.T) {
	t.Parallel()

	options := FileServeOptions{
		AllowedExtensions: []string{".CSS", ".js"},
	}

	if got, err := CleanFileServePath("assets/app.css", options); err != nil || got != "assets/app.css" {
		t.Fatalf("CleanFileServePath(css) = %q, %v; want assets/app.css, nil", got, err)
	}
	if got, err := CleanFileServePath("assets/app.JS", options); err != nil || got != "assets/app.JS" {
		t.Fatalf("CleanFileServePath(JS) = %q, %v; want assets/app.JS, nil", got, err)
	}

	for _, name := range []string{"assets/app.png", "README"} {
		name := name
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			_, err := CleanFileServePath(name, options)
			if !errors.Is(err, ErrFileServeExtensionDenied) {
				t.Fatalf("CleanFileServePath() error = %v, want ErrFileServeExtensionDenied", err)
			}
		})
	}
}

func TestCleanFileServePathRejectsInvalidExtensionAllowlist(t *testing.T) {
	t.Parallel()

	tests := []string{"", ".", "css", ".tar.gz", ".c/ss", ".c:ss"}
	for _, ext := range tests {
		ext := ext
		t.Run(ext, func(t *testing.T) {
			t.Parallel()

			_, err := CleanFileServePath("assets/app.css", FileServeOptions{
				AllowedExtensions: []string{ext},
			})
			if !errors.Is(err, ErrInvalidFileServeOptions) {
				t.Fatalf("CleanFileServePath() error = %v, want ErrInvalidFileServeOptions", err)
			}
		})
	}
}

func TestCleanFileServePathAppliesDotfilePolicy(t *testing.T) {
	t.Parallel()

	for _, name := range []string{".env", "assets/.manifest.json", ".git/config"} {
		name := name
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			_, err := CleanFileServePath(name, FileServeOptions{})
			if !errors.Is(err, ErrFileServeDotfileDenied) {
				t.Fatalf("CleanFileServePath() error = %v, want ErrFileServeDotfileDenied", err)
			}
		})
	}

	got, err := CleanFileServePath(".well-known/assetlinks.json", FileServeOptions{
		AllowedExtensions: []string{".json"},
		Dotfiles:          AllowDotfiles,
	})
	if err != nil {
		t.Fatalf("CleanFileServePath() error = %v", err)
	}
	if got != ".well-known/assetlinks.json" {
		t.Fatalf("CleanFileServePath() = %q, want .well-known/assetlinks.json", got)
	}
}

func TestOpenFileFromRootCleansAndOpensFile(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "public"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "public", "app.txt"), []byte("ok"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "secret.txt"), []byte("secret"), 0o644); err != nil {
		t.Fatal(err)
	}

	file, cleaned, err := OpenFileFromRoot(root, "/public//./app.txt", FileServeOptions{
		AllowedExtensions: []string{".txt"},
	})
	if err != nil {
		t.Fatalf("OpenFileFromRoot() error = %v", err)
	}
	defer file.Close()

	if cleaned != "public/app.txt" {
		t.Fatalf("cleaned path = %q, want public/app.txt", cleaned)
	}
	body, err := io.ReadAll(file)
	if err != nil {
		t.Fatal(err)
	}
	if string(body) != "ok" {
		t.Fatalf("body = %q, want ok", body)
	}

	_, _, err = OpenFileFromRoot(root, "public/../secret.txt", FileServeOptions{
		AllowedExtensions: []string{".txt"},
	})
	if !errors.Is(err, ErrInvalidFileServePath) {
		t.Fatalf("OpenFileFromRoot(traversal) error = %v, want ErrInvalidFileServePath", err)
	}
}

func TestOpenFileFromRootReportsMissingAndDirectory(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "public"), 0o755); err != nil {
		t.Fatal(err)
	}

	_, cleaned, err := OpenFileFromRoot(root, "public/missing.txt", FileServeOptions{})
	if !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("OpenFileFromRoot(missing) error = %v, want fs.ErrNotExist", err)
	}
	if cleaned != "public/missing.txt" {
		t.Fatalf("missing cleaned path = %q, want public/missing.txt", cleaned)
	}

	_, cleaned, err = OpenFileFromRoot(root, "public", FileServeOptions{})
	if !errors.Is(err, fs.ErrNotExist) {
		t.Fatalf("OpenFileFromRoot(directory) error = %v, want fs.ErrNotExist", err)
	}
	if cleaned != "public" {
		t.Fatalf("directory cleaned path = %q, want public", cleaned)
	}
}

func TestOpenFileFromRootRequiresRoot(t *testing.T) {
	t.Parallel()

	_, _, err := OpenFileFromRoot("", "app.txt", FileServeOptions{})
	if !errors.Is(err, ErrFileServeRootRequired) {
		t.Fatalf("OpenFileFromRoot() error = %v, want ErrFileServeRootRequired", err)
	}
}
