package lazuli

import (
	"context"
	"errors"
	"testing"
)

func resetBindingFnRegistry() {
	bindingFnMu.Lock()
	defer bindingFnMu.Unlock()
	bindingFnRegistry = map[string]BindingFn{}
}

func TestRegisterBindingFn_RoundTrip(t *testing.T) {
	resetBindingFnRegistry()
	RegisterBindingFn("noop", func(ctx context.Context, args ...any) (any, error) {
		return "ok", nil
	})
	fn, ok := lookupBindingFn("noop")
	if !ok {
		t.Fatalf("expected registered fn to be discoverable")
	}
	got, err := fn(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != "ok" {
		t.Fatalf("expected 'ok', got %v", got)
	}
}

func TestLookupBindingFn_NotRegistered(t *testing.T) {
	resetBindingFnRegistry()
	_, ok := lookupBindingFn("nope")
	if ok {
		t.Fatalf("expected unknown fn to be missing")
	}
}

func TestRegisterBindingFn_LastWins(t *testing.T) {
	resetBindingFnRegistry()
	RegisterBindingFn("hello", func(ctx context.Context, args ...any) (any, error) {
		return "first", nil
	})
	RegisterBindingFn("hello", func(ctx context.Context, args ...any) (any, error) {
		return "second", nil
	})
	fn, _ := lookupBindingFn("hello")
	got, _ := fn(context.Background())
	if got != "second" {
		t.Fatalf("expected last-registration 'second', got %v", got)
	}
}

func TestBindingFn_ErrorPropagates(t *testing.T) {
	resetBindingFnRegistry()
	sentinel := errors.New("binding boom")
	RegisterBindingFn("explode", func(ctx context.Context, args ...any) (any, error) {
		return nil, sentinel
	})
	fn, _ := lookupBindingFn("explode")
	_, err := fn(context.Background())
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel error, got %v", err)
	}
}

func TestResolveSource_FnCallInvokesRegisteredFn(t *testing.T) {
	resetBindingFnRegistry()
	RegisterBindingFn("upper", func(ctx context.Context, args ...any) (any, error) {
		s, _ := args[0].(string)
		// Naive upper for the test — uses upper-case manually so the
		// stdlib `strings` import doesn't leak into the runtime.
		out := make([]byte, 0, len(s))
		for _, b := range []byte(s) {
			if b >= 'a' && b <= 'z' {
				out = append(out, b-32)
			} else {
				out = append(out, b)
			}
		}
		return string(out), nil
	})

	src := FromFn("upper", []Source{FromConst("hello")})
	ctx := &Ctx{Context: context.Background()}
	got, err := resolveSource(ctx, src, struct{}{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != "HELLO" {
		t.Fatalf("expected 'HELLO', got %v", got)
	}
}

func TestResolveSource_FnCallNotRegisteredReturnsError(t *testing.T) {
	resetBindingFnRegistry()
	src := FromFn("missing", nil)
	ctx := &Ctx{Context: context.Background()}
	_, err := resolveSource(ctx, src, struct{}{})
	if err == nil {
		t.Fatalf("expected error for unregistered fn")
	}
	// Error message names the fn so callers can diagnose.
	if !contains(err.Error(), "@fn.missing") {
		t.Fatalf("expected error to mention @fn.missing, got %v", err)
	}
}

// --- handler-registry bridge (the framework-wide @fn binding bug) ---
//
// A `@fn.X(...)` binding lowers to FromFn("X", ...) with the BARE name,
// but pilots register handlers under the FEATURE-QUALIFIED key via
// RegisterFn("<feature>.X", fn). These tests prove the binding path
// reaches the handler registry across that naming mismatch, with both
// zero-arg and one-arg call shapes and a concrete (non-`any`) return.

func TestResolveSource_FnCall_FallsBackToHandlerRegistry_OneArg(t *testing.T) {
	resetBindingFnRegistry()
	resetHandlerRegistry()
	// Mirror pauta's hash_password: input is `any`, returns `(any, error)`.
	RegisterFn("account.hash_password", func(ctx *Ctx, input any) (any, error) {
		s, _ := input.(string)
		return "phc$" + s, nil
	})

	src := FromFn("hash_password", []Source{FromConst("secret")})
	ctx := &Ctx{Context: context.Background()}
	got, err := resolveSource(ctx, src, struct{}{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != "phc$secret" {
		t.Fatalf("expected 'phc$secret', got %v", got)
	}
}

func TestResolveSource_FnCall_FallsBackToHandlerRegistry_ZeroArg_ConcreteReturn(t *testing.T) {
	resetBindingFnRegistry()
	resetHandlerRegistry()
	// Mirror pauta's placeholder_tenant_id: zero-arg binding, concrete
	// int64 return that must box to `any` and survive as a bigint value.
	const sentinel int64 = 0
	RegisterFn("account.placeholder_tenant_id", func(ctx *Ctx, input any) (int64, error) {
		_ = input
		return sentinel, nil
	})

	src := FromFn("placeholder_tenant_id", nil) // zero-arg
	ctx := &Ctx{Context: context.Background()}
	got, err := resolveSource(ctx, src, struct{}{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	boxed, ok := got.(int64)
	if !ok {
		t.Fatalf("expected boxed int64, got %T (%v)", got, got)
	}
	if boxed != sentinel {
		t.Fatalf("expected %d, got %d", sentinel, boxed)
	}
}

func TestResolveSource_FnCall_BindingRegistryWinsOverHandler(t *testing.T) {
	resetBindingFnRegistry()
	resetHandlerRegistry()
	RegisterFn("account.dup", func(ctx *Ctx, input any) (any, error) {
		return "handler", nil
	})
	RegisterBindingFn("dup", func(ctx context.Context, args ...any) (any, error) {
		return "binding", nil
	})
	src := FromFn("dup", nil)
	ctx := &Ctx{Context: context.Background()}
	got, err := resolveSource(ctx, src, struct{}{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != "binding" {
		t.Fatalf("explicit binding fn must win; got %v", got)
	}
}

func TestResolveSource_FnCall_AmbiguousSuffixFailsClosed(t *testing.T) {
	resetBindingFnRegistry()
	resetHandlerRegistry()
	RegisterFn("account.slug", func(ctx *Ctx, input any) (any, error) {
		return "a", nil
	})
	RegisterFn("billing.slug", func(ctx *Ctx, input any) (any, error) {
		return "b", nil
	})
	src := FromFn("slug", nil)
	ctx := &Ctx{Context: context.Background()}
	_, err := resolveSource(ctx, src, struct{}{})
	if err == nil {
		t.Fatal("expected fail-closed error for ambiguous suffix match")
	}
	if !contains(err.Error(), "@fn.slug") {
		t.Fatalf("error should name the fn; got %v", err)
	}
}

func TestResolveSource_FnCall_NotInEitherRegistry(t *testing.T) {
	resetBindingFnRegistry()
	resetHandlerRegistry()
	src := FromFn("ghost", nil)
	ctx := &Ctx{Context: context.Background()}
	_, err := resolveSource(ctx, src, struct{}{})
	if err == nil {
		t.Fatal("expected error when fn is in neither registry")
	}
	if !contains(err.Error(), "either registry") {
		t.Fatalf("error should mention both registries; got %v", err)
	}
	if !contains(err.Error(), "@fn.ghost") {
		t.Fatalf("error should name the fn; got %v", err)
	}
}

func contains(haystack, needle string) bool {
	if len(needle) == 0 {
		return true
	}
	for i := 0; i+len(needle) <= len(haystack); i++ {
		if haystack[i:i+len(needle)] == needle {
			return true
		}
	}
	return false
}
