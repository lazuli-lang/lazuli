package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestReplayEventsDeliversStoredEvents(t *testing.T) {
	tenant := &Tenant{OrgID: 42}
	filter := EventReplayFilter{
		Names:  []string{"customer.created"},
		Tenant: tenant,
		Since:  time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC),
		Until:  time.Date(2026, 5, 13, 12, 0, 0, 0, time.UTC),
	}
	store := &fakeEventReplayStore{events: []Event{
		{Name: "customer.created", Payload: map[string]any{"id": ID(1)}},
		{Name: "customer.created", Payload: map[string]any{"id": ID(2)}},
	}}

	var got []Event
	summary, err := ReplayEvents(context.Background(), store, filter, func(_ context.Context, event Event) error {
		got = append(got, event)
		return nil
	})
	if err != nil {
		t.Fatalf("ReplayEvents returned error: %v", err)
	}

	if summary.Read != 2 || summary.Replayed != 2 || summary.Failed != 0 || summary.Limited {
		t.Fatalf("ReplayEvents summary = %+v, want 2 replayed and no failures", summary)
	}
	if !reflect.DeepEqual(got, store.events) {
		t.Fatalf("subscriber events = %+v, want %+v", got, store.events)
	}
	if !reflect.DeepEqual(store.filter, filter) {
		t.Fatalf("store filter = %+v, want %+v", store.filter, filter)
	}
}

func TestReplayEventsHonorsMaxEvents(t *testing.T) {
	store := &fakeEventReplayStore{events: []Event{
		{Name: "one"},
		{Name: "two"},
		{Name: "three"},
	}}

	var got []string
	summary, err := ReplayEvents(context.Background(), store, EventReplayFilter{}, func(_ context.Context, event Event) error {
		got = append(got, event.Name)
		return nil
	}, WithReplayMaxEvents(2))
	if err != nil {
		t.Fatalf("ReplayEvents returned error: %v", err)
	}

	if !reflect.DeepEqual(got, []string{"one", "two"}) {
		t.Fatalf("subscriber events = %+v, want [one two]", got)
	}
	if summary.Read != 2 || summary.Replayed != 2 || summary.Failed != 0 || !summary.Limited {
		t.Fatalf("ReplayEvents summary = %+v, want limited after 2 events", summary)
	}
	if store.yielded != 3 {
		t.Fatalf("store yielded = %d, want 3 to detect the replay limit", store.yielded)
	}
}

func TestReplayEventsReturnsSubscriberError(t *testing.T) {
	wantErr := errors.New("projection failed")
	store := &fakeEventReplayStore{events: []Event{
		{Name: "one"},
		{Name: "two"},
	}}

	summary, err := ReplayEvents(context.Background(), store, EventReplayFilter{}, func(context.Context, Event) error {
		return wantErr
	})
	if !errors.Is(err, wantErr) {
		t.Fatalf("ReplayEvents error = %v, want %v", err, wantErr)
	}
	if summary.Read != 1 || summary.Replayed != 0 || summary.Failed != 1 {
		t.Fatalf("ReplayEvents summary = %+v, want one failed event", summary)
	}
	if len(summary.Failures) != 1 || !errors.Is(summary.Failures[0].Err, wantErr) {
		t.Fatalf("ReplayEvents failures = %+v, want subscriber error", summary.Failures)
	}
	if store.yielded != 1 {
		t.Fatalf("store yielded = %d, want replay to stop on first subscriber error", store.yielded)
	}
}

func TestReplayEventsContinueOnError(t *testing.T) {
	wantErr := errors.New("projection failed")
	store := &fakeEventReplayStore{events: []Event{
		{Name: "one"},
		{Name: "two"},
	}}

	summary, err := ReplayEvents(context.Background(), store, EventReplayFilter{}, func(_ context.Context, event Event) error {
		if event.Name == "one" {
			return wantErr
		}
		return nil
	}, WithReplayContinueOnError())
	if !errors.Is(err, wantErr) {
		t.Fatalf("ReplayEvents error = %v, want %v", err, wantErr)
	}
	if summary.Read != 2 || summary.Replayed != 1 || summary.Failed != 1 {
		t.Fatalf("ReplayEvents summary = %+v, want one replayed and one failed", summary)
	}
	if store.yielded != 2 {
		t.Fatalf("store yielded = %d, want all events", store.yielded)
	}
}

func TestReplayEventsReturnsStoreError(t *testing.T) {
	wantErr := errors.New("store unavailable")
	store := &fakeEventReplayStore{err: wantErr}

	summary, err := ReplayEvents(context.Background(), store, EventReplayFilter{}, func(context.Context, Event) error {
		t.Fatal("subscriber should not be called")
		return nil
	})
	if !errors.Is(err, wantErr) {
		t.Fatalf("ReplayEvents error = %v, want %v", err, wantErr)
	}
	if !isZeroReplaySummary(summary) {
		t.Fatalf("ReplayEvents summary = %+v, want zero value", summary)
	}
}

