package storage_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestSpacesPolicyDescriptorNormalizeValidateAndPlan(t *testing.T) {
	t.Parallel()

	policy := storage.SpacesPolicyDescriptor{
		Region:      " NYC3 ",
		Space:       " Customer-Files ",
		EndpointURL: "https://USER:PASS@NYC3.DIGITALOCEANSPACES.COM/?secret=yes#fragment",
		CDNEndpoint: "https://Customer-Files.NYC3.CDN.DIGITALOCEANSPACES.COM/cache/?token=secret",
		AccessMode:  storage.SpacesAccessMode(" public "),
		Capabilities: storage.SpacesS3Capabilities{
			VirtualHostedStyle: true,
			PresignedURLs:      true,
			MultipartUpload:    true,
			ObjectACLs:         true,
			Methods:            []string{"put", "GET", "get"},
			MaxSignedURLAge:    15 * time.Minute,
		},
		ObjectPrefix:    "/objects/",
		AccessKeyID:     " key-id ",
		SecretAccessKey: " secret ",
	}

	normalized, err := policy.Normalize()
	if err != nil {
		t.Fatalf("Normalize() error = %v", err)
	}
	if normalized.Region != "nyc3" {
		t.Fatalf("Region = %q, want nyc3", normalized.Region)
	}
	if normalized.Space != "customer-files" {
		t.Fatalf("Space = %q, want customer-files", normalized.Space)
	}
	if normalized.EndpointURL != "https://nyc3.digitaloceanspaces.com" {
		t.Fatalf("EndpointURL = %q, want canonical origin", normalized.EndpointURL)
	}
	if normalized.CDNEndpoint != "https://customer-files.nyc3.cdn.digitaloceanspaces.com/cache" {
		t.Fatalf("CDNEndpoint = %q, want canonical CDN URL", normalized.CDNEndpoint)
	}
	if normalized.AccessMode != storage.SpacesAccessModePublicRead {
		t.Fatalf("AccessMode = %q, want public-read", normalized.AccessMode)
	}
	if got := normalized.Capabilities.Methods; len(got) != 2 || got[0] != "GET" || got[1] != "PUT" {
		t.Fatalf("Capabilities.Methods = %#v, want [GET PUT]", got)
	}
	if got := normalized.ObjectPrefix; got != "objects" {
		t.Fatalf("ObjectPrefix = %q, want objects", got)
	}
	if err := normalized.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	plan, err := normalized.PlanObjectKey("/invoice.pdf", "tenants", "tenant-a")
	if err != nil {
		t.Fatalf("PlanObjectKey() error = %v", err)
	}
	if plan.Prefix != "objects/tenants/tenant-a/" || plan.Key != "invoice.pdf" || plan.Full != "objects/tenants/tenant-a/invoice.pdf" {
		t.Fatalf("PlanObjectKey() = %#v, want objects/tenants/tenant-a/invoice.pdf", plan)
	}
}

func TestSpacesNormalizers(t *testing.T) {
	t.Parallel()

	if got := storage.NormalizeSpacesRegion("SFO_3"); got != "sfo-3" {
		t.Fatalf("NormalizeSpacesRegion() = %q, want sfo-3", got)
	}
	if got := storage.NormalizeSpacesName(" Reports-2026 "); got != "reports-2026" {
		t.Fatalf("NormalizeSpacesName() = %q, want reports-2026", got)
	}
	endpoint, err := storage.NormalizeSpacesEndpointURL("ams3", "")
	if err != nil {
		t.Fatalf("NormalizeSpacesEndpointURL(default) error = %v", err)
	}
	if endpoint != "https://ams3.digitaloceanspaces.com" {
		t.Fatalf("NormalizeSpacesEndpointURL(default) = %q, want https://ams3.digitaloceanspaces.com", endpoint)
	}
}

func TestSpacesAccessModeValidation(t *testing.T) {
	t.Parallel()

	cases := []struct {
		raw  storage.SpacesAccessMode
		want storage.SpacesAccessMode
	}{
		{raw: "", want: storage.SpacesAccessModePrivate},
		{raw: " PRIVATE ", want: storage.SpacesAccessModePrivate},
		{raw: "public", want: storage.SpacesAccessModePublicRead},
		{raw: "public_read", want: storage.SpacesAccessModePublicRead},
	}
	for _, tc := range cases {
		if got := storage.NormalizeSpacesAccessMode(tc.raw); got != tc.want {
			t.Fatalf("NormalizeSpacesAccessMode(%q) = %q, want %q", tc.raw, got, tc.want)
		}
		if err := storage.ValidateSpacesAccessMode(tc.raw); err != nil {
			t.Fatalf("ValidateSpacesAccessMode(%q) error = %v", tc.raw, err)
		}
	}
	if err := storage.ValidateSpacesAccessMode("authenticated"); !errors.Is(err, storage.ErrSpacesPolicyInvalid) {
		t.Fatalf("ValidateSpacesAccessMode(invalid) error = %v, want ErrSpacesPolicyInvalid", err)
	}
}

