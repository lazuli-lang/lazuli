package storage_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestAzureBlobContainerPolicyNormalizeValidateAndPlan(t *testing.T) {
	t.Parallel()

	policy := storage.AzureBlobContainerPolicy{
		AccountName:   " AppStorage01 ",
		ContainerName: " Customer-Files ",
		AccessTier:    storage.AzureBlobAccessTier(" Hot "),
		PublicMode:    storage.AzureBlobPublicMode(" PRIVATE "),
		KeyPrefix: storage.AzureBlobKeyPrefix{
			Prefix:       "/objects/",
			TenantPrefix: "tenants",
		},
		SAS: storage.AzureBlobSASCapabilities{
			Enabled:     true,
			MaxAge:      15 * time.Minute,
			Permissions: []string{"Write", " read ", "write"},
		},
	}

	normalized, err := policy.Normalize()
	if err != nil {
		t.Fatalf("Normalize() error = %v", err)
	}
	if normalized.AccountName != "appstorage01" {
		t.Fatalf("AccountName = %q, want appstorage01", normalized.AccountName)
	}
	if normalized.ContainerName != "customer-files" {
		t.Fatalf("ContainerName = %q, want customer-files", normalized.ContainerName)
	}
	if normalized.AccessTier != storage.AzureBlobAccessTierHot {
		t.Fatalf("AccessTier = %q, want hot", normalized.AccessTier)
	}
	if normalized.PublicMode != storage.AzureBlobPublicModePrivate {
		t.Fatalf("PublicMode = %q, want private", normalized.PublicMode)
	}
	if !normalized.KeyPrefix.TenantScoped {
		t.Fatal("TenantScoped = false, want true")
	}
	if got := normalized.KeyPrefix.Prefix; got != "objects" {
		t.Fatalf("KeyPrefix.Prefix = %q, want objects", got)
	}
	if got := normalized.SAS.Permissions; len(got) != 2 || got[0] != "read" || got[1] != "write" {
		t.Fatalf("SAS.Permissions = %#v, want [read write]", got)
	}
	if err := normalized.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	prefix, err := normalized.BlobPrefix("tenant-a")
	if err != nil {
		t.Fatalf("BlobPrefix() error = %v", err)
	}
	if prefix != "objects/tenants/tenant-a/" {
		t.Fatalf("BlobPrefix() = %q, want objects/tenants/tenant-a/", prefix)
	}
}

func TestAzureBlobNameNormalizers(t *testing.T) {
	t.Parallel()

	account, err := storage.NormalizeAzureBlobAccountName(" Files123 ")
	if err != nil {
		t.Fatalf("NormalizeAzureBlobAccountName() error = %v", err)
	}
	if account != "files123" {
		t.Fatalf("NormalizeAzureBlobAccountName() = %q, want files123", account)
	}

	container, err := storage.NormalizeAzureBlobContainerName(" Reports-2026 ")
	if err != nil {
		t.Fatalf("NormalizeAzureBlobContainerName() error = %v", err)
	}
	if container != "reports-2026" {
		t.Fatalf("NormalizeAzureBlobContainerName() = %q, want reports-2026", container)
	}
}

func TestAzureBlobAccessTierAndPublicModeValidation(t *testing.T) {
	t.Parallel()

	if err := storage.ValidateAzureBlobAccessTier(storage.AzureBlobAccessTier(" cool ")); err != nil {
		t.Fatalf("ValidateAzureBlobAccessTier(cool) error = %v", err)
	}
	if err := storage.AzureBlobPublicMode(" blob ").Validate(); err != nil {
		t.Fatalf("AzureBlobPublicMode(blob).Validate() error = %v", err)
	}
	if got := storage.AzureBlobAccessTier("made-up").String(); got != "unknown" {
		t.Fatalf("unknown tier String() = %q, want unknown", got)
	}
	if got := storage.AzureBlobPublicMode("shared").String(); got != "unknown" {
		t.Fatalf("unknown mode String() = %q, want unknown", got)
	}
}

func TestPlanAzureBlobKeyPrefix(t *testing.T) {
	t.Parallel()

	global, err := storage.PlanAzureBlobKeyPrefix(storage.AzureBlobKeyPrefix{Prefix: "/exports/"}, "")
	if err != nil {
		t.Fatalf("PlanAzureBlobKeyPrefix(global) error = %v", err)
	}
	if global != "exports/" {
		t.Fatalf("PlanAzureBlobKeyPrefix(global) = %q, want exports/", global)
	}

	tenant, err := storage.PlanAzureBlobKeyPrefix(storage.AzureBlobKeyPrefix{
		Prefix:       "objects",
		TenantPrefix: "tenants",
	}, "tenant-1")
	if err != nil {
		t.Fatalf("PlanAzureBlobKeyPrefix(tenant) error = %v", err)
	}
	if tenant != "objects/tenants/tenant-1/" {
		t.Fatalf("PlanAzureBlobKeyPrefix(tenant) = %q, want objects/tenants/tenant-1/", tenant)
	}

	if _, err := storage.PlanAzureBlobKeyPrefix(storage.AzureBlobKeyPrefix{TenantScoped: true, TenantPrefix: "tenants"}, "tenant/a"); !errors.Is(err, storage.ErrAzureBlobPolicyInvalid) {
		t.Fatalf("PlanAzureBlobKeyPrefix(invalid tenant) error = %v, want ErrAzureBlobPolicyInvalid", err)
	}
}

