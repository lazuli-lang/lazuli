// Package realtime provides adapter-neutral primitives for Lazuli realtime
// features.
package realtime

import (
	"context"
	"errors"
	"fmt"
	"sync"
)

const (
	// DefaultSubscriberBuffer is the default per-subscription message buffer.
	DefaultSubscriberBuffer = 16
	// MaxTopicLength bounds topic names so they remain portable across future
	// broker adapters.
	MaxTopicLength = 256
)

var (
	// ErrInvalidTopic is returned when a topic name is empty, too long, or
	// contains unsupported characters.
	ErrInvalidTopic = errors.New("lazuli/realtime: invalid topic")
	// ErrSubscriberBufferFull reports that a subscriber could not accept a
	// message without blocking the publisher.
	ErrSubscriberBufferFull = errors.New("lazuli/realtime: subscriber buffer full")
)

// Message is the payload delivered to subscribers.
type Message struct {
	Topic string
	Data  []byte
}

// PublishResult reports the outcome of a Publish fanout.
type PublishResult struct {
	Topic        string
	Subscribers  int
	Delivered    int
	Dropped      int
	ErrorReports int
}

// DeliveryError is sent to a subscription's error channel when delivery fails.
type DeliveryError struct {
	Topic string
	Err   error
}

// Error returns a stable human-readable delivery error.
func (e DeliveryError) Error() string {
	if e.Err == nil {
		if e.Topic == "" {
			return "lazuli/realtime: delivery failed"
		}
		return fmt.Sprintf("lazuli/realtime: deliver topic %q: delivery failed", e.Topic)
	}
	if e.Topic == "" {
		return e.Err.Error()
	}
	return fmt.Sprintf("lazuli/realtime: deliver topic %q: %v", e.Topic, e.Err)
}

// Unwrap returns the underlying delivery error.
func (e DeliveryError) Unwrap() error {
	return e.Err
}

// HubOption configures a Hub.
type HubOption func(*Hub)

// WithSubscriberBuffer sets the per-subscription message and error buffer.
//
// A negative size is ignored. A zero size creates unbuffered subscriptions,
// which are valid but will drop publishes unless a receiver is ready.
func WithSubscriberBuffer(size int) HubOption {
	return func(h *Hub) {
		if size >= 0 {
			h.subscriberBuffer = size
		}
	}
}

// Hub is an in-memory pub/sub hub safe for concurrent use.
type Hub struct {
	mu sync.RWMutex

	topics           map[string]map[*Subscription]struct{}
	subscriberBuffer int
}

// NewHub returns an empty in-memory pub/sub hub.
func NewHub(options ...HubOption) *Hub {
	hub := &Hub{
		topics:           make(map[string]map[*Subscription]struct{}),
		subscriberBuffer: DefaultSubscriberBuffer,
	}
	for _, option := range options {
		if option != nil {
			option(hub)
		}
	}
	return hub
}

// Subscribe registers a new subscription for topic.
//
// The returned subscription is automatically unsubscribed and closed when ctx
// is canceled. Call Unsubscribe to release it earlier.
func (h *Hub) Subscribe(ctx context.Context, topic string) (*Subscription, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if err := ValidateTopic(topic); err != nil {
		return nil, err
	}

	h.mu.Lock()
	defer h.mu.Unlock()
	h.initLocked()

	sub := &Subscription{
		hub:      h,
		topic:    topic,
		messages: make(chan Message, h.subscriberBuffer),
		errors:   make(chan error, h.subscriberBuffer),
		done:     make(chan struct{}),
	}
	subs := h.topics[topic]
	if subs == nil {
		subs = make(map[*Subscription]struct{})
		h.topics[topic] = subs
	}
	subs[sub] = struct{}{}

	go sub.closeOnContext(ctx)

	return sub, nil
}

