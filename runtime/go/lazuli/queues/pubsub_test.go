package queues_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/queues"
)

func TestPlanGooglePubSubDescriptorNormalizesAndSummarizes(t *testing.T) {
	labels := map[string]string{
		" Env ": " prod ",
		"team":  " queue ",
		"empty": "  ",
	}
	plan, err := queues.PlanGooglePubSubDescriptor(queues.GooglePubSubDescriptor{
		Topic:                 " projects/orders-prod/topics/events ",
		Subscription:          " projects/orders-prod/subscriptions/workers ",
		AckDeadline:           30 * time.Second,
		EnableMessageOrdering: true,
		OrderingKey:           " tenant-42 ",
		DeadLetter: queues.GooglePubSubDeadLetterPolicy{
			Topic: " projects/orders-prod/topics/events-dlq ",
		},
		EmulatorEndpoint: " HTTP://LOCALHOST:8681/v1 ",
		Labels:           labels,
	})
	if err != nil {
		t.Fatalf("PlanGooglePubSubDescriptor() error = %v", err)
	}

	wantDescriptor := queues.GooglePubSubDescriptor{
		ProjectID:             "orders-prod",
		Topic:                 "events",
		Subscription:          "workers",
		AckDeadline:           30 * time.Second,
		EnableMessageOrdering: true,
		OrderingKey:           "tenant-42",
		DeadLetter: queues.GooglePubSubDeadLetterPolicy{
			Topic:               "events-dlq",
			MaxDeliveryAttempts: queues.DefaultGooglePubSubMaxDeliveryAttempts,
		},
		EmulatorEndpoint: "http://localhost:8681/v1",
		Labels: map[string]string{
			"empty": "",
			"env":   "prod",
			"team":  "queue",
		},
	}
	if !reflect.DeepEqual(plan.Descriptor, wantDescriptor) {
		t.Fatalf("Descriptor = %#v, want %#v", plan.Descriptor, wantDescriptor)
	}
	if plan.TopicPath != "projects/orders-prod/topics/events" {
		t.Fatalf("TopicPath = %q", plan.TopicPath)
	}
	if plan.SubscriptionPath != "projects/orders-prod/subscriptions/workers" {
		t.Fatalf("SubscriptionPath = %q", plan.SubscriptionPath)
	}
	if plan.DeadLetterPath != "projects/orders-prod/topics/events-dlq" {
		t.Fatalf("DeadLetterPath = %q", plan.DeadLetterPath)
	}

	wantSummary := queues.GooglePubSubSummary{
		ProjectID:             "orders-prod",
		Topic:                 "events",
		Subscription:          "workers",
		TopicPath:             "projects/orders-prod/topics/events",
		SubscriptionPath:      "projects/orders-prod/subscriptions/workers",
		AckDeadlineSeconds:    30,
		EnableMessageOrdering: true,
		OrderingKey:           "tenant-42",
		DeadLetterTopic:       "events-dlq",
		DeadLetterPath:        "projects/orders-prod/topics/events-dlq",
		MaxDeliveryAttempts:   queues.DefaultGooglePubSubMaxDeliveryAttempts,
		EmulatorEndpoint:      "http://localhost:8681/v1",
		Labels: map[string]string{
			"empty": "",
			"env":   "prod",
			"team":  "queue",
		},
	}
	if !reflect.DeepEqual(plan.Summary, wantSummary) {
		t.Fatalf("Summary = %#v, want %#v", plan.Summary, wantSummary)
	}
	if _, ok := labels[" Env "]; !ok {
		t.Fatalf("PlanGooglePubSubDescriptor mutated input labels: %#v", labels)
	}
}

func TestNormalizeGooglePubSubDescriptorInfersProjectFromSubscription(t *testing.T) {
	got := queues.NormalizeGooglePubSubDescriptor(queues.GooglePubSubDescriptor{
		Topic:        "events",
		Subscription: "projects/billing-dev/subscriptions/billing-workers",
		AckDeadline:  10 * time.Second,
	})

	if got.ProjectID != "billing-dev" {
		t.Fatalf("ProjectID = %q, want billing-dev", got.ProjectID)
	}
	if got.Topic != "events" {
		t.Fatalf("Topic = %q, want events", got.Topic)
	}
	if got.Subscription != "billing-workers" {
		t.Fatalf("Subscription = %q, want billing-workers", got.Subscription)
	}
}