func TestAzureBlobSASCapabilities(t *testing.T) {
	t.Parallel()

	capabilities := storage.AzureBlobSASCapabilities{
		Enabled:     true,
		MaxAge:      time.Minute,
		Permissions: []string{"List", "read", "list"},
	}
	if err := capabilities.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if !capabilities.AllowsPermission(" list ") {
		t.Fatal("AllowsPermission(list) = false, want true")
	}
	if capabilities.AllowsPermission("delete") {
		t.Fatal("AllowsPermission(delete) = true, want false")
	}

	disabled := storage.AzureBlobSASCapabilities{Permissions: []string{"read"}}.Normalize()
	if disabled.Enabled || len(disabled.Permissions) != 0 {
		t.Fatalf("disabled Normalize() = %#v, want empty disabled capabilities", disabled)
	}
}

func TestAzureBlobContainerPolicyRedactedSummary(t *testing.T) {
	t.Parallel()

	policy := storage.AzureBlobContainerPolicy{
		AccountName:      "filesacct",
		ContainerName:    "exports",
		AccessTier:       storage.AzureBlobAccessTierHot,
		PublicMode:       storage.AzureBlobPublicModePrivate,
		EndpointURL:      "https://user:pass@filesacct.blob.core.windows.net/exports?sig=secret&sp=r#fragment",
		AccountKey:       "secret-key",
		ConnectionString: "DefaultEndpointsProtocol=https;AccountName=filesacct;AccountKey=secret",
		SAS: storage.AzureBlobSASCapabilities{
			Enabled:     true,
			Permissions: []string{"read"},
		},
	}

	summary := policy.RedactedSummary()
	if summary.EndpointURL != "https://filesacct.blob.core.windows.net/exports" {
		t.Fatalf("EndpointURL = %q, want redacted host/path URL", summary.EndpointURL)
	}
	if summary.AccountKey != "redacted" || summary.ConnectionString != "redacted" {
		t.Fatalf("secret summaries = %q/%q, want redacted", summary.AccountKey, summary.ConnectionString)
	}
	if !summary.HasAccountKey || !summary.HasConnectionString {
		t.Fatalf("secret presence = %v/%v, want true/true", summary.HasAccountKey, summary.HasConnectionString)
	}
}

func TestValidateAzureBlobContainerPolicyRejectsInvalidPolicies(t *testing.T) {
	t.Parallel()

	valid := storage.AzureBlobContainerPolicy{
		AccountName:   "filesacct",
		ContainerName: "exports",
		AccessTier:    storage.AzureBlobAccessTierHot,
		PublicMode:    storage.AzureBlobPublicModePrivate,
	}

	cases := []struct {
		name   string
		policy storage.AzureBlobContainerPolicy
	}{
		{
			name: "invalid account",
			policy: func() storage.AzureBlobContainerPolicy {
				p := valid
				p.AccountName = "ab"
				return p
			}(),
		},
		{
			name: "invalid container",
			policy: func() storage.AzureBlobContainerPolicy {
				p := valid
				p.ContainerName = "bad--name"
				return p
			}(),
		},
		{
			name: "unknown access tier",
			policy: func() storage.AzureBlobContainerPolicy {
				p := valid
				p.AccessTier = "premium"
				return p
			}(),
		},
		{
			name: "unknown public mode",
			policy: func() storage.AzureBlobContainerPolicy {
				p := valid
				p.PublicMode = "shared"
				return p
			}(),
		},
		{
			name: "invalid key prefix",
			policy: func() storage.AzureBlobContainerPolicy {
				p := valid
				p.KeyPrefix = storage.AzureBlobKeyPrefix{Prefix: "../objects"}
				return p
			}(),
		},
		{
			name: "sas without permissions",
			policy: func() storage.AzureBlobContainerPolicy {
				p := valid
				p.SAS = storage.AzureBlobSASCapabilities{Enabled: true}
				return p
			}(),
		},
		{
			name: "sas invalid permission",
			policy: func() storage.AzureBlobContainerPolicy {
				p := valid
				p.SAS = storage.AzureBlobSASCapabilities{Enabled: true, Permissions: []string{"execute"}}
				return p
			}(),
		},
		{
			name: "sas negative max age",
			policy: func() storage.AzureBlobContainerPolicy {
				p := valid
				p.SAS = storage.AzureBlobSASCapabilities{Enabled: true, MaxAge: -time.Second, Permissions: []string{"read"}}
				return p
			}(),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateAzureBlobContainerPolicy(tc.policy)
			if !errors.Is(err, storage.ErrAzureBlobPolicyInvalid) {
				t.Fatalf("ValidateAzureBlobContainerPolicy() error = %v, want ErrAzureBlobPolicyInvalid", err)
			}
		})
	}
}
