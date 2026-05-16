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
