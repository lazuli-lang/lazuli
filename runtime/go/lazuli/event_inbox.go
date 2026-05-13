package lazuli

import (
	"context"
	"sync"
	"time"
)

// EventInboxStatus is the consumer-side lifecycle state of an inbound event.
type EventInboxStatus string

const (
	EventInboxReceived   EventInboxStatus = "received"
	EventInboxProcessing EventInboxStatus = "processing"
	EventInboxRetrying   EventInboxStatus = "retrying"
	EventInboxProcessed  EventInboxStatus = "processed"
	EventInboxDead       EventInboxStatus = "dead"
	EventInboxSkipped    EventInboxStatus = "skipped"
)

// EventInboxRecord is the inbound durable message shape used by consumers for
// idempotency, retry scheduling, and processing visibility.
type EventInboxRecord struct {
	ID          string
	Source      string
	Consumer    string
	Event       Event
	DedupeKey   EventIdempotencyKey
	Status      EventInboxStatus
	Retry       EventRetryMetadata
	ReceivedAt  time.Time
	UpdatedAt   time.Time
	CompletedAt time.Time
	Metadata    map[string]string
}

// EventInboxRetryVisibility is a consumer-facing view of when an inbound
// record can be claimed again and how much retry budget remains.
type EventInboxRetryVisibility struct {
	Status        EventInboxStatus
	Attempts      uint32
	MaxAttempts   uint32
	LastAttemptAt time.Time
	NextAttemptAt time.Time
	VisibleAt     time.Time
	LastError     string
	Exhausted     bool
	Visible       bool
}

// EventInboxFilter selects inbox records from a store. Zero values are
// wildcards.
type EventInboxFilter struct {
	Status   EventInboxStatus
	Source   string
	Consumer string
	Tenant   string
}

// EventInboxPlan is a snapshot of inbound records claimed for processing.
type EventInboxPlan struct {
	PlannedAt time.Time
	Records   []EventInboxRecord
}

// EventInboxStore persists inbound event records for idempotent consumers.
type EventInboxStore interface {
	Record(ctx context.Context, record EventInboxRecord) (EventInboxRecord, error)
	PlanReady(ctx context.Context, readyAt time.Time, limit int) (EventInboxPlan, error)
	MarkProcessed(ctx context.Context, id string, at time.Time) (EventInboxRecord, error)
	MarkSkipped(ctx context.Context, id string, at time.Time) (EventInboxRecord, error)
	MarkFailed(ctx context.Context, id string, at time.Time, err error) (EventInboxRecord, error)
	List(ctx context.Context, filter EventInboxFilter) ([]EventInboxRecord, error)
}

// EventInboxDedupeKeyForEvent builds a tenant-scoped dedupe key for an inbound
// event. Source separates transports/producers and consumer separates handlers.
func EventInboxDedupeKeyForEvent(source, consumer string, event Event, key string) EventIdempotencyKey {
	namespace := "inbox"
	if source != "" {
		namespace += ":" + source
	}
	if consumer != "" {
		namespace += ":" + consumer
	}
	return EventIdempotencyKeyForEvent(namespace, event, key)
}

// CanTransitionTo reports whether an inbox record can move from status to next.
func (status EventInboxStatus) CanTransitionTo(next EventInboxStatus) bool {
	return eventInboxStatusToMessageStatus(status).CanTransitionTo(eventInboxStatusToMessageStatus(next))
}

// Terminal reports whether status is a final inbox state.
func (status EventInboxStatus) Terminal() bool {
	return eventInboxStatusToMessageStatus(status).Terminal()
}

// TransitionEventInboxStatus returns record with its status advanced to next,
// preserving the original value when the transition is invalid.
func TransitionEventInboxStatus(
	record EventInboxRecord,
	next EventInboxStatus,
	at time.Time,
) (EventInboxRecord, error) {
	message, err := TransitionEventMessageStatus(
		eventInboxRecordToMessage(record),
		eventInboxStatusToMessageStatus(next),
		at,
	)
	if err != nil {
		return record, err
	}
	return eventInboxRecordFromMessage(message), nil
}

// ScheduleEventInboxRetry records a failed processing attempt. It returns a
// retrying record while attempts remain, or a dead record once exhausted.
func ScheduleEventInboxRetry(
	record EventInboxRecord,
	at time.Time,
	err error,
) (EventInboxRecord, error) {
	message, scheduleErr := ScheduleEventMessageRetry(eventInboxRecordToMessage(record), at, err)
	if scheduleErr != nil {
		return record, scheduleErr
	}
	return eventInboxRecordFromMessage(message), nil
}

