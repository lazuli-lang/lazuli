package storage

import (
	"context"
	"time"
)

// PresignedURLWriter is the optional `ObjectStore` extension for
// adapters that can mint presigned upload URLs. `S3Store` implements
// it via `s3.PresignClient.PresignPutObject`; the local adapter does
// not (browser-direct upload is meaningless against a memory store),
// and callers degrade to `Upload` for those.
type PresignedURLWriter interface {
	SignPut(ctx context.Context, key Key, contentType string, ttl time.Duration) (string, error)
}

// IssueSignedURL asks `store` for a signed download URL valid for
// the contract's `SignedTTL`. The contract must declare
// `Visibility == VisibilitySigned`; otherwise the call returns
// `ErrVisibilityMismatch`.
//
// The doctor diagnostic
// `cap_file_visibility_signed_ttl_mismatch` rejects this combo
// at compile time, so reaching the runtime error path means the
// language contract was bypassed (handcrafted contract value).
//
// Adapter wires:
//   - `S3Store.Sign` — `s3.PresignClient.PresignGetObject`
//   - `LocalStore.Sign` — in-memory TTL token (dev only)
func IssueSignedURL(
	ctx context.Context,
	contract FileContract,
	store ObjectStore,
	key Key,
) (string, error) {
	if contract.Visibility != VisibilitySigned {
		return "", ErrVisibilityMismatch
	}
	if contract.SignedTTL <= 0 {
		return "", ErrVisibilityMismatch
	}
	return store.Sign(ctx, key, contract.SignedTTL)
}

// IssueSignedUploadURL asks `store` for a signed PUT URL valid for
// the contract's `SignedTTL`. Returns `ErrVisibilityMismatch` when
// the contract is not `Signed`, or when the bound store doesn't
// implement `PresignedURLWriter` (e.g. `LocalStore`).
func IssueSignedUploadURL(
	ctx context.Context,
	contract FileContract,
	store ObjectStore,
	key Key,
	contentType string,
) (string, error) {
	if contract.Visibility != VisibilitySigned {
		return "", ErrVisibilityMismatch
	}
	if contract.SignedTTL <= 0 {
		return "", ErrVisibilityMismatch
	}
	writer, ok := store.(PresignedURLWriter)
	if !ok {
		return "", ErrVisibilityMismatch
	}
	return writer.SignPut(ctx, key, contentType, contract.SignedTTL)
}

// IssueSignedURLAt is the test-friendly variant that lets the
// caller pin the issuance instant. The default `IssueSignedURL`
// reads from the adapter's clock; `testing/synctest` flows use
// this hook to align signed URL expiry with the synthetic clock.
func IssueSignedURLAt(
	ctx context.Context,
	contract FileContract,
	store ObjectStore,
	key Key,
	_ time.Time,
) (string, error) {
	// Today the underlying adapters take a `ttl` rather than an
	// absolute deadline. The caller's clock pin happens upstream
	// (e.g. injecting `LocalStore.Clock`). This helper exists so
	// generated code has a stable entry point; later versions
	// can switch to a deadline-based signature without renaming.
	return IssueSignedURL(ctx, contract, store, key)
}