func TestValidateGooglePubSubDescriptorRejectsInvalidMetadata(t *testing.T) {
	validBase := queues.GooglePubSubDescriptor{
		ProjectID:    "orders-prod",
		Topic:        "events",
		Subscription: "workers",
		AckDeadline:  30 * time.Second,
	}
	tests := []struct {
		name string
		desc queues.GooglePubSubDescriptor
		want string
	}{
		{
			name: "missing project",
			desc: queues.GooglePubSubDescriptor{Topic: "events", Subscription: "workers", AckDeadline: 30 * time.Second},
			want: "project id",
		},
		{
			name: "bad project",
			desc: queues.GooglePubSubDescriptor{ProjectID: "Bad_Project", Topic: "events", Subscription: "workers", AckDeadline: 30 * time.Second},
			want: "project id",
		},
		{
			name: "bad topic",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "goog-bad", Subscription: "workers", AckDeadline: 30 * time.Second},
			want: "topic",
		},
		{
			name: "bad subscription",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "events", Subscription: "bad/sub", AckDeadline: 30 * time.Second},
			want: "subscription",
		},
		{
			name: "ack deadline below min",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "events", Subscription: "workers", AckDeadline: 9 * time.Second},
			want: "ack deadline",
		},
		{
			name: "ack deadline above max",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "events", Subscription: "workers", AckDeadline: 601 * time.Second},
			want: "ack deadline",
		},
		{
			name: "ack deadline fractional",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "events", Subscription: "workers", AckDeadline: 10500 * time.Millisecond},
			want: "whole seconds",
		},
		{
			name: "ordering key without ordering",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "events", Subscription: "workers", AckDeadline: 30 * time.Second, OrderingKey: "tenant-42"},
			want: "message ordering",
		},
		{
			name: "dead-letter attempts below min",
			desc: queues.GooglePubSubDescriptor{
				ProjectID:    "orders-prod",
				Topic:        "events",
				Subscription: "workers",
				AckDeadline:  30 * time.Second,
				DeadLetter: queues.GooglePubSubDeadLetterPolicy{
					Topic:               "events-dlq",
					MaxDeliveryAttempts: 4,
				},
			},
			want: "max delivery attempts",
		},
		{
			name: "dead-letter attempts without topic",
			desc: queues.GooglePubSubDescriptor{
				ProjectID:    "orders-prod",
				Topic:        "events",
				Subscription: "workers",
				AckDeadline:  30 * time.Second,
				DeadLetter: queues.GooglePubSubDeadLetterPolicy{
					MaxDeliveryAttempts: 5,
				},
			},
			want: "requires topic",
		},
		{
			name: "emulator endpoint credentials",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "events", Subscription: "workers", AckDeadline: 30 * time.Second, EmulatorEndpoint: "http://user:secret@localhost:8681"},
			want: "credentials",
		},
		{
			name: "resource project mismatch",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "projects/other-prod/topics/events", Subscription: "workers", AckDeadline: 30 * time.Second},
			want: "does not match",
		},
		{
			name: "bad label",
			desc: queues.GooglePubSubDescriptor{ProjectID: "orders-prod", Topic: "events", Subscription: "workers", AckDeadline: 30 * time.Second, Labels: map[string]string{"1bad": "value"}},
			want: "label key",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := queues.ValidateGooglePubSubDescriptor(tt.desc)
			if !errors.Is(err, queues.ErrGooglePubSubDescriptorInvalid) {
				t.Fatalf("ValidateGooglePubSubDescriptor() error = %v, want ErrGooglePubSubDescriptorInvalid", err)
			}
			if !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("ValidateGooglePubSubDescriptor() error = %q, want substring %q", err, tt.want)
			}
		})
	}

	if err := queues.ValidateGooglePubSubDescriptor(validBase); err != nil {
		t.Fatalf("ValidateGooglePubSubDescriptor(valid) error = %v", err)
	}
}

func TestValidateGooglePubSubDeadLetterPolicyAllowsDefaultAttemptsAfterNormalization(t *testing.T) {
	desc := queues.GooglePubSubDescriptor{
		ProjectID:    "orders-prod",
		Topic:        "events",
		Subscription: "workers",
		AckDeadline:  queues.GooglePubSubMaxAckDeadline,
		DeadLetter: queues.GooglePubSubDeadLetterPolicy{
			Topic: "events-dlq",
		},
	}
	if err := queues.ValidateGooglePubSubDescriptor(desc); err != nil {
		t.Fatalf("ValidateGooglePubSubDescriptor() error = %v", err)
	}
}

func TestGooglePubSubDescriptorSummaryOnInvalidInputRedactsEndpoint(t *testing.T) {
	summary := (queues.GooglePubSubDescriptor{
		ProjectID:        "orders-prod",
		Topic:            "events",
		Subscription:     "workers",
		EmulatorEndpoint: "http://user:secret@localhost:8681?token=secret",
	}).Summary()

	if strings.Contains(summary.EmulatorEndpoint, "secret") || strings.Contains(summary.EmulatorEndpoint, "token") {
		t.Fatalf("Summary() leaked emulator secret: %#v", summary)
	}
}

func TestRedactGooglePubSubEmulatorEndpoint(t *testing.T) {
	tests := []struct {
		name string
		raw  string
		want string
	}{
		{
			name: "credentials query and fragment",
			raw:  "http://user:secret@localhost:8681?token=secret#frag",
			want: "http://%5BREDACTED%5D:%5BREDACTED%5D@localhost:8681",
		},
		{
			name: "host port shorthand",
			raw:  "localhost:8681",
			want: "http://localhost:8681",
		},
		{
			name: "invalid",
			raw:  "://bad",
			want: "://bad",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := queues.RedactGooglePubSubEmulatorEndpoint(tt.raw); got != tt.want {
				t.Fatalf("RedactGooglePubSubEmulatorEndpoint() = %q, want %q", got, tt.want)
			}
		})
	}
}
