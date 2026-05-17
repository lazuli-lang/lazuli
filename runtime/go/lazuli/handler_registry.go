package lazuli

import (
	"fmt"
	"sync"
)

// handlerRegistry is the singleton table keyed by qualified handler
// name. Used for command-level `@fn.X` references where the handler
// signature is `func(*Ctx, InputType) (OutputType, error)` — distinct
// from `bindingFnRegistry` (binding fns inside `creates`/`updates`
// field values, variadic `(ctx, args...) (any, error)`).
//
// The registry breaks the import cycle that would otherwise occur when
// generated code in `dist/go/<f>/*.gen.go` (package `<f>gen`) needs to
// reference user handlers in `app/features/<f>/*.go` (package `<f>`).
// Generated `Effect` values use `ReturnsFromRegistry[I, O](name)` and
// resolve the handler at dispatch time; gen never imports user code.
//
// Naming convention: registered handlers use the qualified form
// `<feature>.<command_name>` (e.g. `"account.login"`) so cross-feature
// collisions are impossible.
var (
	handlerMu       sync.RWMutex
	handlerRegistry = map[string]any{}
)

// RegisterFn registers a typed handler function under name. Called from
// user code's `init()` functions, typically alongside the handler's
// declaration:
//
//	package account
//
//	func Login(ctx *lazuli.Ctx, input accountgen.LoginInput) (string, error) { ... }
//
//	func init() {
//	    lazuli.RegisterFn("account.login", Login)
//	}
//
// The handler's exact signature must match what the generated `Effect`
// expects: `func(*Ctx, <InputType>) (<OutputType>, error)`. Signature
// mismatches are detected at first invocation, not at registration —
// `lazuli doctor` verifies `@fn.X` citations resolve to files on disk
// before runtime.
//
// Idempotent — the last registration wins. `lazuli doctor` flags
// duplicate registrations as a structural error.
func RegisterFn(name string, fn any) {
	handlerMu.Lock()
	defer handlerMu.Unlock()
	handlerRegistry[name] = fn
}

// lookupFn returns the handler registered under `name`, or nil when no
// handler is registered.
func lookupFn(name string) any {
	handlerMu.RLock()
	defer handlerMu.RUnlock()
	return handlerRegistry[name]
}

// ReturnsFromRegistry builds a [ReturnsEffect] that resolves its
// handler at invocation time via the handler registry. Used by
// generated `command.gen.go` to wire `Effect` without importing the
// user-authored handler package directly.
//
// The handler must be registered via [RegisterFn] before the command
// dispatches. Missing or mismatched-signature registrations produce a
// 500 [CodeInternal] error at dispatch time, not at construction —
// `lazuli doctor` enforces the static side of the contract by
// verifying every `@fn.X` reference in `.lzi` resolves to a file on
// disk under `app/features/<feature>/<name>.go`.
//
// Type parameters `I` and `O` come from the generated command's input
// and output types respectively; codegen materialises them from the
// IR so the runtime can perform the type assertion without reflection.
func ReturnsFromRegistry[I, O any](name string) ReturnsEffect {
	return ReturnsEffect{
		Handler: func(ctx *Ctx, input any) (any, error) {
			fn := lookupFn(name)
			if fn == nil {
				var zero O
				return zero, &Error{
					Status:  500,
					Code:    CodeInternal,
					Message: fmt.Sprintf("lazuli: no handler registered for %q", name),
				}
			}
			typed, ok := fn.(func(*Ctx, I) (O, error))
			if !ok {
				var zero O
				return zero, &Error{
					Status:  500,
					Code:    CodeInternal,
					Message: fmt.Sprintf("lazuli: handler %q has wrong signature", name),
				}
			}
			typedInput, ok := input.(I)
			if !ok {
				var zero I
				return zero, &Error{
					Status:  500,
					Code:    CodeInternal,
					Message: fmt.Sprintf("lazuli: handler %q received input of wrong type", name),
				}
			}
			return typed(ctx, typedInput)
		},
	}
}
