package storage_test

import (
	"errors"
	"testing"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestBuildQuotaPlanEvaluatesTenantUserBucketRules(t *testing.T) {
	t.Parallel()

	policy := storage.QuotaPolicy{Rules: []storage.QuotaRule{
		storage.TenantQuota("tenant-a", storage.QuotaLimit{SoftBytes: 90, HardBytes: 100}).Named("tenant-a"),
		storage.UserQuota("tenant-a", "user-1", storage.QuotaLimit{SoftBytes: 40, HardBytes: 42}).Named("user-1"),
		storage.BucketQuota("tenant-a", "avatars", storage.QuotaLimit{SoftBytes: 30, HardBytes: 50}).Named("avatars"),
	}}
	usage := []storage.QuotaUsage{
		{Scope: storage.QuotaScope{Tenant: "tenant-a", User: "user-1", Bucket: "avatars"}, Bytes: 35},
		{Scope: storage.QuotaScope{Tenant: "tenant-a", User: "user-2", Bucket: "docs"}, Bytes: 55},
		{Scope: storage.QuotaScope{Tenant: "tenant-b", User: "user-1", Bucket: "avatars"}, Bytes: 1000},
	}
	deltas := []storage.QuotaUsageDelta{
		{Scope: storage.QuotaScope{Tenant: "tenant-a", User: "user-1", Bucket: "avatars"}, Bytes: 8},
	}

	plan, err := storage.BuildQuotaPlan(policy, usage, deltas)
	if err != nil {
		t.Fatalf("BuildQuotaPlan() error = %v", err)
	}
	if !plan.DryRun {
		t.Fatal("plan DryRun = false, want true")
	}
	if plan.Allowed {
		t.Fatal("plan Allowed = true, want false")
	}
	if !plan.SoftLimitExceeded() {
		t.Fatal("SoftLimitExceeded() = false, want true")
	}
	if !plan.HardLimitExceeded() {
		t.Fatal("HardLimitExceeded() = false, want true")
	}
	if len(plan.Entries) != 3 {
		t.Fatalf("entries len = %d, want 3", len(plan.Entries))
	}

	tenant := plan.Entries[0]
	if tenant.RuleName != "tenant-a" || tenant.BeforeBytes != 90 || tenant.DeltaBytes != 8 || tenant.AfterBytes != 98 {
		t.Fatalf("tenant entry = %#v, want aggregated tenant usage 90 + 8", tenant)
	}
	if tenant.Status != storage.QuotaSoftExceeded || !tenant.Allowed {
		t.Fatalf("tenant status/allowed = %s/%v, want soft allowed", tenant.Status, tenant.Allowed)
	}

	user := plan.Entries[1]
	if user.RuleName != "user-1" || user.BeforeBytes != 35 || user.AfterBytes != 43 {
		t.Fatalf("user entry = %#v, want user usage 35 + 8", user)
	}
	if user.Status != storage.QuotaHardExceeded || user.Allowed {
		t.Fatalf("user status/allowed = %s/%v, want hard blocked", user.Status, user.Allowed)
	}

	bucket := plan.Entries[2]
	if bucket.RuleName != "avatars" || bucket.BeforeBytes != 35 || bucket.AfterBytes != 43 {
		t.Fatalf("bucket entry = %#v, want bucket usage 35 + 8", bucket)
	}
	if bucket.Status != storage.QuotaSoftExceeded || !bucket.Allowed {
		t.Fatalf("bucket status/allowed = %s/%v, want soft allowed", bucket.Status, bucket.Allowed)
	}

	if err := plan.Validate(); !errors.Is(err, storage.ErrQuotaHardLimitExceeded) {
		t.Fatalf("plan.Validate() error = %v, want ErrQuotaHardLimitExceeded", err)
	}
}

func TestBuildQuotaPlanAllowsUsageReductionOverHardLimit(t *testing.T) {
	t.Parallel()

	plan, err := storage.BuildQuotaPlan(
		storage.QuotaPolicy{Rules: []storage.QuotaRule{
			storage.TenantQuota("tenant-a", storage.QuotaLimit{HardBytes: 100}).Named("tenant-a"),
		}},
		[]storage.QuotaUsage{
			{Scope: storage.QuotaScope{Tenant: "tenant-a", User: "user-1", Bucket: "imports"}, Bytes: 130},
		},
		[]storage.QuotaUsageDelta{
			{Scope: storage.QuotaScope{Tenant: "tenant-a", User: "user-1", Bucket: "imports"}, Bytes: -10},
		},
	)
	if err != nil {
		t.Fatalf("BuildQuotaPlan() error = %v", err)
	}
	if !plan.Allowed {
		t.Fatal("plan Allowed = false, want true for usage reduction")
	}
	if err := storage.ValidateQuotaPlan(plan); err != nil {
		t.Fatalf("ValidateQuotaPlan() error = %v", err)
	}
	if len(plan.Entries) != 1 {
		t.Fatalf("entries len = %d, want 1", len(plan.Entries))
	}
	entry := plan.Entries[0]
	if entry.Status != storage.QuotaHardExceeded || !entry.Allowed {
		t.Fatalf("entry status/allowed = %s/%v, want hard status but allowed", entry.Status, entry.Allowed)
	}
	if entry.BeforeBytes != 130 || entry.DeltaBytes != -10 || entry.AfterBytes != 120 {
		t.Fatalf("entry bytes = %#v, want 130 - 10 = 120", entry)
	}
}

func TestBuildQuotaPlanIncludesUnmatchedDeltaAsUnlimited(t *testing.T) {
	t.Parallel()

	plan, err := storage.BuildQuotaPlan(
		storage.QuotaPolicy{Rules: []storage.QuotaRule{
			storage.TenantQuota("tenant-a", storage.QuotaLimit{HardBytes: 100}),
		}},
		[]storage.QuotaUsage{
			{Scope: storage.QuotaScope{Tenant: "tenant-b", User: "user-1", Bucket: "exports"}, Bytes: 10},
		},
		[]storage.QuotaUsageDelta{
			{Scope: storage.QuotaScope{Tenant: "tenant-b", User: "user-1", Bucket: "exports"}, Bytes: 5},
		},
	)
	if err != nil {
		t.Fatalf("BuildQuotaPlan() error = %v", err)
	}
	if len(plan.Entries) != 1 {
		t.Fatalf("entries len = %d, want 1", len(plan.Entries))
	}
	entry := plan.Entries[0]
	if entry.RuleName != "" || entry.HardBytes != 0 || entry.Status != storage.QuotaWithinLimit {
		t.Fatalf("unmatched entry = %#v, want unlimited within-limit entry", entry)
	}
	if entry.BeforeBytes != 10 || entry.DeltaBytes != 5 || entry.AfterBytes != 15 {
		t.Fatalf("unmatched bytes = %#v, want 10 + 5 = 15", entry)
	}
}

func TestValidateQuotaPolicyRejectsInvalidRules(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name   string
		policy storage.QuotaPolicy
	}{
		{
			name: "negative soft",
			policy: storage.QuotaPolicy{Rules: []storage.QuotaRule{
				storage.TenantQuota("tenant-a", storage.QuotaLimit{SoftBytes: -1}),
			}},
		},
		{
			name: "soft above hard",
			policy: storage.QuotaPolicy{Rules: []storage.QuotaRule{
				storage.TenantQuota("tenant-a", storage.QuotaLimit{SoftBytes: 11, HardBytes: 10}),
			}},
		},
		{
			name: "duplicate normalized scope",
			policy: storage.QuotaPolicy{Rules: []storage.QuotaRule{
				storage.TenantQuota(" tenant-a ", storage.QuotaLimit{HardBytes: 10}),
				storage.TenantQuota("tenant-a", storage.QuotaLimit{HardBytes: 20}),
			}},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateQuotaPolicy(tc.policy)
			if !errors.Is(err, storage.ErrQuotaPolicyInvalid) {
				t.Fatalf("ValidateQuotaPolicy() error = %v, want ErrQuotaPolicyInvalid", err)
			}
		})
	}
}

