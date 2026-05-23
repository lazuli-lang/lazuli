// Package secrets is the runtime-level Provider seam for the @lazuli/plugin-
// secret-vault binding. The framework reads secrets through a single
// pluggable interface; concrete backend adapters live in
// @lazuli/plugin-secret-vault repos.
//
// Boundary discipline: this file never names a concrete provider. The
// `Provider` interface is implemented by adapter packages selected at
// boot via registry.lzi bindings + Lazurite.toml plugin map.
package secrets

import (
	"context"
	"errors"
	"os"
	"time"
)

// Provider is the vendor-neutral secret-resolution contract. The
// generated runtime calls Get(ctx, key) wherever .lzi declares a
// Secret-typed env reference; adapters fetch from their backend.
//
// Implementations MUST be safe for concurrent use.
type Provider interface {
	// Get returns the secret value for the supplied key. Returns
	// ErrSecretNotFound when the key isn't known to the provider.
	Get(ctx context.Context, key string) (string, error)

	// GetWithLease fetches a secret with explicit TTL renewal
	// semantics. Adapters that don't support leasing return the
	// secret with LeasedSecret.RenewAt == time.Time{} and treat
	// Renew() as a no-op.
	GetWithLease(ctx context.Context, key string, ttl time.Duration) (LeasedSecret, error)
}

// LeasedSecret carries a secret value with an optional renewal
// schedule. Adapters that support leased dynamic credentials populate
// RenewAt; static-secret adapters leave it zero.
type LeasedSecret struct {
	Value   string
	RenewAt time.Time
	Renew   func(ctx context.Context) (LeasedSecret, error) // nil -> no renewal
}

// Typed error sentinels.
var (
	ErrSecretNotFound = errors.New("lazuli/secrets: key not found")
	ErrProviderClosed = errors.New("lazuli/secrets: provider closed")
)

// EnvProvider is the default fallback: reads from os.Getenv. Lazuli
// boot wires this as the default when no @lazuli/plugin-secret-vault binding
// is declared. SECURITY: env-only resolution is the same as the
// pre-FR-SEC-C3 behavior; closer to "no plugin" than "secure".
// Pilots SHOULD declare @lazuli/plugin-secret-vault for prod.
type EnvProvider struct{}

func (e EnvProvider) Get(ctx context.Context, key string) (string, error) {
	v := os.Getenv(key)
	if v == "" {
		return "", ErrSecretNotFound
	}
	return v, nil
}

func (e EnvProvider) GetWithLease(ctx context.Context, key string, ttl time.Duration) (LeasedSecret, error) {
	v, err := e.Get(ctx, key)
	if err != nil {
		return LeasedSecret{}, err
	}
	return LeasedSecret{Value: v}, nil
}
