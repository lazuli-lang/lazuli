package lazuli

import (
	"errors"
	"fmt"
	"path"
	"sort"
	"strings"
	"sync"
)

var (
	// ErrNilEventSubscriptionRegistry is returned when registering without a registry.
	ErrNilEventSubscriptionRegistry = errors.New("lazuli: event subscription registry is nil")

	// ErrEventSubscriptionNameRequired is returned when a subscription has no name.
	ErrEventSubscriptionNameRequired = errors.New("lazuli: event subscription name is required")

	// ErrEventSubscriptionSelectorRequired is returned when a subscription has no event selector.
	ErrEventSubscriptionSelectorRequired = errors.New("lazuli: event subscription event name or pattern is required")

	// ErrEventSubscriptionSelectorConflict is returned when both selector kinds are set.
	ErrEventSubscriptionSelectorConflict = errors.New("lazuli: event subscription event name and pattern are mutually exclusive")

	// ErrNilEventSubscriptionHandler is returned when a subscription has no handler.
	ErrNilEventSubscriptionHandler = errors.New("lazuli: event subscription handler is nil")

	// ErrEventSubscriptionDuplicate is returned when the same named selector is registered twice.
	ErrEventSubscriptionDuplicate = errors.New("lazuli: event subscription already registered")

	// ErrEventSubscriptionPatternInvalid is returned when an event pattern cannot be matched.
	ErrEventSubscriptionPatternInvalid = errors.New("lazuli: event subscription pattern is invalid")

	// ErrEventSubscriptionStatusInvalid is returned when a subscription has an unknown status.
	ErrEventSubscriptionStatusInvalid = errors.New("lazuli: event subscription status is invalid")
)

// EventSubscriptionStatus records whether an event subscription is active.
type EventSubscriptionStatus string

const (
	// EventSubscriptionEnabled marks a subscription as active. The zero value
	// status also defaults to enabled when a subscription is registered.
	EventSubscriptionEnabled EventSubscriptionStatus = "enabled"

	// EventSubscriptionDisabled keeps subscription metadata visible while
	// excluding it from matching results.
	EventSubscriptionDisabled EventSubscriptionStatus = "disabled"
)

// EventSubscription describes one generated event subscriber registration.
//
// Exactly one of EventName or EventPattern must be set. EventPattern uses
// path.Match syntax, which supports glob-style selectors such as "customer.*".
type EventSubscription struct {
	// Name is the stable subscriber name, usually a generated job or handler name.
	Name string

	// Feature is optional metadata for diagnostics and generated registries.
	Feature string

	// EventName matches one exact event name.
	EventName string

	// EventPattern matches event names using path.Match glob syntax.
	EventPattern string

	// Handler is the typed runtime subscriber callback.
	Handler Subscriber

	// Status records whether this subscription participates in matching.
	Status EventSubscriptionStatus

	// DisabledReason optionally explains why a disabled subscription is retained.
	DisabledReason string

	// Order sorts otherwise independent subscribers before name and selector.
	Order int
}

// Enabled reports whether this subscription participates in matching.
func (s EventSubscription) Enabled() bool {
	return s.Status == "" || s.Status == EventSubscriptionEnabled
}

// Matches reports whether this subscription's event selector matches eventName.
func (s EventSubscription) Matches(eventName string) bool {
	eventName = strings.TrimSpace(eventName)
	switch {
	case s.EventName != "":
		return strings.TrimSpace(s.EventName) == eventName
	case s.EventPattern != "":
		matched, err := path.Match(strings.TrimSpace(s.EventPattern), eventName)
		return err == nil && matched
	default:
		return false
	}
}

// EventSubscriptionRegistry stores generated event subscriber metadata.
//
// It is independent from the package-level Subscribe/Publish event bus.
type EventSubscriptionRegistry struct {
	mu            sync.RWMutex
	subscriptions map[eventSubscriptionKey]EventSubscription
}

// NewEventSubscriptionRegistry returns an empty event subscription registry.
func NewEventSubscriptionRegistry() *EventSubscriptionRegistry {
	return &EventSubscriptionRegistry{}
}

// Register records one event subscription.
func (r *EventSubscriptionRegistry) Register(subscription EventSubscription) error {
	if r == nil {
		return ErrNilEventSubscriptionRegistry
	}

	normalized, err := normalizeEventSubscription(subscription)
	if err != nil {
		return err
	}
	key := normalized.subscriptionKey()

	r.mu.Lock()
	defer r.mu.Unlock()
	if r.subscriptions == nil {
		r.subscriptions = make(map[eventSubscriptionKey]EventSubscription)
	}
	if _, ok := r.subscriptions[key]; ok {
		return fmt.Errorf("%w: %q for %q", ErrEventSubscriptionDuplicate, normalized.Name, normalized.selector())
	}
	r.subscriptions[key] = normalized
	return nil
}

