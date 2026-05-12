package storage_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/storage"
)

const dedupTestDigest = "d53333ec4d6aa21b09c10e37b22c507c2ac8b7c558194d9888a05e95c1d98727"

func TestCalculateContentHashReturnsSHA256AndSize(t *testing.T) {
	t.Parallel()

	got, err := storage.CalculateContentHash(strings.NewReader("hello lazuli\n"))
	if err != nil {
		t.Fatalf("CalculateContentHash() error = %v", err)
	}

	if got.Hash != storage.ContentHashSHA256Prefix+dedupTestDigest {
		t.Fatalf("CalculateContentHash().Hash = %q, want sha256 digest", got.Hash)
	}
	if got.Size != int64(len("hello lazuli\n")) {
		t.Fatalf("CalculateContentHash().Size = %d, want %d", got.Size, len("hello lazuli\n"))
	}
}

func TestCalculateContentHashRejectsNilReader(t *testing.T) {
	t.Parallel()

	_, err := storage.CalculateContentHash(nil)
	if !errors.Is(err, storage.ErrDedupMetadataInvalid) {
		t.Fatalf("CalculateContentHash(nil) error = %v, want ErrDedupMetadataInvalid", err)
	}
}

func TestDedupKeyUsesContractScopeAndCanonicalDigest(t *testing.T) {
	t.Parallel()

	key, err := storage.DedupKey(
		storage.Private("ImportBatch", "file", 1024, storage.TextMime("csv")),
		storage.ContentHashSHA256Prefix+strings.ToUpper(dedupTestDigest),
	)
	if err != nil {
		t.Fatalf("DedupKey() error = %v", err)
	}

	want := storage.Key("dedup/resource/ImportBatch/file/sha256/" + dedupTestDigest)
	if key != want {
		t.Fatalf("DedupKey() = %q, want %q", key, want)
	}
}

func TestDedupKeyUsesAPIScope(t *testing.T) {
	t.Parallel()

	key, err := storage.DedupKey(
		storage.FileContract{API: "customer_export"},
		storage.ContentHashSHA256Prefix+dedupTestDigest,
	)
	if err != nil {
		t.Fatalf("DedupKey() error = %v", err)
	}

	want := storage.Key("dedup/api/customer_export/sha256/" + dedupTestDigest)
	if key != want {
		t.Fatalf("DedupKey() = %q, want %q", key, want)
	}
}

func TestDedupKeyRejectsInvalidHashAndUnsafeScope(t *testing.T) {
	t.Parallel()

	_, err := storage.DedupKey(storage.FileContract{API: "customer_export"}, "sha1:"+dedupTestDigest)
	if !errors.Is(err, storage.ErrDedupMetadataInvalid) {
		t.Fatalf("DedupKey() invalid hash error = %v, want ErrDedupMetadataInvalid", err)
	}

	_, err = storage.DedupKey(storage.FileContract{Resource: "ImportBatch", Field: "../file"}, storage.ContentHashSHA256Prefix+dedupTestDigest)
	if !errors.Is(err, storage.ErrDedupMetadataInvalid) {
		t.Fatalf("DedupKey() unsafe scope error = %v, want ErrDedupMetadataInvalid", err)
	}
}

func TestValidateReuseMetadataAcceptsExistingDedupObject(t *testing.T) {
	t.Parallel()

	contract := storage.Private("ImportBatch", "file", 20, storage.TextMime("csv"))
	key, err := storage.DedupKey(contract, storage.ContentHashSHA256Prefix+dedupTestDigest)
	if err != nil {
		t.Fatalf("DedupKey() error = %v", err)
	}
	metadata := storage.ReuseMetadata{
		Key:         key,
		ContentHash: storage.ContentHashSHA256Prefix + dedupTestDigest,
		ContentType: "text/csv;charset=utf-8",
		Size:        int64(len("hello lazuli\n")),
	}

	if err := storage.ValidateReuseMetadata(contract, metadata); err != nil {
		t.Fatalf("ValidateReuseMetadata() error = %v", err)
	}

	ref := metadata.FileRef()
	if ref.Key != metadata.Key || ref.ContentType != metadata.ContentType || ref.Size != metadata.Size {
		t.Fatalf("ReuseMetadata.FileRef() = %#v, want metadata-backed ref", ref)
	}
}

func TestValidateReuseMetadataRejectsMismatchedKey(t *testing.T) {
	t.Parallel()

	err := storage.ValidateReuseMetadata(storage.FileContract{API: "customer_export"}, storage.ReuseMetadata{
		Key:         storage.Key("dedup/api/customer_export/sha256/not-the-hash"),
		ContentHash: storage.ContentHashSHA256Prefix + dedupTestDigest,
		Size:        12,
	})
	if !errors.Is(err, storage.ErrDedupMetadataInvalid) {
		t.Fatalf("ValidateReuseMetadata() error = %v, want ErrDedupMetadataInvalid", err)
	}
}

func TestValidateReuseMetadataReusesUploadMetadataValidation(t *testing.T) {
	t.Parallel()

	contract := storage.Private("ImportBatch", "file", 5, storage.TextMime("csv"))
	key, err := storage.DedupKey(contract, storage.ContentHashSHA256Prefix+dedupTestDigest)
	if err != nil {
		t.Fatalf("DedupKey() error = %v", err)
	}

	oversized := storage.ReuseMetadata{
		Key:         key,
		ContentHash: storage.ContentHashSHA256Prefix + dedupTestDigest,
		ContentType: "text/csv",
		Size:        12,
	}
	if err := storage.ValidateReuseMetadata(contract, oversized); !errors.Is(err, storage.ErrFileSizeExceeded) {
		t.Fatalf("ValidateReuseMetadata() oversized error = %v, want ErrFileSizeExceeded", err)
	}

	rejectedMIME := oversized
	rejectedMIME.Size = 4
	rejectedMIME.ContentType = "application/pdf"
	if err := storage.ValidateReuseMetadata(contract, rejectedMIME); !errors.Is(err, storage.ErrFileMimeRejected) {
		t.Fatalf("ValidateReuseMetadata() MIME error = %v, want ErrFileMimeRejected", err)
	}
}
