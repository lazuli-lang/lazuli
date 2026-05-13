package queues

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestPlanRabbitMQDescriptorNormalizesAndSummarizes(t *testing.T) {
	plan, err := PlanRabbitMQDescriptor(RabbitMQDescriptor{
		URL: " AMQPS://user:secret@Rabbit.EXAMPLE.com:5671/%2Fprod?token=secret&heartbeat=30 ",
		Exchange: RabbitMQExchangeMetadata{
			Name:       " events ",
			Type:       " TOPIC ",
			Durable:    true,
			AutoDelete: false,
		},
		Queue: RabbitMQQueueMetadata{
			Name:    " orders.created ",
			Durable: true,
			DeadLetter: RabbitMQDeadLetterMetadata{
				Exchange:   " events.dlx ",
				RoutingKey: "orders.failed",
			},
		},
		RoutingKey: " orders.created ",
		Prefetch:   32,
	})
	if err != nil {
		t.Fatalf("PlanRabbitMQDescriptor() error = %v", err)
	}

	wantDescriptor := RabbitMQDescriptor{
		URL: "amqps://user:secret@rabbit.example.com:5671/%2Fprod?heartbeat=30&token=secret",
		Exchange: RabbitMQExchangeMetadata{
			Name:    "events",
			Type:    "topic",
			Durable: true,
		},
		Queue: RabbitMQQueueMetadata{
			Name:    "orders.created",
			Durable: true,
			DeadLetter: RabbitMQDeadLetterMetadata{
				Exchange:   "events.dlx",
				RoutingKey: "orders.failed",
			},
		},
		RoutingKey: "orders.created",
		Prefetch:   32,
	}
	if !reflect.DeepEqual(plan.Descriptor, wantDescriptor) {
		t.Fatalf("Descriptor = %#v, want %#v", plan.Descriptor, wantDescriptor)
	}

	wantURL := "amqps://%5BREDACTED%5D:%5BREDACTED%5D@rabbit.example.com:5671/%2Fprod?heartbeat=30&token=%5BREDACTED%5D"
	if plan.Summary.URL != wantURL {
		t.Fatalf("Summary.URL = %q, want %q", plan.Summary.URL, wantURL)
	}
	if strings.Contains(plan.Summary.URL, "secret") {
		t.Fatalf("Summary leaked secret: %#v", plan.Summary)
	}
	if plan.Summary.VHost != "/prod" || !plan.Summary.DeadLetter || plan.Summary.DeadLetterExchange != "events.dlx" {
		t.Fatalf("Summary = %#v", plan.Summary)
	}
}

func TestNormalizeRabbitMQDescriptorDefaultsRoutingKeyAndExchangeType(t *testing.T) {
	got := NormalizeRabbitMQDescriptor(RabbitMQDescriptor{
		URL: "amqp://example.com",
		Exchange: RabbitMQExchangeMetadata{
			Name: "jobs",
		},
		Queue: RabbitMQQueueMetadata{
			Name: "workers",
		},
	})

	if got.Exchange.Type != "direct" {
		t.Fatalf("Exchange.Type = %q, want direct", got.Exchange.Type)
	}
	if got.RoutingKey != "workers" {
		t.Fatalf("RoutingKey = %q, want workers", got.RoutingKey)
	}
}

func TestValidateRabbitMQDescriptorJoinsErrors(t *testing.T) {
	err := ValidateRabbitMQDescriptor(RabbitMQDescriptor{
		URL: "http://example.com/#fragment",
		Exchange: RabbitMQExchangeMetadata{
			Name: "/bad\nexchange",
			Type: "x-custom",
		},
		Queue: RabbitMQQueueMetadata{
			Name:       "bad\nqueue",
			Durable:    true,
			AutoDelete: true,
			DeadLetter: RabbitMQDeadLetterMetadata{
				RoutingKey: "jobs.failed",
			},
		},
		RoutingKey: "jobs.*",
		Prefetch:   RabbitMQMaxPrefetch + 1,
	})
	for _, want := range []error{
		ErrRabbitMQURLInvalid,
		ErrRabbitMQExchangeInvalid,
		ErrRabbitMQQueueInvalid,
		ErrRabbitMQRoutingKeyInvalid,
		ErrRabbitMQPrefetchInvalid,
		ErrRabbitMQDLXInvalid,
	} {
		if !errors.Is(err, want) {
			t.Fatalf("ValidateRabbitMQDescriptor() error = %v, want errors.Is(%v)", err, want)
		}
	}
}

