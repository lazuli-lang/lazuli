package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestMemoryEventStoreAppendAssignsSequencesAndPreservesEvents(t *testing.T) {
	store := NewMemoryEventStore()
	ctx := context.Background()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	tenant := &Tenant{OrgID: 7}
	userID := ID(42)
	payload := map[string]any{
		"id":   ID(100),
		"meta": map[string]any{"tier": "gold"},
		"name": "Acme",
		"raw":  []byte("abc"),
		"tags": []any{"first"},
	}
	event := Event{
		Name:       "customer_created",
		Trace:      true,
		Tenant:     tenant,
		Actor:      ActorUser,
		UserID:     &userID,
		Payload:    payload,
		OccurredAt: now,
	}

	first, err := store.Append(ctx, event)
	if err != nil {
		t.Fatalf("Append() first error = %v", err)
	}
	second, err := store.Append(ctx, Event{
		Name:       "customer_updated",
		Tenant:     &Tenant{OrgID: 7},
		Actor:      ActorSystem,
		Payload:    map[string]any{"id": ID(100), "name": "Acme, Inc."},
		OccurredAt: now.Add(time.Second),
	})
	if err != nil {
		t.Fatalf("Append() second error = %v", err)
	}

	if first.Sequence != 1 || second.Sequence != 2 {
		t.Fatalf("sequences = (%d, %d), want (1, 2)", first.Sequence, second.Sequence)
	}
	assertStoredEventFields(t, first, event)

	tenant.OrgID = 99
	userID = 99
	payload["meta"].(map[string]any)["tier"] = "silver"
	payload["name"] = "mutated"
	payload["raw"].([]byte)[0] = 'z'
	payload["tags"].([]any)[0] = "changed"
	first.Event.Tenant.OrgID = 98
	*first.Event.UserID = 98
	first.Event.Payload["meta"].(map[string]any)["tier"] = "returned mutation"
	first.Event.Payload["name"] = "returned mutation"
	first.Event.Payload["raw"].([]byte)[0] = 'y'
	first.Event.Payload["tags"].([]any)[0] = "returned mutation"

	events, err := store.List(ctx, EventListFilter{})
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(events) != 2 {
		t.Fatalf("List() len = %d, want 2", len(events))
	}
	assertStoredEventFields(t, events[0], Event{
		Name:   "customer_created",
		Trace:  true,
		Tenant: &Tenant{OrgID: 7},
		Actor:  ActorUser,
		UserID: ptrID(42),
		Payload: map[string]any{
			"id":   ID(100),
			"meta": map[string]any{"tier": "gold"},
			"name": "Acme",
			"raw":  []byte("abc"),
			"tags": []any{"first"},
		},
		OccurredAt: now,
	})

	events[0].Event.Tenant.OrgID = 97
	*events[0].Event.UserID = 97
	events[0].Event.Payload["meta"].(map[string]any)["tier"] = "listed mutation"
	events[0].Event.Payload["name"] = "listed mutation"
	events[0].Event.Payload["raw"].([]byte)[0] = 'x'
	events[0].Event.Payload["tags"].([]any)[0] = "listed mutation"

	events, err = store.List(ctx, EventListFilter{})
	if err != nil {
		t.Fatalf("List() after mutation error = %v", err)
	}
	assertStoredEventFields(t, events[0], Event{
		Name:   "customer_created",
		Trace:  true,
		Tenant: &Tenant{OrgID: 7},
		Actor:  ActorUser,
		UserID: ptrID(42),
		Payload: map[string]any{
			"id":   ID(100),
			"meta": map[string]any{"tier": "gold"},
			"name": "Acme",
			"raw":  []byte("abc"),
			"tags": []any{"first"},
		},
		OccurredAt: now,
	})
}

