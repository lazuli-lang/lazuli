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
