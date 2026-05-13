package email

import (
	"context"
	"errors"
	"net/http"
	"testing"
	"time"
)

func TestNewDeliveryJobValidatesMessage(t *testing.T) {
	t.Parallel()

	message := deliveryTestMessage()
	job, err := NewDeliveryJob(" job-1 ", message, MessageLimits{MaxRecipients: 2})
	if err != nil {
		t.Fatalf("NewDeliveryJob() error = %v", err)
	}
	if job.ID != "job-1" {
		t.Fatalf("ID = %q, want trimmed job id", job.ID)
	}
	if job.Status != DeliveryJobQueued {
		t.Fatalf("Status = %q, want queued", job.Status)
	}

	message.Subject = ""
	_, err = NewDeliveryJob("job-2", message, MessageLimits{})
	if !errors.Is(err, ErrInvalidDeliveryJob) {
		t.Fatalf("NewDeliveryJob(invalid) error = %v, want ErrInvalidDeliveryJob", err)
	}
	if !errors.Is(err, ErrInvalidMessage) {
		t.Fatalf("NewDeliveryJob(invalid) error = %v, want ErrInvalidMessage", err)
	}
}

func TestInMemoryOutboxStoresJobsInInsertionOrder(t *testing.T) {
	t.Parallel()

	var outbox InMemoryOutbox
	first, err := NewDeliveryJob("a", deliveryTestMessage(), MessageLimits{})
	if err != nil {
		t.Fatalf("NewDeliveryJob(first): %v", err)
	}
	second, err := NewDeliveryJob("b", deliveryTestMessage(), MessageLimits{})
	if err != nil {
		t.Fatalf("NewDeliveryJob(second): %v", err)
	}
	if err := outbox.Add(first); err != nil {
		t.Fatalf("Add(first): %v", err)
	}
	if err := outbox.Add(second); err != nil {
		t.Fatalf("Add(second): %v", err)
	}
	if err := outbox.Add(second); !errors.Is(err, ErrInvalidDeliveryJob) {
		t.Fatalf("Add(duplicate) error = %v, want ErrInvalidDeliveryJob", err)
	}

	jobs := outbox.Snapshot()
	if len(jobs) != 2 {
		t.Fatalf("Snapshot length = %d, want 2", len(jobs))
	}
	if jobs[0].ID != "a" || jobs[1].ID != "b" {
		t.Fatalf("Snapshot IDs = %q, %q; want insertion order", jobs[0].ID, jobs[1].ID)
	}

	got, err := outbox.Get("b")
	if err != nil {
		t.Fatalf("Get(b): %v", err)
	}
	if got.ID != "b" {
		t.Fatalf("Get(b).ID = %q, want b", got.ID)
	}
	if _, err := outbox.Get("missing"); !errors.Is(err, ErrDeliveryJobNotFound) {
		t.Fatalf("Get(missing) error = %v, want ErrDeliveryJobNotFound", err)
	}
}

