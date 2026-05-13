package storage_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestB2PolicyDescriptorNormalizeValidateAndPlan(t *testing.T) {
	t.Parallel()

	policy := storage.B2PolicyDescriptor{
		Bucket:      " Customer-Files ",
		Region:      " US_West_004 ",
		EndpointURL: " https://api.backblazeb2.com/ ",
		ApplicationKey: storage.B2ApplicationKeyMetadata{
			KeyID:          " key-id-1 ",
			KeyName:        " customer exports ",
			ApplicationKey: " secret-key ",
			Capabilities:   []string{"writeFiles", " readFiles ", "writeFiles"},
			ExpiresAt:      time.Date(2026, 6, 1, 0, 0, 0, 0, time.UTC),
		},
		S3Compatible: storage.B2S3CompatibleMode{
			Enabled:         true,
			Region:          " US_West_004 ",
			EndpointURL:     "https://s3.us-west-004.backblazeb2.com/",
			AddressingStyle: "virtual-host",
		},
		ObjectPrefix: "/objects/",
	}

	normalized := policy.Normalize()
	if normalized.Bucket != "customer-files" {
		t.Fatalf("Bucket = %q, want customer-files", normalized.Bucket)
	}
	if normalized.Region != "us-west-004" {
		t.Fatalf("Region = %q, want us-west-004", normalized.Region)
	}
	if normalized.EndpointURL != "https://api.backblazeb2.com" {
		t.Fatalf("EndpointURL = %q, want trimmed endpoint", normalized.EndpointURL)
	}
	if got := normalized.ApplicationKey.Capabilities; len(got) != 2 || got[0] != "readFiles" || got[1] != "writeFiles" {
		t.Fatalf("Capabilities = %#v, want [readFiles writeFiles]", got)
	}
	if normalized.S3Compatible.AddressingStyle != storage.B2S3AddressingStyleVirtualHost {
		t.Fatalf("AddressingStyle = %q, want virtual_host", normalized.S3Compatible.AddressingStyle)
	}
	if normalized.ObjectPrefix != "objects" {
		t.Fatalf("ObjectPrefix = %q, want objects", normalized.ObjectPrefix)
	}
	if err := normalized.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	plan, err := normalized.PlanObjectKey("/reports/2026.csv", "tenant-a")
	if err != nil {
		t.Fatalf("PlanObjectKey() error = %v", err)
	}
	if plan.Prefix != "objects/tenant-a/" || plan.Key != "reports/2026.csv" || plan.Full != "objects/tenant-a/reports/2026.csv" {
		t.Fatalf("PlanObjectKey() = %#v, want objects/tenant-a/reports/2026.csv", plan)
	}
}

func TestB2Normalizers(t *testing.T) {
	t.Parallel()

	if got := storage.NormalizeB2Bucket(" Reports-2026 "); got != "reports-2026" {
		t.Fatalf("NormalizeB2Bucket() = %q, want reports-2026", got)
	}
	if got := storage.NormalizeB2Region(" EU_Central_003 "); got != "eu-central-003" {
		t.Fatalf("NormalizeB2Region() = %q, want eu-central-003", got)
	}
	if got := storage.NormalizeB2EndpointURL(" https://s3.us-west-004.backblazeb2.com// "); got != "https://s3.us-west-004.backblazeb2.com" {
		t.Fatalf("NormalizeB2EndpointURL() = %q, want endpoint without trailing slashes", got)
	}
	if got := storage.NormalizeB2S3AddressingStyle("path-style"); got != storage.B2S3AddressingStylePath {
		t.Fatalf("NormalizeB2S3AddressingStyle() = %q, want path", got)
	}
}

func TestB2ApplicationKeyMetadata(t *testing.T) {
	t.Parallel()

	metadata := storage.B2ApplicationKeyMetadata{
		KeyID:          "key-id",
		KeyName:        "exports",
		ApplicationKey: "secret",
		Capabilities:   []string{"writeFiles", "readFiles"},
	}
	if err := metadata.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if !metadata.AllowsCapability(" readFiles ") {
		t.Fatal("AllowsCapability(readFiles) = false, want true")
	}
	if metadata.AllowsCapability("deleteFiles") {
		t.Fatal("AllowsCapability(deleteFiles) = true, want false")
	}

	summary := metadata.RedactedSummary()
	if summary.KeyIDRedacted != "[redacted]" || summary.ApplicationKeyRedacted != "[redacted]" {
		t.Fatalf("redacted fields = %q/%q, want redacted", summary.KeyIDRedacted, summary.ApplicationKeyRedacted)
	}
	if !summary.HasApplicationKey {
		t.Fatal("HasApplicationKey = false, want true")
	}
}

func TestB2S3CompatibleMode(t *testing.T) {
	t.Parallel()

	mode := storage.B2S3CompatibleMode{
		Enabled:         true,
		Region:          "us-west-004",
		EndpointURL:     "https://user:pass@s3.us-west-004.backblazeb2.com/bucket?token=secret#fragment",
		AddressingStyle: "path-style",
	}
	if err := mode.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	summary := mode.RedactedSummary()
	if summary.EndpointURL != "https://s3.us-west-004.backblazeb2.com/bucket" {
		t.Fatalf("EndpointURL = %q, want redacted host/path URL", summary.EndpointURL)
	}
	if summary.AddressingStyle != storage.B2S3AddressingStylePath {
		t.Fatalf("AddressingStyle = %q, want path", summary.AddressingStyle)
	}

	disabled := storage.B2S3CompatibleMode{
		Region:          "us-west-004",
		EndpointURL:     "https://s3.us-west-004.backblazeb2.com",
		AddressingStyle: storage.B2S3AddressingStylePath,
	}.Normalize()
	if disabled.Enabled || disabled.EndpointURL != "" || disabled.Region != "" {
		t.Fatalf("disabled Normalize() = %#v, want empty disabled mode", disabled)
	}
}

