package storage

import (
	"context"
	"io"
)

// AuthCheck is the runtime hook that decides whether the caller
// may fetch the underlying file. The generated download endpoint
// passes the same policy + actor + tenant context that the
// command originally enforced when accepting the upload, then
// invokes `FetchPrivate`.
//
// Returning a non-nil error short-circuits the fetch and propagates
// the typed error to the transport — `lazuli/policy_denied` for
// permission failures, `lazuli/not_found` when the caller's
// tenant boundary excludes the object.
type AuthCheck func(ctx context.Context, contract FileContract, key Key) error

// FetchPrivate resolves a `Private`-visibility file through a
// policy-gated download path. The contract must declare
// `Visibility == VisibilityPrivate`; otherwise the call returns
// `ErrVisibilityMismatch`.
//
// `check` is invoked before the adapter is asked for bytes, so a
// policy denial never reveals the underlying storage shape.
func FetchPrivate(
	ctx context.Context,
	contract FileContract,
	store ObjectStore,
	key Key,
	check AuthCheck,
) (io.ReadCloser, error) {
	if contract.Visibility != VisibilityPrivate {
		return nil, ErrVisibilityMismatch
	}
	if check != nil {
		if err := check(ctx, contract, key); err != nil {
			return nil, err
		}
	}
	return store.Get(ctx, key)
}
