// Package mtls is the runtime-level Verifier seam for @lazuli/plugin-mtls.
// Adapters validate client certificates against a configured CA bundle
// (or SPIRE/SPIFFE for service-mesh deployments).
//
// The framework calls Verify in the HTTP middleware chain BEFORE the
// session-cookie path so service-to-service traffic can short-circuit
// to a system actor without a session.
package mtls

import (
	"context"
	"crypto/x509"
	"errors"
)

// Verifier validates a client certificate chain. Implementations
// MUST be safe for concurrent use.
type Verifier interface {
	Verify(ctx context.Context, cert *x509.Certificate, chain [][]*x509.Certificate) (Identity, error)
	Close() error
}

// Identity carries the verified service identity. For SPIRE-style
// IDs the SpiffeID field carries the URI; for plain CA-bundle
// verification it's empty + Subject carries the cert's DN.
type Identity struct {
	SpiffeID string // e.g. "spiffe://example.org/api"
	Subject  string // CN or first SAN
	Issuer   string // CA DN
}

var (
	ErrVerifierUnavailable = errors.New("lazuli/mtls: verifier unavailable")
	ErrUntrustedCA         = errors.New("lazuli/mtls: untrusted CA")
	ErrCertExpired         = errors.New("lazuli/mtls: cert expired")
	ErrCertRevoked         = errors.New("lazuli/mtls: cert revoked")
)

// DenyVerifier rejects ALL mTLS traffic. Default when no adapter
// binds -- mTLS is opt-in.
type DenyVerifier struct{}

func (DenyVerifier) Verify(ctx context.Context, cert *x509.Certificate, chain [][]*x509.Certificate) (Identity, error) {
	return Identity{}, ErrUntrustedCA
}

func (DenyVerifier) Close() error { return nil }
