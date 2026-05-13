package queues

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestPlanKafkaDescriptorNormalizesAndSummarizes(t *testing.T) {
	plan, err := PlanKafkaDescriptor(KafkaDescriptor{
		Brokers: []string{
			" KAFKA://B.EXAMPLE.COM:9093 ",
			"a.example.com",
			"a.example.com:9092",
		},
		Topic:           " orders.created ",
		GroupID:         " workers-primary ",
		ClientID:        " lazuli-worker-1 ",
		Partition:       KafkaAnyPartition,
		Key:             " tenant-42 ",
		DeliveryTimeout: 0,
		TLS: KafkaTLSMetadata{
			ServerName: " Kafka.EXAMPLE.COM ",
			CAFile:     " /etc/ssl/ca.pem ",
			CertFile:   " /etc/ssl/client.pem ",
			KeyFile:    " /etc/ssl/client-key.pem ",
		},
		SASL: KafkaSASLMetadata{
			Mechanism: " plain ",
			Username:  " service-account ",
			Password:  " secret ",
		},
	})
	if err != nil {
		t.Fatalf("PlanKafkaDescriptor() error = %v", err)
	}

	wantBrokers := []string{"a.example.com:9092", "b.example.com:9093"}
	if !reflect.DeepEqual(plan.Descriptor.Brokers, wantBrokers) {
		t.Fatalf("Brokers = %#v, want %#v", plan.Descriptor.Brokers, wantBrokers)
	}
	if plan.Descriptor.Topic != "orders.created" || plan.Descriptor.GroupID != "workers-primary" || plan.Descriptor.ClientID != "lazuli-worker-1" {
		t.Fatalf("descriptor ids were not normalized: %#v", plan.Descriptor)
	}
	if plan.Descriptor.DeliveryTimeout != DefaultKafkaDeliveryTimeout {
		t.Fatalf("DeliveryTimeout = %s, want default %s", plan.Descriptor.DeliveryTimeout, DefaultKafkaDeliveryTimeout)
	}
	if !plan.Descriptor.TLS.Enabled || plan.Descriptor.TLS.ServerName != "kafka.example.com" {
		t.Fatalf("TLS metadata = %#v", plan.Descriptor.TLS)
	}
	if plan.Descriptor.SASL.Mechanism != "PLAIN" {
		t.Fatalf("SASL mechanism = %q, want PLAIN", plan.Descriptor.SASL.Mechanism)
	}

	wantSummary := KafkaDescriptorSummary{
		Brokers:                wantBrokers,
		Topic:                  "orders.created",
		GroupID:                "workers-primary",
		ClientID:               "lazuli-worker-1",
		Partition:              KafkaAnyPartition,
		KeySet:                 true,
		DeliveryTimeout:        DefaultKafkaDeliveryTimeout.String(),
		TLSEnabled:             true,
		TLSServerName:          "kafka.example.com",
		TLSClientCertificate:   true,
		TLSCustomCA:            true,
		SASLEnabled:            true,
		SASLMechanism:          "PLAIN",
		SASLUsername:           "[REDACTED]",
		SASLPasswordConfigured: true,
	}
	if !reflect.DeepEqual(plan.Summary, wantSummary) {
		t.Fatalf("Summary = %#v, want %#v", plan.Summary, wantSummary)
	}
	if strings.Contains(plan.Summary.SASLUsername, "service-account") {
		t.Fatalf("summary leaked sasl username: %#v", plan.Summary)
	}
}

