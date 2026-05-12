// Package jobs — dispatch surface. Concrete queue adapters (River,
// Asynq) bind to this surface via `@adapter.queue.*` resolution.
package jobs

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
	"time"

	"github.com/riverqueue/river"
	"github.com/riverqueue/river/rivertype"
)

// Dispatcher is the runtime adapter surface for enqueuing and running
// jobs. The River adapter (`RiverDispatcher`, below) and any Asynq
// adapter bind their client behind this interface. Codegen depends
// only on `Dispatcher`; the concrete client is resolved at boot.
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

// LazuliJobArgs is the generic River JobArgs envelope every Lazuli
// job dispatches through. We keep one River kind per Lazuli
// `feature/name` pair (rather than one Go struct per kind) so the
// dispatcher doesn't have to know the closed catalog of contracts
// at compile time. River's typed-args ergonomics still apply
// transparently to the worker: each handler unmarshals the payload
// into its own struct.
type LazuliJobArgs struct {
	Feature  string            `json:"feature"`
	JobName  string            `json:"name"`
	JobKind  string            `json:"kind"`
	ID       string            `json:"id,omitempty"`
	Tenant   string            `json:"tenant,omitempty"`
	Payload  json.RawMessage   `json:"payload,omitempty"`
	Metadata map[string]string `json:"metadata,omitempty"`
}

// LazuliRiverKind is the single River "kind" all Lazuli jobs share.
// Per-contract dispatch happens inside `lazuliWorker.Work` based on
// `LazuliJobArgs.JobKind`. This trade keeps the wire one-Worker
// instead of generating a Go struct per contract (which would
// require code generation we explicitly want to avoid).
const LazuliRiverKind = "lazuli"

// Kind implements `river.JobArgs`. All Lazuli jobs share the same
// River kind so we can register one Worker globally; the dispatcher
// maps to per-contract handlers by `JobKind`.
func (LazuliJobArgs) Kind() string { return LazuliRiverKind }

// jobKindFor produces the unique river kind for a contract. Codegen
// holds the inverse mapping at boot so RegisterHandler can wire the
// right HandlerFunc.
func jobKindFor(contract JobContract) string {
	return contract.Feature + "." + contract.Name
}

// DispatchJob runs one job inline. Used by the no-queue reactor path
// and by tests. Honors the contract's timeout (wrapping `ctx`) and
// classifies handler error against `contract.Retry` returning
// `ErrJobMaxRetries` when the retry budget is exhausted, or
// `ErrJobTimeout` when the timeout elapses.
//
// Adapter-side retry (River's Worker.NextRetry / MaxAttempts) takes
// over once the contract is enqueued; this helper exists for the
// inline path and for tests.
func DispatchJob(
	ctx context.Context,
	contract JobContract,
	envelope JobEnvelope,
	handler HandlerFunc,
) error {
	if handler == nil {
		return errors.New("jobs: handler is nil")
	}
	if contract.Timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, contract.Timeout)
		defer cancel()
	}

	var attempts uint32 = 1
	if contract.Retry != nil {
		attempts = contract.Retry.Count + 1
	}

	var lastErr error
	for attempt := uint32(0); attempt < attempts; attempt++ {
		select {
		case <-ctx.Done():
			if errors.Is(ctx.Err(), context.DeadlineExceeded) {
				return ErrJobTimeout
			}
			return ctx.Err()
		default:
		}
		if attempt > 0 && contract.Retry != nil {
			delay := NextDelay(*contract.Retry, attempt)
			if delay > 0 {
				timer := time.NewTimer(delay)
				select {
				case <-ctx.Done():
					timer.Stop()
					if errors.Is(ctx.Err(), context.DeadlineExceeded) {
						return ErrJobTimeout
					}
					return ctx.Err()
				case <-timer.C:
				}
			}
		}
		err := handler(ctx, envelope)
		if err == nil {
			return nil
		}
		lastErr = err
	}
	if lastErr != nil && contract.Retry != nil {
		return fmt.Errorf("%w: last err %v", ErrJobMaxRetries, lastErr)
	}
	return lastErr
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

// --- River wire -------------------------------------------------------------

// RiverInserter is the narrow River surface RiverDispatcher consumes
// for enqueue. `*river.Client[T]` satisfies it; tests inject a fake.
type RiverInserter interface {
	Insert(ctx context.Context, args river.JobArgs, opts *river.InsertOpts) (*rivertype.JobInsertResult, error)
}

// RiverDispatcher wires the jobs.Dispatcher surface to a River
// client. The codegen instantiates one per app at boot, calls
// RegisterHandler for every contract, then `Start(ctx)` on the
// underlying client to begin processing.
//
// Wire scope: ~50 LOC of pass-through. River owns the queue, the
// retry policy, the DLQ, and the transactional Insert. We keep a
// handler map so EnqueueEvent → handler can dispatch by kind.
type RiverDispatcher struct {
	// Client is the live `*river.Client[T]`. Production passes
	// `*river.Client[pgx.Tx]` built with `riverpgxv5.New(dbPool)`.
	// We hold it through an interface to keep this file free of
	// the pgx import (which the `db` bucket owns).
	Client RiverInserter

	// Workers is the River workers bundle. Filled by RegisterHandler
	// the first time a contract is registered.
	Workers *river.Workers

	mu             sync.Mutex
	handlers       map[string]HandlerFunc
	workerInstalled bool
}