func TestSpacesS3Capabilities(t *testing.T) {
	t.Parallel()

	capabilities := storage.SpacesS3Capabilities{
		PresignedURLs:   true,
		Methods:         []string{"head", "GET", "HEAD"},
		MaxSignedURLAge: time.Minute,
	}
	if err := capabilities.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if !capabilities.AllowsMethod(" head ") {
		t.Fatal("AllowsMethod(head) = false, want true")
	}
	if capabilities.AllowsMethod("delete") {
		t.Fatal("AllowsMethod(delete) = true, want false")
	}

	invalid := storage.SpacesS3Capabilities{PresignedURLs: true}
	if err := invalid.Validate(); !errors.Is(err, storage.ErrSpacesPolicyInvalid) {
		t.Fatalf("Validate(presigned without methods) error = %v, want ErrSpacesPolicyInvalid", err)
	}
}

func TestPlanSpacesObjectKey(t *testing.T) {
	t.Parallel()

	plan, err := storage.PlanSpacesObjectKey("/exports/report.csv", "/objects/", "tenants", "tenant-1")
	if err != nil {
		t.Fatalf("PlanSpacesObjectKey() error = %v", err)
	}
	if plan.Prefix != "objects/tenants/tenant-1/" || plan.Key != "exports/report.csv" || plan.Full != "objects/tenants/tenant-1/exports/report.csv" {
		t.Fatalf("PlanSpacesObjectKey() = %#v, want normalized full key", plan)
	}

	if _, err := storage.PlanSpacesObjectKey("../report.csv", "objects"); !errors.Is(err, storage.ErrSpacesPolicyInvalid) {
		t.Fatalf("PlanSpacesObjectKey(invalid key) error = %v, want ErrSpacesPolicyInvalid", err)
	}
}

func TestSpacesPolicyDescriptorRedactedSummary(t *testing.T) {
	t.Parallel()

	policy := storage.SpacesPolicyDescriptor{
		Region:          "nyc3",
		Space:           "exports",
		EndpointURL:     "https://user:pass@nyc3.digitaloceanspaces.com/exports?sig=secret#fragment",
		CDNEndpoint:     "not a url",
		AccessMode:      storage.SpacesAccessModePrivate,
		ObjectPrefix:    "objects",
		AccessKeyID:     "AKIAEXAMPLE",
		SecretAccessKey: "secret-key",
	}

	summary := policy.RedactedSummary()
	if summary.EndpointURL != "https://nyc3.digitaloceanspaces.com/exports" {
		t.Fatalf("EndpointURL = %q, want redacted host/path URL", summary.EndpointURL)
	}
	if summary.CDNEndpoint != "[redacted]" {
		t.Fatalf("CDNEndpoint = %q, want [redacted]", summary.CDNEndpoint)
	}
	if summary.AccessKeyIDRedacted != "[redacted]" || summary.SecretAccessKey != "[redacted]" {
		t.Fatalf("secret summaries = %q/%q, want [redacted]", summary.AccessKeyIDRedacted, summary.SecretAccessKey)
	}
	if !summary.HasAccessKeyID || !summary.HasSecretAccessKey {
		t.Fatalf("secret presence = %v/%v, want true/true", summary.HasAccessKeyID, summary.HasSecretAccessKey)
	}
}

func TestValidateSpacesPolicyDescriptorRejectsInvalidPolicies(t *testing.T) {
	t.Parallel()

	valid := storage.SpacesPolicyDescriptor{
		Region:     "nyc3",
		Space:      "exports",
		AccessMode: storage.SpacesAccessModePrivate,
	}

	cases := []struct {
		name   string
		policy storage.SpacesPolicyDescriptor
	}{
		{
			name: "invalid region",
			policy: func() storage.SpacesPolicyDescriptor {
				p := valid
				p.Region = "-nyc3"
				return p
			}(),
		},
		{
			name: "invalid space",
			policy: func() storage.SpacesPolicyDescriptor {
				p := valid
				p.Space = "bad..name"
				return p
			}(),
		},
		{
			name: "invalid endpoint",
			policy: func() storage.SpacesPolicyDescriptor {
				p := valid
				p.EndpointURL = "ftp://nyc3.digitaloceanspaces.com"
				return p
			}(),
		},
		{
			name: "invalid cdn endpoint",
			policy: func() storage.SpacesPolicyDescriptor {
				p := valid
				p.CDNEndpoint = "://bad"
				return p
			}(),
		},
		{
			name: "unknown access mode",
			policy: func() storage.SpacesPolicyDescriptor {
				p := valid
				p.AccessMode = "shared"
				return p
			}(),
		},
		{
			name: "invalid object prefix",
			policy: func() storage.SpacesPolicyDescriptor {
				p := valid
				p.ObjectPrefix = "../objects"
				return p
			}(),
		},
		{
			name: "negative signed url max age",
			policy: func() storage.SpacesPolicyDescriptor {
				p := valid
				p.Capabilities = storage.SpacesS3Capabilities{MaxSignedURLAge: -time.Second}
				return p
			}(),
		},
		{
			name: "disabled presigned urls with methods",
			policy: func() storage.SpacesPolicyDescriptor {
				p := valid
				p.Capabilities = storage.SpacesS3Capabilities{Methods: []string{"GET"}}
				return p
			}(),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateSpacesPolicyDescriptor(tc.policy)
			if !errors.Is(err, storage.ErrSpacesPolicyInvalid) {
				t.Fatalf("ValidateSpacesPolicyDescriptor() error = %v, want ErrSpacesPolicyInvalid", err)
			}
		})
	}
}
