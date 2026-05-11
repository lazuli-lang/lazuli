// Package jobs — dispatch surface. Concrete queue adapters (River,
// Asynq) bind to this surface via `@adapter.queue.*` resolution.
//
// Phase L Tier 3 / row 33 stubs. Full implementation lands with the
// runtime team; the signatures here commit to the contract shape the
// generated code will call.
package jobs

import (
	"context"
	"errors"
)

// Dispatcher is the runtime adapter surface for enqueuing and running
// jobs. The River adapter (`@runtime/river`) and Asynq adapter
// (`@runtime/asynq`) bind their River/Asynq client behind this
// interface. Codegen depends only on `Dispatcher`; the concrete client
// is resolved at boot.
type Dispatcher interface {
	// EnqueueEvent dispatches a reactor or queued-worker job in
	// response to an emitted event. `Contract.Queue` selects the
	// lane.
	EnqueueEvent(ctx context.Context, contract JobContract, envelope JobEnvelope) error
	// EnqueueSchedule registers a scheduled job with the underlying
	// queue's scheduler. Called once per boot per scheduled job.
	EnqueueSchedule(ctx context.Context, contract JobContract) error
	// RegisterHandler binds a handler callback to a contract. The
	// generated `RegisterJobs(...)` function calls this once per job
	// at boot.
	RegisterHandler(contract JobContract, handler HandlerFunc) error
}

// DispatchJob runs one job inline. Used by the no-queue reactor path
// and by tests. Honors the contract's timeout (wrapping `ctx`) and
// returns ErrJobTimeout / ErrJobMaxRetries appropriately.
//
// Stub: full implementation (River timeout wrapping, panic recovery,
// retry budget enforcement) lands with the runtime team.
func DispatchJob(
	ctx context.Context,
	contract JobContract,
	envelope JobEnvelope,
	handler HandlerFunc,
) error {
	_ = ctx
	_ = contract
	_ = envelope
	_ = handler
	// TODO(runtime): wrap ctx with contract.Timeout, dispatch handler,
	// classify error against contract.Retry, surface
	// observability spans + audit events.
	return errors.New("jobs: DispatchJob not yet implemented")
}

// RegisterJobs is the generated entry point Lazuli codegen emits.
// Codegen produces:
//
//	jobs.RegisterJobs(disp, customerJobContracts, customerJobHandlers)
//
// where the two slices are aligned (same length, same order). Stub
// returns the first registration error.
func RegisterJobs(
	disp Dispatcher,
	contracts []JobContract,
	handlers []HandlerFunc,
) error {
	if len(contracts) != len(handlers) {
		return errors.New("jobs: contract/handler slices misaligned")
	}
	for i, contract := range contracts {
		if err := disp.RegisterHandler(contract, handlers[i]); err != nil {
			return err
		}
	}
	return nil
}