func TestValidateQuotaUsageRejectsInvalidUsage(t *testing.T) {
	t.Parallel()

	err := storage.ValidateQuotaUsage([]storage.QuotaUsage{
		{Scope: storage.QuotaScope{Tenant: "tenant-a"}, Bytes: -1},
	})
	if !errors.Is(err, storage.ErrQuotaUsageInvalid) {
		t.Fatalf("ValidateQuotaUsage() error = %v, want ErrQuotaUsageInvalid", err)
	}

	_, err = storage.BuildQuotaPlan(
		storage.QuotaPolicy{},
		[]storage.QuotaUsage{
			{Scope: storage.QuotaScope{Tenant: "tenant-a"}, Bytes: 4},
		},
		[]storage.QuotaUsageDelta{
			{Scope: storage.QuotaScope{Tenant: "tenant-a"}, Bytes: -5},
		},
	)
	if !errors.Is(err, storage.ErrQuotaUsageInvalid) {
		t.Fatalf("BuildQuotaPlan() overdraw error = %v, want ErrQuotaUsageInvalid", err)
	}
}

func TestQuotaScopeAndStatusHelpers(t *testing.T) {
	t.Parallel()

	scope := storage.QuotaScope{Tenant: " tenant-a ", Bucket: " avatars "}
	if got := scope.Normalize(); got != (storage.QuotaScope{Tenant: "tenant-a", Bucket: "avatars"}) {
		t.Fatalf("Normalize() = %#v", got)
	}
	if !scope.Matches(storage.QuotaScope{Tenant: "tenant-a", User: "user-1", Bucket: "avatars"}) {
		t.Fatal("tenant/bucket predicate did not match concrete scope")
	}
	if scope.Matches(storage.QuotaScope{Tenant: "tenant-b", User: "user-1", Bucket: "avatars"}) {
		t.Fatal("tenant/bucket predicate matched different tenant")
	}
	if got := scope.String(); got != "tenant=tenant-a,bucket=avatars" {
		t.Fatalf("String() = %q, want tenant=tenant-a,bucket=avatars", got)
	}

	statuses := map[storage.QuotaStatus]string{
		storage.QuotaWithinLimit:  "within_limit",
		storage.QuotaSoftExceeded: "soft_limit_exceeded",
		storage.QuotaHardExceeded: "hard_limit_exceeded",
		storage.QuotaStatus(99):   "unknown",
	}
	for status, want := range statuses {
		if got := status.String(); got != want {
			t.Fatalf("QuotaStatus(%d).String() = %q, want %q", status, got, want)
		}
	}
}
