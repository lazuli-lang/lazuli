package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/jobs"
)

func TestEventInboxDedupeKeyForEventScopesSourceConsumerTenant(t *testing.T) {
	key := EventInboxDedupeKeyForEvent("nats", "customer.projector", Event{
		Tenant: &Tenant{OrgID: 7},
	}, "msg-123")

	if key.Namespace != "inbox:nats:customer.projector" {
		t.Fatalf("Namespace = %q, want inbox:nats:customer.projector", key.Namespace)
	}
	if key.Tenant != "7" {
		t.Fatalf("Tenant = %q, want 7", key.Tenant)
	}
	if key.Key != "msg-123" {
		t.Fatalf("Key = %q, want msg-123", key.Key)
	}
	if key.String() != "inbox:nats:customer.projector:7:msg-123" {
		t.Fatalf("String() = %q", key.String())
	}
}

func TestEventInboxStatusTransitions(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	if !EventInboxReceived.CanTransitionTo(EventInboxProcessing) {
		t.Fatal("received should transition to processing")
	}
	if !EventInboxProcessing.CanTransitionTo(EventInboxRetrying) {
		t.Fatal("processing should transition to retrying")
	}
	if !EventInboxProcessed.Terminal() {
		t.Fatal("processed should be terminal")
	}
	if EventInboxProcessed.CanTransitionTo(EventInboxProcessing) {
		t.Fatal("processed should not transition back to processing")
	}

	processing, err := TransitionEventInboxStatus(EventInboxRecord{}, EventInboxProcessing, now)
	if err != nil {
		t.Fatalf("TransitionEventInboxStatus processing error = %v", err)
	}
	if processing.Status != EventInboxProcessing {
		t.Fatalf("Status = %q, want processing", processing.Status)
	}
	if !processing.ReceivedAt.Equal(now) || !processing.UpdatedAt.Equal(now) {
		t.Fatalf("timestamps = (%v, %v), want %v", processing.ReceivedAt, processing.UpdatedAt, now)
	}

	processed, err := TransitionEventInboxStatus(processing, EventInboxProcessed, now.Add(time.Minute))
	if err != nil {
		t.Fatalf("TransitionEventInboxStatus processed error = %v", err)
	}
	if processed.Status != EventInboxProcessed {
		t.Fatalf("Status = %q, want processed", processed.Status)
	}
	if !processed.CompletedAt.Equal(now.Add(time.Minute)) {
		t.Fatalf("CompletedAt = %v, want %v", processed.CompletedAt, now.Add(time.Minute))
	}

	_, err = TransitionEventInboxStatus(processed, EventInboxProcessing, now.Add(2*time.Minute))
	if !errors.Is(err, ErrEventMessageInvalidTransition) {
		t.Fatalf("TransitionEventInboxStatus invalid error = %v, want ErrEventMessageInvalidTransition", err)
	}
}

func TestEventInboxRetryVisibility(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	record := EventInboxRecord{
		Status: EventInboxProcessing,
		Retry: EventRetryMetadata{
			Policy:      RetryPolicy{Count: 1, Backoff: jobs.BackoffFixed},
			Attempts:    1,
			MaxAttempts: 2,
		},
	}

	retrying, err := ScheduleEventInboxRetry(record, now, errors.New("temporary"))
	if err != nil {
		t.Fatalf("ScheduleEventInboxRetry error = %v", err)
	}
	if retrying.Status != EventInboxRetrying {
		t.Fatalf("Status = %q, want retrying", retrying.Status)
	}

	before := retrying.RetryVisibility(now.Add(4 * time.Second))
	if before.Visible {
		t.Fatal("Visible before NextAttemptAt = true, want false")
	}
	if !before.VisibleAt.Equal(now.Add(5 * time.Second)) {
		t.Fatalf("VisibleAt = %v, want %v", before.VisibleAt, now.Add(5*time.Second))
	}
	if before.LastError != "temporary" {
		t.Fatalf("LastError = %q, want temporary", before.LastError)
	}
	if before.Attempts != 1 || before.MaxAttempts != 2 || before.Exhausted {
		t.Fatalf("retry visibility = %+v, want attempt 1 of 2 not exhausted", before)
	}

	atRetry := retrying.RetryVisibility(now.Add(5 * time.Second))
	if !atRetry.Visible {
		t.Fatal("Visible at NextAttemptAt = false, want true")
	}
}

