package lazuli

import (
	"context"
	"sync"
)

// StoredEvent is an Event with a store-assigned sequence number.
type StoredEvent struct {
	// Sequence is monotonically assigned by the EventStore.
	Sequence uint64

	// Event is the original runtime event envelope.
	Event Event
}

// EventListFilter constrains EventStore.List results.
type EventListFilter struct {
	// Name, when set, limits results to events with this name.
	Name string

	// Tenant, when set, limits results to events in this tenant scope.
	Tenant *Tenant

	// SinceSequence returns only events with a sequence greater than this
	// value.
	SinceSequence uint64
}

// EventStore persists runtime events in append order.
type EventStore interface {
	// Append stores event and assigns the next monotonically increasing
	// sequence number.
	Append(ctx context.Context, event Event) (StoredEvent, error)

	// List returns a sequence-ordered snapshot matching filter.
	List(ctx context.Context, filter EventListFilter) ([]StoredEvent, error)
}

// MemoryEventStore is an in-process append-only EventStore.
type MemoryEventStore struct {
	mu sync.RWMutex

	nextSequence uint64
	events       []StoredEvent
}

var _ EventStore = (*MemoryEventStore)(nil)

// NewMemoryEventStore returns an empty in-process event store.
func NewMemoryEventStore() *MemoryEventStore {
	return &MemoryEventStore{}
}

// Append stores event and assigns the next sequence number.
func (s *MemoryEventStore) Append(ctx context.Context, event Event) (StoredEvent, error) {
	if err := ctx.Err(); err != nil {
		return StoredEvent{}, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	s.nextSequence++
	stored := StoredEvent{
		Sequence: s.nextSequence,
		Event:    cloneEvent(event),
	}
	s.events = append(s.events, stored)
	return cloneStoredEvent(stored), nil
}

// List returns events in append order after applying filter.
func (s *MemoryEventStore) List(ctx context.Context, filter EventListFilter) ([]StoredEvent, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	s.mu.RLock()
	defer s.mu.RUnlock()

	results := make([]StoredEvent, 0, len(s.events))
	for _, stored := range s.events {
		if filter.Name != "" && stored.Event.Name != filter.Name {
			continue
		}
		if filter.Tenant != nil && !sameTenant(stored.Event.Tenant, filter.Tenant) {
			continue
		}
		if stored.Sequence <= filter.SinceSequence {
			continue
		}
		results = append(results, cloneStoredEvent(stored))
	}
	return results, nil
}

func sameTenant(left, right *Tenant) bool {
	if left == nil || right == nil {
		return left == right
	}
	return left.OrgID == right.OrgID
}

func cloneStoredEvent(stored StoredEvent) StoredEvent {
	stored.Event = cloneEvent(stored.Event)
	return stored
}

func cloneEvent(event Event) Event {
	if event.Tenant != nil {
		tenant := *event.Tenant
		event.Tenant = &tenant
	}
	if event.UserID != nil {
		userID := *event.UserID
		event.UserID = &userID
	}
	if event.Payload != nil {
		event.Payload = cloneEventPayload(event.Payload)
	}
	return event
}

func cloneEventPayload(payload map[string]any) map[string]any {
	if payload == nil {
		return nil
	}
	out := make(map[string]any, len(payload))
	for key, value := range payload {
		out[key] = cloneEventValue(value)
	}
	return out
}

func cloneEventValue(value any) any {
	switch v := value.(type) {
	case map[string]any:
		return cloneEventPayload(v)
	case []any:
		out := make([]any, len(v))
		for i := range v {
			out[i] = cloneEventValue(v[i])
		}
		return out
	case []byte:
		out := make([]byte, len(v))
		copy(out, v)
		return out
	default:
		return v
	}
}
