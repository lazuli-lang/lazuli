package lazuli

import (
	"context"
	"log/slog"
	"sync"
	"time"
)

// Event is the typed envelope every subscriber receives. The runtime builds
// it from a Command/Workflow/Job's `Emits` declaration after the surrounding
// transaction commits.
type Event struct {
	// Name is the event name as declared in the DSL (`customer_created`,
	// `tag_assignment_created`, etc.). Subscribers register against this
	// name.
	Name string

	// Trace marks `event.trace` events. Trace events are observability
	// only — the spike publishes them through the same bus, but the
	// reaction-graph layer (when it lands) refuses to subscribe to them
	// from production jobs.
	Trace bool

	// Tenant is the tenant scope under which the producing transaction ran.
	Tenant *Tenant

	// Actor identifies who/what triggered the producing operation.
	Actor Actor

	// UserID is non-nil when Actor == ActorUser.
	UserID *ID

	// Payload carries the event's typed fields, derived from the producing
	// effect's bindings (`from creates|updates|deletes`) or from the
	// explicit `Bind` map on EventEmit.
	Payload map[string]any

	// OccurredAt is the time the producing transaction committed.
	OccurredAt time.Time
}

// Subscriber is the runtime contract for a function that receives one event.
// Returning an error logs but does not roll back the producing transaction:
// emits are post-commit and best-effort by design.
type Subscriber func(ctx context.Context, e Event) error

// eventBus is the in-process event bus. v0 spike: synchronous in-process
// delivery in the order subscribers were registered. Future cuts add
// goroutine-per-subscriber, river-queued durable delivery, and cross-service
// transport via NATS/Redis adapters.
var eventBus = struct {
	sync.RWMutex
	subscribers map[string][]Subscriber
}{
	subscribers: make(map[string][]Subscriber),
}

// Subscribe registers fn to receive every Event with the given name.
// Generated cross-feature reaction code calls this at package init.
func Subscribe(name string, fn Subscriber) {
	eventBus.Lock()
	defer eventBus.Unlock()
	eventBus.subscribers[name] = append(eventBus.subscribers[name], fn)
}

// Publish delivers the event to every registered subscriber. The runtime
// invokes this from `Command.Handle` after a successful commit. Subscriber
// errors are logged but never propagated — the producing tx has already
// committed.
func Publish(ctx context.Context, e Event) {
	eventBus.RLock()
	subs := eventBus.subscribers[e.Name]
	dup := make([]Subscriber, len(subs))
	copy(dup, subs)
	eventBus.RUnlock()

	if len(dup) == 0 {
		slog.Debug("lazuli event published with no subscribers", "name", e.Name)
		return
	}
	for _, fn := range dup {
		if err := fn(ctx, e); err != nil {
			slog.Error("lazuli event subscriber failed",
				"event", e.Name, "error", err)
		}
	}
}

// SubscribersOf returns a snapshot of subscribers registered for the given
// event name. Useful for diagnostics and tests.
func SubscribersOf(name string) []Subscriber {
	eventBus.RLock()
	defer eventBus.RUnlock()
	subs := eventBus.subscribers[name]
	dup := make([]Subscriber, len(subs))
	copy(dup, subs)
	return dup
}
