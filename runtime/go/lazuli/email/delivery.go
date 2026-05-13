package email

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"
)

var (
	// ErrInvalidDeliveryJob is returned for invalid provider-neutral delivery jobs.
	ErrInvalidDeliveryJob = errors.New("email: invalid delivery job")
	// ErrDeliveryJobNotFound is returned when an outbox lookup cannot find a job.
	ErrDeliveryJobNotFound = errors.New("email: delivery job not found")
)

// Sender delivers one provider-neutral Message. Implementations may wrap SMTP,
// HTTP APIs, or in-memory fakes; this package never performs network I/O.
type Sender interface {
	Send(ctx context.Context, message Message) (DeliveryReceipt, error)
}

// DeliveryReceipt is the provider-neutral result returned by a Sender.
type DeliveryReceipt struct {
	// ProviderMessageID is an optional stable provider identifier.
	ProviderMessageID string
	// StatusCode is an optional HTTP-style provider status code.
	StatusCode int
}

// DeliveryJobStatus is the stored state of a provider-neutral delivery job.
type DeliveryJobStatus string

const (
	DeliveryJobQueued DeliveryJobStatus = "queued"
	DeliveryJobSent   DeliveryJobStatus = "sent"
	DeliveryJobRetry  DeliveryJobStatus = "retry"
	DeliveryJobFailed DeliveryJobStatus = "failed"
)

// DeliveryJob is one message plus deterministic delivery orchestration state.
type DeliveryJob struct {
	ID      string
	Message Message

	Status  DeliveryJobStatus
	Attempt int

	ProviderMessageID string
	LastStatusCode    int
	LastError         string
	FailureClass      DeliveryFailureClass
	NextDelay         time.Duration
}

// NewDeliveryJob validates message and returns a queued delivery job.
func NewDeliveryJob(id string, message Message, limits MessageLimits) (DeliveryJob, error) {
	id, err := normalizeDeliveryJobID(id)
	if err != nil {
		return DeliveryJob{}, err
	}
	if err := ValidateMessage(message, limits); err != nil {
		return DeliveryJob{}, fmt.Errorf("%w: %w", ErrInvalidDeliveryJob, err)
	}
	return DeliveryJob{
		ID:      id,
		Message: message,
		Status:  DeliveryJobQueued,
	}, nil
}

// InMemoryOutbox stores delivery jobs in insertion order for deterministic
// tests and local orchestration. It is safe for concurrent use.
type InMemoryOutbox struct {
	mu    sync.Mutex
	jobs  []DeliveryJob
	index map[string]int
}

// Add inserts a validated job. Job IDs must be unique.
func (o *InMemoryOutbox) Add(job DeliveryJob) error {
	normalized, err := normalizeDeliveryJob(job)
	if err != nil {
		return err
	}

	o.mu.Lock()
	defer o.mu.Unlock()

	if o.index == nil {
		o.index = make(map[string]int)
	}
	if _, exists := o.index[normalized.ID]; exists {
		return deliveryJobInvalidf("duplicate job id %q", normalized.ID)
	}
	o.index[normalized.ID] = len(o.jobs)
	o.jobs = append(o.jobs, normalized)
	return nil
}

// Snapshot returns jobs in insertion order.
func (o *InMemoryOutbox) Snapshot() []DeliveryJob {
	o.mu.Lock()
	defer o.mu.Unlock()

	return cloneDeliveryJobs(o.jobs)
}

// Get returns one job by ID.
func (o *InMemoryOutbox) Get(id string) (DeliveryJob, error) {
	id, err := normalizeDeliveryJobID(id)
	if err != nil {
		return DeliveryJob{}, err
	}

	o.mu.Lock()
	defer o.mu.Unlock()

	i, ok := o.index[id]
	if !ok {
		return DeliveryJob{}, ErrDeliveryJobNotFound
	}
	return o.jobs[i], nil
}

// DispatchOptions configures one deterministic outbox dispatch pass.
type DispatchOptions struct {
	RetrySchedule RetrySchedule
}

// DispatchSummary summarizes a dispatch pass in job order.
type DispatchSummary struct {
	Total     int
	Attempted int
	Sent      int
	Retry     int
	Failed    int
	Skipped   int
	Results   []DeliveryResult
}

