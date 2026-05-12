package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/jobs"
)

func TestEventMessageStatusTransitions(t *testing.T) {
	if !EventMessagePending.CanTransitionTo(EventMessageClaimed) {
		t.Fatal("pending should transition to claimed")
	}
	if !EventMessageClaimed.CanTransitionTo(EventMessageRetrying) {
		t.Fatal("claimed should transition to retrying")
	}
	if !EventMessageDelivered.Terminal() {
		t.Fatal("delivered should be terminal")
	}
	if EventMessageDelivered.CanTransitionTo(EventMessagePending) {
		t.Fatal("delivered should not transition back to pending")
	}

	_, err := TransitionEventMessageStatus(
		EventMessageEnvelope{Status: EventMessageDelivered},
		EventMessageClaimed,
		time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC),
	)
	if !errors.Is(err, ErrEventMessageInvalidTransition) {
		t.Fatalf("TransitionEventMessageStatus error = %v, want ErrEventMessageInvalidTransition", err)
	}
}

func TestScheduleEventMessageRetryUsesRetryPolicyAndDeadLetters(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	message := EventMessageEnvelope{
		Status: EventMessageClaimed,
		Retry: EventRetryMetadata{
			Policy:      RetryPolicy{Count: 1, Backoff: jobs.BackoffFixed},
			Attempts:    1,
			MaxAttempts: 2,
		},
	}

	retrying, err := ScheduleEventMessageRetry(message, now, errors.New("temporary"))
	if err != nil {
		t.Fatalf("ScheduleEventMessageRetry retrying error = %v", err)
	}
	if retrying.Status != EventMessageRetrying {
		t.Fatalf("Status = %q, want retrying", retrying.Status)
	}
	if retrying.Retry.NextAttemptAt != now.Add(5*time.Second) {
		t.Fatalf("NextAttemptAt = %v, want %v", retrying.Retry.NextAttemptAt, now.Add(5*time.Second))
	}
	if retrying.Retry.LastError != "temporary" {
		t.Fatalf("LastError = %q, want temporary", retrying.Retry.LastError)
	}

	retrying.Status = EventMessageClaimed
	retrying.Retry.Attempts = 2
	dead, err := ScheduleEventMessageRetry(retrying, now.Add(time.Minute), errors.New("permanent"))
	if err != nil {
		t.Fatalf("ScheduleEventMessageRetry dead error = %v", err)
	}
	if dead.Status != EventMessageDead {
		t.Fatalf("Status = %q, want dead", dead.Status)
	}
	if !dead.CompletedAt.Equal(now.Add(time.Minute)) {
		t.Fatalf("CompletedAt = %v, want %v", dead.CompletedAt, now.Add(time.Minute))
	}
	if !dead.Retry.NextAttemptAt.IsZero() {
		t.Fatalf("NextAttemptAt = %v, want zero after dead-letter", dead.Retry.NextAttemptAt)
	}
}

func TestMemoryEventMessagePlannerAddClonesAndRejectsDuplicateIdempotency(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	planner := NewMemoryEventMessagePlanner()
	planner.Clock = func() time.Time { return now }

	payload := map[string]any{
		"customer_id": ID(42),
		"meta":        map[string]any{"tier": "gold"},
	}
	metadata := map[string]string{"trace_id": "trace-1"}
	key := EventIdempotencyKeyForEvent("outbox:publisher", Event{
		Tenant: &Tenant{OrgID: 7},
	}, "event-1")

	stored, err := planner.Add(context.Background(), EventMessageEnvelope{
		Direction:      EventMessageOutbox,
		Event:          Event{Name: "customer_created", Tenant: &Tenant{OrgID: 7}, Payload: payload},
		IdempotencyKey: key,
		Metadata:       metadata,
	})
	if err != nil {
		t.Fatalf("Add() error = %v", err)
	}
	if stored.ID == "" {
		t.Fatal("Add() ID is empty")
	}
	if stored.Status != EventMessagePending {
		t.Fatalf("Status = %q, want pending", stored.Status)
	}
	if !stored.CreatedAt.Equal(now) || !stored.UpdatedAt.Equal(now) {
		t.Fatalf("timestamps = (%v, %v), want %v", stored.CreatedAt, stored.UpdatedAt, now)
	}

	payload["meta"].(map[string]any)["tier"] = "silver"
	metadata["trace_id"] = "mutated"
	stored.Event.Payload["meta"].(map[string]any)["tier"] = "returned mutation"
	stored.Metadata["trace_id"] = "returned mutation"

	messages, err := planner.List(context.Background(), EventMessageFilter{})
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(messages) != 1 {
		t.Fatalf("List() len = %d, want 1", len(messages))
	}
	wantPayload := map[string]any{
		"customer_id": ID(42),
		"meta":        map[string]any{"tier": "gold"},
	}
	if !reflect.DeepEqual(messages[0].Event.Payload, wantPayload) {
		t.Fatalf("Payload = %#v, want %#v", messages[0].Event.Payload, wantPayload)
	}
	if messages[0].Metadata["trace_id"] != "trace-1" {
		t.Fatalf("Metadata trace_id = %q, want trace-1", messages[0].Metadata["trace_id"])
	}

	_, err = planner.Add(context.Background(), EventMessageEnvelope{
		Event:          Event{Name: "customer_created", Tenant: &Tenant{OrgID: 7}},
		IdempotencyKey: key,
	})
	if !errors.Is(err, ErrEventMessageDuplicate) {
		t.Fatalf("duplicate Add() error = %v, want ErrEventMessageDuplicate", err)
	}
}

