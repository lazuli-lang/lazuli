package secret

import (
	"context"
	"encoding/json"
	"errors"
)

const (
	// RuntimeSecretMask is emitted when a RuntimeSecret is formatted or marshaled.
	RuntimeSecretMask = "[REDACTED]"
)

var (
	// ErrRuntimeSecretRevealUnavailable is returned when a handle has no reveal callback.
	ErrRuntimeSecretRevealUnavailable = errors.New("lazuli/secret: runtime secret reveal unavailable")
)

// RevealFunc reveals secret bytes for a normalized reference.
//
// It is intended for test fixtures and adapter wiring. Callers should prefer
// RuntimeSecret.Reveal so returned buffers are defensively copied.
type RevealFunc func(context.Context, SecretRef) ([]byte, error)

// RuntimeSecret is an opaque runtime handle to secret material.
//
// The handle carries only a SecretRef and an optional reveal callback. Its
// formatted and marshaled forms are always redacted; callers must opt in to
// secret access through Reveal.
type RuntimeSecret struct {
	ref    SecretRef
	reveal RevealFunc
}

// RuntimeSecretRef returns an opaque runtime secret handle for ref.
func RuntimeSecretRef(ref SecretRef) RuntimeSecret {
	return RuntimeSecret{ref: ref}
}

// NewRuntimeSecret returns an opaque runtime secret handle with reveal callback.
func NewRuntimeSecret(ref SecretRef, reveal RevealFunc) RuntimeSecret {
	return RuntimeSecretRef(ref).WithReveal(reveal)
}

// RuntimeSecretFromStore returns a runtime secret handle that resolves ref from store.
func RuntimeSecretFromStore(ref SecretRef, store Store) RuntimeSecret {
	if store == nil {
		return RuntimeSecretRef(ref)
	}
	return NewRuntimeSecret(ref, func(ctx context.Context, ref SecretRef) ([]byte, error) {
		return store.Resolve(ctx, ref)
	})
}

// ValidateRuntimeSecret checks that handle identifies a secret.
func ValidateRuntimeSecret(handle RuntimeSecret) error {
	return handle.Validate()
}

// WithReveal returns a copy of handle with reveal callback attached.
func (s RuntimeSecret) WithReveal(reveal RevealFunc) RuntimeSecret {
	s.reveal = reveal
	return s
}

// Ref returns the normalized secret reference carried by handle.
//
// An invalid handle returns the zero SecretRef; use Validate to distinguish
// invalid input from an intentionally empty zero value.
func (s RuntimeSecret) Ref() SecretRef {
	ref, err := normalizeRef(s.ref)
	if err != nil {
		return SecretRef{}
	}
	return ref
}

// Validate checks that handle identifies a secret. A reveal callback is optional.
func (s RuntimeSecret) Validate() error {
	_, err := normalizeRef(s.ref)
	return err
}

// Reveal returns the secret bytes by invoking the handle's reveal callback.
func (s RuntimeSecret) Reveal(ctx context.Context) ([]byte, error) {
	if err := contextErr(ctx); err != nil {
		return nil, err
	}

	ref, err := normalizeRef(s.ref)
	if err != nil {
		return nil, err
	}
	if s.reveal == nil {
		return nil, ErrRuntimeSecretRevealUnavailable
	}

	value, err := s.reveal(ctx, ref)
	if err != nil {
		return nil, err
	}
	return cloneBytes(value), nil
}

// RevealString returns the secret bytes as a string.
func (s RuntimeSecret) RevealString(ctx context.Context) (string, error) {
	value, err := s.Reveal(ctx)
	if err != nil {
		return "", err
	}
	return string(value), nil
}

// String returns a redacted representation of the handle.
func (s RuntimeSecret) String() string {
	return RuntimeSecretMask
}

// GoString returns a redacted representation for %#v formatting.
func (s RuntimeSecret) GoString() string {
	return `secret.RuntimeSecret("` + RuntimeSecretMask + `")`
}

// MarshalText emits a redacted representation of the handle.
func (s RuntimeSecret) MarshalText() ([]byte, error) {
	return []byte(s.String()), nil
}

// MarshalJSON emits a redacted JSON string for the handle.
func (s RuntimeSecret) MarshalJSON() ([]byte, error) {
	return json.Marshal(s.String())
}
