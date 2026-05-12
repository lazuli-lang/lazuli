package security

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"fmt"
	"strings"

	"lazuli.dev/runtime/lazuli/secret"
)

// PasswordPepper is a versioned secret used to HMAC a plaintext password
// before it is passed to a password hash algorithm.
type PasswordPepper struct {
	ID     string
	secret []byte
}

// PasswordPepperSource identifies a versioned pepper in a secret store.
type PasswordPepperSource struct {
	// ID is the stable version id persisted next to generated password hashes.
	ID string
	// Ref identifies the secret value in the provider-neutral secret store.
	Ref secret.SecretRef
}

// PasswordPepperRing contains the current pepper for new hashes and previous
// peppers that must still verify existing hashes after rotation.
type PasswordPepperRing struct {
	Current  PasswordPepper
	Previous []PasswordPepper
}

// PasswordPepperMatch describes which pepper verified a stored password hash.
type PasswordPepperMatch struct {
	PepperID      string
	NeedsRotation bool
}

// PasswordPepperVerifier validates a peppered password against a stored hash.
type PasswordPepperVerifier func(peppered []byte) (bool, error)

// PasswordPepperStringVerifier validates a base64-encoded peppered password.
type PasswordPepperStringVerifier func(peppered string) (bool, error)

var (
	// ErrInvalidPasswordPepper is returned when a pepper id, source, or secret
	// value is empty or duplicate.
	ErrInvalidPasswordPepper = errors.New("lazuli/security: invalid_password_pepper")
	// ErrPasswordPepperStoreRequired is returned when resolving peppers without
	// a secret store.
	ErrPasswordPepperStoreRequired = errors.New("lazuli/security: password_pepper_store_required")
	// ErrPasswordPepperNotFound is returned when a referenced pepper cannot be
	// resolved.
	ErrPasswordPepperNotFound = errors.New("lazuli/security: password_pepper_not_found")
	// ErrPasswordPepperMismatch is returned when no current or previous pepper
	// verifies a stored password hash.
	ErrPasswordPepperMismatch = errors.New("lazuli/security: password_pepper_mismatch")
	// ErrPasswordPepperVerifierRequired is returned when rotation verification is
	// called without a verifier callback.
	ErrPasswordPepperVerifierRequired = errors.New("lazuli/security: password_pepper_verifier_required")
)

// NewPasswordPepper returns a pepper with defensive copies of key material.
func NewPasswordPepper(id string, value []byte) (PasswordPepper, error) {
	id = strings.TrimSpace(id)
	if id == "" {
		return PasswordPepper{}, fmt.Errorf("%w: empty id", ErrInvalidPasswordPepper)
	}
	if len(value) == 0 {
		return PasswordPepper{}, fmt.Errorf("%w: empty secret", ErrInvalidPasswordPepper)
	}
	return PasswordPepper{
		ID:     id,
		secret: cloneBytes(value),
	}, nil
}

// Secret returns a defensive copy of the pepper key material.
func (p PasswordPepper) Secret() []byte {
	return cloneBytes(p.secret)
}

// PasswordPepperSecret returns a source whose secret version label matches id.
func PasswordPepperSecret(id string, ref secret.SecretRef) PasswordPepperSource {
	id = strings.TrimSpace(id)
	ref.Version = secret.VersionLabel(id)
	return PasswordPepperSource{
		ID:  id,
		Ref: ref,
	}
}

// ResolvePasswordPepper resolves a single pepper from a provider-neutral
// secret store.
func ResolvePasswordPepper(ctx context.Context, store secret.Store, source PasswordPepperSource) (PasswordPepper, error) {
	if store == nil {
		return PasswordPepper{}, ErrPasswordPepperStoreRequired
	}
	if strings.TrimSpace(source.ID) == "" {
		return PasswordPepper{}, fmt.Errorf("%w: empty id", ErrInvalidPasswordPepper)
	}

	value, err := store.Resolve(ctx, source.Ref)
	if err != nil {
		if errors.Is(err, secret.ErrNotFound) {
			return PasswordPepper{}, fmt.Errorf("%w: %s", ErrPasswordPepperNotFound, source.ID)
		}
		return PasswordPepper{}, err
	}
	return NewPasswordPepper(source.ID, value)
}

// ResolvePasswordPepperEnv resolves a pepper from an environment variable.
func ResolvePasswordPepperEnv(ctx context.Context, id, envName string) (PasswordPepper, error) {
	return ResolvePasswordPepper(ctx, secret.EnvStore{}, PasswordPepperSource{
		ID:  id,
		Ref: secret.Env(envName),
	})
}

// NewPasswordPepperRing validates and returns a ring with the current pepper
// first and previous peppers available for rotation verification.
func NewPasswordPepperRing(current PasswordPepper, previous ...PasswordPepper) (PasswordPepperRing, error) {
	if err := current.validate(); err != nil {
		return PasswordPepperRing{}, err
	}
	seen := map[string]struct{}{current.ID: {}}
	copiedPrevious := make([]PasswordPepper, 0, len(previous))
	for _, pepper := range previous {
		if err := pepper.validate(); err != nil {
			return PasswordPepperRing{}, err
		}
		if _, ok := seen[pepper.ID]; ok {
			return PasswordPepperRing{}, fmt.Errorf("%w: duplicate id %s", ErrInvalidPasswordPepper, pepper.ID)
		}
		seen[pepper.ID] = struct{}{}
		copiedPrevious = append(copiedPrevious, pepper.clone())
	}
	return PasswordPepperRing{
		Current:  current.clone(),
		Previous: copiedPrevious,
	}, nil
}

