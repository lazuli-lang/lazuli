package auth

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

func TestServiceAccountPrincipalIDs(t *testing.T) {
	t.Parallel()

	if got, want := ServiceAccountPrincipalID(42), "service_account:42"; got != want {
		t.Fatalf("ServiceAccountPrincipalID() = %q, want %q", got, want)
	}
	if got, want := ServiceAccountPrincipalIDForOrg(7, 42), "org:7:service_account:42"; got != want {
		t.Fatalf("ServiceAccountPrincipalIDForOrg() = %q, want %q", got, want)
	}

	named, err := NamedServiceAccountPrincipalID("billing-sync")
	if err != nil {
		t.Fatalf("NamedServiceAccountPrincipalID() error = %v", err)
	}
	if named != "service_account:billing-sync" {
		t.Fatalf("NamedServiceAccountPrincipalID() = %q", named)
	}

	principal, err := ParseServiceAccountPrincipalID("org:7:service_account:42")
	if err != nil {
		t.Fatalf("ParseServiceAccountPrincipalID() error = %v", err)
	}
	if principal.OrgID != 7 || principal.ServiceAccountID != 42 || principal.Name != "" {
		t.Fatalf("principal = %#v, want org/account ids", principal)
	}

	namedPrincipal, err := ParseServiceAccountPrincipalID(named)
	if err != nil {
		t.Fatalf("ParseServiceAccountPrincipalID(named) error = %v", err)
	}
	if namedPrincipal.Name != "billing-sync" {
		t.Fatalf("named principal = %#v, want name", namedPrincipal)
	}

	if got := (ServiceAccountPrincipal{OrgID: 7, Name: "crm-sync"}).PrincipalID(); got != "org:7:service_account:crm-sync" {
		t.Fatalf("ServiceAccountPrincipal.PrincipalID() = %q", got)
	}
	if err := ValidateServiceAccountPrincipalID("user:42"); !errors.Is(err, ErrServiceAccountPrincipalInvalid) {
		t.Fatalf("ValidateServiceAccountPrincipalID(invalid) error = %v, want ErrServiceAccountPrincipalInvalid", err)
	}
	if got := ServiceAccountPrincipalID(0, "bad name"); got != "" {
		t.Fatalf("ServiceAccountPrincipalID(invalid name) = %q, want empty", got)
	}
}

func TestServiceAccountScopeValidationAndMatching(t *testing.T) {
	t.Parallel()

	scopes := NormalizeServiceAccountScopes([]string{" orders:read ", "", "orders:read", "orders:*", "*:admin"})
	if len(scopes) != 3 {
		t.Fatalf("normalized scope count = %d, want 3: %#v", len(scopes), scopes)
	}
	if !scopes.Has("orders:write") {
		t.Fatalf("scopes.Has(orders:write) = false, want true")
	}
	if !scopes.HasAny("billing:read", "orders:read") {
		t.Fatalf("scopes.HasAny() = false, want true")
	}
	if scopes.Has("billing:*") {
		t.Fatalf("required wildcard matched unexpectedly")
	}
	if err := ValidateServiceAccountScopes(scopes); err != nil {
		t.Fatalf("ValidateServiceAccountScopes(valid) error = %v", err)
	}
	if err := ValidateServiceAccountScope("orders read"); !errors.Is(err, ErrServiceAccountScopeInvalid) {
		t.Fatalf("ValidateServiceAccountScope(invalid) error = %v, want ErrServiceAccountScopeInvalid", err)
	}
}