func TestReplayEventsRespectsCanceledContext(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	store := &fakeEventReplayStore{events: []Event{{Name: "one"}}}

	summary, err := ReplayEvents(ctx, store, EventReplayFilter{}, func(context.Context, Event) error {
		t.Fatal("subscriber should not be called")
		return nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("ReplayEvents error = %v, want context.Canceled", err)
	}
	if !isZeroReplaySummary(summary) {
		t.Fatalf("ReplayEvents summary = %+v, want zero value", summary)
	}
	if store.calls != 0 {
		t.Fatalf("store calls = %d, want 0", store.calls)
	}
}

func TestReplayEventsStopsWhenContextCanceledDuringReplay(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	store := &fakeEventReplayStore{events: []Event{
		{Name: "one"},
		{Name: "two"},
	}}

	summary, err := ReplayEvents(ctx, store, EventReplayFilter{}, func(context.Context, Event) error {
		cancel()
		return nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("ReplayEvents error = %v, want context.Canceled", err)
	}
	if summary.Read != 1 || summary.Replayed != 1 || summary.Failed != 0 {
		t.Fatalf("ReplayEvents summary = %+v, want first event replayed before cancellation", summary)
	}
	if store.yielded != 1 {
		t.Fatalf("store yielded = %d, want replay to stop after cancellation", store.yielded)
	}
}

func TestReplayEventsRejectsInvalidInputs(t *testing.T) {
	store := &fakeEventReplayStore{}

	if _, err := ReplayEvents(context.Background(), nil, EventReplayFilter{}, func(context.Context, Event) error {
		return nil
	}); !errors.Is(err, ErrNilEventStore) {
		t.Fatalf("ReplayEvents nil store error = %v, want ErrNilEventStore", err)
	}

	var typedNilStore *fakeEventReplayStore
	if _, err := ReplayEvents(context.Background(), typedNilStore, EventReplayFilter{}, func(context.Context, Event) error {
		return nil
	}); !errors.Is(err, ErrNilEventStore) {
		t.Fatalf("ReplayEvents typed nil store error = %v, want ErrNilEventStore", err)
	}

	if _, err := ReplayEvents(context.Background(), store, EventReplayFilter{}, nil); !errors.Is(err, ErrNilEventSubscriber) {
		t.Fatalf("ReplayEvents nil subscriber error = %v, want ErrNilEventSubscriber", err)
	}

	if _, err := ReplayEvents(context.Background(), store, EventReplayFilter{}, func(context.Context, Event) error {
		return nil
	}, WithReplayMaxEvents(-1)); !errors.Is(err, ErrEventReplayMaxEventsInvalid) {
		t.Fatalf("ReplayEvents negative max error = %v, want ErrEventReplayMaxEventsInvalid", err)
	}
}

func TestReplayEventsDoesNotPublishGlobally(t *testing.T) {
	eventName := t.Name()
	globalCalls := 0
	Subscribe(eventName, func(context.Context, Event) error {
		globalCalls++
		return nil
	})
	store := &fakeEventReplayStore{events: []Event{{Name: eventName}}}

	replayCalls := 0
	if _, err := ReplayEvents(context.Background(), store, EventReplayFilter{}, func(context.Context, Event) error {
		replayCalls++
		return nil
	}); err != nil {
		t.Fatalf("ReplayEvents returned error: %v", err)
	}

	if replayCalls != 1 {
		t.Fatalf("replay subscriber calls = %d, want 1", replayCalls)
	}
	if globalCalls != 0 {
		t.Fatalf("global subscriber calls = %d, want 0", globalCalls)
	}
}

func isZeroReplaySummary(summary EventReplaySummary) bool {
	return summary.Read == 0 &&
		summary.Replayed == 0 &&
		summary.Failed == 0 &&
		!summary.Limited &&
		len(summary.Failures) == 0
}

type fakeEventReplayStore struct {
	calls   int
	events  []Event
	err     error
	filter  EventReplayFilter
	yielded int
}

var _ EventStore = (*fakeEventReplayStore)(nil)

func (s *fakeEventReplayStore) ReplayEvents(ctx context.Context, filter EventReplayFilter, yield func(Event) error) error {
	s.calls++
	s.filter = filter
	if s.err != nil {
		return s.err
	}
	for _, event := range s.events {
		if err := ctx.Err(); err != nil {
			return err
		}
		s.yielded++
		if err := yield(event); err != nil {
			return err
		}
	}
	return nil
}
