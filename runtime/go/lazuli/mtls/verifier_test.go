package mtls

import (
	"context"
	"errors"
	"testing"
)

func TestDenyVerifierRejects(t *testing.T) {
	var verifier DenyVerifier

	identity, err := verifier.Verify(context.Background(), nil, nil)
	if !errors.Is(err, ErrUntrustedCA) {
		t.Fatalf("Verify() error = %v, want %v", err, ErrUntrustedCA)
	}
	if identity != (Identity{}) {
		t.Fatalf("Verify() identity = %#v, want zero value", identity)
	}
	if err := verifier.Close(); err != nil {
		t.Fatalf("Close() error = %v, want nil", err)
	}
}
