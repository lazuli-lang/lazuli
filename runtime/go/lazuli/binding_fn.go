package lazuli

import (
	"context"
	"fmt"
	"reflect"
	"strings"
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

// resolveBindingFn resolves a `@fn.<name>` reference used inside a
// `creates`/`updates` binding (or a query filter RHS) to an invokable
// closure, bridging the TWO fn registries that the framework exposes:
//
//  1. The explicit binding-fn registry (populated by [RegisterBindingFn]),
//     keyed by the exact `name` codegen emits — the bare `@fn.<name>`.
//  2. The handler registry (populated by [RegisterFn]), keyed by the
//     FEATURE-QUALIFIED `<feature>.<name>` form. This is the registry that
//     pilots actually populate for their `@fn.X` handlers (the same
//     handlers used by command-level `@fn` via [ReturnsFromRegistry]), so
//     a `field = @fn.X(...)` binding must be able to reach it.
//
// Codegen emits the BARE name in [FromFn] (e.g. "hash_password"), but
// pilots register handlers under the qualified key (e.g.
// "account.hash_password"). resolveBindingFn reconciles the mismatch:
//   - exact binding-fn registration wins (explicit opt-in, highest fidelity);
//   - then an exact handler-registry hit on the bare name;
//   - then a UNIQUE `*.<bare>` suffix match across handler-registry keys.
//     Ambiguity (two features registering the same bare name) is a
//     fail-closed error, never a silent wrong pick.
//
// A matched handler-registry fn has the shape
// `func(*Ctx, I) (O, error)` and is adapted to the BindingFn calling
// convention via reflection — mirroring [ReturnsFromRegistry]'s adapter
// but boxing the concrete `O` (e.g. int64) back to `any` so it can flow
// into the INSERT as the column value.
//
// Returns (nil, false) when the name is in NEITHER registry, or when the
// suffix match is ambiguous (the caller turns this into a clear 500).
func resolveBindingFn(name string) (BindingFn, bool) {
	if fn, ok := lookupBindingFn(name); ok {
		return fn, true
	}
	// Fall back to the handler registry. Try the bare name first (a
	// handler registered without a feature prefix), then the unique
	// feature-qualified key.
	if h := lookupFn(name); h != nil {
		return adaptHandlerToBindingFn(h), true
	}
	if h, ok := lookupFnBySuffix(name); ok {
		return adaptHandlerToBindingFn(h), true
	}
	return nil, false
}

// lookupFnBySuffix scans the handler registry for a key of the form
// `<feature>.<bare>` and returns its handler iff exactly one such key
// exists. Two matches (a same-bare-named fn in two features) returns
// (nil, false) so the caller fails closed rather than guessing.
func lookupFnBySuffix(bare string) (any, bool) {
	suffix := "." + bare
	handlerMu.RLock()
	defer handlerMu.RUnlock()
	var match any
	found := 0
	for key, fn := range handlerRegistry {
		if strings.HasSuffix(key, suffix) {
			match = fn
			found++
		}
	}
	if found != 1 {
		return nil, false
	}
	return match, true
}

// adaptHandlerToBindingFn wraps a handler-registry fn
// (`func(*Ctx, I) (O, error)`, stored as `any`) so it satisfies the
// [BindingFn] calling convention `func(ctx, args...) (any, error)`.
//
// Mirrors [ReturnsFromRegistry]'s reflection adapter, but:
//   - the *Ctx is recovered from the passed-in context.Context (the
//     binding resolver threads the live *Ctx through as the context arg);
//   - arity follows the binding call: zero args → the handler's input is
//     its zero value (matches handlers that do `_ = input`); one arg →
//     the resolved arg is passed straight through (matches handlers that
//     read `passwordOf(input)`); the input param is typed `any` in pilot
//     handlers so no per-type assertion is needed, but we convert when a
//     handler declares a concrete input type;
//   - the concrete return `O` is boxed back to `any`.
func adaptHandlerToBindingFn(h any) BindingFn {
	return func(ctx context.Context, args ...any) (any, error) {
		fnVal := reflect.ValueOf(h)
		fnType := fnVal.Type()
		if fnType.Kind() != reflect.Func ||
			fnType.NumIn() != 2 || fnType.NumOut() != 2 {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "binding fn bridge: handler has wrong signature " +
					"(want func(*Ctx, I) (O, error)), got " + fnType.String()}
		}

		// arg 0: *Ctx — recover the live *Ctx from the context.Context.
		lazCtx, ok := ctx.(*Ctx)
		if !ok {
			// Binding resolution always passes a *Ctx; defend anyway so a
			// stray context.Context yields a clear error, not a panic.
			lazCtx = &Ctx{Context: ctx}
		}
		ctxArg := reflect.ValueOf(lazCtx)
		if !ctxArg.Type().AssignableTo(fnType.In(0)) {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: fmt.Sprintf("binding fn bridge: handler first param %s "+
					"is not *lazuli.Ctx", fnType.In(0))}
		}

		// arg 1: the single input. Zero-arg binding → zero value of the
		// declared input type; one-arg binding → the resolved arg;
		// N>1-arg binding → construct the handler's struct input by
		// mapping each positional arg to the struct field at the same
		// index (declaration order).
		inType := fnType.In(1)
		var inArg reflect.Value
		switch len(args) {
		case 0:
			inArg = reflect.Zero(inType)
		case 1:
			inArg = boxInto(args[0], inType)
		default:
			built, err := buildStructInput(inType, args)
			if err != nil {
				return nil, err
			}
			inArg = built
		}

		out := fnVal.Call([]reflect.Value{ctxArg, inArg})
		// out[1] is error.
		var err error
		if e := out[1].Interface(); e != nil {
			err, _ = e.(error)
		}
		if err != nil {
			return nil, err
		}
		// Box the concrete return O back to any (int64 → bigint, etc.).
		return out[0].Interface(), nil
	}
}

