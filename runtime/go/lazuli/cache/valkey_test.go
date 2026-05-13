package cache

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestPlanValkeyCacheNormalizesDescriptor(t *testing.T) {
	plan, err := PlanValkeyCache(ValkeyDescriptor{
		Address:   " valkey://cache.example.test:6380/0 ",
		DB:        2,
		TLS:       ValkeyTLSMetadata{Enabled: true, ServerName: " cache.example.test "},
		Auth:      ValkeyAuthMetadata{Username: " app ", PasswordEnv: " VALKEY_PASSWORD "},
		KeyPrefix: " Customer Reports ",
		TTL: ValkeyTTLPolicy{
			DefaultTTL: 5 * time.Minute,
			MinimumTTL: time.Minute,
			MaximumTTL: time.Hour,
		},
	})
	if err != nil {
		t.Fatalf("PlanValkeyCache() error = %v", err)
	}

	if plan.Mode != "standalone" {
		t.Fatalf("PlanValkeyCache().Mode = %q, want standalone", plan.Mode)
	}
	if got := plan.Descriptor.KeyPrefix; got != "customer-reports" {
		t.Fatalf("PlanValkeyCache().Descriptor.KeyPrefix = %q, want customer-reports", got)
	}
	if got := plan.Descriptor.TLS.ServerName; got != "cache.example.test" {
		t.Fatalf("PlanValkeyCache().Descriptor.TLS.ServerName = %q, want cache.example.test", got)
	}
	if got := plan.Prefixes.EntryPrefix; got != "customer-reports:entry:" {
		t.Fatalf("PlanValkeyCache().Prefixes.EntryPrefix = %q, want customer-reports:entry:", got)
	}
	if got := plan.Prefixes.QueryPattern; got != "customer-reports:entry:%s:*" {
		t.Fatalf("PlanValkeyCache().Prefixes.QueryPattern = %q, want customer-reports:entry:%%s:*", got)
	}
}

func TestValidateValkeyDescriptorRejectsInvalidMetadata(t *testing.T) {
	tests := []struct {
		name       string
		descriptor ValkeyDescriptor
		fragments  []string
	}{
		{
			name:       "bad address and db",
			descriptor: ValkeyDescriptor{Address: "cache.example.test", DB: -1},
			fragments:  []string{"db must not be negative", "address must include host and port"},
		},
		{
			name: "tls metadata disabled",
			descriptor: ValkeyDescriptor{
				Address: valkeyDefaultAddress,
				TLS:     ValkeyTLSMetadata{ServerName: "cache.example.test"},
			},
			fragments: []string{"tls metadata requires TLS to be enabled"},
		},
		{
			name: "auth conflicts and username whitespace",
			descriptor: ValkeyDescriptor{
				Address: valkeyDefaultAddress,
				Auth: ValkeyAuthMetadata{
					Username:    "app user",
					Password:    "secret",
					PasswordEnv: "VALKEY_PASSWORD",
				},
			},
			fragments: []string{"username must not contain whitespace", "password and password env are mutually exclusive"},
		},
		{
			name: "ttl bounds",
			descriptor: ValkeyDescriptor{
				Address: valkeyDefaultAddress,
				TTL: ValkeyTTLPolicy{
					DefaultTTL: time.Second,
					MinimumTTL: time.Minute,
					MaximumTTL: 30 * time.Second,
				},
			},
			fragments: []string{"minimum TTL must not exceed maximum TTL", "default TTL must not be below minimum TTL"},
		},
		{
			name: "cluster and sentinel conflict",
			descriptor: ValkeyDescriptor{
				Address: valkeyDefaultAddress,
				Cluster: ValkeyClusterMetadata{
					Enabled:   true,
					Addresses: []string{"cluster-b:6379"},
				},
				Sentinel: ValkeySentinelMetadata{
					MasterName: "primary",
					Addresses:  []string{"sentinel-a:26379"},
				},
			},
			fragments: []string{"cluster and sentinel modes are mutually exclusive"},
		},
		{
			name: "sentinel missing pieces",
			descriptor: ValkeyDescriptor{
				Address:  valkeyDefaultAddress,
				Sentinel: ValkeySentinelMetadata{Addresses: []string{"sentinel-a:26379"}},
			},
			fragments: []string{"sentinel master name is required"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateValkeyDescriptor(tt.descriptor)
			if !errors.Is(err, ErrInvalidValkeyDescriptor) {
				t.Fatalf("ValidateValkeyDescriptor() error = %v, want ErrInvalidValkeyDescriptor", err)
			}
			for _, fragment := range tt.fragments {
				if !strings.Contains(err.Error(), fragment) {
					t.Fatalf("ValidateValkeyDescriptor() error = %q, want fragment %q", err, fragment)
				}
			}
		})
	}
}

