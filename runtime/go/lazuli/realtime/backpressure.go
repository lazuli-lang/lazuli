// Package realtime contains small helpers for websocket, SSE, and other
// realtime adapters.
package realtime

import (
	"sync"
	"time"
)

const (
	// DefaultMaxQueuedMessages is the fallback bounded queue capacity.
	DefaultMaxQueuedMessages = 64

	// DefaultReconnectBaseDelay is the first reconnect retry delay.
	DefaultReconnectBaseDelay = 500 * time.Millisecond
	// DefaultReconnectMaxDelay caps exponential reconnect retry backoff.
	DefaultReconnectMaxDelay = 30 * time.Second
)

// ConnectionState describes the lifecycle of a realtime connection.
type ConnectionState string

const (
	ConnectionStateDisconnected ConnectionState = "disconnected"
	ConnectionStateConnecting   ConnectionState = "connecting"
	ConnectionStateConnected    ConnectionState = "connected"
	ConnectionStateReconnecting ConnectionState = "reconnecting"
	ConnectionStateClosed       ConnectionState = "closed"
)

// Active reports whether the connection can send realtime messages now.
func (s ConnectionState) Active() bool {
	return s == ConnectionStateConnected
}

// Terminal reports whether the state represents a permanently closed
// connection.
func (s ConnectionState) Terminal() bool {
	return s == ConnectionStateClosed
}

// ConnectionStateSnapshot is a point-in-time connection lifecycle view.
type ConnectionStateSnapshot struct {
	State ConnectionState `json:"state"`
}

// ConnectionStateTracker stores the current connection state.
//
// The zero value is ready to use and starts disconnected.
type ConnectionStateTracker struct {
	mu    sync.RWMutex
	state ConnectionState
}

// NewConnectionStateTracker returns a tracker initialized with state. Empty
// state is treated as disconnected.
func NewConnectionStateTracker(state ConnectionState) *ConnectionStateTracker {
	tracker := &ConnectionStateTracker{}
	tracker.SetState(state)
	return tracker
}

// State returns the current connection state.
func (t *ConnectionStateTracker) State() ConnectionState {
	if t == nil {
		return ConnectionStateDisconnected
	}

	t.mu.RLock()
	defer t.mu.RUnlock()
	return normalizeConnectionState(t.state)
}

// SetState updates the connection state and returns the normalized value that
// was stored.
func (t *ConnectionStateTracker) SetState(state ConnectionState) ConnectionState {
	if t == nil {
		return normalizeConnectionState(state)
	}

	state = normalizeConnectionState(state)
	t.mu.Lock()
	defer t.mu.Unlock()
	t.state = state
	return state
}

// Snapshot returns a point-in-time copy of the current state.
func (t *ConnectionStateTracker) Snapshot() ConnectionStateSnapshot {
	return ConnectionStateSnapshot{State: t.State()}
}

func normalizeConnectionState(state ConnectionState) ConnectionState {
	if state == "" {
		return ConnectionStateDisconnected
	}
	return state
}

// DropPolicy describes how a bounded realtime queue behaves when it is full.
type DropPolicy string

const (
	// DropNewest drops the message that was just offered when the queue is
	// already full.
	DropNewest DropPolicy = "drop_newest"
	// DropOldest drops the oldest queued message to make room for the new
	// message.
	DropOldest DropPolicy = "drop_oldest"
)

// Normalize returns a supported drop policy. Empty or unknown values use
// DropNewest so already-accepted messages keep their delivery order.
func (p DropPolicy) Normalize() DropPolicy {
	if p == DropOldest {
		return DropOldest
	}
	return DropNewest
}

// MessageQueueOptions configures a bounded realtime message queue.
type MessageQueueOptions struct {
	// MaxQueuedMessages is the maximum number of messages held in memory.
	// Values less than one use DefaultMaxQueuedMessages.
	MaxQueuedMessages int

	// DropPolicy controls which message is dropped when the queue is full.
	// Empty or unknown values use DropNewest.
	DropPolicy DropPolicy
}

// Normalize returns options with defaults applied.
func (o MessageQueueOptions) Normalize() MessageQueueOptions {
	if o.MaxQueuedMessages < 1 {
		o.MaxQueuedMessages = DefaultMaxQueuedMessages
	}
	o.DropPolicy = o.DropPolicy.Normalize()
	return o
}

