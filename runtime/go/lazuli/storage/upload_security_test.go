package storage

import (
	"errors"
	"io"
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

type countingReader struct {
	r    io.Reader
	read int64
}

func (r *countingReader) Read(p []byte) (int, error) {
	n, err := r.r.Read(p)
	r.read += int64(n)
	return n, err
}