func TestValkeyTopologyMetadataIsDeterministic(t *testing.T) {
	cluster, err := PlanValkeyCache(ValkeyDescriptor{
		Cluster: ValkeyClusterMetadata{
			Enabled:   true,
			Addresses: []string{" cluster-b:6379 ", "cluster-a:6379", "cluster-b:6379"},
		},
	})
	if err != nil {
		t.Fatalf("PlanValkeyCache(cluster) error = %v", err)
	}
	if cluster.Mode != "cluster" {
		t.Fatalf("PlanValkeyCache(cluster).Mode = %q, want cluster", cluster.Mode)
	}
	wantClusterAddresses := []string{"cluster-a:6379", "cluster-b:6379"}
	if !reflect.DeepEqual(cluster.Descriptor.Cluster.Addresses, wantClusterAddresses) {
		t.Fatalf("cluster addresses = %#v, want %#v", cluster.Descriptor.Cluster.Addresses, wantClusterAddresses)
	}

	sentinel, err := PlanValkeyCache(ValkeyDescriptor{
		Sentinel: ValkeySentinelMetadata{
			MasterName: " primary ",
			Addresses:  []string{"sentinel-b:26379", " sentinel-a:26379 "},
		},
	})
	if err != nil {
		t.Fatalf("PlanValkeyCache(sentinel) error = %v", err)
	}
	if sentinel.Mode != "sentinel" {
		t.Fatalf("PlanValkeyCache(sentinel).Mode = %q, want sentinel", sentinel.Mode)
	}
	wantSentinelAddresses := []string{"sentinel-a:26379", "sentinel-b:26379"}
	if !reflect.DeepEqual(sentinel.Descriptor.Sentinel.Addresses, wantSentinelAddresses) {
		t.Fatalf("sentinel addresses = %#v, want %#v", sentinel.Descriptor.Sentinel.Addresses, wantSentinelAddresses)
	}
}

func TestPlanValkeyTTLAppliesDefaultsAndBounds(t *testing.T) {
	policy := ValkeyTTLPolicy{
		DefaultTTL: 5 * time.Minute,
		MinimumTTL: time.Minute,
		MaximumTTL: 10 * time.Minute,
	}
	tests := []struct {
		name      string
		requested time.Duration
		want      time.Duration
	}{
		{name: "default", requested: 0, want: 5 * time.Minute},
		{name: "minimum", requested: time.Second, want: time.Minute},
		{name: "maximum", requested: time.Hour, want: 10 * time.Minute},
		{name: "requested", requested: 2 * time.Minute, want: 2 * time.Minute},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := PlanValkeyTTL(policy, tt.requested)
			if err != nil {
				t.Fatalf("PlanValkeyTTL() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("PlanValkeyTTL() = %s, want %s", got, tt.want)
			}
		})
	}

	if got, err := PlanValkeyTTL(ValkeyTTLPolicy{}, 0); err != nil || got != valkeyDefaultTTL {
		t.Fatalf("PlanValkeyTTL(empty, 0) = %s, %v; want %s, nil", got, err, valkeyDefaultTTL)
	}
	if _, err := PlanValkeyTTL(policy, -time.Second); !errors.Is(err, ErrInvalidValkeyDescriptor) {
		t.Fatalf("PlanValkeyTTL(negative) error = %v, want ErrInvalidValkeyDescriptor", err)
	}
}

func TestRedactValkeyDescriptorSummary(t *testing.T) {
	summary := RedactValkeyDescriptor(ValkeyDescriptor{
		Address:   "rediss://app:secret@cache.example.test:6380/0?protocol=3",
		DB:        1,
		TLS:       ValkeyTLSMetadata{Enabled: true, ServerName: "cache.example.test"},
		Auth:      ValkeyAuthMetadata{Password: "secret"},
		KeyPrefix: "Billing",
		TTL:       ValkeyTTLPolicy{DefaultTTL: time.Minute},
	})

	if summary.Address != "rediss://app:redacted@cache.example.test:6380/0?redacted" {
		t.Fatalf("RedactValkeyDescriptor().Address = %q", summary.Address)
	}
	if strings.Contains(summary.Address, "secret") || strings.Contains(summary.Address, "protocol=3") {
		t.Fatalf("RedactValkeyDescriptor().Address leaked secret/query: %q", summary.Address)
	}
	if summary.Auth != "password" {
		t.Fatalf("RedactValkeyDescriptor().Auth = %q, want password", summary.Auth)
	}
	if summary.KeyPrefix != "billing" {
		t.Fatalf("RedactValkeyDescriptor().KeyPrefix = %q, want billing", summary.KeyPrefix)
	}
}