func TestNormalizeRabbitMQURL(t *testing.T) {
	tests := []struct {
		name string
		raw  string
		want string
	}{
		{name: "amqp", raw: " AMQP://User:Secret@Rabbit.EXAMPLE.com:5672/%2F ", want: "amqp://User:Secret@rabbit.example.com:5672/%2F"},
		{name: "amqps", raw: "AMQPS://rabbit.example.com:5671/vhost?b=2&a=1", want: "amqps://rabbit.example.com:5671/vhost?a=1&b=2"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := NormalizeRabbitMQURL(tt.raw)
			if err != nil {
				t.Fatalf("NormalizeRabbitMQURL() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeRabbitMQURL() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestValidateRabbitMQURLRejectsInvalidURLs(t *testing.T) {
	tests := []string{
		"",
		"example.com:5672",
		"http://example.com:5672",
		"amqp://",
		"amqp://example.com/a/b",
		"amqp://example.com/#fragment",
		"amqp://example.com:bad",
	}
	for _, raw := range tests {
		t.Run(raw, func(t *testing.T) {
			if err := ValidateRabbitMQURL(raw); !errors.Is(err, ErrRabbitMQURLInvalid) {
				t.Fatalf("ValidateRabbitMQURL(%q) error = %v, want ErrRabbitMQURLInvalid", raw, err)
			}
		})
	}
}

func TestValidateRabbitMQRoutingKey(t *testing.T) {
	valid := []string{"jobs.created", "tenant_42.jobs-created", "a"}
	for _, routingKey := range valid {
		if err := ValidateRabbitMQRoutingKey(routingKey); err != nil {
			t.Fatalf("ValidateRabbitMQRoutingKey(%q) error = %v", routingKey, err)
		}
	}

	invalid := []string{"", "jobs.*", "jobs.#", ".jobs", "jobs.", "jobs..created", "jobs created"}
	for _, routingKey := range invalid {
		if err := ValidateRabbitMQRoutingKey(routingKey); !errors.Is(err, ErrRabbitMQRoutingKeyInvalid) {
			t.Fatalf("ValidateRabbitMQRoutingKey(%q) error = %v, want ErrRabbitMQRoutingKeyInvalid", routingKey, err)
		}
	}
}

func TestValidateRabbitMQPrefetch(t *testing.T) {
	for _, prefetch := range []int{RabbitMQMinPrefetch, 1, RabbitMQMaxPrefetch} {
		if err := ValidateRabbitMQPrefetch(prefetch); err != nil {
			t.Fatalf("ValidateRabbitMQPrefetch(%d) error = %v", prefetch, err)
		}
	}
	for _, prefetch := range []int{-1, RabbitMQMaxPrefetch + 1} {
		if err := ValidateRabbitMQPrefetch(prefetch); !errors.Is(err, ErrRabbitMQPrefetchInvalid) {
			t.Fatalf("ValidateRabbitMQPrefetch(%d) error = %v, want ErrRabbitMQPrefetchInvalid", prefetch, err)
		}
	}
}

func TestRedactRabbitMQURL(t *testing.T) {
	got := RedactRabbitMQURL("amqp://user:secret@example.com:5672/%2F?password=secret&heartbeat=30")
	want := "amqp://%5BREDACTED%5D:%5BREDACTED%5D@example.com:5672/%2F?heartbeat=30&password=%5BREDACTED%5D"
	if got != want {
		t.Fatalf("RedactRabbitMQURL() = %q, want %q", got, want)
	}

	if got := RedactRabbitMQURL("amqp://example.com:5672"); got != "amqp://example.com:5672" {
		t.Fatalf("RedactRabbitMQURL(no secret) = %q", got)
	}
}