// NewRiverDispatcher returns a fresh dispatcher with an empty
// workers bundle. Pass the live River client after creating the
// dispatcher and registering the handlers (River requires Workers
// be passed in `river.Config` before the client is built; see the
// adapter package for the orchestration).
func NewRiverDispatcher() *RiverDispatcher {
	return &RiverDispatcher{
		Workers:  river.NewWorkers(),
		handlers: make(map[string]HandlerFunc),
	}
}

// EnqueueEvent inserts the contract+envelope into the River queue.
// `Contract.Queue` overrides the default lane; retry budget comes
// from `Contract.Retry.Count` translated to River's MaxAttempts.
func (d *RiverDispatcher) EnqueueEvent(ctx context.Context, contract JobContract, envelope JobEnvelope) error {
	if d.Client == nil {
		return errors.New("jobs: RiverDispatcher has no Client")
	}
	payload, err := json.Marshal(envelope.Payload)
	if err != nil {
		return err
	}
	args := LazuliJobArgs{
		Feature:  contract.Feature,
		JobName:  contract.Name,
		JobKind:  jobKindFor(contract),
		ID:       envelope.ID,
		Tenant:   envelope.Tenant,
		Payload:  payload,
		Metadata: envelope.Metadata,
	}
	opts := &river.InsertOpts{}
	if contract.Queue != "" {
		opts.Queue = contract.Queue
	}
	if contract.Retry != nil && contract.Retry.Count > 0 {
		// MaxAttempts counts the original run plus retries.
		opts.MaxAttempts = int(contract.Retry.Count) + 1
	}
	_, err = d.Client.Insert(ctx, args, opts)
	return err
}

// EnqueueSchedule binds a scheduled job to the River periodic-job
// service. Stub: River's periodic registration ergonomics
// (`PeriodicJobs` in `river.Config`) require contracts to be passed
// at Client construction time, not after Start. The codegen will
// gather scheduled contracts and hand them to the adapter's
// constructor; this hook stays for forward compatibility but is a
// no-op in v0.
func (d *RiverDispatcher) EnqueueSchedule(_ context.Context, contract JobContract) error {
	if contract.Trigger.Kind != "schedule" {
		return errors.New("jobs: EnqueueSchedule called on non-schedule trigger")
	}
	// River requires periodic jobs registered before Start. The
	// adapter package threads contract.Trigger.Cron into
	// river.Config.PeriodicJobs at boot.
	return nil
}

// RegisterHandler binds a handler callback to a contract. The first
// call also installs the single shared `lazuliWorker` into the
// River workers bundle. Subsequent calls only populate the
// per-contract handler map (River would panic on a duplicate
// `AddWorker` for the same kind).
func (d *RiverDispatcher) RegisterHandler(contract JobContract, handler HandlerFunc) error {
	kind := jobKindFor(contract)
	d.mu.Lock()
	if _, dup := d.handlers[kind]; dup {
		d.mu.Unlock()
		return fmt.Errorf("jobs: duplicate handler for kind %q", kind)
	}
	d.handlers[kind] = handler
	installWorker := !d.workerInstalled
	if installWorker {
		d.workerInstalled = true
	}
	d.mu.Unlock()
	if installWorker {
		river.AddWorker(d.Workers, &lazuliWorker{lookup: d.lookup})
	}
	return nil
}

func (d *RiverDispatcher) lookup(kind string) HandlerFunc {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.handlers[kind]
}

// lazuliWorker is the single River-side Worker shim that bridges a
// River-typed `*river.Job[LazuliJobArgs]` to the dispatcher's
// per-contract HandlerFunc map. River registers Workers by JobArgs
// type (one per kind); since every Lazuli job shares
// `LazuliRiverKind`, we install exactly one Worker and dispatch by
// the `JobKind` field stored on the JobArgs.
type lazuliWorker struct {
	river.WorkerDefaults[LazuliJobArgs]
	lookup func(string) HandlerFunc
}

// Work is the River entry point. It looks up the handler the
// dispatcher registered for the per-contract JobKind and forwards
// the typed envelope.
func (w *lazuliWorker) Work(ctx context.Context, job *river.Job[LazuliJobArgs]) error {
	handler := w.lookup(job.Args.JobKind)
	if handler == nil {
		return fmt.Errorf("jobs: no handler registered for kind %q", job.Args.JobKind)
	}
	var payload map[string]any
	if len(job.Args.Payload) > 0 {
		if err := json.Unmarshal(job.Args.Payload, &payload); err != nil {
			return err
		}
	}
	envelope := JobEnvelope{
		ID:       job.Args.ID,
		Tenant:   job.Args.Tenant,
		Payload:  payload,
		Metadata: job.Args.Metadata,
	}
	return handler(ctx, envelope)
}
