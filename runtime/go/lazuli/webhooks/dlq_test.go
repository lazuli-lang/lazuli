package webhooks

import (
	"context"
	"errors"
	"strconv"
	"sync"
	"testing"
	"time"
)

var _ DLQStore = (*MemoryDLQStore)(nil)

func TestMemoryDLQStoreAppendListCapturesEntry(t *testing.T) {
	store := NewMemoryDLQStore()
	now := time.Date(2026, 5, 12, 12, 30, 0, 0, time.FixedZone("UTC-3", -3*60*60))
	store.nowFunc = func() time.Time { return now }
	body := []byte(`{"id":"evt_123"}`)

	stored, err := store.Append(context.Background(), DLQEntry{
		Feature:    "billing",
		Name:       "invoice_paid",
		EnvelopeID: "evt_123",
		Body:       body,
		Error:      "handler failed",
		Attempts:   4,
	})
	if err != nil {
		t.Fatalf("Append() error = %v", err)
	}

	if stored.ID != "dlq-1" {
		t.Fatalf("stored ID = %q, want dlq-1", stored.ID)
	}
	if !stored.Timestamp.Equal(now.UTC()) {
		t.Fatalf("stored Timestamp = %s, want %s", stored.Timestamp, now.UTC())
	}
	if stored.Feature != "billing" || stored.Name != "invoice_paid" || stored.EnvelopeID != "evt_123" {
		t.Fatalf("stored contract/envelope fields = %+v", stored)
	}
	if stored.Error != "handler failed" {
		t.Fatalf("stored Error = %q, want handler failed", stored.Error)
	}
	if stored.Attempts != 4 {
		t.Fatalf("stored Attempts = %d, want 4", stored.Attempts)
	}
	if string(stored.Body) != `{"id":"evt_123"}` {
		t.Fatalf("stored Body = %q", string(stored.Body))
	}

	body[0] = '['
	stored.Body[1] = 'x'
	entries, err := store.List(context.Background())
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("List() len = %d, want 1", len(entries))
	}
	if string(entries[0].Body) != `{"id":"evt_123"}` {
		t.Fatalf("List()[0].Body = %q, want original body", string(entries[0].Body))
	}

	entries[0].Body[0] = '['
	entries, err = store.List(context.Background())
	if err != nil {
		t.Fatalf("List() after mutation error = %v", err)
	}
	if string(entries[0].Body) != `{"id":"evt_123"}` {
		t.Fatalf("stored body changed through List() result: %q", string(entries[0].Body))
	}
}

func TestMemoryDLQStoreAckRemovesEntry(t *testing.T) {
	store := NewMemoryDLQStore()

	first, err := store.Append(context.Background(), DLQEntry{Feature: "crm", Name: "customer_upsert", EnvelopeID: "evt_1"})
	if err != nil {
		t.Fatalf("Append() first error = %v", err)
	}
	second, err := store.Append(context.Background(), DLQEntry{Feature: "crm", Name: "customer_upsert", EnvelopeID: "evt_2"})
	if err != nil {
		t.Fatalf("Append() second error = %v", err)
	}

	ok, err := store.Ack(context.Background(), first.ID)
	if err != nil {
		t.Fatalf("Ack() error = %v", err)
	}
	if !ok {
		t.Fatal("Ack() ok = false, want true")
	}
	ok, err = store.Ack(context.Background(), first.ID)
	if err != nil {
		t.Fatalf("Ack() missing error = %v", err)
	}
	if ok {
		t.Fatal("Ack() missing ok = true, want false")
	}

	entries, err := store.List(context.Background())
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(entries) != 1 || entries[0].ID != second.ID {
		t.Fatalf("remaining entries = %+v, want only %s", entries, second.ID)
	}
}

func TestMemoryDLQStoreRequeueRemovesAndReturnsEntry(t *testing.T) {
	store := NewMemoryDLQStore()
	entry, err := store.Append(context.Background(), DLQEntry{
		Feature:    "crm",
		Name:       "customer_upsert",
		EnvelopeID: "evt_123",
		Body:       []byte("payload"),
		Error:      "timeout",
		Attempts:   3,
	})
	if err != nil {
		t.Fatalf("Append() error = %v", err)
	}

	got, ok, err := store.Requeue(context.Background(), entry.ID)
	if err != nil {
		t.Fatalf("Requeue() error = %v", err)
	}
	if !ok {
		t.Fatal("Requeue() ok = false, want true")
	}
	if got.ID != entry.ID || got.Feature != entry.Feature || got.Name != entry.Name ||
		got.EnvelopeID != entry.EnvelopeID || got.Error != entry.Error || got.Attempts != entry.Attempts ||
		string(got.Body) != string(entry.Body) {
		t.Fatalf("Requeue() entry = %+v, want %+v", got, entry)
	}

	got.Body[0] = 'P'
	entries, err := store.List(context.Background())
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(entries) != 0 {
		t.Fatalf("List() len after Requeue = %d, want 0", len(entries))
	}

	_, ok, err = store.Requeue(context.Background(), entry.ID)
	if err != nil {
		t.Fatalf("Requeue() missing error = %v", err)
	}
	if ok {
		t.Fatal("Requeue() missing ok = true, want false")
	}
}

func TestMemoryDLQStoreAppendRejectsDuplicateID(t *testing.T) {
	store := NewMemoryDLQStore()

	if _, err := store.Append(context.Background(), DLQEntry{ID: "manual", EnvelopeID: "evt_1"}); err != nil {
		t.Fatalf("Append() first error = %v", err)
	}
	if _, err := store.Append(context.Background(), DLQEntry{ID: "manual", EnvelopeID: "evt_2"}); !errors.Is(err, ErrWebhookDLQDuplicate) {
		t.Fatalf("Append() duplicate error = %v, want ErrWebhookDLQDuplicate", err)
	}
}

func TestMemoryDLQStoreZeroValueAndConcurrentAppend(t *testing.T) {
	var store MemoryDLQStore

	const entries = 64
	var wg sync.WaitGroup
	wg.Add(entries)
	for i := 0; i < entries; i++ {
		i := i
		go func() {
			defer wg.Done()
			_, err := store.Append(context.Background(), DLQEntry{
				Feature:    "feature",
				Name:       "hook",
				EnvelopeID: "evt_" + strconv.Itoa(i),
			})
			if err != nil {
				t.Errorf("Append() error = %v", err)
			}
		}()
	}
	wg.Wait()

	got, err := store.List(context.Background())
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(got) != entries {
		t.Fatalf("List() len = %d, want %d", len(got), entries)
	}
	seen := make(map[string]bool, len(got))
	for _, entry := range got {
		if entry.ID == "" {
			t.Fatalf("entry has empty ID: %+v", entry)
		}
		if seen[entry.ID] {
			t.Fatalf("duplicate ID in List(): %q", entry.ID)
		}
		seen[entry.ID] = true
	}
}

func TestMemoryDLQStoreMethodsHonorCanceledContext(t *testing.T) {
	store := NewMemoryDLQStore()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if _, err := store.Append(ctx, DLQEntry{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("Append() error = %v, want context.Canceled", err)
	}
	if _, err := store.List(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("List() error = %v, want context.Canceled", err)
	}
	if _, err := store.Ack(ctx, "missing"); !errors.Is(err, context.Canceled) {
		t.Fatalf("Ack() error = %v, want context.Canceled", err)
	}
	if _, _, err := store.Requeue(ctx, "missing"); !errors.Is(err, context.Canceled) {
		t.Fatalf("Requeue() error = %v, want context.Canceled", err)
	}
}
