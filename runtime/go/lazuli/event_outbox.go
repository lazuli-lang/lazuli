package lazuli

import (
	"context"
	"errors"
	"fmt"
	"strconv"
	"sync"
	"time"

	"lazuli.dev/runtime/lazuli/jobs"
)

// EventMessageDirection identifies whether a durable event message is waiting
// to be published out of this process or accepted into a consumer inbox.
type EventMessageDirection string

const (
	// EventMessageOutbox is the producer-side outbox direction.
	EventMessageOutbox EventMessageDirection = "outbox"
	// EventMessageInbox is the consumer-side inbox direction.
	EventMessageInbox EventMessageDirection = "inbox"
)

// EventMessageStatus is the lifecycle state of an outbox or inbox message.
type EventMessageStatus string

const (
	EventMessagePending   EventMessageStatus = "pending"
	EventMessageClaimed   EventMessageStatus = "claimed"
	EventMessageRetrying  EventMessageStatus = "retrying"
	EventMessageDelivered EventMessageStatus = "delivered"
	EventMessageDead      EventMessageStatus = "dead"
	EventMessageSkipped   EventMessageStatus = "skipped"
)

var (
	// ErrEventMessageDuplicate is returned when a message id or idempotency key
	// is already present in a planner.
	ErrEventMessageDuplicate = errors.New("lazuli: event message duplicate")

	// ErrEventMessageNotFound is returned when a planner cannot find the
	// requested message.
	ErrEventMessageNotFound = errors.New("lazuli: event message not found")

	// ErrEventMessageInvalidTransition is returned when a status change is not
	// allowed by the event message lifecycle.
	ErrEventMessageInvalidTransition = errors.New("lazuli: event message invalid status transition")
)

// EventIdempotencyKey scopes duplicate suppression for an outbox or inbox
// message. Namespace separates producers/consumers, Tenant separates tenant
// scopes, and Key is the evaluated idempotency expression.
type EventIdempotencyKey struct {
	Namespace string
	Tenant    string
	Key       string
}

// Empty reports whether key has no evaluated idempotency value.
func (k EventIdempotencyKey) Empty() bool {
	return k.Key == ""
}

// String returns a stable diagnostic representation of key.
func (k EventIdempotencyKey) String() string {
	if k.Empty() {
		return ""
	}
	return k.Namespace + ":" + k.Tenant + ":" + k.Key
}

// EventIdempotencyKeyForEvent builds a tenant-scoped idempotency key for event.
func EventIdempotencyKeyForEvent(namespace string, event Event, key string) EventIdempotencyKey {
	tenant := ""
	if event.Tenant != nil {
		tenant = strconv.FormatInt(int64(event.Tenant.OrgID), 10)
	}
	return EventIdempotencyKey{
		Namespace: namespace,
		Tenant:    tenant,
		Key:       key,
	}
}

// EventRetryMetadata tracks retry policy, attempt count, and next scheduling
// time for an outbox or inbox message. MaxAttempts is total attempts including
// the initial delivery. A zero MaxAttempts normalizes to one attempt.
type EventRetryMetadata struct {
	Policy        RetryPolicy
	Attempts      uint32
	MaxAttempts   uint32
	LastAttemptAt time.Time
	NextAttemptAt time.Time
	LastError     string
}

// NewEventRetryMetadata initializes retry metadata from policy. Nil policy
// means exactly one attempt.
func NewEventRetryMetadata(policy *RetryPolicy) EventRetryMetadata {
	metadata := EventRetryMetadata{MaxAttempts: 1}
	if policy != nil {
		metadata.Policy = *policy
		metadata.MaxAttempts = policy.Count + 1
	}
	if metadata.MaxAttempts == 0 {
		metadata.MaxAttempts = 1
	}
	return metadata
}

// Exhausted reports whether no more attempts are available.
func (m EventRetryMetadata) Exhausted() bool {
	m = normalizeEventRetryMetadata(m)
	return m.Attempts >= m.MaxAttempts
}

// NextDelay returns the delay before the next attempt based on the attempts
// already consumed.
func (m EventRetryMetadata) NextDelay() time.Duration {
	if m.Attempts == 0 {
		return 0
	}
	return jobs.NextDelay(m.Policy, m.Attempts)
}

