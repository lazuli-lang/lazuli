package lazuli

// EventGroup describes a family of related events declared by a feature.
// Generated code emits these values from `event_group` declarations so
// tooling and runtime integrations can inspect the event contract.
type EventGroup struct {
	Pattern  string
	Resource string
	Events   []EventDescriptor
}

// EventDescriptor describes a single event contained in an EventGroup.
type EventDescriptor struct {
	Name        string
	PayloadType string
}

// EventEmit declares a single event publication from a command, workflow
// transition, job, or webhook. The runtime publishes after the surrounding
// transaction commits; consumers receive via the event bus.
type EventEmit struct {
	// Name is the event name (e.g. "customer_created"). The runtime
	// qualifies it with the feature owning the emit unless the name is
	// already qualified.
	Name string

	// From is the payload source. When `FromCreates` / `FromUpdates` /
	// `FromDeletes`, the runtime derives the payload from the surrounding
	// command effect's bindings by name match. When empty, the command must
	// also declare explicit `Bind` here.
	From EventDeriveFrom

	// Bind is an explicit payload binding. Used when payload fields don't
	// align with the effect's binding names (e.g.
	// `emits customer_reassigned` with `to_owner_id = input.owner_id`).
	Bind Bindings
}

// EventDeriveFrom names the effect a derived emit reads its payload from.
type EventDeriveFrom string

const (
	FromExplicit EventDeriveFrom = ""        // body has explicit Bind
	FromCreates  EventDeriveFrom = "creates" // emits from creates effect
	FromUpdates  EventDeriveFrom = "updates" // emits from updates effect
	FromDeletes  EventDeriveFrom = "deletes" // emits from deletes effect
)

// EventTraceEmit is the equivalent for `event.trace` events. Trace events do
// not enter the reaction graph; jobs cannot trigger from them. The runtime
// publishes them to observability sinks only.
type EventTraceEmit struct {
	Name string
	Bind Bindings
}
