package security

import (
	"errors"
	"path/filepath"
	"testing"
)

func TestNormalizeSlashPathCleansSafeRelativePaths(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		want string
	}{
		{name: "", want: ""},
		{name: ".", want: ""},
		{name: "./assets//app.css", want: "assets/app.css"},
		{name: `uploads\avatars\me.png`, want: "uploads/avatars/me.png"},
		{name: "uploads/./avatars/me.png", want: "uploads/avatars/me.png"},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			got, err := NormalizeSlashPath(test.name)
			if err != nil {
				t.Fatalf("NormalizeSlashPath() error = %v", err)
			}
			if got != test.want {
				t.Fatalf("NormalizeSlashPath() = %q, want %q", got, test.want)
			}
		})
	}
}

func TestNormalizeSlashPathRejectsTraversalShapes(t *testing.T) {
	t.Parallel()
	tests := []string{
		"..",
		"../secret.txt",
		"public/../secret.txt",
		"public/..",
		"/etc/passwd",
		`C:\Windows\win.ini`,
		"C:/Windows/win.ini",
		`C:Windows\win.ini`,
		`\\server\share\file.txt`,
		"safe\x00file.txt",
	}
	for _, name := range tests {
		name := name
		t.Run(name, func(t *testing.T) {
			t.Parallel()
			_, err := NormalizeSlashPath(name)
			if !errors.Is(err, ErrPathTraversalRejected) {
				t.Fatalf("NormalizeSlashPath() error = %v, want ErrPathTraversalRejected", err)
			}
		})
	}
}

func TestJoinUnderRootReturnsAbsolutePathUnderRoot(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	absoluteRoot, err := filepath.Abs(root)
	if err != nil {
		t.Fatalf("filepath.Abs() error = %v", err)
	}

	got, err := JoinUnderRoot(root, `uploads\avatars\me.png`)
	if err != nil {
		t.Fatalf("JoinUnderRoot() error = %v", err)
	}
	want := filepath.Join(absoluteRoot, "uploads", "avatars", "me.png")
	if got != want {
		t.Fatalf("JoinUnderRoot() = %q, want %q", got, want)
	}
}

func TestJoinUnderRootAllowsEmptyPathAtRoot(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	absoluteRoot, err := filepath.Abs(root)
	if err != nil {
		t.Fatalf("filepath.Abs() error = %v", err)
	}

	got, err := JoinUnderRoot(root, ".")
	if err != nil {
		t.Fatalf("JoinUnderRoot() error = %v", err)
	}
	if got != absoluteRoot {
		t.Fatalf("JoinUnderRoot() = %q, want %q", got, absoluteRoot)
	}
}

func TestJoinUnderRootRejectsUnsafePaths(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	tests := []string{
		"..",
		"../secret.txt",
		`..\secret.txt`,
		"/etc/passwd",
		`C:\Windows\win.ini`,
		"C:/Windows/win.ini",
		`C:Windows\win.ini`,
		`\\server\share\file.txt`,
		"safe\x00file.txt",
	}
	for _, name := range tests {
		name := name
		t.Run(name, func(t *testing.T) {
			t.Parallel()
			_, err := JoinUnderRoot(root, name)
			if !errors.Is(err, ErrPathTraversalRejected) {
				t.Fatalf("JoinUnderRoot() error = %v, want ErrPathTraversalRejected", err)
			}
		})
	}
}

func TestJoinUnderRootRejectsUnsafeRoot(t *testing.T) {
	t.Parallel()
	for _, root := range []string{"", "safe\x00root"} {
		root := root
		t.Run(root, func(t *testing.T) {
			t.Parallel()
			_, err := JoinUnderRoot(root, "file.txt")
			if !errors.Is(err, ErrPathTraversalRejected) {
				t.Fatalf("JoinUnderRoot() error = %v, want ErrPathTraversalRejected", err)
			}
		})
	}
}