// EventMessageEnvelope is the durable message shape shared by outbox and inbox
// adapters. It wraps the existing runtime Event without changing EventStore.
type EventMessageEnvelope struct {
	ID             string
	Direction      EventMessageDirection
	Source         string
	Destination    string
	Consumer       string
	Event          Event
	IdempotencyKey EventIdempotencyKey
	Status         EventMessageStatus
	Retry          EventRetryMetadata
	CreatedAt      time.Time
	UpdatedAt      time.Time
	CompletedAt    time.Time
	Metadata       map[string]string
}

// EventMessageFilter selects messages from a planner. Zero values are
// wildcards.
type EventMessageFilter struct {
	Direction EventMessageDirection
	Status    EventMessageStatus
	Consumer  string
	Tenant    string
}

// EventMessagePlan is a snapshot of messages claimed for delivery.
type EventMessagePlan struct {
	PlannedAt time.Time
	Messages  []EventMessageEnvelope
}

// CanTransitionTo reports whether a message can move from status to next.
func (status EventMessageStatus) CanTransitionTo(next EventMessageStatus) bool {
	status = normalizeEventMessageStatus(status)
	next = normalizeEventMessageStatus(next)
	if status == next {
		return true
	}

	switch status {
	case EventMessagePending:
		return next == EventMessageClaimed ||
			next == EventMessageDelivered ||
			next == EventMessageDead ||
			next == EventMessageSkipped
	case EventMessageRetrying:
		return next == EventMessageClaimed || next == EventMessageDead
	case EventMessageClaimed:
		return next == EventMessageDelivered ||
			next == EventMessageRetrying ||
			next == EventMessageDead ||
			next == EventMessageSkipped
	default:
		return false
	}
}

// Terminal reports whether status is a final message state.
func (status EventMessageStatus) Terminal() bool {
	switch normalizeEventMessageStatus(status) {
	case EventMessageDelivered, EventMessageDead, EventMessageSkipped:
		return true
	default:
		return false
	}
}

// TransitionEventMessageStatus returns message with its status advanced to
// next, preserving the original value when the transition is invalid.
func TransitionEventMessageStatus(
	message EventMessageEnvelope,
	next EventMessageStatus,
	at time.Time,
) (EventMessageEnvelope, error) {
	current := normalizeEventMessageStatus(message.Status)
	next = normalizeEventMessageStatus(next)
	if !current.CanTransitionTo(next) {
		return message, fmt.Errorf("%w: %s to %s", ErrEventMessageInvalidTransition, current, next)
	}

	message.Status = next
	if message.CreatedAt.IsZero() {
		message.CreatedAt = at
	}
	message.UpdatedAt = at
	if next.Terminal() {
		message.CompletedAt = at
	}
	return message, nil
}

// ScheduleEventMessageRetry records a failed attempt. It returns a retrying
// message while attempts remain, or a dead message once the retry budget is
// exhausted.
func ScheduleEventMessageRetry(
	message EventMessageEnvelope,
	at time.Time,
	err error,
) (EventMessageEnvelope, error) {
	message.Retry = normalizeEventRetryMetadata(message.Retry)
	if err != nil {
		message.Retry.LastError = err.Error()
	} else {
		message.Retry.LastError = ""
	}

	if message.Retry.Exhausted() {
		message.Retry.NextAttemptAt = time.Time{}
		return TransitionEventMessageStatus(message, EventMessageDead, at)
	}

	message.Retry.NextAttemptAt = at.Add(message.Retry.NextDelay())
	return TransitionEventMessageStatus(message, EventMessageRetrying, at)
}

// MemoryEventMessagePlanner is an in-process reference planner for outbox and
// inbox messages. It is safe for concurrent use. Production adapters can bind a
// durable implementation with the same envelope and status semantics.
//
// The zero value is ready to use.
type MemoryEventMessagePlanner struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu          sync.RWMutex
	nextID      uint64
	messages    []EventMessageEnvelope
	index       map[string]int
	idempotency map[EventIdempotencyKey]string
}

// NewMemoryEventMessagePlanner returns an empty in-process planner.
func NewMemoryEventMessagePlanner() *MemoryEventMessagePlanner {
	return &MemoryEventMessagePlanner{
		index:       make(map[string]int),
		idempotency: make(map[EventIdempotencyKey]string),
	}
}

