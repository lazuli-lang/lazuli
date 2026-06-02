package lazuli

import (
	"errors"
	"strings"
	"testing"
)

// resetHandlerRegistry clears the registry for test isolation. Tests
// that touch the global must call this in their setup.
func resetHandlerRegistry() {
	handlerMu.Lock()
	defer handlerMu.Unlock()
	handlerRegistry = map[string]any{}
}

func TestRegisterFn_RoundTrip(t *testing.T) {
	resetHandlerRegistry()
	called := false
	RegisterFn("acct.echo", func(ctx *Ctx, in string) (string, error) {
		called = true
		return in + "!", nil
	})

	effect := ReturnsFromRegistry[string, string]("acct.echo")
	out, err := effect.Handler(&Ctx{}, "hi")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !called {
		t.Fatal("registered handler was not invoked")
	}
	if out != "hi!" {
		t.Fatalf("unexpected output: %q", out)
	}
}

func TestReturnsFromRegistry_MissingHandlerReturnsInternalError(t *testing.T) {
	resetHandlerRegistry()
	effect := ReturnsFromRegistry[string, string]("acct.missing")
	_, err := effect.Handler(&Ctx{}, "anything")
	if err == nil {
		t.Fatal("expected error for missing handler")
	}
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("expected *lazuli.Error, got %T", err)
	}
	if le.Status != 500 || le.Code != CodeInternal {
		t.Fatalf("expected 500 CodeInternal, got %d %s", le.Status, le.Code)
	}
	if !strings.Contains(le.Message, `"acct.missing"`) {
		t.Fatalf("error should name the unresolved handler; got %q", le.Message)
	}
}

func TestReturnsFromRegistry_WrongSignatureReturnsInternalError(t *testing.T) {
	resetHandlerRegistry()
	// Register a handler with input type `int` but resolve as `string`.
	RegisterFn("acct.misaligned", func(ctx *Ctx, in int) (string, error) {
		return "", nil
	})
	effect := ReturnsFromRegistry[string, string]("acct.misaligned")
	_, err := effect.Handler(&Ctx{}, "hi")
	if err == nil {
		t.Fatal("expected signature-mismatch error")
	}
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("expected *lazuli.Error, got %T", err)
	}
	if le.Status != 500 || le.Code != CodeInternal {
		t.Fatalf("expected 500 CodeInternal, got %d %s", le.Status, le.Code)
	}
	if !strings.Contains(le.Message, "wrong signature") {
		t.Fatalf("error should mention signature mismatch; got %q", le.Message)
	}
}

func TestReturnsFromRegistry_WrongInputTypeReturnsInternalError(t *testing.T) {
	resetHandlerRegistry()
	// Handler registered with correct signature, but dispatched input is
	// the wrong type — defensive check in the runtime, not user error.
	RegisterFn("acct.echo", func(ctx *Ctx, in string) (string, error) {
		return in, nil
	})
	effect := ReturnsFromRegistry[string, string]("acct.echo")
	_, err := effect.Handler(&Ctx{}, 42) // int instead of string
	if err == nil {
		t.Fatal("expected input-type error")
	}
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("expected *lazuli.Error, got %T", err)
	}
	if !strings.Contains(le.Message, "wrong type") {
		t.Fatalf("error should mention input type; got %q", le.Message)
	}
}

func TestRegisterFn_LastRegistrationWins(t *testing.T) {
	resetHandlerRegistry()
	RegisterFn("acct.echo", func(ctx *Ctx, in string) (string, error) {
		return "first", nil
	})
	RegisterFn("acct.echo", func(ctx *Ctx, in string) (string, error) {
		return "second", nil
	})
	effect := ReturnsFromRegistry[string, string]("acct.echo")
	out, err := effect.Handler(&Ctx{}, "ignored")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if out != "second" {
		t.Fatalf("expected last-registration to win; got %q", out)
	}
}