func TestMemoryEventMessagePlannerPlanReadyClaimsDueMessages(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	planner := NewMemoryEventMessagePlanner()
	planner.Clock = func() time.Time { return now }

	first, err := planner.Add(context.Background(), EventMessageEnvelope{
		ID:        "msg-1",
		Direction: EventMessageOutbox,
		Event:     Event{Name: "customer_created", Tenant: &Tenant{OrgID: 1}},
		Retry:     NewEventRetryMetadata(&RetryPolicy{Count: 1, Backoff: jobs.BackoffFixed}),
	})
	if err != nil {
		t.Fatalf("Add() first error = %v", err)
	}
	_, err = planner.Add(context.Background(), EventMessageEnvelope{
		ID:        "msg-2",
		Direction: EventMessageInbox,
		Consumer:  "customer.projector",
		Event:     Event{Name: "customer_created", Tenant: &Tenant{OrgID: 1}},
		Status:    EventMessageRetrying,
		Retry: EventRetryMetadata{
			MaxAttempts:   3,
			NextAttemptAt: now.Add(time.Minute),
		},
	})
	if err != nil {
		t.Fatalf("Add() second error = %v", err)
	}

	plan, err := planner.PlanReady(context.Background(), now, 1)
	if err != nil {
		t.Fatalf("PlanReady() error = %v", err)
	}
	if len(plan.Messages) != 1 {
		t.Fatalf("PlanReady() len = %d, want 1", len(plan.Messages))
	}
	if plan.Messages[0].ID != first.ID || plan.Messages[0].Status != EventMessageClaimed {
		t.Fatalf("planned message = %+v, want first claimed", plan.Messages[0])
	}
	if plan.Messages[0].Retry.Attempts != 1 || !plan.Messages[0].Retry.LastAttemptAt.Equal(now) {
		t.Fatalf("retry metadata = %+v, want attempt 1 at %v", plan.Messages[0].Retry, now)
	}

	plan, err = planner.PlanReady(context.Background(), now.Add(time.Minute), 0)
	if err != nil {
		t.Fatalf("PlanReady() second error = %v", err)
	}
	if len(plan.Messages) != 1 {
		t.Fatalf("PlanReady() second len = %d, want 1", len(plan.Messages))
	}
	if plan.Messages[0].ID != "msg-2" || plan.Messages[0].Status != EventMessageClaimed {
		t.Fatalf("second planned message = %+v, want msg-2 claimed", plan.Messages[0])
	}

	inboxMessages, err := planner.List(context.Background(), EventMessageFilter{
		Direction: EventMessageInbox,
		Consumer:  "customer.projector",
		Tenant:    "1",
	})
	if err != nil {
		t.Fatalf("List() inbox error = %v", err)
	}
	if len(inboxMessages) != 1 || inboxMessages[0].ID != "msg-2" {
		t.Fatalf("inbox filter = %+v, want msg-2", inboxMessages)
	}
}

func TestMemoryEventMessagePlannerMarkFailedSchedulesRetryThenDead(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	planner := NewMemoryEventMessagePlanner()
	_, err := planner.Add(context.Background(), EventMessageEnvelope{
		ID:    "msg-1",
		Event: Event{Name: "customer_created"},
		Retry: NewEventRetryMetadata(&RetryPolicy{
			Count:   1,
			Backoff: jobs.BackoffFixed,
		}),
	})
	if err != nil {
		t.Fatalf("Add() error = %v", err)
	}

	plan, err := planner.PlanReady(context.Background(), now, 0)
	if err != nil {
		t.Fatalf("PlanReady() error = %v", err)
	}
	if len(plan.Messages) != 1 {
		t.Fatalf("PlanReady() len = %d, want 1", len(plan.Messages))
	}

	retrying, err := planner.MarkFailed(context.Background(), "msg-1", now, errors.New("temporary"))
	if err != nil {
		t.Fatalf("MarkFailed() retrying error = %v", err)
	}
	if retrying.Status != EventMessageRetrying {
		t.Fatalf("Status = %q, want retrying", retrying.Status)
	}
	if retrying.Retry.Attempts != 1 {
		t.Fatalf("Attempts = %d, want 1", retrying.Retry.Attempts)
	}
	if retrying.Retry.NextAttemptAt != now.Add(5*time.Second) {
		t.Fatalf("NextAttemptAt = %v, want %v", retrying.Retry.NextAttemptAt, now.Add(5*time.Second))
	}

	plan, err = planner.PlanReady(context.Background(), now.Add(4*time.Second), 0)
	if err != nil {
		t.Fatalf("PlanReady() before retry error = %v", err)
	}
	if len(plan.Messages) != 0 {
		t.Fatalf("PlanReady() before retry len = %d, want 0", len(plan.Messages))
	}

	plan, err = planner.PlanReady(context.Background(), now.Add(5*time.Second), 0)
	if err != nil {
		t.Fatalf("PlanReady() retry error = %v", err)
	}
	if len(plan.Messages) != 1 || plan.Messages[0].Retry.Attempts != 2 {
		t.Fatalf("retry plan = %+v, want attempt 2", plan.Messages)
	}

	dead, err := planner.MarkFailed(context.Background(), "msg-1", now.Add(5*time.Second), errors.New("permanent"))
	if err != nil {
		t.Fatalf("MarkFailed() dead error = %v", err)
	}
	if dead.Status != EventMessageDead {
		t.Fatalf("Status = %q, want dead", dead.Status)
	}
	if !dead.CompletedAt.Equal(now.Add(5 * time.Second)) {
		t.Fatalf("CompletedAt = %v, want %v", dead.CompletedAt, now.Add(5*time.Second))
	}
}
