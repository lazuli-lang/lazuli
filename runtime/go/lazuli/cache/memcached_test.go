package cache

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestNormalizeMemcachedServerAddress(t *testing.T) {
	tests := []struct {
		name    string
		address string
		want    string
	}{
		{name: "empty", address: "  ", want: ""},
		{name: "host gets default port", address: " Cache.EXAMPLE.com ", want: "cache.example.com:11211"},
		{name: "host port is preserved", address: "Cache.EXAMPLE.com:22122", want: "cache.example.com:22122"},
		{name: "ipv6 is bracketed", address: "2001:db8::1", want: "[2001:db8::1]:11211"},
		{name: "url drops secrets", address: "memcached://user:secret@Cache.EXAMPLE.com:11211/pool?token=abc#frag", want: "memcached://cache.example.com:11211/pool"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := NormalizeMemcachedServerAddress(tt.address); got != tt.want {
				t.Fatalf("NormalizeMemcachedServerAddress(%q) = %q, want %q", tt.address, got, tt.want)
			}
		})
	}
}

func TestValidateMemcachedKey(t *testing.T) {
	valid := strings.Repeat("a", 250)
	if err := ValidateMemcachedKey(valid); err != nil {
		t.Fatalf("ValidateMemcachedKey(valid) error = %v", err)
	}

	tests := []struct {
		name string
		key  string
	}{
		{name: "empty", key: ""},
		{name: "too long", key: strings.Repeat("a", 251)},
		{name: "space", key: "customer list"},
		{name: "control", key: "customer\nlist"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateMemcachedKey(tt.key)
			if !errors.Is(err, ErrMemcachedDescriptorInvalid) {
				t.Fatalf("ValidateMemcachedKey(%q) error = %v, want ErrMemcachedDescriptorInvalid", tt.key, err)
			}
		})
	}
}

func TestValidateMemcachedServerAddress(t *testing.T) {
	if err := ValidateMemcachedServerAddress("cache.example.com:11211"); err != nil {
		t.Fatalf("ValidateMemcachedServerAddress(valid) error = %v", err)
	}
	if err := ValidateMemcachedServerAddress("bad host"); !errors.Is(err, ErrMemcachedDescriptorInvalid) {
		t.Fatalf("ValidateMemcachedServerAddress(invalid) error = %v, want ErrMemcachedDescriptorInvalid", err)
	}
}

func TestClampMemcachedTTL(t *testing.T) {
	tests := []struct {
		name string
		ttl  time.Duration
		want MemcachedTTLPlan
	}{
		{name: "zero never expires", ttl: 0, want: MemcachedTTLPlan{NeverExpires: true}},
		{name: "negative never expires", ttl: -time.Second, want: MemcachedTTLPlan{NeverExpires: true}},
		{name: "rounds up seconds", ttl: 1500 * time.Millisecond, want: MemcachedTTLPlan{Duration: 1500 * time.Millisecond, Seconds: 2}},
		{name: "thirty days marks boundary", ttl: 30 * 24 * time.Hour, want: MemcachedTTLPlan{Duration: 30 * 24 * time.Hour, Seconds: 2592000, AbsoluteExpirationBoundary: true}},
		{name: "over thirty days clamps", ttl: 31 * 24 * time.Hour, want: MemcachedTTLPlan{Duration: 30 * 24 * time.Hour, Seconds: 2592000, Clamped: true, AbsoluteExpirationBoundary: true}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := ClampMemcachedTTL(tt.ttl); got != tt.want {
				t.Fatalf("ClampMemcachedTTL(%v) = %#v, want %#v", tt.ttl, got, tt.want)
			}
		})
	}
}

func TestPlanMemcachedNamespacePrefix(t *testing.T) {
	tests := []struct {
		namespace string
		want      string
	}{
		{namespace: "", want: ""},
		{namespace: " Customer Reports ", want: "customer-reports:"},
		{namespace: "billing_v2.cache", want: "billing_v2.cache:"},
	}

	for _, tt := range tests {
		if got := PlanMemcachedNamespacePrefix(tt.namespace); got != tt.want {
			t.Fatalf("PlanMemcachedNamespacePrefix(%q) = %q, want %q", tt.namespace, got, tt.want)
		}
	}
}

