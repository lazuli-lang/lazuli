package lazuli

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"time"
)

var (
	// ErrNilEventStore is returned when ReplayEvents receives no event store.
	ErrNilEventStore = errors.New("lazuli: event store is nil")

	// ErrNilEventSubscriber is returned when ReplayEvents receives no subscriber.
	ErrNilEventSubscriber = errors.New("lazuli: event replay subscriber is nil")

	// ErrEventReplayMaxEventsInvalid is returned when MaxEvents is negative.
	ErrEventReplayMaxEventsInvalid = errors.New("lazuli: event replay max events is invalid")
)

var errEventReplayLimitReached = errors.New("lazuli: event replay limit reached")

// EventReplayFilter describes the event subset an EventStore should stream.
//
// Names limits replay to specific event names. An empty Names slice means all
// event names. Since is an inclusive OccurredAt lower bound, and Until is an
// exclusive OccurredAt upper bound. A nil Tenant leaves tenant selection to the
// store.
type EventReplayFilter struct {
	Names  []string
	Tenant *Tenant
	Since  time.Time
	Until  time.Time
}

// EventStore is the minimal durable event store contract required for replay.
//
// Implementations should stream matching events in deterministic order, call
// yield synchronously, stop when yield returns an error, and return that error
// so ReplayEvents can preserve subscriber and context failures.
type EventStore interface {
	ReplayEvents(ctx context.Context, filter EventReplayFilter, yield func(Event) error) error
}

// ReplayOptions configures ReplayEvents.
type ReplayOptions struct {
	// MaxEvents caps how many events are delivered to subscriber. Zero means no
	// cap; negative values are rejected.
	MaxEvents int

	// ContinueOnError keeps replaying after subscriber errors and returns the
	// joined subscriber errors after the store is exhausted.
	ContinueOnError bool
}

// ReplayOption configures ReplayEvents.
type ReplayOption func(*ReplayOptions)

// WithReplayMaxEvents caps how many events ReplayEvents delivers.
//
// A max value of zero means no cap. Negative values make ReplayEvents return
// ErrEventReplayMaxEventsInvalid.
func WithReplayMaxEvents(max int) ReplayOption {
	return func(options *ReplayOptions) {
		options.MaxEvents = max
	}
}

// WithReplayContinueOnError keeps replaying after subscriber errors.
func WithReplayContinueOnError() ReplayOption {
	return func(options *ReplayOptions) {
		options.ContinueOnError = true
	}
}

// EventReplayFailure records one subscriber failure during replay.
type EventReplayFailure struct {
	// Event is the event that failed delivery.
	Event Event
	// Err is the subscriber error with replay context.
	Err error
}

// EventReplaySummary summarizes a ReplayEvents run.
type EventReplaySummary struct {
	// Read is the number of events accepted from the store for delivery.
	Read int
	// Replayed is the number of events delivered without subscriber error.
	Replayed int
	// Failed is the number of events whose subscriber returned an error.
	Failed int
	// Limited reports that MaxEvents stopped replay before the store was exhausted.
	Limited bool
	// Failures preserves subscriber failures in replay order.
	Failures []EventReplayFailure
}

// ReplayEvents streams matching events from store to subscriber and returns a
// delivery summary.
//
// ReplayEvents does not call Publish or consult global subscriptions. It
// delivers only to subscriber, returns store/context/subscriber errors, and
// honors WithReplayMaxEvents before invoking subscriber for the next event.
func ReplayEvents(
	ctx context.Context,
	store EventStore,
	filter EventReplayFilter,
	subscriber Subscriber,
	options ...ReplayOption,
) (EventReplaySummary, error) {
	if ctx == nil {
		ctx = context.Background()
	}

	var summary EventReplaySummary
	if isNilEventStore(store) {
		return summary, ErrNilEventStore
	}
	if subscriber == nil {
		return summary, ErrNilEventSubscriber
	}
	if err := ctx.Err(); err != nil {
		return summary, err
	}

	opts := replayOptions(options)
	if opts.MaxEvents < 0 {
		return summary, ErrEventReplayMaxEventsInvalid
	}

	var stoppedBySubscriber error
	filter.Names = append([]string(nil), filter.Names...)
	err := store.ReplayEvents(ctx, filter, func(event Event) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		if opts.MaxEvents > 0 && summary.Read >= opts.MaxEvents {
			summary.Limited = true
			return errEventReplayLimitReached
		}

		summary.Read++
		if err := subscriber(ctx, event); err != nil {
			failure := EventReplayFailure{
				Event: event,
				Err:   fmt.Errorf("lazuli: replay event %q: %w", event.Name, err),
			}
			summary.Failures = append(summary.Failures, failure)
			summary.Failed++
			if !opts.ContinueOnError {
				stoppedBySubscriber = failure.Err
				return failure.Err
			}
			return nil
		}

		summary.Replayed++
		if err := ctx.Err(); err != nil {
			return err
		}
		return nil
	})

	subscriberErr := replayFailureError(summary.Failures)
	switch {
	case err == nil:
		return summary, subscriberErr
	case errors.Is(err, errEventReplayLimitReached):
		return summary, subscriberErr
	case stoppedBySubscriber != nil && errors.Is(err, stoppedBySubscriber):
		return summary, err
	default:
		return summary, errors.Join(subscriberErr, fmt.Errorf("lazuli: replay events: %w", err))
	}
}

func replayOptions(options []ReplayOption) ReplayOptions {
	var opts ReplayOptions
	for _, option := range options {
		if option != nil {
			option(&opts)
		}
	}
	return opts
}

func replayFailureError(failures []EventReplayFailure) error {
	if len(failures) == 0 {
		return nil
	}

	errs := make([]error, 0, len(failures))
	for _, failure := range failures {
		errs = append(errs, failure.Err)
	}
	return errors.Join(errs...)
}

func isNilEventStore(store EventStore) bool {
	if store == nil {
		return true
	}

	value := reflect.ValueOf(store)
	switch value.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Ptr, reflect.Slice:
		return value.IsNil()
	default:
		return false
	}
}
