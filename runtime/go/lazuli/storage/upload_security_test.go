package storage

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestUploadRejectsOverdeclaredSize verifies the stream-side cap
// catches clients that declare a small size but stream past
// contract.MaxSize. Closes SEC-H7.
func TestUploadRejectsOverdeclaredSize(t *testing.T) {
	t.Parallel()

	contract := FileContract{
		Resource: "Profile",
		Field:    "photo",
		MaxSize:  100,
		Accept:   []MimeType{{Family: "image", Subtype: "jpeg"}},
	}
	meta := Metadata{
		Filename:    "avatar.jpg",
		ContentType: "image/jpeg",
		Size:        50,
	}
	body := &countingReader{r: strings.NewReader(strings.Repeat("a", 1024))}
	store := NewLocalStore("")

	_, err := Upload(t.Context(), contract, store, body, meta)
	if !errors.Is(err, ErrFileSizeExceeded) {
		t.Fatalf("expected ErrFileSizeExceeded, got %v", err)
	}
	if body.read > contract.MaxSize+1 {
		t.Fatalf("upload read %d bytes from source; want at most %d", body.read, contract.MaxSize+1)
	}
	got, err := store.Get(t.Context(), Key("Profile/photo/avatar.jpg"))
	if err == nil {
		got.Close()
		t.Fatal("oversized upload was persisted")
	}
	if !errors.Is(err, ErrFileNotFound) {
		t.Fatalf("expected no stored object after rejection, got %v", err)
	}
}

func TestUploadRejectsPathTraversalFilename(t *testing.T) {
	t.Parallel()

	contract := FileContract{
		Resource: "Profile",
		Field:    "photo",
		MaxSize:  100,
	}
	store := NewLocalStore(t.TempDir())

	for _, name := range []string{"../../etc/passwd", "avatar:1.jpg"} {
		_, err := Upload(t.Context(), contract, store, strings.NewReader("x"), Metadata{
			Filename: name,
			Size:     1,
		})
		if err == nil {
			t.Fatalf("expected unsafe filename %q to be rejected", name)
		}
	}
}

func TestLocalStorePutRejectsEscapedRoot(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	escapeName := filepath.Base(root) + "-escape.txt"
	outside := filepath.Join(filepath.Dir(root), escapeName)
	_ = os.Remove(outside)

	store := NewLocalStore(root)
	err := store.Put(t.Context(), Key("../"+escapeName), strings.NewReader("secret"), "text/plain")
	if err == nil {
		t.Fatal("expected escaped key to be rejected")
	}
	if _, statErr := os.Stat(outside); !errors.Is(statErr, os.ErrNotExist) {
		t.Fatalf("escaped write created %s: %v", outside, statErr)
	}
}

type countingReader struct {
	r    io.Reader
	read int64
}

func (r *countingReader) Read(p []byte) (int, error) {
	n, err := r.r.Read(p)
	r.read += int64(n)
	return n, err
}
