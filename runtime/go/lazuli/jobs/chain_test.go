package jobs

import (
	"context"
	"errors"
	"testing"
)

type fakeChainStepDispatcher struct {
	calls []ChainStepInput
	fn    func(context.Context, ChainStepInput) (any, error)
}

func (d *fakeChainStepDispatcher) DispatchChainStep(ctx context.Context, input ChainStepInput) (any, error) {
	d.calls = append(d.calls, input)
	return d.fn(ctx, input)
}

func TestDispatchChainPropagatesStepResults(t *testing.T) {
	t.Parallel()
	chain := ChainDefinition{
		Feature: "customer",
		Name:    "onboard",
		Steps: []ChainStep{
			{
				Name:     "load_customer",
				Contract: JobContract{Feature: "customer", Name: "load_customer"},
			},
			{
				Name:     "send_email",
				Contract: JobContract{Feature: "customer", Name: "send_email"},
			},
		},
	}
	dispatcher := &fakeChainStepDispatcher{
		fn: func(_ context.Context, input ChainStepInput) (any, error) {
			switch input.Index {
			case 0:
				return map[string]any{"customer_id": "cus_123"}, nil
			case 1:
				last, ok := input.LastResult()
				if !ok {
					t.Fatal("second step did not receive previous result")
				}
				if last.StepName != "load_customer" {
					t.Fatalf("last StepName = %q, want %q", last.StepName, "load_customer")
				}
				payload, ok := last.Value.(map[string]any)
				if !ok {
					t.Fatalf("last Value = %T, want map[string]any", last.Value)
				}
				if payload["customer_id"] != "cus_123" {
					t.Fatalf("customer_id = %v, want cus_123", payload["customer_id"])
				}
				found, ok := input.PreviousResult("load_customer")
				if !ok || found.Value == nil {
					t.Fatal("PreviousResult did not find load_customer result")
				}
				return "sent", nil
			default:
				t.Fatalf("unexpected step index %d", input.Index)
				return nil, nil
			}
		},
	}

	results, err := DispatchChain(context.Background(), chain, dispatcher)
	if err != nil {
		t.Fatalf("DispatchChain: %v", err)
	}
	if len(results) != 2 {
		t.Fatalf("len(results) = %d, want 2", len(results))
	}
	if results[1].Value != "sent" {
		t.Fatalf("second result Value = %v, want sent", results[1].Value)
	}
	if len(dispatcher.calls[1].Previous) != 1 {
		t.Fatalf("second step Previous len = %d, want 1", len(dispatcher.calls[1].Previous))
	}
}

func TestDispatchChainStopsOnErrorByDefault(t *testing.T) {
	t.Parallel()
	boom := errors.New("boom")
	chain := ChainDefinition{
		Name: "stop",
		Steps: []ChainStep{
			{Name: "first", Contract: JobContract{Feature: "f", Name: "first"}},
			{Name: "second", Contract: JobContract{Feature: "f", Name: "second"}},
		},
	}
	dispatcher := &fakeChainStepDispatcher{
		fn: func(_ context.Context, input ChainStepInput) (any, error) {
			if input.Index == 0 {
				return nil, boom
			}
			t.Fatalf("unexpected dispatch of step %d", input.Index)
			return nil, nil
		},
	}

	results, err := DispatchChain(context.Background(), chain, dispatcher)
	if !errors.Is(err, boom) {
		t.Fatalf("error = %v, want boom", err)
	}
	if len(results) != 1 {
		t.Fatalf("len(results) = %d, want 1", len(results))
	}
	if !results[0].Failed() {
		t.Fatal("first result should be failed")
	}
	if len(dispatcher.calls) != 1 {
		t.Fatalf("calls = %d, want 1", len(dispatcher.calls))
	}
}

func TestDispatchChainContinuesOnErrorWhenConfigured(t *testing.T) {
	t.Parallel()
	boom := errors.New("boom")
	chain := ChainDefinition{
		Name:    "continue",
		OnError: ChainContinueOnError,
		Steps: []ChainStep{
			{Name: "first", Contract: JobContract{Feature: "f", Name: "first"}},
			{Name: "second", Contract: JobContract{Feature: "f", Name: "second"}},
		},
	}
	dispatcher := &fakeChainStepDispatcher{
		fn: func(_ context.Context, input ChainStepInput) (any, error) {
			if input.Index == 0 {
				return nil, boom
			}
			previous, ok := input.PreviousResult("first")
			if !ok {
				t.Fatal("second step did not receive failed first result")
			}
			if !previous.Failed() {
				t.Fatal("first result should be marked failed")
			}
			return "second-ok", nil
		},
	}

	results, err := DispatchChain(context.Background(), chain, dispatcher)
	if err != nil {
		t.Fatalf("DispatchChain: %v", err)
	}
	if len(results) != 2 {
		t.Fatalf("len(results) = %d, want 2", len(results))
	}
	if !errors.Is(results[0].Err, boom) {
		t.Fatalf("first result Err = %v, want boom", results[0].Err)
	}
	if results[1].Value != "second-ok" {
		t.Fatalf("second result Value = %v, want second-ok", results[1].Value)
	}
}

func TestDispatchChainRejectsInvalidPolicy(t *testing.T) {
	t.Parallel()
	dispatcher := &fakeChainStepDispatcher{
		fn: func(context.Context, ChainStepInput) (any, error) {
			t.Fatal("dispatcher should not be called")
			return nil, nil
		},
	}

	_, err := DispatchChain(context.Background(), ChainDefinition{OnError: ChainErrorPolicy("retry_forever")}, dispatcher)
	if err == nil {
		t.Fatal("expected invalid policy error")
	}
}