// EnqueueResult reports how an offered message affected a queue.
type EnqueueResult[T any] struct {
	// Accepted reports whether the offered message was queued.
	Accepted bool
	// Dropped reports whether any message was dropped.
	Dropped bool
	// DropPolicy is the policy used when Dropped is true.
	DropPolicy DropPolicy
	// DroppedMessage is the message that was dropped. It is the offered
	// message for DropNewest and the evicted queued message for DropOldest.
	DroppedMessage T
}

// BackpressureMetricsSnapshot is a point-in-time view of queue backpressure
// counters.
type BackpressureMetricsSnapshot struct {
	MaxQueuedMessages     int    `json:"max_queued_messages"`
	QueuedMessages        int    `json:"queued_messages"`
	EnqueuedMessages      uint64 `json:"enqueued_messages"`
	DequeuedMessages      uint64 `json:"dequeued_messages"`
	DroppedMessages       uint64 `json:"dropped_messages"`
	DroppedOldestMessages uint64 `json:"dropped_oldest_messages"`
	DroppedNewestMessages uint64 `json:"dropped_newest_messages"`
}

// MessageQueue is a mutex-protected bounded FIFO queue for realtime messages.
//
// The zero value is ready to use with default options.
type MessageQueue[T any] struct {
	mu      sync.Mutex
	options MessageQueueOptions
	items   []T
	metrics BackpressureMetricsSnapshot
}

// NewMessageQueue returns a bounded queue with opts normalized.
func NewMessageQueue[T any](opts MessageQueueOptions) *MessageQueue[T] {
	opts = opts.Normalize()
	return &MessageQueue[T]{
		options: opts,
		items:   make([]T, 0, opts.MaxQueuedMessages),
		metrics: BackpressureMetricsSnapshot{
			MaxQueuedMessages: opts.MaxQueuedMessages,
		},
	}
}

// Enqueue offers message to the queue without blocking.
func (q *MessageQueue[T]) Enqueue(message T) EnqueueResult[T] {
	q.mu.Lock()
	defer q.mu.Unlock()
	q.initLocked()

	if len(q.items) < q.options.MaxQueuedMessages {
		q.items = append(q.items, message)
		q.metrics.QueuedMessages = len(q.items)
		q.metrics.EnqueuedMessages++
		return EnqueueResult[T]{Accepted: true}
	}

	switch q.options.DropPolicy {
	case DropOldest:
		dropped := q.items[0]
		copy(q.items, q.items[1:])
		q.items[len(q.items)-1] = message
		q.metrics.QueuedMessages = len(q.items)
		q.metrics.EnqueuedMessages++
		q.metrics.DroppedMessages++
		q.metrics.DroppedOldestMessages++
		return EnqueueResult[T]{
			Accepted:       true,
			Dropped:        true,
			DropPolicy:     DropOldest,
			DroppedMessage: dropped,
		}
	default:
		q.metrics.DroppedMessages++
		q.metrics.DroppedNewestMessages++
		return EnqueueResult[T]{
			Dropped:        true,
			DropPolicy:     DropNewest,
			DroppedMessage: message,
		}
	}
}

// Dequeue removes and returns the oldest queued message.
func (q *MessageQueue[T]) Dequeue() (T, bool) {
	q.mu.Lock()
	defer q.mu.Unlock()
	q.initLocked()

	var zero T
	if len(q.items) == 0 {
		return zero, false
	}

	message := q.items[0]
	copy(q.items, q.items[1:])
	q.items[len(q.items)-1] = zero
	q.items = q.items[:len(q.items)-1]
	q.metrics.QueuedMessages = len(q.items)
	q.metrics.DequeuedMessages++
	return message, true
}

// Len returns the number of messages currently queued.
func (q *MessageQueue[T]) Len() int {
	q.mu.Lock()
	defer q.mu.Unlock()
	q.initLocked()
	return len(q.items)
}

// MaxQueuedMessages returns the queue capacity.
func (q *MessageQueue[T]) MaxQueuedMessages() int {
	q.mu.Lock()
	defer q.mu.Unlock()
	q.initLocked()
	return q.options.MaxQueuedMessages
}

// Snapshot returns a copy of the queue metrics.
func (q *MessageQueue[T]) Snapshot() BackpressureMetricsSnapshot {
	q.mu.Lock()
	defer q.mu.Unlock()
	q.initLocked()

	q.metrics.QueuedMessages = len(q.items)
	return q.metrics
}

func (q *MessageQueue[T]) initLocked() {
	if q.options.MaxQueuedMessages > 0 {
		return
	}
	q.options = q.options.Normalize()
	q.items = make([]T, 0, q.options.MaxQueuedMessages)
	q.metrics.MaxQueuedMessages = q.options.MaxQueuedMessages
}

