package cache

import (
	"context"
	"errors"
)

// Adapter selection happens at boot from `registry.lzi`'s `cache
// <slot>` capability. The default in-process LRU is wired via
// `@runtime/local`; production-grade Redis lives in `@runtime/redis`.
//
// Bound is the singleton resolved at boot. The runtime asserts a
// backend was bound before serving the first query that declares a
// `cache` block.
var bound Backend

// Bind installs the active adapter. Called once at boot from generated
// dependency wiring. Re-binding panics so production runtimes fail
// loudly on misconfiguration.
func Bind(b Backend) {
	if bound != nil {
		panic("lazuli/cache: Bind called twice; adapter is already bound")
	}
	if b == nil {
		panic("lazuli/cache: Bind called with nil backend")
	}
	bound = b
}

// Active returns the bound backend. Returns ErrNotBound when no
// backend is wired (e.g. tests that don't declare a `cache` block).
func Active() (Backend, error) {
	if bound == nil {
		return nil, ErrNotBound
	}
	return bound, nil
}

// ErrNotBound signals that the runtime saw a `cache` declaration but
// no adapter was bound at boot. Generated code can treat this as a
// configuration error.
var ErrNotBound = errors.New("lazuli/cache: no backend bound (declare `cache <slot>` in registry.lzi and bind it at boot)")

// MustGet is a convenience helper for generated code that already knows
// a backend is bound at the call site.
func MustGet(ctx context.Context, key string) ([]byte, bool) {
	b, err := Active()
	if err != nil {
		return nil, false
	}
	value, hit, err := b.Get(ctx, key)
	if err != nil {
		return nil, false
	}
	return value, hit
}