// Add stores message and returns the normalized snapshot. Duplicate IDs or
// active idempotency keys return ErrEventMessageDuplicate.
func (p *MemoryEventMessagePlanner) Add(
	ctx context.Context,
	message EventMessageEnvelope,
) (EventMessageEnvelope, error) {
	if err := eventMessageContextErr(ctx); err != nil {
		return EventMessageEnvelope{}, err
	}

	p.mu.Lock()
	defer p.mu.Unlock()
	p.ensureLocked()

	message = p.normalizeMessageLocked(message)
	if _, exists := p.index[message.ID]; exists {
		return EventMessageEnvelope{}, fmt.Errorf("%w: id %q", ErrEventMessageDuplicate, message.ID)
	}
	if !message.IdempotencyKey.Empty() {
		if existingID, exists := p.idempotency[message.IdempotencyKey]; exists {
			return EventMessageEnvelope{}, fmt.Errorf(
				"%w: idempotency key %q already assigned to %q",
				ErrEventMessageDuplicate,
				message.IdempotencyKey.String(),
				existingID,
			)
		}
	}

	p.index[message.ID] = len(p.messages)
	if !message.IdempotencyKey.Empty() {
		p.idempotency[message.IdempotencyKey] = message.ID
	}
	p.messages = append(p.messages, cloneEventMessageEnvelope(message))
	return cloneEventMessageEnvelope(message), nil
}

// PlanReady claims pending or retrying messages due at or before readyAt. A
// limit less than or equal to zero means no limit.
func (p *MemoryEventMessagePlanner) PlanReady(
	ctx context.Context,
	readyAt time.Time,
	limit int,
) (EventMessagePlan, error) {
	if err := eventMessageContextErr(ctx); err != nil {
		return EventMessagePlan{}, err
	}

	p.mu.Lock()
	defer p.mu.Unlock()
	p.ensureLocked()

	plan := EventMessagePlan{PlannedAt: readyAt}
	for i := range p.messages {
		if limit > 0 && len(plan.Messages) >= limit {
			break
		}
		message := p.messages[i]
		status := normalizeEventMessageStatus(message.Status)
		if status != EventMessagePending && status != EventMessageRetrying {
			continue
		}
		if !message.Retry.NextAttemptAt.IsZero() && message.Retry.NextAttemptAt.After(readyAt) {
			continue
		}

		message.Retry = normalizeEventRetryMetadata(message.Retry)
		message.Retry.Attempts++
		message.Retry.LastAttemptAt = readyAt
		message.Retry.NextAttemptAt = time.Time{}

		claimed, err := TransitionEventMessageStatus(message, EventMessageClaimed, readyAt)
		if err != nil {
			return EventMessagePlan{}, err
		}
		p.messages[i] = cloneEventMessageEnvelope(claimed)
		plan.Messages = append(plan.Messages, cloneEventMessageEnvelope(claimed))
	}
	return plan, nil
}

// MarkDelivered marks a claimed message as delivered.
func (p *MemoryEventMessagePlanner) MarkDelivered(
	ctx context.Context,
	id string,
	at time.Time,
) (EventMessageEnvelope, error) {
	return p.transition(ctx, id, at, func(message EventMessageEnvelope) (EventMessageEnvelope, error) {
		return TransitionEventMessageStatus(message, EventMessageDelivered, at)
	})
}

// MarkSkipped marks a claimed or pending inbox message as intentionally skipped,
// usually because a consumer observed an already-processed idempotency key.
func (p *MemoryEventMessagePlanner) MarkSkipped(
	ctx context.Context,
	id string,
	at time.Time,
) (EventMessageEnvelope, error) {
	return p.transition(ctx, id, at, func(message EventMessageEnvelope) (EventMessageEnvelope, error) {
		return TransitionEventMessageStatus(message, EventMessageSkipped, at)
	})
}

// MarkFailed records a failed claimed attempt and schedules the next retry or
// dead-letters the message when the retry budget is exhausted.
func (p *MemoryEventMessagePlanner) MarkFailed(
	ctx context.Context,
	id string,
	at time.Time,
	err error,
) (EventMessageEnvelope, error) {
	return p.transition(ctx, id, at, func(message EventMessageEnvelope) (EventMessageEnvelope, error) {
		return ScheduleEventMessageRetry(message, at, err)
	})
}

// List returns messages matching filter in insertion order.
func (p *MemoryEventMessagePlanner) List(
	ctx context.Context,
	filter EventMessageFilter,
) ([]EventMessageEnvelope, error) {
	if err := eventMessageContextErr(ctx); err != nil {
		return nil, err
	}

	p.mu.RLock()
	defer p.mu.RUnlock()

	out := make([]EventMessageEnvelope, 0, len(p.messages))
	for _, message := range p.messages {
		if filter.matches(message) {
			out = append(out, cloneEventMessageEnvelope(message))
		}
	}
	return out, nil
}