func TestMemoryEventInboxStoreRecordClonesAndRejectsDuplicateDedupeKey(t *testing.T) {
	store := NewMemoryEventInboxStore()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	payload := map[string]any{
		"customer_id": ID(42),
		"meta":        map[string]any{"tier": "gold"},
	}
	metadata := map[string]string{"trace_id": "trace-1"}
	key := EventInboxDedupeKeyForEvent("nats", "customer.projector", Event{
		Tenant: &Tenant{OrgID: 7},
	}, "msg-123")

	stored, err := store.Record(context.Background(), EventInboxRecord{
		ID:         "inbox-1",
		Source:     "nats",
		Consumer:   "customer.projector",
		Event:      Event{Name: "customer_created", Tenant: &Tenant{OrgID: 7}, Payload: payload},
		DedupeKey:  key,
		ReceivedAt: now,
		Metadata:   metadata,
	})
	if err != nil {
		t.Fatalf("Record() error = %v", err)
	}
	if stored.Status != EventInboxReceived {
		t.Fatalf("Status = %q, want received", stored.Status)
	}
	if !stored.ReceivedAt.Equal(now) || !stored.UpdatedAt.Equal(now) {
		t.Fatalf("timestamps = (%v, %v), want %v", stored.ReceivedAt, stored.UpdatedAt, now)
	}

	payload["meta"].(map[string]any)["tier"] = "silver"
	metadata["trace_id"] = "mutated"
	stored.Event.Payload["meta"].(map[string]any)["tier"] = "returned mutation"
	stored.Metadata["trace_id"] = "returned mutation"

	records, err := store.List(context.Background(), EventInboxFilter{
		Source:   "nats",
		Consumer: "customer.projector",
		Tenant:   "7",
	})
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(records) != 1 {
		t.Fatalf("List() len = %d, want 1", len(records))
	}
	wantPayload := map[string]any{
		"customer_id": ID(42),
		"meta":        map[string]any{"tier": "gold"},
	}
	if !reflect.DeepEqual(records[0].Event.Payload, wantPayload) {
		t.Fatalf("Payload = %#v, want %#v", records[0].Event.Payload, wantPayload)
	}
	if records[0].Metadata["trace_id"] != "trace-1" {
		t.Fatalf("Metadata trace_id = %q, want trace-1", records[0].Metadata["trace_id"])
	}

	_, err = store.Record(context.Background(), EventInboxRecord{
		ID:        "inbox-2",
		Source:    "nats",
		Consumer:  "customer.projector",
		Event:     Event{Name: "customer_created", Tenant: &Tenant{OrgID: 7}},
		DedupeKey: key,
	})
	if !errors.Is(err, ErrEventMessageDuplicate) {
		t.Fatalf("duplicate Record() error = %v, want ErrEventMessageDuplicate", err)
	}
}

func TestMemoryEventInboxStorePlanReadyAndLifecycle(t *testing.T) {
	store := NewMemoryEventInboxStore()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	_, err := store.Record(context.Background(), EventInboxRecord{
		ID:         "inbox-1",
		Source:     "nats",
		Consumer:   "customer.projector",
		Event:      Event{Name: "customer_created", Tenant: &Tenant{OrgID: 7}},
		Retry:      NewEventRetryMetadata(&RetryPolicy{Count: 1, Backoff: jobs.BackoffFixed}),
		ReceivedAt: now,
	})
	if err != nil {
		t.Fatalf("Record() first error = %v", err)
	}
	_, err = store.Record(context.Background(), EventInboxRecord{
		ID:         "inbox-2",
		Source:     "kafka",
		Consumer:   "customer.projector",
		Event:      Event{Name: "customer_updated", Tenant: &Tenant{OrgID: 7}},
		ReceivedAt: now,
	})
	if err != nil {
		t.Fatalf("Record() second error = %v", err)
	}

	plan, err := store.PlanReady(context.Background(), now, 1)
	if err != nil {
		t.Fatalf("PlanReady() error = %v", err)
	}
	if len(plan.Records) != 1 {
		t.Fatalf("PlanReady() len = %d, want 1", len(plan.Records))
	}
	if plan.Records[0].ID != "inbox-1" || plan.Records[0].Status != EventInboxProcessing {
		t.Fatalf("planned record = %+v, want inbox-1 processing", plan.Records[0])
	}
	if plan.Records[0].Retry.Attempts != 1 || !plan.Records[0].Retry.LastAttemptAt.Equal(now) {
		t.Fatalf("retry metadata = %+v, want attempt 1 at %v", plan.Records[0].Retry, now)
	}

	processed, err := store.MarkProcessed(context.Background(), "inbox-1", now.Add(time.Minute))
	if err != nil {
		t.Fatalf("MarkProcessed() error = %v", err)
	}
	if processed.Status != EventInboxProcessed {
		t.Fatalf("Status = %q, want processed", processed.Status)
	}
	if !processed.CompletedAt.Equal(now.Add(time.Minute)) {
		t.Fatalf("CompletedAt = %v, want %v", processed.CompletedAt, now.Add(time.Minute))
	}

	processedRecords, err := store.List(context.Background(), EventInboxFilter{
		Status:   EventInboxProcessed,
		Consumer: "customer.projector",
		Tenant:   "7",
	})
	if err != nil {
		t.Fatalf("List() processed error = %v", err)
	}
	if len(processedRecords) != 1 || processedRecords[0].ID != "inbox-1" {
		t.Fatalf("processed filter = %+v, want inbox-1", processedRecords)
	}

	skipped, err := store.MarkSkipped(context.Background(), "inbox-2", now.Add(time.Minute))
	if err != nil {
		t.Fatalf("MarkSkipped() error = %v", err)
	}
	if skipped.Status != EventInboxSkipped {
		t.Fatalf("Status = %q, want skipped", skipped.Status)
	}
}