func TestValidateServiceAccountKey(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	active := ServiceAccountKeyMetadata{
		KeyID:       "key_123",
		PrincipalID: "org:7:service_account:42",
		Scopes:      ServiceAccountScopes{"orders:*", "billing:invoice:read"},
		CreatedAt:   now.Add(-time.Hour),
		NotBefore:   now.Add(-time.Minute),
		ExpiresAt:   now.Add(time.Hour),
	}

	if err := ValidateServiceAccountKey(active, now, "orders:write", "billing:invoice:read"); err != nil {
		t.Fatalf("ValidateServiceAccountKey(active) error = %v", err)
	}
	if err := active.Validate(now, "orders:read"); err != nil {
		t.Fatalf("ServiceAccountKeyMetadata.Validate(active) error = %v", err)
	}

	for _, tt := range []struct {
		name string
		meta ServiceAccountKeyMetadata
		want error
	}{
		{
			name: "not yet valid",
			meta: ServiceAccountKeyMetadata{Scopes: ServiceAccountScopes{"*"}, NotBefore: now.Add(time.Minute), ExpiresAt: now.Add(time.Hour)},
			want: ErrServiceAccountKeyNotYetValid,
		},
		{
			name: "expired",
			meta: ServiceAccountKeyMetadata{Scopes: ServiceAccountScopes{"*"}, ExpiresAt: now.Add(-time.Second)},
			want: ErrServiceAccountKeyExpired,
		},
		{
			name: "rotated",
			meta: ServiceAccountKeyMetadata{Scopes: ServiceAccountScopes{"*"}, ExpiresAt: now.Add(time.Hour), RotatedAt: now},
			want: ErrServiceAccountKeyRotated,
		},
		{
			name: "revoked",
			meta: ServiceAccountKeyMetadata{Scopes: ServiceAccountScopes{"*"}, ExpiresAt: now.Add(time.Hour), RevokedAt: now},
			want: ErrServiceAccountKeyRevoked,
		},
		{
			name: "scope denied",
			meta: ServiceAccountKeyMetadata{Scopes: ServiceAccountScopes{"orders:read"}, ExpiresAt: now.Add(time.Hour)},
			want: ErrServiceAccountScopeDenied,
		},
	} {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if err := ValidateServiceAccountKey(tt.meta, now, "orders:write"); !errors.Is(err, tt.want) {
				t.Fatalf("ValidateServiceAccountKey() error = %v, want %v", err, tt.want)
			}
		})
	}

	if err := ValidateServiceAccountKey(active, now, "orders read"); !errors.Is(err, ErrTokenInvalid) {
		t.Fatalf("ValidateServiceAccountKey(invalid scope) error = %v, want ErrTokenInvalid", err)
	}
}

func TestServiceAccountKeyMetadataValidationAndClone(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	meta := ServiceAccountKeyMetadata{
		ID:               "key_123",
		KeyID:            "key_123",
		ServiceAccountID: lazuli.ID(42),
		OrgID:            lazuli.ID(7),
		PrincipalID:      "org:7:service_account:42",
		Name:             "Deploy service",
		KeyName:          "May rotation",
		Algorithm:        "ed25519",
		Fingerprint:      "SHA256:abcdef",
		Scopes:           ServiceAccountScopes{"orders:read"},
		CreatedAt:        now,
		NotBefore:        now,
		ExpiresAt:        now.Add(24 * time.Hour),
		LastUsedAt:       now.Add(time.Hour),
		RotationDueAt:    now.Add(20 * time.Hour),
		Attrs: map[string]any{
			"label": "deploy",
			"tags":  []string{"ci"},
			"meta":  map[string]any{"tier": "prod"},
		},
	}

	if err := ValidateServiceAccountKeyMetadata(meta); err != nil {
		t.Fatalf("ValidateServiceAccountKeyMetadata(valid) error = %v", err)
	}

	cloned := meta.Clone()
	cloned.Scopes[0] = "orders:write"
	cloned.Attrs["label"] = "mutated"
	cloned.Attrs["tags"].([]string)[0] = "mutated"
	cloned.Attrs["meta"].(map[string]any)["tier"] = "mutated"

	if meta.Scopes[0] != "orders:read" {
		t.Fatalf("original scope mutated: %#v", meta.Scopes)
	}
	if meta.Attrs["label"] != "deploy" || meta.Attrs["tags"].([]string)[0] != "ci" || meta.Attrs["meta"].(map[string]any)["tier"] != "prod" {
		t.Fatalf("original attrs mutated: %#v", meta.Attrs)
	}

	bad := meta
	bad.PrincipalID = "org:99:service_account:42"
	if err := ValidateServiceAccountKeyMetadata(bad); !errors.Is(err, ErrServiceAccountKeyInvalid) {
		t.Fatalf("ValidateServiceAccountKeyMetadata(mismatch) error = %v, want ErrServiceAccountKeyInvalid", err)
	}

	bad = meta
	bad.KeyID = "key_456"
	if err := ValidateServiceAccountKeyMetadata(bad); !errors.Is(err, ErrServiceAccountKeyInvalid) {
		t.Fatalf("ValidateServiceAccountKeyMetadata(key mismatch) error = %v, want ErrServiceAccountKeyInvalid", err)
	}
}

