package jobs

import (
	"context"
	"encoding/json"
	"errors"
	"sync/atomic"
	"testing"
	"time"

	"github.com/riverqueue/river"
	"github.com/riverqueue/river/rivertype"
)

// fakeRiver captures Insert calls so the dispatcher's wire can be
// asserted without standing up a Postgres-backed River client.
type fakeRiver struct {
	calls []fakeInsertCall
}

type fakeInsertCall struct {
	args river.JobArgs
	opts *river.InsertOpts
}

func (f *fakeRiver) Insert(_ context.Context, args river.JobArgs, opts *river.InsertOpts) (*rivertype.JobInsertResult, error) {
	f.calls = append(f.calls, fakeInsertCall{args: args, opts: opts})
	return &rivertype.JobInsertResult{}, nil
}

func TestDispatchJobInlineHappyPath(t *testing.T) {
	t.Parallel()
	contract := JobContract{Feature: "customer", Name: "send_welcome"}
	called := atomic.Bool{}
	handler := func(ctx context.Context, env JobEnvelope) error {
		called.Store(true)
		return nil
	}
	if err := DispatchJob(context.Background(), contract, JobEnvelope{}, handler); err != nil {
		t.Fatalf("DispatchJob: %v", err)
	}
	if !called.Load() {
		t.Fatalf("handler was not invoked")
	}
}

func TestDispatchJobReturnsMaxRetries(t *testing.T) {
	t.Parallel()
	contract := JobContract{
		Feature: "customer",
		Name:    "always_fails",
		Retry:   &RetryPolicy{Count: 0, Backoff: BackoffFixed},
	}
	handler := func(_ context.Context, _ JobEnvelope) error {
		return errors.New("boom")
	}
	err := DispatchJob(context.Background(), contract, JobEnvelope{}, handler)
	if !errors.Is(err, ErrJobMaxRetries) {
		t.Fatalf("expected ErrJobMaxRetries, got %v", err)
	}
}

func TestDispatchJobHonorsTimeout(t *testing.T) {
	t.Parallel()
	contract := JobContract{
		Feature: "customer",
		Name:    "slow",
		Timeout: 25 * time.Millisecond,
	}
	handler := func(ctx context.Context, _ JobEnvelope) error {
		select {
		case <-time.After(5 * time.Second):
			return nil
		case <-ctx.Done():
			return ctx.Err()
		}
	}
	err := DispatchJob(context.Background(), contract, JobEnvelope{}, handler)
	if !errors.Is(err, ErrJobTimeout) && !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("expected ErrJobTimeout or DeadlineExceeded, got %v", err)
	}
}

func TestRegisterJobsAlignedSlices(t *testing.T) {
	t.Parallel()
	disp := NewRiverDispatcher()
	contracts := []JobContract{
		{Feature: "customer", Name: "a"},
		{Feature: "customer", Name: "b"},
	}
	handlers := []HandlerFunc{
		func(context.Context, JobEnvelope) error { return nil },
		func(context.Context, JobEnvelope) error { return nil },
	}
	if err := RegisterJobs(disp, contracts, handlers); err != nil {
		t.Fatalf("RegisterJobs: %v", err)
	}
	if disp.lookup("customer.a") == nil || disp.lookup("customer.b") == nil {
		t.Fatalf("handler map not populated")
	}
}

func TestRegisterJobsMisalignedSlices(t *testing.T) {
	t.Parallel()
	disp := NewRiverDispatcher()
	err := RegisterJobs(disp,
		[]JobContract{{Feature: "f", Name: "a"}},
		[]HandlerFunc{},
	)
	if err == nil {
		t.Fatalf("expected error for misaligned slices")
	}
}

