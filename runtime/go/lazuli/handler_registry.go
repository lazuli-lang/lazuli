package lazuli

import (
	"fmt"
	"reflect"
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

// HandlerFromRegistry builds the `Handler` func for a generated
// `lazuli.Api[I, O]` value, resolving the user-authored handler at
// dispatch time via the handler registry. This is the api-surface
// analogue of [ReturnsFromRegistry] (which serves the command surface):
// it lets generated `api.gen.go` wire the declared `handler @fn.<name>`
// (or convention `./api/<name>.go`) WITHOUT importing the user handler
// package, breaking the same `<f>gen` → `<f>` import cycle.
//
// The returned closure is non-nil, so the api's `HandlerChecker()`
// reports the endpoint as wired and the HTTP mount loop mounts the
// route — fixing API-HANDLER-UNWIRED-001 (the previously-DOA api
// surface). The handler must be registered via [RegisterFn] under the
// same qualified name (`<feature>.<handler>`) before the first request;
// a missing or mis-typed registration yields a 500 [CodeInternal] at
// dispatch (never a silent 404), exactly mirroring the command path.
//
// `lazuli doctor` enforces the static side: every `@fn.X` reference
// must resolve to a file on disk before runtime.
func HandlerFromRegistry[I, O any](name string) func(ctx *Ctx, input I) (O, error) {
	return func(ctx *Ctx, input I) (O, error) {
		var zero O
		fn := lookupFn(name)
		if fn == nil {
			return zero, &Error{
				Status:  500,
				Code:    CodeInternal,
				Message: fmt.Sprintf("lazuli: no handler registered for %q", name),
			}
		}
		// Fast path: the registered fn has exactly the api's signature.
		if typed, ok := fn.(func(*Ctx, I) (O, error)); ok {
			return typed(ctx, input)
		}
		// Compatible-input fallback. The same `@fn.<name>` handler backs
		// BOTH the command surface (registered as
		// `func(*Ctx, struct{}) (User, error)` — anonymous empty input)
		// and the api surface, whose generated Args type is a DISTINCT
		// named struct (`type MeApiArgs struct{}`). `MeApiArgs` and
		// `struct{}` are different Go types, so the exact assertion above
		// fails even though the input is byte-identical (zero-size). Rather
		// than force the author to register a second api-only handler, we
		// invoke the registered fn via reflection when its input type is
		// assignable from / convertible to the api's `I` and its output is
		// `O`. This keeps a single `RegisterFn("<feature>.<name>", Fn)` per
		// handler — the whole point of the registry bridge.
		out, err, ok := callRegistryFnReflect[I, O](fn, ctx, input)
		if !ok {
			return zero, &Error{
				Status:  500,
				Code:    CodeInternal,
				Message: fmt.Sprintf("lazuli: handler %q has wrong signature", name),
			}
		}
		return out, err
	}
}

// callRegistryFnReflect invokes a registered handler whose concrete
// input type differs from the api's `I` but is layout-compatible (the
// command-vs-api empty-struct case, and any future named-alias drift).
// Returns ok=false when `fn` is not a `func(*Ctx, X) (O, error)` whose
// `X` accepts an `I` value — the caller then surfaces the standard
// wrong-signature 500. Never panics on a mismatch.
func callRegistryFnReflect[I, O any](fn any, ctx *Ctx, input I) (O, error, bool) {
	var zero O
	fv := reflect.ValueOf(fn)
	ft := fv.Type()
	if ft.Kind() != reflect.Func || ft.NumIn() != 2 || ft.NumOut() != 2 {
		return zero, nil, false
	}
	// in[0] must be *Ctx; out[0] must be O; out[1] must be error.
	if ft.In(0) != reflect.TypeOf((*Ctx)(nil)) {
		return zero, nil, false
	}
	if ft.Out(0) != reflect.TypeOf(zero) {
		return zero, nil, false
	}
	if ft.Out(1) != reflect.TypeOf((*error)(nil)).Elem() {
		return zero, nil, false
	}
	inputVal := reflect.ValueOf(input)
	paramType := ft.In(1)
	switch {
	case inputVal.Type().AssignableTo(paramType):
		// already the right type (rare here; fast path would have caught
		// an exact func match, but a differing-but-assignable param slips
		// through).
	case inputVal.Type().ConvertibleTo(paramType):
		inputVal = inputVal.Convert(paramType)
	default:
		return zero, nil, false
	}
	results := fv.Call([]reflect.Value{reflect.ValueOf(ctx), inputVal})
	out, _ := results[0].Interface().(O)
	var err error
	if e := results[1].Interface(); e != nil {
		err, _ = e.(error)
	}
	return out, err, true
}