// Publish fans out data to every current subscriber of topic.
//
// Delivery never waits for slow subscribers. When a subscriber's message buffer
// is full, the message is dropped for that subscriber and a DeliveryError is
// reported to the subscriber's error channel when that can also be done without
// blocking.
func (h *Hub) Publish(ctx context.Context, topic string, data []byte) (PublishResult, error) {
	result := PublishResult{Topic: topic}
	if err := ctx.Err(); err != nil {
		return result, err
	}
	if err := ValidateTopic(topic); err != nil {
		return result, err
	}

	h.mu.RLock()
	defer h.mu.RUnlock()

	subs := h.topics[topic]
	result.Subscribers = len(subs)
	for sub := range subs {
		if err := ctx.Err(); err != nil {
			return result, err
		}

		message := Message{
			Topic: topic,
			Data:  cloneBytes(data),
		}
		select {
		case sub.messages <- message:
			result.Delivered++
		default:
			result.Dropped++
			if sub.reportLocked(DeliveryError{Topic: topic, Err: ErrSubscriberBufferFull}) {
				result.ErrorReports++
			}
		}
	}
	return result, nil
}

// ValidateTopic validates a topic name.
//
// Topic names are deliberately restricted to portable ASCII characters:
// letters, digits, slash, colon, dot, underscore, and dash.
func ValidateTopic(topic string) error {
	switch {
	case topic == "":
		return fmt.Errorf("%w: required", ErrInvalidTopic)
	case len(topic) > MaxTopicLength:
		return fmt.Errorf("%w: length %d exceeds %d", ErrInvalidTopic, len(topic), MaxTopicLength)
	}

	for _, r := range topic {
		if validTopicRune(r) {
			continue
		}
		return fmt.Errorf("%w: unsupported character %q", ErrInvalidTopic, r)
	}
	return nil
}

func validTopicRune(r rune) bool {
	switch {
	case r >= 'a' && r <= 'z':
		return true
	case r >= 'A' && r <= 'Z':
		return true
	case r >= '0' && r <= '9':
		return true
	case r == '/' || r == ':' || r == '.' || r == '_' || r == '-':
		return true
	default:
		return false
	}
}

func (h *Hub) unsubscribe(sub *Subscription) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if sub.closed {
		return
	}

	if subs := h.topics[sub.topic]; subs != nil {
		delete(subs, sub)
		if len(subs) == 0 {
			delete(h.topics, sub.topic)
		}
	}

	sub.closed = true
	close(sub.messages)
	close(sub.errors)
	close(sub.done)
}

func (h *Hub) initLocked() {
	if h.topics == nil {
		h.topics = make(map[string]map[*Subscription]struct{})
	}
}

func cloneBytes(data []byte) []byte {
	if data == nil {
		return nil
	}
	return append([]byte(nil), data...)
}

// Subscription receives messages for one topic.
type Subscription struct {
	hub   *Hub
	topic string

	messages chan Message
	errors   chan error
	done     chan struct{}
	closed   bool
}

// Topic returns the subscribed topic.
func (s *Subscription) Topic() string {
	return s.topic
}

// Messages returns the channel that receives published messages.
func (s *Subscription) Messages() <-chan Message {
	return s.messages
}

// Errors returns the channel that receives nonblocking delivery errors.
func (s *Subscription) Errors() <-chan error {
	return s.errors
}

// Done is closed when the subscription has been unsubscribed.
func (s *Subscription) Done() <-chan struct{} {
	return s.done
}

// Unsubscribe removes this subscription and closes its channels.
func (s *Subscription) Unsubscribe() {
	if s == nil || s.hub == nil {
		return
	}
	s.hub.unsubscribe(s)
}

func (s *Subscription) closeOnContext(ctx context.Context) {
	select {
	case <-ctx.Done():
		s.Unsubscribe()
	case <-s.done:
	}
}

func (s *Subscription) reportLocked(err error) bool {
	select {
	case s.errors <- err:
		return true
	default:
		return false
	}
}