func TestReturnsFromRegistry_PropagatesHandlerError(t *testing.T) {
	resetHandlerRegistry()
	sentinel := errors.New("handler boom")
	RegisterFn("acct.echo", func(ctx *Ctx, in string) (string, error) {
		return "", sentinel
	})
	effect := ReturnsFromRegistry[string, string]("acct.echo")
	_, err := effect.Handler(&Ctx{}, "hi")
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel error; got %v", err)
	}
}

// --- HandlerFromRegistry (api-surface bridge, W3) -------------------------

type meResult struct{ ID int }

// meApiArgs is a DISTINCT named empty struct, mirroring the codegen's
// generated `type MeApiArgs struct{}` for a no-path-param api. The
// shared handler is registered with the command surface's anonymous
// `struct{}` input — the exact case that broke the naive type assertion.
type meApiArgs struct{}

func TestHandlerFromRegistry_ExactSignature(t *testing.T) {
	resetHandlerRegistry()
	RegisterFn("account.me", func(ctx *Ctx, in meApiArgs) (meResult, error) {
		return meResult{ID: 7}, nil
	})
	h := HandlerFromRegistry[meApiArgs, meResult]("account.me")
	out, err := h(&Ctx{}, meApiArgs{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if out.ID != 7 {
		t.Fatalf("unexpected output: %+v", out)
	}
}

// The real pauta `api me` shape: the handler is registered ONCE under
// `account.me` with the command surface's anonymous `struct{}` input,
// but the api's generated Args type is the named `meApiArgs`. The exact
// assertion fails; the reflection fallback must still invoke it so the
// endpoint serves (200) instead of 500.
func TestHandlerFromRegistry_NamedEmptyStructFallback(t *testing.T) {
	resetHandlerRegistry()
	called := false
	RegisterFn("account.me", func(ctx *Ctx, in struct{}) (meResult, error) {
		called = true
		return meResult{ID: 42}, nil
	})
	h := HandlerFromRegistry[meApiArgs, meResult]("account.me")
	out, err := h(&Ctx{}, meApiArgs{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !called {
		t.Fatal("registered struct{} handler was not invoked via fallback")
	}
	if out.ID != 42 {
		t.Fatalf("unexpected output: %+v", out)
	}
}

func TestHandlerFromRegistry_MissingHandler(t *testing.T) {
	resetHandlerRegistry()
	h := HandlerFromRegistry[meApiArgs, meResult]("account.missing")
	_, err := h(&Ctx{}, meApiArgs{})
	var le *Error
	if !errors.As(err, &le) || le.Status != 500 {
		t.Fatalf("expected 500 lazuli.Error; got %v", err)
	}
	if !strings.Contains(le.Message, "no handler registered") {
		t.Fatalf("error should name the missing handler; got %q", le.Message)
	}
}

func TestHandlerFromRegistry_WrongOutputType(t *testing.T) {
	resetHandlerRegistry()
	// Registered output is `string`, api expects `meResult` — the
	// fallback must reject it (O mismatch) with a wrong-signature 500.
	RegisterFn("account.me", func(ctx *Ctx, in struct{}) (string, error) {
		return "nope", nil
	})
	h := HandlerFromRegistry[meApiArgs, meResult]("account.me")
	_, err := h(&Ctx{}, meApiArgs{})
	var le *Error
	if !errors.As(err, &le) || le.Status != 500 {
		t.Fatalf("expected 500 lazuli.Error; got %v", err)
	}
	if !strings.Contains(le.Message, "wrong signature") {
		t.Fatalf("error should mention signature mismatch; got %q", le.Message)
	}
}

func TestHandlerFromRegistry_PropagatesError(t *testing.T) {
	resetHandlerRegistry()
	sentinel := errors.New("boom")
	RegisterFn("account.me", func(ctx *Ctx, in struct{}) (meResult, error) {
		return meResult{}, sentinel
	})
	h := HandlerFromRegistry[meApiArgs, meResult]("account.me")
	_, err := h(&Ctx{}, meApiArgs{})
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel error; got %v", err)
	}
}