func TestRiverDispatcherEnqueueEvent(t *testing.T) {
	t.Parallel()
	fake := &fakeRiver{}
	disp := NewRiverDispatcher()
	disp.Client = fake

	contract := JobContract{
		Feature: "customer",
		Name:    "send_welcome",
		Queue:   "transactional",
		Retry:   &RetryPolicy{Count: 2, Backoff: BackoffExponential},
	}
	envelope := JobEnvelope{
		ID:      "env-1",
		Tenant:  "tenant-7",
		Payload: map[string]any{"customer_id": 42},
	}
	if err := disp.EnqueueEvent(context.Background(), contract, envelope); err != nil {
		t.Fatalf("EnqueueEvent: %v", err)
	}
	if len(fake.calls) != 1 {
		t.Fatalf("expected 1 Insert call, got %d", len(fake.calls))
	}
	call := fake.calls[0]
	if call.opts == nil {
		t.Fatal("expected InsertOpts, got nil")
	}
	if call.opts.Queue != "transactional" {
		t.Fatalf("opts.Queue = %q, want %q", call.opts.Queue, "transactional")
	}
	if call.opts.MaxAttempts != 3 {
		t.Fatalf("opts.MaxAttempts = %d, want 3 (retry count 2 + initial)", call.opts.MaxAttempts)
	}
	args, ok := call.args.(LazuliJobArgs)
	if !ok {
		t.Fatalf("expected LazuliJobArgs, got %T", call.args)
	}
	if args.JobKind != "customer.send_welcome" {
		t.Fatalf("Kind = %q, want %q", args.JobKind, "customer.send_welcome")
	}
	if args.Tenant != "tenant-7" {
		t.Fatalf("Tenant = %q, want %q", args.Tenant, "tenant-7")
	}
	// Payload is marshalled JSON.
	var got map[string]any
	if err := json.Unmarshal(args.Payload, &got); err != nil {
		t.Fatalf("unmarshal Payload: %v", err)
	}
	if got["customer_id"].(float64) != 42 {
		t.Fatalf("Payload customer_id = %v, want 42", got["customer_id"])
	}
}

func TestRiverDispatcherRegisterHandlerDeduplicates(t *testing.T) {
	t.Parallel()
	disp := NewRiverDispatcher()
	contract := JobContract{Feature: "f", Name: "j"}
	handler := func(context.Context, JobEnvelope) error { return nil }
	if err := disp.RegisterHandler(contract, handler); err != nil {
		t.Fatalf("first RegisterHandler: %v", err)
	}
	if err := disp.RegisterHandler(contract, handler); err == nil {
		t.Fatal("expected duplicate RegisterHandler to fail")
	}
}

func TestRiverDispatcherWorkerRoundTrip(t *testing.T) {
	t.Parallel()
	disp := NewRiverDispatcher()
	contract := JobContract{Feature: "billing", Name: "settle"}
	var captured JobEnvelope
	handler := func(_ context.Context, env JobEnvelope) error {
		captured = env
		return nil
	}
	if err := disp.RegisterHandler(contract, handler); err != nil {
		t.Fatalf("RegisterHandler: %v", err)
	}
	// Synthesize the args River would deliver to Work().
	payload, _ := json.Marshal(map[string]any{"invoice_id": 99})
	args := LazuliJobArgs{
		Feature: "billing", JobName: "settle", JobKind: "billing.settle",
		ID: "env-9", Tenant: "tenant-2", Payload: payload,
	}
	worker := &lazuliWorker{lookup: disp.lookup}
	if err := worker.Work(context.Background(), &river.Job[LazuliJobArgs]{Args: args}); err != nil {
		t.Fatalf("Work: %v", err)
	}
	if captured.ID != "env-9" || captured.Tenant != "tenant-2" {
		t.Fatalf("envelope not forwarded: %+v", captured)
	}
	if captured.Payload["invoice_id"].(float64) != 99 {
		t.Fatalf("payload not forwarded: %+v", captured.Payload)
	}
}

func TestNextDelayCatalogValues(t *testing.T) {
	t.Parallel()
	if NextDelay(RetryPolicy{Backoff: BackoffFixed}, 0) != 0 {
		t.Fatal("attempt 0 must be 0")
	}
	if d := NextDelay(RetryPolicy{Backoff: BackoffFixed}, 1); d != 5*time.Second {
		t.Fatalf("fixed @1 = %v, want 5s", d)
	}
	if d := NextDelay(RetryPolicy{Backoff: BackoffExponential}, 3); d != 20*time.Second {
		t.Fatalf("exponential @3 = %v, want 20s", d)
	}
	if d := NextDelay(RetryPolicy{Backoff: BackoffExponential}, 20); d != 5*time.Minute {
		t.Fatalf("exponential @20 (capped) = %v, want 5m", d)
	}
}

func TestShouldRetryBudget(t *testing.T) {
	t.Parallel()
	if ShouldRetry(nil, 1) {
		t.Fatal("nil policy must not retry")
	}
	policy := &RetryPolicy{Count: 3}
	if !ShouldRetry(policy, 2) {
		t.Fatal("attempt 2 must still retry against budget 3")
	}
	if ShouldRetry(policy, 3) {
		t.Fatal("attempt 3 must not retry once budget hit")
	}
}
