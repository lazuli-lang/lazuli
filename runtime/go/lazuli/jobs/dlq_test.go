package jobs

import (
	"context"
	"errors"
	"strconv"
	"sync"
	"testing"
	"time"
)

func TestMemoryDeadLetterStoreAppendListAndClone(t *testing.T) {
	t.Parallel()

	failedAt := time.Date(2026, 5, 12, 18, 0, 0, 0, time.UTC)
	store := NewMemoryDeadLetterStore()
	store.Clock = func() time.Time { return failedAt }

	entry, err := store.Append(context.Background(), DeadLetterEntry{
		Feature:    "billing",
		Name:       "settle_invoice",
		EnvelopeID: "env-1",
		Tenant:     "tenant-1",
		Payload: map[string]any{
			"invoice_id": "inv-1",
			"nested":     map[string]any{"state": "failed"},
			"bytes":      []byte("original"),
		},
		Attempts: 3,
		Error:    "payment gateway timeout",
	})
	if err != nil {
		t.Fatalf("Append: %v", err)
	}
	if entry.ID == "" {
		t.Fatal("Append did not assign ID")
	}
	if !entry.FailedAt.Equal(failedAt) {
		t.Fatalf("FailedAt = %v, want %v", entry.FailedAt, failedAt)
	}

	entry.Payload["invoice_id"] = "mutated"
	entry.Payload["nested"].(map[string]any)["state"] = "mutated"
	entry.Payload["bytes"].([]byte)[0] = 'X'

	entries, err := store.List(context.Background(), DeadLetterFilter{
		Feature: "billing",
		Name:    "settle_invoice",
		Tenant:  "tenant-1",
	})
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("List returned %d entries, want 1", len(entries))
	}
	got := entries[0]
	if got.Feature != "billing" || got.Name != "settle_invoice" || got.EnvelopeID != "env-1" {
		t.Fatalf("entry identity not preserved: %+v", got)
	}
	if got.Attempts != 3 || got.Error != "payment gateway timeout" {
		t.Fatalf("failure metadata not preserved: %+v", got)
	}
	if got.Payload["invoice_id"] != "inv-1" {
		t.Fatalf("Payload invoice_id = %q, want inv-1", got.Payload["invoice_id"])
	}
	if got.Payload["nested"].(map[string]any)["state"] != "failed" {
		t.Fatalf("nested payload was shared: %+v", got.Payload["nested"])
	}
	if string(got.Payload["bytes"].([]byte)) != "original" {
		t.Fatalf("byte payload was shared: %q", string(got.Payload["bytes"].([]byte)))
	}

	entries[0].Payload["invoice_id"] = "changed-again"
	again, err := store.List(context.Background(), DeadLetterFilter{})
	if err != nil {
		t.Fatalf("List again: %v", err)
	}
	if again[0].Payload["invoice_id"] != "inv-1" {
		t.Fatalf("List returned shared payload map: %q", again[0].Payload["invoice_id"])
	}
}

func TestMemoryDeadLetterStoreAckRemovesEntry(t *testing.T) {
	t.Parallel()

	store := NewMemoryDeadLetterStore()
	entry, err := store.Append(context.Background(), DeadLetterEntry{
		Feature:    "customer",
		Name:       "sync",
		EnvelopeID: "env-ack",
	})
	if err != nil {
		t.Fatalf("Append: %v", err)
	}
	if err := store.Ack(context.Background(), entry.ID); err != nil {
		t.Fatalf("Ack: %v", err)
	}
	entries, err := store.List(context.Background(), DeadLetterFilter{})
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(entries) != 0 {
		t.Fatalf("List returned %d entries after Ack, want 0", len(entries))
	}
	if err := store.Ack(context.Background(), entry.ID); !errors.Is(err, ErrDeadLetterEntryNotFound) {
		t.Fatalf("Ack missing error = %v, want ErrDeadLetterEntryNotFound", err)
	}
}