// Subscriptions returns every registered subscription in deterministic order.
func (r *EventSubscriptionRegistry) Subscriptions() []EventSubscription {
	if r == nil {
		return nil
	}

	r.mu.RLock()
	defer r.mu.RUnlock()

	out := make([]EventSubscription, 0, len(r.subscriptions))
	for _, subscription := range r.subscriptions {
		out = append(out, subscription)
	}
	sortEventSubscriptions(out)
	return out
}

// Matching returns enabled subscriptions whose event selector matches eventName.
func (r *EventSubscriptionRegistry) Matching(eventName string) []EventSubscription {
	if r == nil {
		return nil
	}

	eventName = strings.TrimSpace(eventName)
	r.mu.RLock()
	defer r.mu.RUnlock()

	out := make([]EventSubscription, 0, len(r.subscriptions))
	for _, subscription := range r.subscriptions {
		if subscription.Enabled() && subscription.Matches(eventName) {
			out = append(out, subscription)
		}
	}
	sortEventSubscriptions(out)
	return out
}

// Subscribers returns handlers for enabled subscriptions that match eventName.
func (r *EventSubscriptionRegistry) Subscribers(eventName string) []Subscriber {
	matches := r.Matching(eventName)
	out := make([]Subscriber, 0, len(matches))
	for _, match := range matches {
		out = append(out, match.Handler)
	}
	return out
}

type eventSubscriptionKey struct {
	name     string
	selector string
}

func normalizeEventSubscription(subscription EventSubscription) (EventSubscription, error) {
	subscription.Name = strings.TrimSpace(subscription.Name)
	subscription.Feature = strings.TrimSpace(subscription.Feature)
	subscription.EventName = strings.TrimSpace(subscription.EventName)
	subscription.EventPattern = strings.TrimSpace(subscription.EventPattern)
	subscription.DisabledReason = strings.TrimSpace(subscription.DisabledReason)

	if subscription.Name == "" {
		return EventSubscription{}, ErrEventSubscriptionNameRequired
	}
	if subscription.EventName == "" && subscription.EventPattern == "" {
		return EventSubscription{}, fmt.Errorf("%w: %q", ErrEventSubscriptionSelectorRequired, subscription.Name)
	}
	if subscription.EventName != "" && subscription.EventPattern != "" {
		return EventSubscription{}, fmt.Errorf("%w: %q", ErrEventSubscriptionSelectorConflict, subscription.Name)
	}
	if subscription.Handler == nil {
		return EventSubscription{}, fmt.Errorf("%w: %q", ErrNilEventSubscriptionHandler, subscription.Name)
	}
	if subscription.EventPattern != "" {
		if _, err := path.Match(subscription.EventPattern, ""); err != nil {
			return EventSubscription{}, fmt.Errorf("%w: %q: %v", ErrEventSubscriptionPatternInvalid, subscription.EventPattern, err)
		}
	}

	switch subscription.Status {
	case "":
		subscription.Status = EventSubscriptionEnabled
	case EventSubscriptionEnabled, EventSubscriptionDisabled:
	default:
		return EventSubscription{}, fmt.Errorf("%w: %q", ErrEventSubscriptionStatusInvalid, subscription.Status)
	}

	return subscription, nil
}

func (s EventSubscription) subscriptionKey() eventSubscriptionKey {
	return eventSubscriptionKey{
		name:     s.Name,
		selector: s.selector(),
	}
}

func (s EventSubscription) selector() string {
	if s.EventName != "" {
		return s.EventName
	}
	return s.EventPattern
}

func sortEventSubscriptions(subscriptions []EventSubscription) {
	sort.Slice(subscriptions, func(i, j int) bool {
		left := subscriptions[i]
		right := subscriptions[j]

		if left.Order != right.Order {
			return left.Order < right.Order
		}
		if left.Name != right.Name {
			return left.Name < right.Name
		}
		leftSelectorRank := eventSubscriptionSelectorRank(left)
		rightSelectorRank := eventSubscriptionSelectorRank(right)
		if leftSelectorRank != rightSelectorRank {
			return leftSelectorRank < rightSelectorRank
		}
		if left.EventName != right.EventName {
			return left.EventName < right.EventName
		}
		if left.EventPattern != right.EventPattern {
			return left.EventPattern < right.EventPattern
		}
		if left.Feature != right.Feature {
			return left.Feature < right.Feature
		}
		return left.Status < right.Status
	})
}

func eventSubscriptionSelectorRank(subscription EventSubscription) int {
	if subscription.EventName != "" {
		return 0
	}
	return 1
}