// RetryVisibility returns a snapshot of retry state and current visibility.
func (record EventInboxRecord) RetryVisibility(at time.Time) EventInboxRetryVisibility {
	return EventInboxRetryVisibilityAt(record, at)
}

// EventInboxRetryVisibilityAt returns a snapshot of retry state and whether
// record is visible for processing at the supplied time.
func EventInboxRetryVisibilityAt(record EventInboxRecord, at time.Time) EventInboxRetryVisibility {
	status := normalizeEventInboxStatus(record.Status)
	retry := normalizeEventRetryMetadata(record.Retry)
	visibleAt := record.ReceivedAt
	if status == EventInboxRetrying && !retry.NextAttemptAt.IsZero() {
		visibleAt = retry.NextAttemptAt
	}
	if visibleAt.IsZero() {
		visibleAt = record.UpdatedAt
	}

	visible := status == EventInboxReceived || status == EventInboxRetrying
	if visible && !visibleAt.IsZero() && visibleAt.After(at) {
		visible = false
	}

	return EventInboxRetryVisibility{
		Status:        status,
		Attempts:      retry.Attempts,
		MaxAttempts:   retry.MaxAttempts,
		LastAttemptAt: retry.LastAttemptAt,
		NextAttemptAt: retry.NextAttemptAt,
		VisibleAt:     visibleAt,
		LastError:     retry.LastError,
		Exhausted:     retry.Exhausted(),
		Visible:       visible,
	}
}

// MemoryEventInboxStore is an in-process EventInboxStore reference
// implementation. It is safe for concurrent use. Production deployments that
// need durable cross-process dedupe should bind a persistent adapter.
//
// The zero value is ready to use.
type MemoryEventInboxStore struct {
	mu      sync.Mutex
	planner *MemoryEventMessagePlanner
}

var _ EventInboxStore = (*MemoryEventInboxStore)(nil)

// NewMemoryEventInboxStore returns an empty in-process inbox store.
func NewMemoryEventInboxStore() *MemoryEventInboxStore {
	return &MemoryEventInboxStore{
		planner: NewMemoryEventMessagePlanner(),
	}
}

// Record stores an inbound event record and returns the normalized snapshot.
func (s *MemoryEventInboxStore) Record(
	ctx context.Context,
	record EventInboxRecord,
) (EventInboxRecord, error) {
	message, err := s.eventMessagePlanner().Add(ctx, eventInboxRecordToMessage(record))
	if err != nil {
		return EventInboxRecord{}, err
	}
	return eventInboxRecordFromMessage(message), nil
}

// PlanReady claims received or retrying records due at or before readyAt. A
// limit less than or equal to zero means no limit.
func (s *MemoryEventInboxStore) PlanReady(
	ctx context.Context,
	readyAt time.Time,
	limit int,
) (EventInboxPlan, error) {
	plan, err := s.eventMessagePlanner().PlanReady(ctx, readyAt, limit)
	if err != nil {
		return EventInboxPlan{}, err
	}

	out := EventInboxPlan{
		PlannedAt: plan.PlannedAt,
		Records:   make([]EventInboxRecord, len(plan.Messages)),
	}
	for i, message := range plan.Messages {
		out.Records[i] = eventInboxRecordFromMessage(message)
	}
	return out, nil
}

// MarkProcessed marks a processing record as processed.
func (s *MemoryEventInboxStore) MarkProcessed(
	ctx context.Context,
	id string,
	at time.Time,
) (EventInboxRecord, error) {
	message, err := s.eventMessagePlanner().MarkDelivered(ctx, id, at)
	if err != nil {
		return EventInboxRecord{}, err
	}
	return eventInboxRecordFromMessage(message), nil
}

// MarkSkipped marks a received or processing record as intentionally skipped.
func (s *MemoryEventInboxStore) MarkSkipped(
	ctx context.Context,
	id string,
	at time.Time,
) (EventInboxRecord, error) {
	message, err := s.eventMessagePlanner().MarkSkipped(ctx, id, at)
	if err != nil {
		return EventInboxRecord{}, err
	}
	return eventInboxRecordFromMessage(message), nil
}

