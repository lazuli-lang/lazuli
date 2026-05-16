package lazuli

import (
	"context"
	"sync"
)

// BindingFn is the user-side adapter that the runtime invokes when a
// `creates`/`updates` binding or a query filter expresses an
// `@fn.<name>(<arg>...)` call. Args arrive in declaration order, each
// already resolved by the runtime against its source (input / ctx /
// target / const / nested fn).
//
// Closes WAR-VOCAB-CREATES-FN-CALL-01. The fn is registered once at
// boot via [RegisterBindingFn].
//
// The fn returns the value to bind to the target column / filter side,
// or an error that aborts the command + propagates as a 500.
type BindingFn func(ctx context.Context, args ...any) (any, error)

// bindingFnRegistry is the singleton table keyed by qualified fn name.
// The codegen emits the unqualified name today (e.g. "hash_password");
// app-side registrations match that exact string. A future codegen
// pass may qualify with feature (e.g. "account.hash_password") — the
// registry contract stays string-keyed either way.
var (
	bindingFnMu       sync.RWMutex
	bindingFnRegistry = map[string]BindingFn{}
)

// RegisterBindingFn registers a user-authored binding fn under the
// given name. Idempotent — the last registration wins. Call at init()
// time before [Mux] is built.
//
//	func init() {
//	    lazuli.RegisterBindingFn("hash_password",
//	        func(ctx context.Context, args ...any) (any, error) {
//	            plaintext, _ := args[0].(string)
//	            return auth.HashPassword(ctx, passwordContract, plaintext)
//	        },
//	    )
//	}
func RegisterBindingFn(name string, fn BindingFn) {
	bindingFnMu.Lock()
	defer bindingFnMu.Unlock()
	bindingFnRegistry[name] = fn
}

// lookupBindingFn returns the fn registered under `name`, or
// (nil, false) when no fn is registered.
func lookupBindingFn(name string) (BindingFn, bool) {
	bindingFnMu.RLock()
	defer bindingFnMu.RUnlock()
	fn, ok := bindingFnRegistry[name]
	return fn, ok
}