func TestValidateKafkaDescriptorJoinsErrors(t *testing.T) {
	err := ValidateKafkaDescriptor(KafkaDescriptor{
		Brokers:         []string{"kafka://user:secret@example.com:9092/path"},
		Topic:           "orders created",
		GroupID:         "workers/primary",
		ClientID:        "client\none",
		Partition:       -2,
		Key:             "bad\nkey",
		DeliveryTimeout: time.Millisecond,
		TLS:             KafkaTLSMetadata{Enabled: true, CertFile: "client.pem"},
		SASL:            KafkaSASLMetadata{Mechanism: "plain", Username: "user"},
	})
	for _, want := range []error{
		ErrKafkaBrokerInvalid,
		ErrKafkaTopicInvalid,
		ErrKafkaGroupIDInvalid,
		ErrKafkaClientIDInvalid,
		ErrKafkaPartitionKeyInvalid,
		ErrKafkaDeliveryTimeoutInvalid,
		ErrKafkaTLSInvalid,
		ErrKafkaSASLInvalid,
	} {
		if !errors.Is(err, want) {
			t.Fatalf("ValidateKafkaDescriptor() error = %v, want errors.Is(%v)", err, want)
		}
	}
}

func TestNormalizeKafkaBrokerAddress(t *testing.T) {
	tests := []struct {
		name string
		raw  string
		want string
	}{
		{name: "host with port", raw: " Example.COM:19092 ", want: "example.com:19092"},
		{name: "default port", raw: "Example.COM", want: "example.com:9092"},
		{name: "scheme host", raw: "KAFKA://Broker.EXAMPLE.COM:9093", want: "broker.example.com:9093"},
		{name: "ipv6", raw: "[2001:db8::1]:9094", want: "[2001:db8::1]:9094"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := NormalizeKafkaBrokerAddress(tt.raw)
			if err != nil {
				t.Fatalf("NormalizeKafkaBrokerAddress() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeKafkaBrokerAddress() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestValidateKafkaBrokerAddressRejectsInvalidAddresses(t *testing.T) {
	tests := []string{
		"",
		"kafka://user:secret@example.com:9092",
		"kafka://example.com:9092/path",
		"example.com:bad",
		"bad host:9092",
		"example.com:9092/path",
	}
	for _, raw := range tests {
		t.Run(raw, func(t *testing.T) {
			if err := ValidateKafkaBrokerAddress(raw); !errors.Is(err, ErrKafkaBrokerInvalid) {
				t.Fatalf("ValidateKafkaBrokerAddress(%q) error = %v, want ErrKafkaBrokerInvalid", raw, err)
			}
		})
	}
}

func TestValidateKafkaIDs(t *testing.T) {
	validTopics := []string{"orders", "tenant_42.orders-created", "a"}
	for _, topic := range validTopics {
		if err := ValidateKafkaTopic(topic); err != nil {
			t.Fatalf("ValidateKafkaTopic(%q) error = %v", topic, err)
		}
	}
	invalidTopics := []string{"", ".", "..", "orders created", "orders/created"}
	for _, topic := range invalidTopics {
		if err := ValidateKafkaTopic(topic); !errors.Is(err, ErrKafkaTopicInvalid) {
			t.Fatalf("ValidateKafkaTopic(%q) error = %v, want ErrKafkaTopicInvalid", topic, err)
		}
	}
	if err := ValidateKafkaGroupID("workers.primary-1"); err != nil {
		t.Fatalf("ValidateKafkaGroupID(valid) error = %v", err)
	}
	if err := ValidateKafkaGroupID("workers/primary"); !errors.Is(err, ErrKafkaGroupIDInvalid) {
		t.Fatalf("ValidateKafkaGroupID(invalid) error = %v, want ErrKafkaGroupIDInvalid", err)
	}
	if err := ValidateKafkaClientID("worker 1"); err != nil {
		t.Fatalf("ValidateKafkaClientID(valid) error = %v", err)
	}
	if err := ValidateKafkaClientID("worker\n1"); !errors.Is(err, ErrKafkaClientIDInvalid) {
		t.Fatalf("ValidateKafkaClientID(invalid) error = %v, want ErrKafkaClientIDInvalid", err)
	}
}

func TestValidateKafkaPartitionKeyAndDeliveryTimeout(t *testing.T) {
	if err := ValidateKafkaPartitionKeyMetadata(KafkaAnyPartition, "tenant-42"); err != nil {
		t.Fatalf("ValidateKafkaPartitionKeyMetadata(valid) error = %v", err)
	}
	if err := ValidateKafkaPartitionKeyMetadata(-2, "tenant-42"); !errors.Is(err, ErrKafkaPartitionKeyInvalid) {
		t.Fatalf("ValidateKafkaPartitionKeyMetadata(partition) error = %v, want ErrKafkaPartitionKeyInvalid", err)
	}
	if err := ValidateKafkaPartitionKeyMetadata(0, "tenant\n42"); !errors.Is(err, ErrKafkaPartitionKeyInvalid) {
		t.Fatalf("ValidateKafkaPartitionKeyMetadata(key) error = %v, want ErrKafkaPartitionKeyInvalid", err)
	}
	if err := ValidateKafkaDeliveryTimeout(MaxKafkaDeliveryTimeout); err != nil {
		t.Fatalf("ValidateKafkaDeliveryTimeout(valid) error = %v", err)
	}
	if err := ValidateKafkaDeliveryTimeout(MaxKafkaDeliveryTimeout + time.Second); !errors.Is(err, ErrKafkaDeliveryTimeoutInvalid) {
		t.Fatalf("ValidateKafkaDeliveryTimeout(invalid) error = %v, want ErrKafkaDeliveryTimeoutInvalid", err)
	}
}

func TestValidateKafkaTLSAndSASLMetadata(t *testing.T) {
	if err := ValidateKafkaTLSMetadata(KafkaTLSMetadata{CertFile: "client.pem", KeyFile: "client.key"}); err != nil {
		t.Fatalf("ValidateKafkaTLSMetadata(valid) error = %v", err)
	}
	if err := ValidateKafkaTLSMetadata(KafkaTLSMetadata{Enabled: true, KeyFile: "client.key"}); !errors.Is(err, ErrKafkaTLSInvalid) {
		t.Fatalf("ValidateKafkaTLSMetadata(invalid) error = %v, want ErrKafkaTLSInvalid", err)
	}

	validSASL := []KafkaSASLMetadata{
		{Mechanism: "PLAIN", Username: "user", Password: "secret"},
		{Mechanism: "SCRAM-SHA-256", Username: "user", Password: "secret"},
		{Mechanism: "OAUTHBEARER", Token: "token"},
	}
	for _, meta := range validSASL {
		if err := ValidateKafkaSASLMetadata(meta); err != nil {
			t.Fatalf("ValidateKafkaSASLMetadata(%#v) error = %v", meta, err)
		}
	}
	if err := ValidateKafkaSASLMetadata(KafkaSASLMetadata{Mechanism: "PLAIN", Username: "user"}); !errors.Is(err, ErrKafkaSASLInvalid) {
		t.Fatalf("ValidateKafkaSASLMetadata(invalid plain) error = %v, want ErrKafkaSASLInvalid", err)
	}
	if err := ValidateKafkaSASLMetadata(KafkaSASLMetadata{Mechanism: "GSSAPI"}); !errors.Is(err, ErrKafkaSASLInvalid) {
		t.Fatalf("ValidateKafkaSASLMetadata(unsupported) error = %v, want ErrKafkaSASLInvalid", err)
	}
}

func TestRedactKafkaBrokerAddress(t *testing.T) {
	got := RedactKafkaBrokerAddress("kafka://user:secret@example.com:9092")
	want := "kafka://%5BREDACTED%5D:%5BREDACTED%5D@example.com:9092"
	if got != want {
		t.Fatalf("RedactKafkaBrokerAddress() = %q, want %q", got, want)
	}
	if got := RedactKafkaBrokerAddress("example.com:9092"); got != "example.com:9092" {
		t.Fatalf("RedactKafkaBrokerAddress(no userinfo) = %q", got)
	}
}
