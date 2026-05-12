package jobs

import (
	"context"
	"errors"
	"fmt"
)

// ChainErrorPolicy controls how a chain reacts when one step fails.
type ChainErrorPolicy string

const (
	// ChainStopOnError stops execution at the first failed step.
	ChainStopOnError ChainErrorPolicy = "stop_on_error"
	// ChainContinueOnError records failed steps and continues with later steps.
	ChainContinueOnError ChainErrorPolicy = "continue_on_error"
)

// ChainDefinition is the lowered shape for an ordered job chain.
type ChainDefinition struct {
	Feature string
	Name    string
	Steps   []ChainStep
	OnError ChainErrorPolicy
}

// ChainStep is one dispatchable job in a chain.
type ChainStep struct {
	Name     string
	Contract JobContract
	Envelope JobEnvelope
}

// ChainStepInput is the adapter-neutral payload passed to each chain step.
type ChainStepInput struct {
	Chain    ChainDefinition
	Index    int
	Step     ChainStep
	Previous []ChainStepResult
}

// LastResult returns the most recent completed step result.
func (i ChainStepInput) LastResult() (ChainStepResult, bool) {
	if len(i.Previous) == 0 {
		return ChainStepResult{}, false
	}
	return i.Previous[len(i.Previous)-1], true
}

// PreviousResult returns the most recent result with the given step name.
func (i ChainStepInput) PreviousResult(stepName string) (ChainStepResult, bool) {
	for idx := len(i.Previous) - 1; idx >= 0; idx-- {
		if i.Previous[idx].StepName == stepName {
			return i.Previous[idx], true
		}
	}
	return ChainStepResult{}, false
}

// ChainStepResult records the outcome of a completed chain step.
type ChainStepResult struct {
	Index    int
	StepName string
	Contract JobContract
	Envelope JobEnvelope
	Value    any
	Err      error
}

// Failed reports whether the step returned an error.
func (r ChainStepResult) Failed() bool {
	return r.Err != nil
}

// ChainStepDispatcher is the adapter-neutral surface for running or enqueuing a chain step.
type ChainStepDispatcher interface {
	DispatchChainStep(ctx context.Context, input ChainStepInput) (any, error)
}

// DispatchChain dispatches each chain step in order, passing prior results to later steps.
func DispatchChain(ctx context.Context, chain ChainDefinition, dispatcher ChainStepDispatcher) ([]ChainStepResult, error) {
	if dispatcher == nil {
		return nil, errors.New("jobs: chain dispatcher is nil")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if err := validateChainErrorPolicy(chain.OnError); err != nil {
		return nil, err
	}

	results := make([]ChainStepResult, 0, len(chain.Steps))
	for idx, step := range chain.Steps {
		input := ChainStepInput{
			Chain:    chain,
			Index:    idx,
			Step:     step,
			Previous: append([]ChainStepResult(nil), results...),
		}
		value, err := dispatcher.DispatchChainStep(ctx, input)
		result := ChainStepResult{
			Index:    idx,
			StepName: step.Name,
			Contract: step.Contract,
			Envelope: step.Envelope,
			Value:    value,
			Err:      err,
		}
		results = append(results, result)
		if err != nil && chain.stopOnError() {
			return results, fmt.Errorf("jobs: chain %q step %q failed: %w", chain.Name, step.Name, err)
		}
	}
	return results, nil
}

func (c ChainDefinition) stopOnError() bool {
	return c.OnError == "" || c.OnError == ChainStopOnError
}

func validateChainErrorPolicy(policy ChainErrorPolicy) error {
	switch policy {
	case "", ChainStopOnError, ChainContinueOnError:
		return nil
	default:
		return fmt.Errorf("jobs: unknown chain error policy %q", policy)
	}
}