func TestDispatchSummarizesSuccessRetryAndPermanentFailure(t *testing.T) {
	t.Parallel()

	var outbox InMemoryOutbox
	for _, id := range []string{"sent", "retry", "failed"} {
		job, err := NewDeliveryJob(id, deliveryTestMessage(), MessageLimits{})
		if err != nil {
			t.Fatalf("NewDeliveryJob(%s): %v", id, err)
		}
		if err := outbox.Add(job); err != nil {
			t.Fatalf("Add(%s): %v", id, err)
		}
	}

	sender := sequenceSender{
		results: []sendResult{
			{receipt: DeliveryReceipt{ProviderMessageID: "provider-1", StatusCode: http.StatusAccepted}},
			{receipt: DeliveryReceipt{StatusCode: http.StatusServiceUnavailable}},
			{receipt: DeliveryReceipt{StatusCode: http.StatusBadRequest}, err: errors.New("bad payload")},
		},
	}
	summary := Dispatch(context.Background(), &outbox, &sender, DispatchOptions{
		RetrySchedule: RetrySchedule{
			MaxAttempts: 3,
			BaseDelay:   time.Second,
			MaxDelay:    10 * time.Second,
		},
	})

	if summary.Total != 3 || summary.Attempted != 3 || summary.Sent != 1 || summary.Retry != 1 || summary.Failed != 1 || summary.Skipped != 0 {
		t.Fatalf("summary = %+v, want one sent, one retry, one failed", summary)
	}
	if len(summary.Results) != 3 {
		t.Fatalf("Results length = %d, want 3", len(summary.Results))
	}
	if summary.Results[0].JobID != "sent" || summary.Results[0].Status != DeliveryJobSent || summary.Results[0].ProviderMessageID != "provider-1" {
		t.Fatalf("first result = %+v, want sent provider result", summary.Results[0])
	}
	if summary.Results[1].JobID != "retry" || summary.Results[1].Status != DeliveryJobRetry || summary.Results[1].NextDelay != time.Second {
		t.Fatalf("second result = %+v, want retry with 1s delay", summary.Results[1])
	}
	if summary.Results[2].JobID != "failed" || summary.Results[2].Status != DeliveryJobFailed || summary.Results[2].FailureClass != DeliveryFailurePermanent {
		t.Fatalf("third result = %+v, want permanent failure", summary.Results[2])
	}
}

func TestDispatchRetriesExistingRetryJobAndSkipsTerminalJobs(t *testing.T) {
	t.Parallel()

	var outbox InMemoryOutbox
	retryJob, err := NewDeliveryJob("retry", deliveryTestMessage(), MessageLimits{})
	if err != nil {
		t.Fatalf("NewDeliveryJob(retry): %v", err)
	}
	retryJob.Status = DeliveryJobRetry
	retryJob.Attempt = 1
	if err := outbox.Add(retryJob); err != nil {
		t.Fatalf("Add(retry): %v", err)
	}
	sentJob := retryJob
	sentJob.ID = "sent"
	sentJob.Status = DeliveryJobSent
	outbox.jobs = append(outbox.jobs, sentJob)
	outbox.index["sent"] = 1

	sender := sequenceSender{
		results: []sendResult{{err: errors.New("provider unavailable")}},
	}
	summary := Dispatch(context.Background(), &outbox, &sender, DispatchOptions{
		RetrySchedule: RetrySchedule{
			MaxAttempts: 2,
			BaseDelay:   time.Second,
			MaxDelay:    time.Second,
		},
	})

	if summary.Attempted != 1 || summary.Failed != 1 || summary.Skipped != 1 {
		t.Fatalf("summary = %+v, want one attempted final failure and one skipped", summary)
	}
	got, err := outbox.Get("retry")
	if err != nil {
		t.Fatalf("Get(retry): %v", err)
	}
	if got.Attempt != 2 || got.Status != DeliveryJobFailed || got.FailureClass != DeliveryFailureTransient {
		t.Fatalf("retry job = %+v, want failed after transient max attempt", got)
	}
	if sender.calls != 1 {
		t.Fatalf("sender calls = %d, want 1", sender.calls)
	}
}

func deliveryTestMessage() Message {
	return Message{
		From:     Address{Email: "sender@example.test"},
		To:       []Address{{Email: "recipient@example.test"}},
		Subject:  "Delivery test",
		TextBody: "hello",
	}
}

type sequenceSender struct {
	results []sendResult
	calls   int
}

func (s *sequenceSender) Send(context.Context, Message) (DeliveryReceipt, error) {
	if s.calls >= len(s.results) {
		s.calls++
		return DeliveryReceipt{}, errors.New("unexpected send")
	}
	result := s.results[s.calls]
	s.calls++
	return result.receipt, result.err
}

type sendResult struct {
	receipt DeliveryReceipt
	err     error
}