// ResolvePasswordPepperRing resolves the current pepper and any previous
// peppers from a provider-neutral secret store.
func ResolvePasswordPepperRing(ctx context.Context, store secret.Store, current PasswordPepperSource, previous ...PasswordPepperSource) (PasswordPepperRing, error) {
	currentPepper, err := ResolvePasswordPepper(ctx, store, current)
	if err != nil {
		return PasswordPepperRing{}, err
	}
	previousPeppers := make([]PasswordPepper, 0, len(previous))
	for _, source := range previous {
		pepper, err := ResolvePasswordPepper(ctx, store, source)
		if err != nil {
			return PasswordPepperRing{}, err
		}
		previousPeppers = append(previousPeppers, pepper)
	}
	return NewPasswordPepperRing(currentPepper, previousPeppers...)
}

// ApplyPasswordPepper returns HMAC-SHA256(password) using pepper as the key.
func ApplyPasswordPepper(password []byte, pepper PasswordPepper) ([]byte, error) {
	if err := pepper.validate(); err != nil {
		return nil, err
	}
	mac := hmac.New(sha256.New, pepper.secret)
	if _, err := mac.Write(password); err != nil {
		return nil, err
	}
	return mac.Sum(nil), nil
}

// ApplyPasswordPepperString returns a base64-encoded HMAC-SHA256 value,
// suitable for generated auth code that hashes string password inputs.
func ApplyPasswordPepperString(password string, pepper PasswordPepper) (string, error) {
	peppered, err := ApplyPasswordPepper([]byte(password), pepper)
	if err != nil {
		return "", err
	}
	return base64.RawStdEncoding.EncodeToString(peppered), nil
}

// VerifyPasswordWithPepperRotation tries the current pepper first, then
// previous peppers. The returned match tells callers whether the stored hash
// should be rehashed with the current pepper.
func VerifyPasswordWithPepperRotation(password []byte, ring PasswordPepperRing, verifier PasswordPepperVerifier) (PasswordPepperMatch, error) {
	if verifier == nil {
		return PasswordPepperMatch{}, ErrPasswordPepperVerifierRequired
	}
	peppers, err := ring.orderedPeppers()
	if err != nil {
		return PasswordPepperMatch{}, err
	}
	for i, pepper := range peppers {
		peppered, err := ApplyPasswordPepper(password, pepper)
		if err != nil {
			return PasswordPepperMatch{}, err
		}
		ok, err := verifier(peppered)
		if err != nil {
			return PasswordPepperMatch{}, err
		}
		if ok {
			return PasswordPepperMatch{
				PepperID:      pepper.ID,
				NeedsRotation: i != 0,
			}, nil
		}
	}
	return PasswordPepperMatch{}, ErrPasswordPepperMismatch
}

// VerifyPasswordStringWithPepperRotation is the string variant of
// VerifyPasswordWithPepperRotation.
func VerifyPasswordStringWithPepperRotation(password string, ring PasswordPepperRing, verifier PasswordPepperStringVerifier) (PasswordPepperMatch, error) {
	if verifier == nil {
		return PasswordPepperMatch{}, ErrPasswordPepperVerifierRequired
	}
	return VerifyPasswordWithPepperRotation([]byte(password), ring, func(peppered []byte) (bool, error) {
		return verifier(base64.RawStdEncoding.EncodeToString(peppered))
	})
}

func (p PasswordPepper) validate() error {
	if strings.TrimSpace(p.ID) == "" {
		return fmt.Errorf("%w: empty id", ErrInvalidPasswordPepper)
	}
	if len(p.secret) == 0 {
		return fmt.Errorf("%w: empty secret", ErrInvalidPasswordPepper)
	}
	return nil
}

func (p PasswordPepper) clone() PasswordPepper {
	return PasswordPepper{
		ID:     p.ID,
		secret: cloneBytes(p.secret),
	}
}

func (r PasswordPepperRing) orderedPeppers() ([]PasswordPepper, error) {
	if err := r.Current.validate(); err != nil {
		return nil, err
	}
	peppers := make([]PasswordPepper, 0, 1+len(r.Previous))
	seen := map[string]struct{}{r.Current.ID: {}}
	peppers = append(peppers, r.Current.clone())
	for _, pepper := range r.Previous {
		if err := pepper.validate(); err != nil {
			return nil, err
		}
		if _, ok := seen[pepper.ID]; ok {
			return nil, fmt.Errorf("%w: duplicate id %s", ErrInvalidPasswordPepper, pepper.ID)
		}
		seen[pepper.ID] = struct{}{}
		peppers = append(peppers, pepper.clone())
	}
	return peppers, nil
}