func TestMemoryEventInboxStoreRetriesUntilDead(t *testing.T) {
	store := NewMemoryEventInboxStore()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	_, err := store.Record(context.Background(), EventInboxRecord{
		ID:       "inbox-1",
		Consumer: "customer.projector",
		Event:    Event{Name: "customer_created"},
		Retry: NewEventRetryMetadata(&RetryPolicy{
			Count:   1,
			Backoff: jobs.BackoffFixed,
		}),
		ReceivedAt: now,
	})
	if err != nil {
		t.Fatalf("Record() error = %v", err)
	}

	plan, err := store.PlanReady(context.Background(), now, 0)
	if err != nil {
		t.Fatalf("PlanReady() error = %v", err)
	}
	if len(plan.Records) != 1 {
		t.Fatalf("PlanReady() len = %d, want 1", len(plan.Records))
	}

	retrying, err := store.MarkFailed(context.Background(), "inbox-1", now, errors.New("temporary"))
	if err != nil {
		t.Fatalf("MarkFailed() retrying error = %v", err)
	}
	if retrying.Status != EventInboxRetrying {
		t.Fatalf("Status = %q, want retrying", retrying.Status)
	}
	if retrying.Retry.NextAttemptAt != now.Add(5*time.Second) {
		t.Fatalf("NextAttemptAt = %v, want %v", retrying.Retry.NextAttemptAt, now.Add(5*time.Second))
	}
	if retrying.RetryVisibility(now.Add(4 * time.Second)).Visible {
		t.Fatal("retrying record visible before NextAttemptAt")
	}

	plan, err = store.PlanReady(context.Background(), now.Add(4*time.Second), 0)
	if err != nil {
		t.Fatalf("PlanReady() before retry error = %v", err)
	}
	if len(plan.Records) != 0 {
		t.Fatalf("PlanReady() before retry len = %d, want 0", len(plan.Records))
	}

	plan, err = store.PlanReady(context.Background(), now.Add(5*time.Second), 0)
	if err != nil {
		t.Fatalf("PlanReady() retry error = %v", err)
	}
	if len(plan.Records) != 1 || plan.Records[0].Retry.Attempts != 2 {
		t.Fatalf("retry plan = %+v, want attempt 2", plan.Records)
	}

	dead, err := store.MarkFailed(context.Background(), "inbox-1", now.Add(5*time.Second), errors.New("permanent"))
	if err != nil {
		t.Fatalf("MarkFailed() dead error = %v", err)
	}
	if dead.Status != EventInboxDead {
		t.Fatalf("Status = %q, want dead", dead.Status)
	}
	visibility := dead.RetryVisibility(now.Add(5 * time.Second))
	if visibility.Visible {
		t.Fatal("dead record visible = true, want false")
	}
	if !visibility.Exhausted {
		t.Fatalf("Exhausted = false, want true: %+v", visibility)
	}
}