// DeliveryResult records the result for one job in a dispatch pass.
type DeliveryResult struct {
	JobID string

	Attempt int
	Status  DeliveryJobStatus

	ProviderMessageID string
	StatusCode        int
	Error             string
	FailureClass      DeliveryFailureClass
	NextDelay         time.Duration
}

// Dispatch sends each queued or retryable job once and updates the outbox with
// deterministic retry/backoff decisions. Jobs already sent or failed are
// skipped.
func Dispatch(ctx context.Context, outbox *InMemoryOutbox, sender Sender, opts DispatchOptions) DispatchSummary {
	schedule := opts.RetrySchedule.Normalize()

	outbox.mu.Lock()
	defer outbox.mu.Unlock()

	summary := DispatchSummary{Total: len(outbox.jobs)}
	for i := range outbox.jobs {
		job := &outbox.jobs[i]
		if job.Status != DeliveryJobQueued && job.Status != DeliveryJobRetry {
			summary.Skipped++
			summary.Results = append(summary.Results, deliveryResultFromJob(*job))
			continue
		}

		summary.Attempted++
		attempt := job.Attempt + 1
		receipt, err := sender.Send(ctx, job.Message)
		env := NewDeliveryAttemptEnvelope(attempt, receipt.StatusCode, err, schedule)

		job.Attempt = env.Attempt
		job.ProviderMessageID = receipt.ProviderMessageID
		job.LastStatusCode = receipt.StatusCode
		job.LastError = errorString(err)
		job.FailureClass = env.FailureClass
		job.NextDelay = env.NextDelay
		switch {
		case env.Successful():
			job.Status = DeliveryJobSent
			summary.Sent++
		case env.ShouldRetry():
			job.Status = DeliveryJobRetry
			summary.Retry++
		default:
			job.Status = DeliveryJobFailed
			summary.Failed++
		}

		summary.Results = append(summary.Results, deliveryResultFromJob(*job))
	}
	return summary
}

func normalizeDeliveryJob(job DeliveryJob) (DeliveryJob, error) {
	id, err := normalizeDeliveryJobID(job.ID)
	if err != nil {
		return DeliveryJob{}, err
	}
	if job.Status == "" {
		job.Status = DeliveryJobQueued
	}
	switch job.Status {
	case DeliveryJobQueued, DeliveryJobRetry:
	case DeliveryJobSent, DeliveryJobFailed:
		return DeliveryJob{}, deliveryJobInvalidf("job %q cannot be added with terminal status %q", id, job.Status)
	default:
		return DeliveryJob{}, deliveryJobInvalidf("job %q has invalid status %q", id, job.Status)
	}
	if job.Attempt < 0 {
		return DeliveryJob{}, deliveryJobInvalidf("job %q attempt must be non-negative", id)
	}
	if err := ValidateMessage(job.Message, MessageLimits{}); err != nil {
		return DeliveryJob{}, fmt.Errorf("%w: %w", ErrInvalidDeliveryJob, err)
	}
	job.ID = id
	return job, nil
}

func normalizeDeliveryJobID(id string) (string, error) {
	id = strings.TrimSpace(id)
	if id == "" {
		return "", deliveryJobInvalidf("id is required")
	}
	if containsControl(id) {
		return "", deliveryJobInvalidf("id contains control characters")
	}
	return id, nil
}

func deliveryResultFromJob(job DeliveryJob) DeliveryResult {
	return DeliveryResult{
		JobID:             job.ID,
		Attempt:           job.Attempt,
		Status:            job.Status,
		ProviderMessageID: job.ProviderMessageID,
		StatusCode:        job.LastStatusCode,
		Error:             job.LastError,
		FailureClass:      job.FailureClass,
		NextDelay:         job.NextDelay,
	}
}

func cloneDeliveryJobs(jobs []DeliveryJob) []DeliveryJob {
	if len(jobs) == 0 {
		return nil
	}
	cloned := make([]DeliveryJob, len(jobs))
	copy(cloned, jobs)
	return cloned
}

func errorString(err error) string {
	if err == nil {
		return ""
	}
	return err.Error()
}

func deliveryJobInvalidf(format string, args ...any) error {
	return fmt.Errorf("%w: "+format, append([]any{ErrInvalidDeliveryJob}, args...)...)
}