func TestMemoryEventStoreListFiltersByNameTenantAndSequence(t *testing.T) {
	store := NewMemoryEventStore()
	ctx := context.Background()

	mustAppendEvent(t, store, Event{Name: "customer_created", Tenant: &Tenant{OrgID: 1}})
	mustAppendEvent(t, store, Event{Name: "customer_updated", Tenant: &Tenant{OrgID: 1}})
	mustAppendEvent(t, store, Event{Name: "customer_created", Tenant: &Tenant{OrgID: 2}})
	mustAppendEvent(t, store, Event{Name: "customer_created"})

	tests := []struct {
		name      string
		filter    EventListFilter
		sequences []uint64
	}{
		{
			name:      "name",
			filter:    EventListFilter{Name: "customer_created"},
			sequences: []uint64{1, 3, 4},
		},
		{
			name:      "tenant",
			filter:    EventListFilter{Tenant: &Tenant{OrgID: 1}},
			sequences: []uint64{1, 2},
		},
		{
			name:      "since sequence",
			filter:    EventListFilter{SinceSequence: 2},
			sequences: []uint64{3, 4},
		},
		{
			name:      "combined",
			filter:    EventListFilter{Name: "customer_created", Tenant: &Tenant{OrgID: 2}, SinceSequence: 1},
			sequences: []uint64{3},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			events, err := store.List(ctx, tt.filter)
			if err != nil {
				t.Fatalf("List() error = %v", err)
			}
			assertSequences(t, events, tt.sequences)
		})
	}
}

func TestMemoryEventStoreReturnsContextErrors(t *testing.T) {
	store := NewMemoryEventStore()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if _, err := store.Append(ctx, Event{Name: "customer_created"}); !errors.Is(err, context.Canceled) {
		t.Fatalf("Append() error = %v, want context.Canceled", err)
	}
	if _, err := store.List(ctx, EventListFilter{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("List() error = %v, want context.Canceled", err)
	}
}

func mustAppendEvent(t *testing.T, store *MemoryEventStore, event Event) StoredEvent {
	t.Helper()

	stored, err := store.Append(context.Background(), event)
	if err != nil {
		t.Fatalf("Append(%q) error = %v", event.Name, err)
	}
	return stored
}

func assertSequences(t *testing.T, events []StoredEvent, want []uint64) {
	t.Helper()

	if len(events) != len(want) {
		t.Fatalf("List() len = %d, want %d", len(events), len(want))
	}
	for i, event := range events {
		if event.Sequence != want[i] {
			t.Fatalf("List()[%d].Sequence = %d, want %d", i, event.Sequence, want[i])
		}
	}
}

func assertStoredEventFields(t *testing.T, stored StoredEvent, want Event) {
	t.Helper()

	if stored.Event.Name != want.Name {
		t.Fatalf("Event.Name = %q, want %q", stored.Event.Name, want.Name)
	}
	if stored.Event.Trace != want.Trace {
		t.Fatalf("Event.Trace = %v, want %v", stored.Event.Trace, want.Trace)
	}
	if got, want := tenantOrgID(stored.Event.Tenant), tenantOrgID(want.Tenant); got != want {
		t.Fatalf("Event.Tenant.OrgID = %v, want %v", got, want)
	}
	if stored.Event.Actor != want.Actor {
		t.Fatalf("Event.Actor = %q, want %q", stored.Event.Actor, want.Actor)
	}
	if got, want := idValue(stored.Event.UserID), idValue(want.UserID); got != want {
		t.Fatalf("Event.UserID = %v, want %v", got, want)
	}
	if !stored.Event.OccurredAt.Equal(want.OccurredAt) {
		t.Fatalf("Event.OccurredAt = %v, want %v", stored.Event.OccurredAt, want.OccurredAt)
	}
	if len(stored.Event.Payload) != len(want.Payload) {
		t.Fatalf("Event.Payload len = %d, want %d", len(stored.Event.Payload), len(want.Payload))
	}
	for key, wantValue := range want.Payload {
		if got := stored.Event.Payload[key]; !reflect.DeepEqual(got, wantValue) {
			t.Fatalf("Event.Payload[%q] = %v, want %v", key, got, wantValue)
		}
	}
}

func ptrID(id ID) *ID {
	return &id
}

func idValue(id *ID) any {
	if id == nil {
		return nil
	}
	return *id
}

func tenantOrgID(tenant *Tenant) any {
	if tenant == nil {
		return nil
	}
	return tenant.OrgID
}