func (p *MemoryEventMessagePlanner) transition(
	ctx context.Context,
	id string,
	at time.Time,
	fn func(EventMessageEnvelope) (EventMessageEnvelope, error),
) (EventMessageEnvelope, error) {
	if err := eventMessageContextErr(ctx); err != nil {
		return EventMessageEnvelope{}, err
	}

	p.mu.Lock()
	defer p.mu.Unlock()
	p.ensureLocked()

	idx, ok := p.index[id]
	if !ok {
		return EventMessageEnvelope{}, ErrEventMessageNotFound
	}

	updated, err := fn(cloneEventMessageEnvelope(p.messages[idx]))
	if err != nil {
		return EventMessageEnvelope{}, err
	}
	p.messages[idx] = cloneEventMessageEnvelope(updated)
	return cloneEventMessageEnvelope(updated), nil
}

func (p *MemoryEventMessagePlanner) ensureLocked() {
	if p.index == nil {
		p.index = make(map[string]int, len(p.messages))
		for i, message := range p.messages {
			p.index[message.ID] = i
		}
	}
	if p.idempotency == nil {
		p.idempotency = make(map[EventIdempotencyKey]string)
		for _, message := range p.messages {
			if !message.IdempotencyKey.Empty() {
				p.idempotency[message.IdempotencyKey] = message.ID
			}
		}
	}
}

func (p *MemoryEventMessagePlanner) normalizeMessageLocked(message EventMessageEnvelope) EventMessageEnvelope {
	message = cloneEventMessageEnvelope(message)
	now := p.nowLocked().UTC()
	if message.ID == "" {
		message.ID = p.nextIDLocked()
	}
	if message.Direction == "" {
		message.Direction = EventMessageOutbox
	}
	message.Status = normalizeEventMessageStatus(message.Status)
	message.Retry = normalizeEventRetryMetadata(message.Retry)
	if message.CreatedAt.IsZero() {
		message.CreatedAt = now
	}
	if message.UpdatedAt.IsZero() {
		message.UpdatedAt = message.CreatedAt
	}
	return message
}

func (p *MemoryEventMessagePlanner) nextIDLocked() string {
	for {
		p.nextID++
		id := "event-message-" + strconv.FormatUint(p.nextID, 10)
		if _, exists := p.index[id]; !exists {
			return id
		}
	}
}

func (p *MemoryEventMessagePlanner) nowLocked() time.Time {
	if p.Clock != nil {
		return p.Clock()
	}
	return time.Now()
}

func (f EventMessageFilter) matches(message EventMessageEnvelope) bool {
	if f.Direction != "" && f.Direction != message.Direction {
		return false
	}
	if f.Status != "" && normalizeEventMessageStatus(f.Status) != normalizeEventMessageStatus(message.Status) {
		return false
	}
	if f.Consumer != "" && f.Consumer != message.Consumer {
		return false
	}
	if f.Tenant != "" {
		tenant := ""
		if message.Event.Tenant != nil {
			tenant = strconv.FormatInt(int64(message.Event.Tenant.OrgID), 10)
		}
		if f.Tenant != tenant {
			return false
		}
	}
	return true
}

func normalizeEventMessageStatus(status EventMessageStatus) EventMessageStatus {
	if status == "" {
		return EventMessagePending
	}
	return status
}

func normalizeEventRetryMetadata(metadata EventRetryMetadata) EventRetryMetadata {
	if metadata.MaxAttempts == 0 {
		metadata.MaxAttempts = metadata.Policy.Count + 1
	}
	if metadata.MaxAttempts == 0 {
		metadata.MaxAttempts = 1
	}
	return metadata
}

func eventMessageContextErr(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
		return nil
	}
}

func cloneEventMessageEnvelope(message EventMessageEnvelope) EventMessageEnvelope {
	message.Event = cloneEvent(message.Event)
	message.Metadata = cloneEventMessageMetadata(message.Metadata)
	return message
}

func cloneEventMessageMetadata(metadata map[string]string) map[string]string {
	if metadata == nil {
		return nil
	}
	out := make(map[string]string, len(metadata))
	for key, value := range metadata {
		out[key] = value
	}
	return out
}