// boxInto converts a resolved binding arg into the handler's declared
// input type. When the handler's input is `any`/interface (the pilot
// convention), the arg passes through untouched. When it's a concrete
// type the arg is convertible to, it is converted; otherwise the raw
// reflect.Value of the arg is used and Go's call will type-check it.
func boxInto(arg any, inType reflect.Type) reflect.Value {
	if arg == nil {
		return reflect.Zero(inType)
	}
	av := reflect.ValueOf(arg)
	if av.Type().AssignableTo(inType) {
		return av
	}
	if av.Type().ConvertibleTo(inType) && inType.Kind() != reflect.Interface {
		return av.Convert(inType)
	}
	// Interface input (e.g. `any`) that arg doesn't directly assign to is
	// impossible (everything assigns to `any`); for a concrete mismatch
	// hand the raw value to Call and let it surface a clear panic-free
	// error path via the wrong-type assertion. To stay panic-free we box
	// through a fresh addressable value of inType when possible.
	if inType.Kind() == reflect.Interface {
		box := reflect.New(inType).Elem()
		box.Set(av)
		return box
	}
	return av
}

// buildStructInput constructs the handler's single struct input from N>1
// positional binding args, mapping `args[i]` onto the i-th EXPORTED field
// in declaration order. This is the multi-arg `@fn.foo(a, b, c)` case: the
// handler declares `func(*Ctx, Input) (O, error)` where `Input` is a
// struct whose fields correspond positionally to the binding args.
//
// Fails closed (a clear 500, never a panic) when:
//   - the input is not a struct (so N>1 args have nowhere to land);
//   - the arg count != the exported-field count (arity mismatch);
//   - an arg can't be coerced into its field's type.
//
// Each arg is coerced via [coerceFieldValue], which handles pointer fields
// (`*string` ← a `string`/`nil` from an optional input), value fields, and
// numeric widening (the `UserID lazuli.ID`/int64 field ← an int64 actor id).
func buildStructInput(inType reflect.Type, args []any) (reflect.Value, error) {
	// Unwrap a single level of pointer so a handler declaring `*Input`
	// still gets a populated struct (defensive — pilots declare a value).
	targetType := inType
	wantPtr := false
	if targetType.Kind() == reflect.Pointer {
		wantPtr = true
		targetType = targetType.Elem()
	}
	if targetType.Kind() != reflect.Struct {
		return reflect.Value{}, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("binding fn bridge: @fn was called with %d args "+
				"but the handler input %s is not a struct (multi-arg @fn requires "+
				"a struct input whose fields map to the args by position)",
				len(args), inType)}
	}

	// Collect exported fields in declaration order. Unexported fields are
	// not settable via reflection and have no DSL counterpart, so they are
	// excluded from the positional mapping.
	exported := make([]int, 0, targetType.NumField())
	for i := 0; i < targetType.NumField(); i++ {
		if targetType.Field(i).IsExported() {
			exported = append(exported, i)
		}
	}
	if len(exported) != len(args) {
		return reflect.Value{}, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("binding fn bridge: @fn was called with %d args "+
				"but the handler input %s has %d exported field(s) — arg count must "+
				"match field count (mapped by position)",
				len(args), targetType, len(exported))}
	}

	out := reflect.New(targetType).Elem()
	for argIdx, fieldIdx := range exported {
		field := out.Field(fieldIdx)
		ft := targetType.Field(fieldIdx)
		coerced, err := coerceFieldValue(args[argIdx], field.Type())
		if err != nil {
			return reflect.Value{}, &Error{Status: 500, Code: CodeInternal,
				Message: fmt.Sprintf("binding fn bridge: @fn arg %d cannot bind to "+
					"%s.%s (%s): %v", argIdx, targetType, ft.Name, field.Type(), err)}
		}
		field.Set(coerced)
	}

	if wantPtr {
		ptr := reflect.New(targetType)
		ptr.Elem().Set(out)
		return ptr, nil
	}
	return out, nil
}

