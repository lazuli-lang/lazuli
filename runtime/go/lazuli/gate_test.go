package lazuli

import (
	"errors"
	"testing"
)

func TestRunPreludeEmptyIsNoop(t *testing.T) {
	if err := RunPrelude(&Ctx{}, nil); err != nil {
		t.Fatalf("nil prelude must be a no-op, got %v", err)
	}
	if err := RunPrelude(&Ctx{}, []GateRef{}); err != nil {
		t.Fatalf("empty prelude must be a no-op, got %v", err)
	}
}

func TestRunPreludeDelegatesToRegisteredRunner(t *testing.T) {
	prev := PreludeRunner
	t.Cleanup(func() { PreludeRunner = prev })

	sentinel := errors.New("forbidden")
	var sawPrelude []GateRef
	PreludeRunner = func(_ *Ctx, prelude []GateRef) error {
		sawPrelude = prelude
		return sentinel
	}

	prelude := []GateRef{{Kind: GateBehind, Name: "feature_a"}}
	if err := RunPrelude(&Ctx{}, prelude); !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel error, got %v", err)
	}
	if len(sawPrelude) != 1 || sawPrelude[0].Name != "feature_a" {
		t.Fatalf("runner saw %+v", sawPrelude)
	}
}

func TestRunIncrementDelegatesToRegisteredRunner(t *testing.T) {
	prev := IncrementRunner
	t.Cleanup(func() { IncrementRunner = prev })

	called := 0
	IncrementRunner = func(_ *Ctx, _ []GateRef) error {
		called++
		return nil
	}

	prelude := []GateRef{{Kind: GateQuota, Name: "limit_a"}}
	if err := RunIncrement(&Ctx{}, prelude); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if called != 1 {
		t.Fatalf("expected increment runner called once, got %d", called)
	}
}

func TestApiInvokeRunsPreludeBeforeHandler(t *testing.T) {
	prev := PreludeRunner
	t.Cleanup(func() { PreludeRunner = prev })

	sentinel := errors.New("blocked")
	PreludeRunner = func(_ *Ctx, _ []GateRef) error { return sentinel }

	called := false
	api := Api[struct{}, string]{
		Name:    "test.api",
		Handler: func(_ *Ctx, _ struct{}) (string, error) { called = true; return "ok", nil },
		Prelude: []GateRef{{Kind: GateBehind, Name: "feature_a"}},
	}
	_, err := api.Invoke(&Ctx{}, struct{}{})
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel from prelude, got %v", err)
	}
	if called {
		t.Fatal("handler must not be invoked when prelude refuses")
	}
}

func TestApiInvokePassesPreludeRunsHandler(t *testing.T) {
	prev := PreludeRunner
	prevIncr := IncrementRunner
	t.Cleanup(func() {
		PreludeRunner = prev
		IncrementRunner = prevIncr
	})

	PreludeRunner = func(_ *Ctx, _ []GateRef) error { return nil }
	incrCalls := 0
	IncrementRunner = func(_ *Ctx, _ []GateRef) error { incrCalls++; return nil }

	api := Api[struct{}, string]{
		Name:    "test.api",
		Handler: func(_ *Ctx, _ struct{}) (string, error) { return "ok", nil },
		Prelude: []GateRef{{Kind: GateQuota, Name: "limit_a"}},
	}
	got, err := api.Invoke(&Ctx{}, struct{}{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != "ok" {
		t.Fatalf("got %q", got)
	}
	if incrCalls != 1 {
		t.Fatalf("expected one post-success increment, got %d", incrCalls)
	}
}
