package queues

import (
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestPlanNATSDescriptorNormalizesAndSummarizes(t *testing.T) {
	plan, err := PlanNATSDescriptor(NATSDescriptor{
		Servers: []string{
			" NATS://user:secret@B.example.com:4222 ",
			"nats://a.example.com:4222",
			"nats://a.example.com:4222",
		},
		Subject:    " jobs.created ",
		QueueGroup: "workers-primary",
		JetStream: NATSJetStreamMetadata{
			Stream:        "ORDERS",
			Consumer:      "worker-1",
			Subjects:      []string{"orders.updated", " orders.created ", "orders.created"},
			FilterSubject: "orders.created",
		},
	})
	if err != nil {
		t.Fatalf("PlanNATSDescriptor() error = %v", err)
	}

	wantServers := []string{"nats://a.example.com:4222", "nats://user:secret@b.example.com:4222"}
	if !reflect.DeepEqual(plan.Servers, wantServers) {
		t.Fatalf("Servers = %#v, want %#v", plan.Servers, wantServers)
	}
	wantSubjects := []string{"orders.created", "orders.updated"}
	if !reflect.DeepEqual(plan.JetStream.Subjects, wantSubjects) {
		t.Fatalf("JetStream.Subjects = %#v, want %#v", plan.JetStream.Subjects, wantSubjects)
	}
	if plan.AckWait != DefaultNATSAckWait || plan.ConnectTimeout != DefaultNATSConnectTimeout {
		t.Fatalf("timeouts = %s/%s, want defaults", plan.AckWait, plan.ConnectTimeout)
	}

	wantSummaryServers := []string{"nats://%5BREDACTED%5D:%5BREDACTED%5D@b.example.com:4222", "nats://a.example.com:4222"}
	if !reflect.DeepEqual(plan.Summary.Servers, wantSummaryServers) {
		t.Fatalf("Summary.Servers = %#v, want %#v", plan.Summary.Servers, wantSummaryServers)
	}
	if !plan.Summary.JetStream || plan.Summary.Subject != "jobs.created" || plan.Summary.Stream != "ORDERS" {
		t.Fatalf("Summary = %#v", plan.Summary)
	}
}

func TestValidateNATSDescriptorJoinsErrors(t *testing.T) {
	err := ValidateNATSDescriptor(NATSDescriptor{
		Servers:        []string{"http://example.com/query?token=secret"},
		Subject:        "jobs.*",
		QueueGroup:     "workers.primary",
		AckWait:        time.Millisecond,
		ConnectTimeout: 2 * time.Hour,
		JetStream: NATSJetStreamMetadata{
			Stream:        "orders.stream",
			Subjects:      []string{"orders.>"},
			FilterSubject: "orders.*",
		},
	})
	for _, want := range []error{
		ErrNATSServerURLInvalid,
		ErrNATSSubjectInvalid,
		ErrNATSQueueGroupInvalid,
		ErrNATSJetStreamInvalid,
		ErrNATSAckWaitInvalid,
		ErrNATSWaitInvalid,
	} {
		if !errors.Is(err, want) {
			t.Fatalf("ValidateNATSDescriptor() error = %v, want errors.Is(%v)", err, want)
		}
	}
}

func TestNormalizeNATSServerURL(t *testing.T) {
	tests := []struct {
		name string
		raw  string
		want string
	}{
		{name: "nats", raw: " NATS://Example.COM:4222 ", want: "nats://example.com:4222"},
		{name: "tls", raw: "tls://HOST.internal", want: "tls://host.internal"},
		{name: "websocket", raw: "WSS://NATS.example.com:443", want: "wss://nats.example.com:443"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := NormalizeNATSServerURL(tt.raw)
			if err != nil {
				t.Fatalf("NormalizeNATSServerURL() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeNATSServerURL() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestValidateNATSServerURLRejectsInvalidURLs(t *testing.T) {
	tests := []string{
		"",
		"example.com:4222",
		"http://example.com:4222",
		"nats://",
		"nats://example.com/path",
		"nats://example.com?token=secret",
		"nats://example.com:bad",
	}
	for _, raw := range tests {
		t.Run(raw, func(t *testing.T) {
			if err := ValidateNATSServerURL(raw); !errors.Is(err, ErrNATSServerURLInvalid) {
				t.Fatalf("ValidateNATSServerURL(%q) error = %v, want ErrNATSServerURLInvalid", raw, err)
			}
		})
	}
}

func TestValidateNATSSubject(t *testing.T) {
	valid := []string{"jobs.created", "tenant_42.jobs-created", "a"}
	for _, subject := range valid {
		if err := ValidateNATSSubject(subject); err != nil {
			t.Fatalf("ValidateNATSSubject(%q) error = %v", subject, err)
		}
	}

	invalid := []string{"", "jobs.*", "jobs.>", ".jobs", "jobs.", "jobs..created", "jobs created"}
	for _, subject := range invalid {
		if err := ValidateNATSSubject(subject); !errors.Is(err, ErrNATSSubjectInvalid) {
			t.Fatalf("ValidateNATSSubject(%q) error = %v, want ErrNATSSubjectInvalid", subject, err)
		}
	}
}

func TestValidateNATSQueueGroup(t *testing.T) {
	valid := []string{"workers", "workers-primary", "workers_1"}
	for _, group := range valid {
		if err := ValidateNATSQueueGroup(group); err != nil {
			t.Fatalf("ValidateNATSQueueGroup(%q) error = %v", group, err)
		}
	}

	invalid := []string{"", "workers.primary", "workers/*", "workers primary"}
	for _, group := range invalid {
		if err := ValidateNATSQueueGroup(group); !errors.Is(err, ErrNATSQueueGroupInvalid) {
			t.Fatalf("ValidateNATSQueueGroup(%q) error = %v, want ErrNATSQueueGroupInvalid", group, err)
		}
	}
}

func TestValidateNATSJetStreamMetadata(t *testing.T) {
	if err := ValidateNATSJetStreamMetadata(NATSJetStreamMetadata{
		Stream:        "ORDERS",
		Consumer:      "workers-1",
		Subjects:      []string{"orders.created"},
		FilterSubject: "orders.created",
	}); err != nil {
		t.Fatalf("ValidateNATSJetStreamMetadata(valid) error = %v", err)
	}

	err := ValidateNATSJetStreamMetadata(NATSJetStreamMetadata{
		Stream:        "orders.stream",
		Consumer:      "workers/1",
		Subjects:      []string{"orders.*"},
		FilterSubject: "orders.>",
	})
	if !errors.Is(err, ErrNATSJetStreamInvalid) {
		t.Fatalf("ValidateNATSJetStreamMetadata(invalid) error = %v, want ErrNATSJetStreamInvalid", err)
	}
}

func TestRedactNATSServerURL(t *testing.T) {
	got := RedactNATSServerURL("nats://user:secret@example.com:4222")
	want := "nats://%5BREDACTED%5D:%5BREDACTED%5D@example.com:4222"
	if got != want {
		t.Fatalf("RedactNATSServerURL() = %q, want %q", got, want)
	}

	if got := RedactNATSServerURL("nats://example.com:4222"); got != "nats://example.com:4222" {
		t.Fatalf("RedactNATSServerURL(no userinfo) = %q", got)
	}
}