func TestPlanB2ObjectKey(t *testing.T) {
	t.Parallel()

	plan, err := storage.PlanB2ObjectKey("/file.txt", "/exports/", "tenant-1")
	if err != nil {
		t.Fatalf("PlanB2ObjectKey() error = %v", err)
	}
	if plan.Prefix != "exports/tenant-1/" || plan.Key != "file.txt" || plan.Full != "exports/tenant-1/file.txt" {
		t.Fatalf("PlanB2ObjectKey() = %#v, want exports/tenant-1/file.txt", plan)
	}

	if _, err := storage.PlanB2ObjectKey("", "exports"); !errors.Is(err, storage.ErrB2PolicyInvalid) {
		t.Fatalf("PlanB2ObjectKey(empty key) error = %v, want ErrB2PolicyInvalid", err)
	}
	if _, err := storage.PlanB2ObjectKey("file.txt", "../exports"); !errors.Is(err, storage.ErrB2PolicyInvalid) {
		t.Fatalf("PlanB2ObjectKey(invalid prefix) error = %v, want ErrB2PolicyInvalid", err)
	}
}

func TestB2PolicyRedactedSummary(t *testing.T) {
	t.Parallel()

	policy := storage.B2PolicyDescriptor{
		Bucket:      "files-2026",
		Region:      "us-west-004",
		EndpointURL: "https://user:pass@api.backblazeb2.com/b2api/v3?authorization=secret#fragment",
		ApplicationKey: storage.B2ApplicationKeyMetadata{
			KeyID:          "key-id",
			ApplicationKey: "secret",
			Capabilities:   []string{"readFiles"},
		},
		ObjectPrefix: "/objects/",
	}

	summary := policy.RedactedSummary()
	if summary.EndpointURL != "https://api.backblazeb2.com/b2api/v3" {
		t.Fatalf("EndpointURL = %q, want redacted host/path URL", summary.EndpointURL)
	}
	if summary.ApplicationKey.KeyIDRedacted != "[redacted]" || summary.ApplicationKey.ApplicationKeyRedacted != "[redacted]" {
		t.Fatalf("application key summary = %#v, want redacted", summary.ApplicationKey)
	}
	if summary.ObjectPrefix != "objects" {
		t.Fatalf("ObjectPrefix = %q, want objects", summary.ObjectPrefix)
	}
}

func TestValidateB2PolicyDescriptorRejectsInvalidPolicies(t *testing.T) {
	t.Parallel()

	valid := storage.B2PolicyDescriptor{
		Bucket:      "files-2026",
		Region:      "us-west-004",
		EndpointURL: "https://api.backblazeb2.com",
		ApplicationKey: storage.B2ApplicationKeyMetadata{
			KeyID:          "key-id",
			ApplicationKey: "secret",
			Capabilities:   []string{"readFiles"},
		},
		S3Compatible: storage.B2S3CompatibleMode{
			Enabled:         true,
			Region:          "us-west-004",
			EndpointURL:     "https://s3.us-west-004.backblazeb2.com",
			AddressingStyle: storage.B2S3AddressingStyleVirtualHost,
		},
	}

	cases := []struct {
		name   string
		policy storage.B2PolicyDescriptor
	}{
		{
			name: "invalid bucket",
			policy: func() storage.B2PolicyDescriptor {
				p := valid
				p.Bucket = "bad_name"
				return p
			}(),
		},
		{
			name: "invalid region",
			policy: func() storage.B2PolicyDescriptor {
				p := valid
				p.Region = "us west"
				return p
			}(),
		},
		{
			name: "http endpoint",
			policy: func() storage.B2PolicyDescriptor {
				p := valid
				p.EndpointURL = "http://api.backblazeb2.com"
				return p
			}(),
		},
		{
			name: "missing application key secret",
			policy: func() storage.B2PolicyDescriptor {
				p := valid
				p.ApplicationKey.ApplicationKey = ""
				return p
			}(),
		},
		{
			name: "unknown capability",
			policy: func() storage.B2PolicyDescriptor {
				p := valid
				p.ApplicationKey.Capabilities = []string{"executeFiles"}
				return p
			}(),
		},
		{
			name: "unknown s3 addressing style",
			policy: func() storage.B2PolicyDescriptor {
				p := valid
				p.S3Compatible.AddressingStyle = "dns"
				return p
			}(),
		},
		{
			name: "invalid object prefix",
			policy: func() storage.B2PolicyDescriptor {
				p := valid
				p.ObjectPrefix = "objects/../admin"
				return p
			}(),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateB2PolicyDescriptor(tc.policy)
			if !errors.Is(err, storage.ErrB2PolicyInvalid) {
				t.Fatalf("ValidateB2PolicyDescriptor() error = %v, want ErrB2PolicyInvalid", err)
			}
		})
	}
}