func TestPlanMemcachedDescriptor(t *testing.T) {
	plan, err := PlanMemcachedDescriptor(MemcachedDescriptor{
		Servers:   []string{" Cache-2.EXAMPLE.com:11211 ", "cache-1.example.com", "cache-1.example.com:11211"},
		Namespace: " Customer Reports ",
		Key:       "list:active",
		TTL:       31 * 24 * time.Hour,
	})
	if err != nil {
		t.Fatalf("PlanMemcachedDescriptor() error = %v", err)
	}

	if want := []string{"cache-1.example.com:11211", "cache-2.example.com:11211"}; !reflect.DeepEqual(plan.Servers, want) {
		t.Fatalf("PlanMemcachedDescriptor().Servers = %#v, want %#v", plan.Servers, want)
	}
	if plan.Namespace != "customer-reports" {
		t.Fatalf("PlanMemcachedDescriptor().Namespace = %q, want customer-reports", plan.Namespace)
	}
	if plan.KeyPrefix != "customer-reports:" {
		t.Fatalf("PlanMemcachedDescriptor().KeyPrefix = %q, want customer-reports:", plan.KeyPrefix)
	}
	if plan.Key != "list:active" {
		t.Fatalf("PlanMemcachedDescriptor().Key = %q, want list:active", plan.Key)
	}
	if !plan.TTL.Clamped || !plan.TTL.AbsoluteExpirationBoundary {
		t.Fatalf("PlanMemcachedDescriptor().TTL = %#v, want clamped absolute boundary metadata", plan.TTL)
	}
}

func TestPlanMemcachedDescriptorValidatesInputs(t *testing.T) {
	_, err := PlanMemcachedDescriptor(MemcachedDescriptor{
		Key: strings.Repeat("a", 251),
	})
	if !errors.Is(err, ErrMemcachedDescriptorInvalid) {
		t.Fatalf("PlanMemcachedDescriptor(invalid) error = %v, want ErrMemcachedDescriptorInvalid", err)
	}
}

func TestValidateMemcachedDescriptorDoesNotMutateInput(t *testing.T) {
	descriptor := MemcachedDescriptor{
		Servers:   []string{" Cache.EXAMPLE.com "},
		Namespace: " Customer Reports ",
		Key:       "list",
		TTL:       time.Minute,
	}

	if err := ValidateMemcachedDescriptor(descriptor); err != nil {
		t.Fatalf("ValidateMemcachedDescriptor() error = %v", err)
	}
	if descriptor.Servers[0] != " Cache.EXAMPLE.com " || descriptor.Namespace != " Customer Reports " {
		t.Fatalf("ValidateMemcachedDescriptor() mutated descriptor: %#v", descriptor)
	}
}

func TestMemcachedRedactedSummary(t *testing.T) {
	plan, err := PlanMemcachedDescriptor(MemcachedDescriptor{
		Servers:   []string{"memcached://user:secret@cache.example.com:11211/pool?token=abc"},
		Namespace: "tenant-a",
		Key:       "list",
		TTL:       time.Minute,
	})
	if err != nil {
		t.Fatalf("PlanMemcachedDescriptor() error = %v", err)
	}

	summary := plan.RedactedSummary()
	wantServers := []string{"memcached://cache.example.com:11211/pool"}
	if !reflect.DeepEqual(summary.Servers, wantServers) {
		t.Fatalf("RedactedSummary().Servers = %#v, want %#v", summary.Servers, wantServers)
	}
	if summary.ServerCount != 1 {
		t.Fatalf("RedactedSummary().ServerCount = %d, want 1", summary.ServerCount)
	}
	if summary.Namespace != "tenant-a" || summary.KeyPrefix != "tenant-a:" {
		t.Fatalf("RedactedSummary() namespace fields = %#v", summary)
	}
	if summary.TTLSeconds != 60 || summary.NeverExpires || summary.TTLClamped || summary.AbsoluteBoundary {
		t.Fatalf("RedactedSummary() ttl fields = %#v", summary)
	}
}
