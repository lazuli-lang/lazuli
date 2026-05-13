package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"
)

func TestEventSubscriptionRegistryMatchesEnabledSubscriptionsInOrder(t *testing.T) {
	registry := NewEventSubscriptionRegistry()
	calls := []string{}

	registrations := []EventSubscription{
		{
			Name:         "customer.pattern.late",
			Feature:      "crm",
			EventPattern: "customer.*",
			Handler:      recordEventSubscriptionCall(&calls, "late"),
			Order:        30,
		},
		{
			Name:           "customer.disabled",
			Feature:        "crm",
			EventName:      "customer.created",
			Handler:        recordEventSubscriptionCall(&calls, "disabled"),
			Status:         EventSubscriptionDisabled,
			DisabledReason: "rollout paused",
			Order:          5,
		},
		{
			Name:      "customer.exact",
			Feature:   "crm",
			EventName: "customer.created",
			Handler:   recordEventSubscriptionCall(&calls, "exact"),
			Order:     10,
		},
		{
			Name:         "customer.pattern",
			Feature:      "crm",
			EventPattern: "customer.*",
			Handler:      recordEventSubscriptionCall(&calls, "pattern"),
			Order:        20,
		},
		{
			Name:      "invoice.exact",
			Feature:   "billing",
			EventName: "invoice.created",
			Handler:   recordEventSubscriptionCall(&calls, "invoice"),
			Order:     1,
		},
	}
	for _, registration := range registrations {
		if err := registry.Register(registration); err != nil {
			t.Fatalf("Register(%q) error = %v", registration.Name, err)
		}
	}

	matches := registry.Matching("customer.created")
	if got, want := eventSubscriptionNames(matches), []string{
		"customer.exact",
		"customer.pattern",
		"customer.pattern.late",
	}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Matching() names = %v, want %v", got, want)
	}

	if got, want := eventSubscriptionNames(registry.Subscriptions()), []string{
		"invoice.exact",
		"customer.disabled",
		"customer.exact",
		"customer.pattern",
		"customer.pattern.late",
	}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Subscriptions() names = %v, want %v", got, want)
	}

	all := registry.Subscriptions()
	if all[1].Status != EventSubscriptionDisabled || all[1].DisabledReason != "rollout paused" {
		t.Fatalf("disabled metadata = (%q, %q), want disabled rollout paused", all[1].Status, all[1].DisabledReason)
	}

	for _, subscriber := range registry.Subscribers("customer.created") {
		if err := subscriber(context.Background(), Event{Name: "customer.created"}); err != nil {
			t.Fatalf("subscriber returned error: %v", err)
		}
	}
	if want := []string{"exact", "pattern", "late"}; !reflect.DeepEqual(calls, want) {
		t.Fatalf("subscriber calls = %v, want %v", calls, want)
	}
}

func TestEventSubscriptionRegistryValidation(t *testing.T) {
	registry := NewEventSubscriptionRegistry()
	handler := func(context.Context, Event) error { return nil }

	tests := []struct {
		name         string
		subscription EventSubscription
		wantErr      error
	}{
		{
			name:         "empty name",
			subscription: EventSubscription{EventName: "customer.created", Handler: handler},
			wantErr:      ErrEventSubscriptionNameRequired,
		},
		{
			name:         "missing selector",
			subscription: EventSubscription{Name: "customer.created.handler", Handler: handler},
			wantErr:      ErrEventSubscriptionSelectorRequired,
		},
		{
			name: "selector conflict",
			subscription: EventSubscription{
				Name:         "customer.created.handler",
				EventName:    "customer.created",
				EventPattern: "customer.*",
				Handler:      handler,
			},
			wantErr: ErrEventSubscriptionSelectorConflict,
		},
		{
			name:         "nil handler",
			subscription: EventSubscription{Name: "customer.created.handler", EventName: "customer.created"},
			wantErr:      ErrNilEventSubscriptionHandler,
		},
		{
			name: "invalid pattern",
			subscription: EventSubscription{
				Name:         "customer.created.handler",
				EventPattern: "customer.[",
				Handler:      handler,
			},
			wantErr: ErrEventSubscriptionPatternInvalid,
		},
		{
			name: "invalid status",
			subscription: EventSubscription{
				Name:      "customer.created.handler",
				EventName: "customer.created",
				Handler:   handler,
				Status:    EventSubscriptionStatus("paused"),
			},
			wantErr: ErrEventSubscriptionStatusInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := registry.Register(tt.subscription); !errors.Is(err, tt.wantErr) {
				t.Fatalf("Register() error = %v, want %v", err, tt.wantErr)
			}
		})
	}
}

func TestEventSubscriptionRegistryRejectsDuplicateNamedSelector(t *testing.T) {
	registry := NewEventSubscriptionRegistry()
	handler := func(context.Context, Event) error { return nil }
	subscription := EventSubscription{
		Name:      "customer.created.handler",
		EventName: "customer.created",
		Handler:   handler,
	}

	if err := registry.Register(subscription); err != nil {
		t.Fatalf("first Register() error = %v", err)
	}
	if err := registry.Register(subscription); !errors.Is(err, ErrEventSubscriptionDuplicate) {
		t.Fatalf("second Register() error = %v, want ErrEventSubscriptionDuplicate", err)
	}

	otherSelector := subscription
	otherSelector.EventName = "customer.updated"
	if err := registry.Register(otherSelector); err != nil {
		t.Fatalf("Register() with different selector error = %v", err)
	}
}

func TestEventSubscriptionMatchesEventNameAndPattern(t *testing.T) {
	exact := EventSubscription{EventName: "customer.created"}
	if !exact.Matches(" customer.created ") {
		t.Fatal("exact subscription did not match event name")
	}
	if exact.Matches("customer.updated") {
		t.Fatal("exact subscription matched different event name")
	}

	pattern := EventSubscription{EventPattern: "customer.*"}
	if !pattern.Matches("customer.updated") {
		t.Fatal("pattern subscription did not match event name")
	}
	if pattern.Matches("invoice.created") {
		t.Fatal("pattern subscription matched unrelated event name")
	}
}

func TestEventSubscriptionRegistryNilRegister(t *testing.T) {
	var registry *EventSubscriptionRegistry
	err := registry.Register(EventSubscription{})
	if !errors.Is(err, ErrNilEventSubscriptionRegistry) {
		t.Fatalf("Register() nil registry error = %v, want ErrNilEventSubscriptionRegistry", err)
	}
}

func eventSubscriptionNames(subscriptions []EventSubscription) []string {
	names := make([]string, 0, len(subscriptions))
	for _, subscription := range subscriptions {
		names = append(names, subscription.Name)
	}
	return names
}

func recordEventSubscriptionCall(calls *[]string, name string) Subscriber {
	return func(context.Context, Event) error {
		*calls = append(*calls, name)
		return nil
	}
}
