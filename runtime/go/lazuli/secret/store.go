// Package secret provides the provider-neutral runtime contract for resolving
// secrets declared by Lazuli apps and adapters.
package secret

import (
	"context"
	"errors"
	"fmt"
	"os"
	"strings"
	"sync"
)

// VersionLabel names a provider-neutral secret version label.
//
// Stores decide how to map labels such as "current", "previous", or
// deployment-specific aliases to their backing provider.
type VersionLabel string

// SecretRef identifies a secret by logical name and optional version label.
type SecretRef struct {
	// Name is the provider-neutral secret name, such as an env variable name
	// or adapter-defined logical key.
	Name string
	// Version is an optional provider-neutral version label. Empty means the
	// store's default version for Name.
	Version VersionLabel
}

// Store resolves secret bytes by reference.
//
// Implementations must be safe for concurrent use and must not return buffers
// that callers can mutate in-place inside the backing store.
type Store interface {
	// Resolve returns the bytes for ref or ErrNotFound when the secret is not
	// available in this store.
	Resolve(ctx context.Context, ref SecretRef) ([]byte, error)
}

var (
	// ErrInvalidRef is returned when a SecretRef cannot identify a secret.
	ErrInvalidRef = errors.New("lazuli/secret: invalid reference")
	// ErrNotFound is returned when a store cannot resolve a SecretRef.
	ErrNotFound = errors.New("lazuli/secret: not found")
)

// Ref returns a SecretRef for name.
func Ref(name string) SecretRef {
	return SecretRef{Name: name}
}

// WithVersion returns a copy of ref bound to version.
func (ref SecretRef) WithVersion(version VersionLabel) SecretRef {
	ref.Version = version
	return ref
}

// Env returns a SecretRef whose Name is resolved from an environment variable.
//
// The accepted input is either "NAME" or the Lazuli source spelling
// "env.NAME".
func Env(name string) SecretRef {
	return SecretRef{Name: trimEnvPrefix(name)}
}

// ResolveEnv resolves ref from process environment variables using os.LookupEnv.
func ResolveEnv(ctx context.Context, ref SecretRef) ([]byte, error) {
	return EnvStore{}.Resolve(ctx, ref)
}

// EnvStore resolves secret references from environment variables.
//
// EnvStore is intended for local development, tests, and boot-time adapter
// wiring where deployment infrastructure has already selected the correct
// secret version. Version labels are accepted on SecretRef but do not change
// the environment variable name.
type EnvStore struct {
	// LookupEnv returns an environment value by name. Defaults to os.LookupEnv
	// when nil.
	LookupEnv func(string) (string, bool)
}

var _ Store = EnvStore{}

// Resolve returns the bytes for ref from the configured environment lookup.
func (s EnvStore) Resolve(ctx context.Context, ref SecretRef) ([]byte, error) {
	if err := contextErr(ctx); err != nil {
		return nil, err
	}

	ref, err := normalizeRef(ref)
	if err != nil {
		return nil, err
	}

	lookup := s.LookupEnv
	if lookup == nil {
		lookup = os.LookupEnv
	}
	value, ok := lookup(trimEnvPrefix(ref.Name))
	if !ok || value == "" {
		return nil, fmt.Errorf("%w: %s", ErrNotFound, ref.Name)
	}
	return []byte(value), nil
}

// MemoryStore is an in-process Store implementation.
//
// It is safe for concurrent use, stores defensive copies of values passed to
// Put, and returns defensive copies from Resolve.
type MemoryStore struct {
	mu      sync.RWMutex
	secrets map[SecretRef][]byte
}

var _ Store = (*MemoryStore)(nil)

// NewMemoryStore returns an empty in-process secret store.
func NewMemoryStore() *MemoryStore {
	return &MemoryStore{
		secrets: make(map[SecretRef][]byte),
	}
}

// Put stores value for ref, replacing any existing value.
func (s *MemoryStore) Put(ctx context.Context, ref SecretRef, value []byte) error {
	if err := contextErr(ctx); err != nil {
		return err
	}

	ref, err := normalizeRef(ref)
	if err != nil {
		return err
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	s.ensureLocked()
	s.secrets[ref] = cloneBytes(value)
	return nil
}

// Resolve returns the stored value for ref.
func (s *MemoryStore) Resolve(ctx context.Context, ref SecretRef) ([]byte, error) {
	if err := contextErr(ctx); err != nil {
		return nil, err
	}

	ref, err := normalizeRef(ref)
	if err != nil {
		return nil, err
	}

	s.mu.RLock()
	defer s.mu.RUnlock()
	value, ok := s.secrets[ref]
	if !ok {
		return nil, fmt.Errorf("%w: %s", ErrNotFound, ref.Name)
	}
	return cloneBytes(value), nil
}

// Delete removes ref from the store. Missing refs are ignored.
func (s *MemoryStore) Delete(ctx context.Context, ref SecretRef) error {
	if err := contextErr(ctx); err != nil {
		return err
	}

	ref, err := normalizeRef(ref)
	if err != nil {
		return err
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	s.ensureLocked()
	delete(s.secrets, ref)
	return nil
}

func (s *MemoryStore) ensureLocked() {
	if s.secrets == nil {
		s.secrets = make(map[SecretRef][]byte)
	}
}

func normalizeRef(ref SecretRef) (SecretRef, error) {
	ref.Name = strings.TrimSpace(trimEnvPrefix(ref.Name))
	ref.Version = VersionLabel(strings.TrimSpace(string(ref.Version)))
	if ref.Name == "" {
		return SecretRef{}, ErrInvalidRef
	}
	return ref, nil
}

func trimEnvPrefix(name string) string {
	return strings.TrimPrefix(strings.TrimSpace(name), "env.")
}

func contextErr(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
		return nil
	}
}

func cloneBytes(value []byte) []byte {
	if value == nil {
		return nil
	}
	out := make([]byte, len(value))
	copy(out, value)
	return out
}