// ReconnectRetrySchedule configures deterministic realtime reconnect backoff.
//
// Attempt numbers are one-based and refer only to reconnect attempts. Attempt
// 1 waits BaseDelay, attempt 2 doubles that delay, and later attempts continue
// doubling until MaxDelay. MaxAttempts equal to zero means unlimited attempts.
type ReconnectRetrySchedule struct {
	MaxAttempts int
	BaseDelay   time.Duration
	MaxDelay    time.Duration
}

// Normalize returns schedule with delay defaults and a valid delay cap applied.
func (s ReconnectRetrySchedule) Normalize() ReconnectRetrySchedule {
	if s.MaxAttempts < 0 {
		s.MaxAttempts = 0
	}
	if s.BaseDelay <= 0 {
		s.BaseDelay = DefaultReconnectBaseDelay
	}
	if s.MaxDelay <= 0 {
		s.MaxDelay = DefaultReconnectMaxDelay
	}
	if s.MaxDelay < s.BaseDelay {
		s.MaxDelay = s.BaseDelay
	}
	return s
}

// ShouldAttempt reports whether the one-based reconnect attempt is within the
// schedule's retry budget.
func (s ReconnectRetrySchedule) ShouldAttempt(attempt int) bool {
	if attempt < 1 {
		return false
	}
	s = s.Normalize()
	return s.MaxAttempts == 0 || attempt <= s.MaxAttempts
}

// DelayBeforeAttempt returns the wait before the one-based reconnect attempt.
func (s ReconnectRetrySchedule) DelayBeforeAttempt(attempt int) time.Duration {
	if !s.ShouldAttempt(attempt) {
		return 0
	}

	s = s.Normalize()
	delay := s.BaseDelay
	for i := 1; i < attempt; i++ {
		if delay >= s.MaxDelay/2 {
			return s.MaxDelay
		}
		delay *= 2
		if delay > s.MaxDelay {
			return s.MaxDelay
		}
	}
	return delay
}

// NextDelay returns the delay before the next attempt after afterAttempt.
func (s ReconnectRetrySchedule) NextDelay(afterAttempt int) time.Duration {
	return s.DelayBeforeAttempt(afterAttempt + 1)
}

// ReconnectMetricsSnapshot is a point-in-time view of reconnect attempts.
type ReconnectMetricsSnapshot struct {
	MaxAttempts int           `json:"max_attempts"`
	Attempts    uint64        `json:"attempts"`
	LastDelay   time.Duration `json:"last_delay"`
	Exhausted   bool          `json:"exhausted"`
}

// ReconnectTracker records reconnect attempts against a retry schedule.
//
// The zero value is ready to use with the default unlimited schedule.
type ReconnectTracker struct {
	mu        sync.Mutex
	schedule  ReconnectRetrySchedule
	attempts  uint64
	lastDelay time.Duration
	exhausted bool
}

// NewReconnectTracker returns a tracker using schedule.
func NewReconnectTracker(schedule ReconnectRetrySchedule) *ReconnectTracker {
	return &ReconnectTracker{schedule: schedule.Normalize()}
}

// NextDelay records the next reconnect attempt and returns its delay. The bool
// is false when the schedule is exhausted.
func (t *ReconnectTracker) NextDelay() (time.Duration, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.initLocked()

	next := int(t.attempts) + 1
	if !t.schedule.ShouldAttempt(next) {
		t.exhausted = true
		return 0, false
	}

	delay := t.schedule.DelayBeforeAttempt(next)
	t.attempts++
	t.lastDelay = delay
	return delay, true
}

// Reset clears attempt counters after a successful reconnect.
func (t *ReconnectTracker) Reset() {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.initLocked()

	t.attempts = 0
	t.lastDelay = 0
	t.exhausted = false
}

// Snapshot returns a copy of reconnect metrics.
func (t *ReconnectTracker) Snapshot() ReconnectMetricsSnapshot {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.initLocked()

	return ReconnectMetricsSnapshot{
		MaxAttempts: t.schedule.MaxAttempts,
		Attempts:    t.attempts,
		LastDelay:   t.lastDelay,
		Exhausted:   t.exhausted,
	}
}

func (t *ReconnectTracker) initLocked() {
	if t.schedule.BaseDelay > 0 && t.schedule.MaxDelay > 0 {
		return
	}
	t.schedule = t.schedule.Normalize()
}