// MarkFailed records a failed processing attempt and schedules the next retry
// or dead-letters the record when the retry budget is exhausted.
func (s *MemoryEventInboxStore) MarkFailed(
	ctx context.Context,
	id string,
	at time.Time,
	err error,
) (EventInboxRecord, error) {
	message, markErr := s.eventMessagePlanner().MarkFailed(ctx, id, at, err)
	if markErr != nil {
		return EventInboxRecord{}, markErr
	}
	return eventInboxRecordFromMessage(message), nil
}

// List returns records matching filter in insertion order.
func (s *MemoryEventInboxStore) List(
	ctx context.Context,
	filter EventInboxFilter,
) ([]EventInboxRecord, error) {
	messages, err := s.eventMessagePlanner().List(ctx, filter.eventMessageFilter())
	if err != nil {
		return nil, err
	}

	records := make([]EventInboxRecord, 0, len(messages))
	for _, message := range messages {
		record := eventInboxRecordFromMessage(message)
		if filter.Source != "" && record.Source != filter.Source {
			continue
		}
		records = append(records, record)
	}
	return records, nil
}

func (s *MemoryEventInboxStore) eventMessagePlanner() *MemoryEventMessagePlanner {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.planner == nil {
		s.planner = NewMemoryEventMessagePlanner()
	}
	return s.planner
}

func (filter EventInboxFilter) eventMessageFilter() EventMessageFilter {
	out := EventMessageFilter{
		Direction: EventMessageInbox,
		Consumer:  filter.Consumer,
		Tenant:    filter.Tenant,
	}
	if filter.Status != "" {
		out.Status = eventInboxStatusToMessageStatus(filter.Status)
	}
	return out
}

func eventInboxRecordToMessage(record EventInboxRecord) EventMessageEnvelope {
	return EventMessageEnvelope{
		ID:             record.ID,
		Direction:      EventMessageInbox,
		Source:         record.Source,
		Consumer:       record.Consumer,
		Event:          cloneEvent(record.Event),
		IdempotencyKey: record.DedupeKey,
		Status:         eventInboxStatusToMessageStatus(record.Status),
		Retry:          record.Retry,
		CreatedAt:      record.ReceivedAt,
		UpdatedAt:      record.UpdatedAt,
		CompletedAt:    record.CompletedAt,
		Metadata:       cloneEventMessageMetadata(record.Metadata),
	}
}

func eventInboxRecordFromMessage(message EventMessageEnvelope) EventInboxRecord {
	message = cloneEventMessageEnvelope(message)
	return EventInboxRecord{
		ID:          message.ID,
		Source:      message.Source,
		Consumer:    message.Consumer,
		Event:       message.Event,
		DedupeKey:   message.IdempotencyKey,
		Status:      eventInboxStatusFromMessageStatus(message.Status),
		Retry:       normalizeEventRetryMetadata(message.Retry),
		ReceivedAt:  message.CreatedAt,
		UpdatedAt:   message.UpdatedAt,
		CompletedAt: message.CompletedAt,
		Metadata:    message.Metadata,
	}
}

func eventInboxStatusToMessageStatus(status EventInboxStatus) EventMessageStatus {
	switch normalizeEventInboxStatus(status) {
	case EventInboxReceived:
		return EventMessagePending
	case EventInboxProcessing:
		return EventMessageClaimed
	case EventInboxRetrying:
		return EventMessageRetrying
	case EventInboxProcessed:
		return EventMessageDelivered
	case EventInboxDead:
		return EventMessageDead
	case EventInboxSkipped:
		return EventMessageSkipped
	default:
		return EventMessageStatus(status)
	}
}

func eventInboxStatusFromMessageStatus(status EventMessageStatus) EventInboxStatus {
	switch normalizeEventMessageStatus(status) {
	case EventMessagePending:
		return EventInboxReceived
	case EventMessageClaimed:
		return EventInboxProcessing
	case EventMessageRetrying:
		return EventInboxRetrying
	case EventMessageDelivered:
		return EventInboxProcessed
	case EventMessageDead:
		return EventInboxDead
	case EventMessageSkipped:
		return EventInboxSkipped
	default:
		return EventInboxStatus(status)
	}
}

func normalizeEventInboxStatus(status EventInboxStatus) EventInboxStatus {
	if status == "" {
		return EventInboxReceived
	}
	return status
}