func TestMemoryDeadLetterStoreRequeueReady(t *testing.T) {
	t.Parallel()

	store := NewMemoryDeadLetterStore()
	firstAt := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	secondAt := firstAt.Add(5 * time.Minute)
	futureAt := firstAt.Add(time.Hour)

	first, err := store.Append(context.Background(), DeadLetterEntry{
		Feature:    "billing",
		Name:       "charge",
		EnvelopeID: "env-1",
		FailedAt:   firstAt,
	})
	if err != nil {
		t.Fatalf("Append first: %v", err)
	}
	if _, err := store.Append(context.Background(), DeadLetterEntry{
		Feature:    "billing",
		Name:       "charge",
		EnvelopeID: "env-2",
		FailedAt:   secondAt,
	}); err != nil {
		t.Fatalf("Append second: %v", err)
	}
	if _, err := store.Append(context.Background(), DeadLetterEntry{
		Feature:    "billing",
		Name:       "charge",
		EnvelopeID: "env-3",
		FailedAt:   futureAt,
	}); err != nil {
		t.Fatalf("Append future: %v", err)
	}

	ready, err := store.RequeueReady(context.Background(), secondAt, 1)
	if err != nil {
		t.Fatalf("RequeueReady: %v", err)
	}
	if len(ready) != 1 || ready[0].EnvelopeID != "env-1" {
		t.Fatalf("limited ready = %+v, want env-1 only", ready)
	}

	if err := store.Ack(context.Background(), first.ID); err != nil {
		t.Fatalf("Ack first: %v", err)
	}
	ready, err = store.RequeueReady(context.Background(), secondAt, 0)
	if err != nil {
		t.Fatalf("RequeueReady after Ack: %v", err)
	}
	if len(ready) != 1 || ready[0].EnvelopeID != "env-2" {
		t.Fatalf("ready after Ack = %+v, want env-2 only", ready)
	}
}

func TestMemoryDeadLetterStoreContextCancellation(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	store := NewMemoryDeadLetterStore()

	if _, err := store.Append(ctx, DeadLetterEntry{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("Append error = %v, want context.Canceled", err)
	}
	if _, err := store.List(ctx, DeadLetterFilter{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("List error = %v, want context.Canceled", err)
	}
	if err := store.Ack(ctx, "dlq-1"); !errors.Is(err, context.Canceled) {
		t.Fatalf("Ack error = %v, want context.Canceled", err)
	}
	if _, err := store.RequeueReady(ctx, time.Now(), 0); !errors.Is(err, context.Canceled) {
		t.Fatalf("RequeueReady error = %v, want context.Canceled", err)
	}
}

func TestMemoryDeadLetterStoreConcurrentAccess(t *testing.T) {
	t.Parallel()

	store := NewMemoryDeadLetterStore()
	const count = 64

	var wg sync.WaitGroup
	for i := 0; i < count; i++ {
		i := i
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := store.Append(context.Background(), DeadLetterEntry{
				Feature:    "feature",
				Name:       "job",
				EnvelopeID: "env-" + strconv.Itoa(i),
				Tenant:     "tenant",
			}); err != nil {
				t.Errorf("Append: %v", err)
			}
		}()
	}
	for i := 0; i < count; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := store.List(context.Background(), DeadLetterFilter{Feature: "feature"}); err != nil {
				t.Errorf("List: %v", err)
			}
			if _, err := store.RequeueReady(context.Background(), time.Now().Add(time.Hour), 0); err != nil {
				t.Errorf("RequeueReady: %v", err)
			}
		}()
	}
	wg.Wait()

	entries, err := store.List(context.Background(), DeadLetterFilter{
		Feature: "feature",
		Name:    "job",
		Tenant:  "tenant",
	})
	if err != nil {
		t.Fatalf("List final: %v", err)
	}
	if len(entries) != count {
		t.Fatalf("List final returned %d entries, want %d", len(entries), count)
	}
}
