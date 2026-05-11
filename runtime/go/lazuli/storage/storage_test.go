package storage_test

import (
	"context"
	"errors"
	"io"
	"strings"
	"testing"
	"testing/synctest"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestLocalStoreRoundTripUploadAndPrivateFetch(t *testing.T) {
	t.Parallel()

	contract := storage.FileContract{
		Resource:   "ImportBatch",
		Field:      "file",
		MaxSize:    25 * 1024 * 1024,
		Accept:     []storage.MimeType{{Family: "text", Subtype: "csv"}},
		Visibility: storage.VisibilityPrivate,
	}

	body := strings.NewReader("id,name\n1,alice\n2,bob\n")
	store := storage.NewLocalStore("")

	key, err := storage.Upload(context.Background(), contract, store, body, storage.Metadata{
		Filename:    "people.csv",
		ContentType: "text/csv",
		Size:        20,
	})
	if err != nil {
		t.Fatalf("Upload failed: %v", err)
	}
	if key == "" {
		t.Fatal("Upload returned an empty key")
	}

	reader, err := storage.FetchPrivate(context.Background(), contract, store, key, nil)
	if err != nil {
		t.Fatalf("FetchPrivate failed: %v", err)
	}
	defer reader.Close()
	got, err := io.ReadAll(reader)
	if err != nil {
		t.Fatalf("ReadAll failed: %v", err)
	}
	if string(got) != "id,name\n1,alice\n2,bob\n" {
		t.Fatalf("FetchPrivate returned unexpected bytes: %q", got)
	}
}

func TestUploadRejectsOversizedBody(t *testing.T) {
	t.Parallel()

	contract := storage.FileContract{
		MaxSize: 1024,
		Accept:  []storage.MimeType{{Family: "text", Subtype: "csv"}},
	}
	store := storage.NewLocalStore("")
	_, err := storage.Upload(context.Background(), contract, store, strings.NewReader(""), storage.Metadata{
		ContentType: "text/csv",
		Size:        2048,
	})
	if !errors.Is(err, storage.ErrFileSizeExceeded) {
		t.Fatalf("expected ErrFileSizeExceeded, got %v", err)
	}
}

func TestUploadRejectsUnacceptedMime(t *testing.T) {
	t.Parallel()

	contract := storage.FileContract{
		MaxSize: 1024,
		Accept:  []storage.MimeType{{Family: "text", Subtype: "csv"}},
	}
	store := storage.NewLocalStore("")
	_, err := storage.Upload(context.Background(), contract, store, strings.NewReader(""), storage.Metadata{
		ContentType: "application/pdf",
		Size:        100,
	})
	if !errors.Is(err, storage.ErrFileMimeRejected) {
		t.Fatalf("expected ErrFileMimeRejected, got %v", err)
	}
}

func TestFetchPrivateRejectsWrongVisibility(t *testing.T) {
	t.Parallel()

	contract := storage.FileContract{
		Visibility: storage.VisibilitySigned,
	}
	store := storage.NewLocalStore("")
	_, err := storage.FetchPrivate(context.Background(), contract, store, storage.Key("nope"), nil)
	if !errors.Is(err, storage.ErrVisibilityMismatch) {
		t.Fatalf("expected ErrVisibilityMismatch, got %v", err)
	}
}

func TestIssueSignedURLRequiresSignedVisibility(t *testing.T) {
	t.Parallel()

	contract := storage.FileContract{
		Visibility: storage.VisibilityPrivate,
		SignedTTL:  time.Hour,
	}
	store := storage.NewLocalStore("")
	_, err := storage.IssueSignedURL(context.Background(), contract, store, storage.Key("nope"))
	if !errors.Is(err, storage.ErrVisibilityMismatch) {
		t.Fatalf("expected ErrVisibilityMismatch, got %v", err)
	}
}

// TestSignedURLTTLExpiry exercises the round-trip described in
// `docs/proposals/bucket-storage-cycle.md` §Evals/Testes via
// `testing/synctest` (stable in Go 1.25+). The synthetic clock
// advances past the contract TTL and the resolve call surfaces
// `ErrSignedURLExpired`.
func TestSignedURLTTLExpiry(t *testing.T) {
	synctest.Test(t, func(t *testing.T) {
		now := time.Now()
		clock := func() time.Time { return now }

		contract := storage.FileContract{
			API:        "customer_export",
			MaxSize:    100 * 1024 * 1024,
			Accept:     []storage.MimeType{{Family: "text", Subtype: "csv"}},
			Visibility: storage.VisibilitySigned,
			SignedTTL:  1 * time.Hour,
		}
		store := storage.NewLocalStore("")
		store.Clock = func() time.Time { return clock() }

		key, err := storage.Upload(context.Background(), contract, store, strings.NewReader("csv,bytes\n"), storage.Metadata{
			Filename:    "export.csv",
			ContentType: "text/csv",
			Size:        10,
		})
		if err != nil {
			t.Fatalf("Upload failed: %v", err)
		}

		token, err := storage.IssueSignedURL(context.Background(), contract, store, key)
		if err != nil {
			t.Fatalf("IssueSignedURL failed: %v", err)
		}

		// Within TTL: resolve succeeds.
		got, err := store.Resolve(token)
		if err != nil {
			t.Fatalf("Resolve within TTL failed: %v", err)
		}
		if got != key {
			t.Fatalf("Resolve returned wrong key: %q (want %q)", got, key)
		}

		// Advance synthetic clock past TTL: resolve fails.
		now = now.Add(2 * time.Hour)
		_, err = store.Resolve(token)
		if !errors.Is(err, storage.ErrSignedURLExpired) {
			t.Fatalf("expected ErrSignedURLExpired after TTL, got %v", err)
		}
	})
}

func TestMimeMatchesWildcards(t *testing.T) {
	t.Parallel()

	cases := []struct {
		left, right storage.MimeType
		want        bool
	}{
		{storage.MimeType{Family: "text", Subtype: "csv"}, storage.MimeType{Family: "text", Subtype: "csv"}, true},
		{storage.MimeType{Family: "text", Subtype: "csv"}, storage.MimeType{Family: "text", Subtype: "*"}, true},
		{storage.MimeType{Family: "text", Subtype: "csv"}, storage.MimeType{Family: "*", Subtype: "*"}, true},
		{storage.MimeType{Family: "text", Subtype: "csv"}, storage.MimeType{Family: "application", Subtype: "json"}, false},
	}
	for _, tc := range cases {
		if got := tc.left.Matches(tc.right); got != tc.want {
			t.Fatalf("%s.Matches(%s) = %v (want %v)", tc.left, tc.right, got, tc.want)
		}
	}
}
