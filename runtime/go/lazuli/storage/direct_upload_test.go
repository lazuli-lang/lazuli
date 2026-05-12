package storage_test

import (
	"context"
	"errors"
	"io"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

type directUploadStore struct {
	key         storage.Key
	contentType string
	ttl         time.Duration
}

func (s *directUploadStore) Put(context.Context, storage.Key, io.Reader, string) error {
	return errors.New("Put should not be called for direct upload tickets")
}

func (s *directUploadStore) Get(context.Context, storage.Key) (io.ReadCloser, error) {
	return io.NopCloser(strings.NewReader("")), nil
}

func (s *directUploadStore) Sign(context.Context, storage.Key, time.Duration) (string, error) {
	return "", errors.New("Sign should not be called for direct upload tickets")
}

func (s *directUploadStore) Delete(context.Context, storage.Key) error {
	return nil
}

func (s *directUploadStore) SignPut(_ context.Context, key storage.Key, contentType string, ttl time.Duration) (string, error) {
	s.key = key
	s.contentType = contentType
	s.ttl = ttl
	return "https://uploads.test/" + string(key), nil
}

func TestIssueDirectUploadReturnsTicket(t *testing.T) {
	t.Parallel()

	contract := storage.Public("Profile", "avatar", 5<<20, storage.ImageAny())
	store := &directUploadStore{}
	var signer storage.ObjectStore = store

	ticket, err := storage.IssueDirectUpload(context.Background(), contract, signer, storage.DirectUploadRequest{
		Filename:    "me.png",
		ContentType: "image/png",
		Size:        4096,
	}, 2*time.Minute)
	if err != nil {
		t.Fatalf("IssueDirectUpload failed: %v", err)
	}

	if ticket.Key != storage.Key("Profile/avatar/me.png") {
		t.Fatalf("Key = %q, want Profile/avatar/me.png", ticket.Key)
	}
	if ticket.UploadURL != "https://uploads.test/Profile/avatar/me.png" {
		t.Fatalf("UploadURL = %q", ticket.UploadURL)
	}
	if ticket.Headers["Content-Type"] != "image/png" {
		t.Fatalf("Content-Type header = %q, want image/png", ticket.Headers["Content-Type"])
	}
	if len(ticket.Headers) != 1 {
		t.Fatalf("Headers = %v, want only Content-Type", ticket.Headers)
	}

	if store.key != ticket.Key {
		t.Fatalf("SignPut key = %q, want %q", store.key, ticket.Key)
	}
	if store.contentType != "image/png" {
		t.Fatalf("SignPut contentType = %q, want image/png", store.contentType)
	}
	if store.ttl != 2*time.Minute {
		t.Fatalf("SignPut ttl = %v, want 2m", store.ttl)
	}
}

func TestIssueDirectUploadValidatesMetadata(t *testing.T) {
	t.Parallel()

	store := &directUploadStore{}
	contract := storage.Private("ImportBatch", "file", 1024, storage.TextMime("csv"))

	_, err := storage.IssueDirectUpload(context.Background(), contract, store, storage.DirectUploadRequest{
		Filename:    "people.csv",
		ContentType: "text/csv",
		Size:        2048,
	}, time.Minute)
	if !errors.Is(err, storage.ErrFileSizeExceeded) {
		t.Fatalf("oversized request: want ErrFileSizeExceeded, got %v", err)
	}

	_, err = storage.IssueDirectUpload(context.Background(), contract, store, storage.DirectUploadRequest{
		Filename:    "people.pdf",
		ContentType: "application/pdf",
		Size:        512,
	}, time.Minute)
	if !errors.Is(err, storage.ErrFileMimeRejected) {
		t.Fatalf("unaccepted MIME: want ErrFileMimeRejected, got %v", err)
	}
}

func TestIssueDirectUploadValidatesVisibilityAndTTL(t *testing.T) {
	t.Parallel()

	store := &directUploadStore{}
	metadata := storage.DirectUploadRequest{
		Filename:    "avatar.png",
		ContentType: "image/png",
		Size:        512,
	}

	_, err := storage.IssueDirectUpload(context.Background(), storage.Public("Profile", "avatar", 1024, storage.ImageAny()), store, metadata, 0)
	if !errors.Is(err, storage.ErrVisibilityMismatch) {
		t.Fatalf("ttl=0: want ErrVisibilityMismatch, got %v", err)
	}

	contract := storage.FileContract{
		Resource:   "Profile",
		Field:      "avatar",
		MaxSize:    1024,
		Accept:     []storage.MimeType{storage.ImageAny()},
		Visibility: storage.VisibilitySigned,
	}
	_, err = storage.IssueDirectUpload(context.Background(), contract, store, metadata, time.Minute)
	if !errors.Is(err, storage.ErrVisibilityMismatch) {
		t.Fatalf("signed visibility without SignedTTL: want ErrVisibilityMismatch, got %v", err)
	}
}

func TestIssueDirectUploadRequiresSignPut(t *testing.T) {
	t.Parallel()

	contract := storage.Public("Profile", "avatar", 1024, storage.ImageAny())
	store := storage.NewLocalStore("")

	_, err := storage.IssueDirectUpload(context.Background(), contract, store, storage.DirectUploadRequest{
		Filename:    "avatar.png",
		ContentType: "image/png",
		Size:        512,
	}, time.Minute)
	if !errors.Is(err, storage.ErrVisibilityMismatch) {
		t.Fatalf("store without SignPut: want ErrVisibilityMismatch, got %v", err)
	}
}