func TestServiceAccountKeyRotationPlan(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	meta := ServiceAccountKeyMetadata{
		KeyID:            "key_123",
		ServiceAccountID: 42,
		CreatedAt:        now.Add(-10 * 24 * time.Hour),
		ExpiresAt:        now.Add(2 * 24 * time.Hour),
		Scopes:           ServiceAccountScopes{"*"},
	}
	policy := ServiceAccountKeyRotationPolicy{
		MaxAge:             7 * 24 * time.Hour,
		RotateBeforeExpiry: 3 * 24 * time.Hour,
		GracePeriod:        time.Hour,
	}

	window := meta.RotationWindow(policy)
	if want := now.Add(-3 * 24 * time.Hour); !window.OpensAt.Equal(want) {
		t.Fatalf("window.OpensAt = %v, want %v", window.OpensAt, want)
	}
	if !window.Contains(now) {
		t.Fatalf("window.Contains(now) = false, want true")
	}
	if !window.GraceUntil.Equal(meta.ExpiresAt.Add(time.Hour)) {
		t.Fatalf("window.GraceUntil = %v, want expiry + grace", window.GraceUntil)
	}

	plan := PlanServiceAccountKeyRotation(now, meta, policy)
	if !plan.Rotate {
		t.Fatalf("plan.Rotate = false, want true")
	}
	if !serviceAccountTestHasReason(plan.Reasons, ServiceAccountKeyRotationReasonMaxAge) {
		t.Fatalf("plan.Reasons = %#v, want max_age", plan.Reasons)
	}
	if !serviceAccountTestHasReason(plan.Reasons, ServiceAccountKeyRotationReasonExpiresSoon) {
		t.Fatalf("plan.Reasons = %#v, want expires_soon", plan.Reasons)
	}

	revoked := meta
	revoked.RevokedAt = now
	if ShouldRotateServiceAccountKey(now, revoked, policy) {
		t.Fatalf("ShouldRotateServiceAccountKey(revoked) = true, want false")
	}
}

func TestServiceAccountAuditSafeDisplayAndPayload(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	meta := ServiceAccountKeyMetadata{
		KeyID:            "key_123",
		ServiceAccountID: 42,
		OrgID:            7,
		Name:             "Deploy service",
		KeyName:          "May rotation",
		Algorithm:        "ed25519",
		Fingerprint:      "SHA256:abcdef",
		Scopes:           ServiceAccountScopes{" orders:read ", "orders:read", "orders:write"},
		CreatedAt:        now.Add(-time.Hour),
		ExpiresAt:        now.Add(time.Hour),
		Attrs:            map[string]any{"secret": "must not appear"},
	}

	display := meta.AuditSafeDisplay(now)
	if display.PrincipalID != "org:7:service_account:42" {
		t.Fatalf("display.PrincipalID = %q", display.PrincipalID)
	}
	if display.Status != ServiceAccountKeyStatusActive {
		t.Fatalf("display.Status = %q, want active", display.Status)
	}
	if len(display.Scopes) != 2 {
		t.Fatalf("display.Scopes = %#v, want normalized scopes", display.Scopes)
	}

	payload, err := BuildServiceAccountKeyAuditPayload(meta, now)
	if err != nil {
		t.Fatalf("BuildServiceAccountKeyAuditPayload() error = %v", err)
	}
	if strings.Contains(string(payload), "must not appear") || strings.Contains(string(payload), "Attrs") {
		t.Fatalf("audit payload leaked attrs: %s", payload)
	}

	var decoded map[string]any
	if err := json.Unmarshal(payload, &decoded); err != nil {
		t.Fatalf("audit payload JSON decode error = %v", err)
	}
	if decoded["key_id"] != "key_123" {
		t.Fatalf("payload key_id = %v, want key_123", decoded["key_id"])
	}
}

func serviceAccountTestHasReason(reasons []ServiceAccountKeyRotationReason, want ServiceAccountKeyRotationReason) bool {
	for _, reason := range reasons {
		if reason == want {
			return true
		}
	}
	return false
}