// coerceFieldValue converts a resolved binding arg (`any`: string, *string,
// int64, etc.) into a reflect.Value assignable to a struct field of type
// `ft`. Handles, in order:
//   - nil arg → the field's zero value (a nil pointer for `*string`);
//   - directly assignable (e.g. int64 → lazuli.ID alias, string → string);
//   - pointer field ← a value of the pointee type (e.g. `*string` ← string,
//     including the readPath case where an optional `*string` input was
//     already deref'd to a bare string); also numeric/convertible pointee;
//   - convertible value (numeric widening, named-type aliases).
//
// Returns an error (never panics) when no coercion applies.
func coerceFieldValue(arg any, ft reflect.Type) (reflect.Value, error) {
	if arg == nil {
		return reflect.Zero(ft), nil
	}
	av := reflect.ValueOf(arg)

	// Direct assignment (covers value→value and the int64→ID alias).
	if av.Type().AssignableTo(ft) {
		return av, nil
	}
	// Convertible value field (numeric widening, named scalar aliases).
	if ft.Kind() != reflect.Pointer && av.Type().ConvertibleTo(ft) {
		return av.Convert(ft), nil
	}

	// Pointer field: build a *T from a T-or-convertible arg.
	if ft.Kind() == reflect.Pointer {
		elem := ft.Elem()
		var inner reflect.Value
		switch {
		case av.Type().AssignableTo(elem):
			inner = av
		case av.Type().ConvertibleTo(elem):
			inner = av.Convert(elem)
		case av.Kind() == reflect.Pointer && av.Type().AssignableTo(ft):
			// arg is already the right pointer type (assignable handled
			// above, but a nil-typed pointer slips through to here).
			return av, nil
		default:
			return reflect.Value{}, fmt.Errorf(
				"arg of type %s is not assignable/convertible to pointee %s",
				av.Type(), elem)
		}
		p := reflect.New(elem)
		p.Elem().Set(inner)
		return p, nil
	}

	return reflect.Value{}, fmt.Errorf(
		"arg of type %s is not assignable/convertible to %s", av.Type(), ft)
}
