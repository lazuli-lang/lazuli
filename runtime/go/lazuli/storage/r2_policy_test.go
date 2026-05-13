package storage_test

import (
	"errors"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/storage"
)

const r2AccountID = "0123456789abcdef0123456789abcdef"

func TestR2PolicyDescriptorNormalizesAndValidates(t *testing.T) {
	t.Parallel()

	descriptor := storage.R2PolicyDescriptor{
		AccountID:   " 0123456789ABCDEF0123456789ABCDEF ",
		Bucket:      " App-Uploads ",
		AccessMode:  "public-read",
		EndpointURL: "https://user:pass@0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/?sig=secret#frag",
		PublicURL:   "https://media.example.test/assets?token=secret",
		S3: storage.R2S3Compatibility{
			Enabled:          true,
			PathStyle:        true,
			VirtualHostStyle: true,
			PresignedURLs:    true,
			MultipartUpload:  true,
			Methods:          []string{"put", "GET", "put"},
		},
		ObjectPrefix: "/objects/uploads/",
		AccessKeyID:  " access-key-id ",
		SecretKey:    " secret-key ",
	}

	normalized := descriptor.Normalize()
	if normalized.AccountID != r2AccountID {
		t.Fatalf("Normalize().AccountID = %q, want %s", normalized.AccountID, r2AccountID)
	}
	if normalized.Bucket != "app-uploads" {
		t.Fatalf("Normalize().Bucket = %q, want app-uploads", normalized.Bucket)
	}
	if normalized.AccessMode != storage.R2AccessModePublic {
		t.Fatalf("Normalize().AccessMode = %q, want public", normalized.AccessMode)
	}
	if normalized.EndpointURL != "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/" {
		t.Fatalf("Normalize().EndpointURL = %q, want redacted endpoint shape", normalized.EndpointURL)
	}
	if normalized.PublicURL != "https://media.example.test/assets" {
		t.Fatalf("Normalize().PublicURL = %q, want public URL without query", normalized.PublicURL)
	}
	if !reflect.DeepEqual(normalized.S3.Methods, []string{"GET", "PUT"}) {
		t.Fatalf("Normalize().S3.Methods = %#v, want GET/PUT", normalized.S3.Methods)
	}
	if normalized.ObjectPrefix != "objects/uploads" {
		t.Fatalf("Normalize().ObjectPrefix = %q, want objects/uploads", normalized.ObjectPrefix)
	}
	if err := descriptor.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if !normalized.S3.AllowsMethod("get") {
		t.Fatal("AllowsMethod did not accept normalized GET")
	}
	if normalized.S3.AllowsMethod("patch") {
		t.Fatal("AllowsMethod accepted PATCH")
	}
}

func TestR2NormalizationHelpers(t *testing.T) {
	t.Parallel()

	if got := storage.NormalizeR2AccountID(" ABCDEF0123456789ABCDEF0123456789 "); got != "abcdef0123456789abcdef0123456789" {
		t.Fatalf("NormalizeR2AccountID() = %q, want lowercase account id", got)
	}
	if got := storage.NormalizeR2Bucket(" My-Bucket "); got != "my-bucket" {
		t.Fatalf("NormalizeR2Bucket() = %q, want my-bucket", got)
	}
	if got := storage.NormalizeR2AccessMode("anonymous"); got != storage.R2AccessModePublic {
		t.Fatalf("NormalizeR2AccessMode(anonymous) = %q, want public", got)
	}
	if got := storage.NormalizeR2AccessMode(""); got != storage.R2AccessModePrivate {
		t.Fatalf("NormalizeR2AccessMode(empty) = %q, want private", got)
	}
	if got := storage.BuildR2S3Endpoint(" ABCDEF0123456789ABCDEF0123456789 "); got != "https://abcdef0123456789abcdef0123456789.r2.cloudflarestorage.com" {
		t.Fatalf("BuildR2S3Endpoint() = %q, want account endpoint", got)
	}
}

func TestDeriveR2EndpointPlan(t *testing.T) {
	t.Parallel()

	plan, err := storage.DeriveR2EndpointPlan(r2AccountID, "App-Uploads", "public", "https://cdn.example.test/files?sig=secret")
	if err != nil {
		t.Fatalf("DeriveR2EndpointPlan() error = %v", err)
	}
	if plan.S3Endpoint != "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com" {
		t.Fatalf("S3Endpoint = %q, want account endpoint", plan.S3Endpoint)
	}
	if plan.BucketHost != "app-uploads.0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com" {
		t.Fatalf("BucketHost = %q, want bucket account host", plan.BucketHost)
	}
	if plan.BucketURL != "https://app-uploads.0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com" {
		t.Fatalf("BucketURL = %q, want bucket URL", plan.BucketURL)
	}
	if plan.PublicURL != "https://cdn.example.test/files" {
		t.Fatalf("PublicURL = %q, want query redacted public URL", plan.PublicURL)
	}

	descriptor := storage.R2PolicyDescriptor{
		AccountID:  r2AccountID,
		Bucket:     "files-app",
		AccessMode: storage.R2AccessModePrivate,
		S3: storage.R2S3Compatibility{
			Enabled:   true,
			PathStyle: true,
		},
	}
	planned, err := descriptor.EndpointPlan()
	if err != nil {
		t.Fatalf("EndpointPlan() error = %v", err)
	}
	if planned.S3Endpoint != storage.BuildR2S3Endpoint(r2AccountID) {
		t.Fatalf("EndpointPlan().S3Endpoint = %q, want default endpoint", planned.S3Endpoint)
	}
}

func TestPlanR2ObjectKey(t *testing.T) {
	t.Parallel()

	plan, err := storage.PlanR2ObjectKey("/tenant-a/avatar.png", "/objects/", " users ")
	if err != nil {
		t.Fatalf("PlanR2ObjectKey() error = %v", err)
	}
	if plan.Prefix != "objects/users/" {
		t.Fatalf("Prefix = %q, want objects/users/", plan.Prefix)
	}
	if plan.Key != "tenant-a/avatar.png" {
		t.Fatalf("Key = %q, want tenant-a/avatar.png", plan.Key)
	}
	if plan.Full != "objects/users/tenant-a/avatar.png" {
		t.Fatalf("Full = %q, want objects/users/tenant-a/avatar.png", plan.Full)
	}

	descriptor := storage.R2PolicyDescriptor{
		AccountID:    r2AccountID,
		Bucket:       "files-app",
		AccessMode:   storage.R2AccessModePrivate,
		ObjectPrefix: "objects",
		S3: storage.R2S3Compatibility{
			Enabled:   true,
			PathStyle: true,
		},
	}
	planned, err := descriptor.PlanObjectKey("reports/2026.csv", "tenants", "tenant-a")
	if err != nil {
		t.Fatalf("PlanObjectKey() error = %v", err)
	}
	if planned.Full != "objects/tenants/tenant-a/reports/2026.csv" {
		t.Fatalf("PlanObjectKey().Full = %q, want objects/tenants/tenant-a/reports/2026.csv", planned.Full)
	}
}

func TestR2PolicyRedactedSummary(t *testing.T) {
	t.Parallel()

	descriptor := storage.R2PolicyDescriptor{
		AccountID:   r2AccountID,
		Bucket:      "files-app",
		AccessMode:  storage.R2AccessModePublic,
		EndpointURL: "https://user:pass@0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/exports?sig=secret#frag",
		PublicURL:   "https://cdn.example.test/files?sig=secret",
		S3: storage.R2S3Compatibility{
			Enabled:          true,
			PathStyle:        true,
			VirtualHostStyle: true,
			Methods:          []string{"GET"},
		},
		ObjectPrefix: "private",
		AccessKeyID:  "key-id",
		SecretKey:    "secret",
	}

	summary := descriptor.RedactedSummary()
	if summary.AccountIDRedacted != "0123...cdef" {
		t.Fatalf("AccountIDRedacted = %q, want partial account redaction", summary.AccountIDRedacted)
	}
	if summary.EndpointURL != "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/exports" {
		t.Fatalf("EndpointURL = %q, want host/path URL", summary.EndpointURL)
	}
	if summary.PublicURL != "https://cdn.example.test/files" {
		t.Fatalf("PublicURL = %q, want host/path URL", summary.PublicURL)
	}
	if summary.AccessKeyID != "[redacted]" || summary.SecretKey != "[redacted]" {
		t.Fatalf("credential redaction = %q/%q, want [redacted]", summary.AccessKeyID, summary.SecretKey)
	}
	if !summary.HasAccessKeyID || !summary.HasSecretKey {
		t.Fatal("summary did not preserve credential presence flags")
	}
	if !reflect.DeepEqual(summary.S3.Methods, []string{"GET"}) {
		t.Fatalf("summary methods = %#v, want GET", summary.S3.Methods)
	}

	summary.S3.Methods[0] = "PUT"
	if got := descriptor.RedactedSummary().S3.Methods[0]; got != "GET" {
		t.Fatalf("RedactedSummary returned shared methods slice, got %q", got)
	}
}

func TestR2ValidationRejectsInvalidDescriptors(t *testing.T) {
	t.Parallel()

	validS3 := storage.R2S3Compatibility{
		Enabled:   true,
		PathStyle: true,
	}
	cases := []struct {
		name       string
		descriptor storage.R2PolicyDescriptor
	}{
		{
			name: "invalid account id",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:  "not-an-account",
				Bucket:     "files-app",
				AccessMode: storage.R2AccessModePrivate,
				S3:         validS3,
			},
		},
		{
			name: "invalid bucket",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:  r2AccountID,
				Bucket:     "files_app",
				AccessMode: storage.R2AccessModePrivate,
				S3:         validS3,
			},
		},
		{
			name: "unknown access mode",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:  r2AccountID,
				Bucket:     "files-app",
				AccessMode: "signed",
				S3:         validS3,
			},
		},
		{
			name: "private with public url",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:  r2AccountID,
				Bucket:     "files-app",
				AccessMode: storage.R2AccessModePrivate,
				PublicURL:  "https://cdn.example.test/files",
				S3:         validS3,
			},
		},
		{
			name: "http endpoint",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:   r2AccountID,
				Bucket:      "files-app",
				AccessMode:  storage.R2AccessModePrivate,
				EndpointURL: "http://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com",
				S3:          validS3,
			},
		},
		{
			name: "disabled s3 with metadata",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:  r2AccountID,
				Bucket:     "files-app",
				AccessMode: storage.R2AccessModePrivate,
				S3: storage.R2S3Compatibility{
					PathStyle: true,
				},
			},
		},
		{
			name: "s3 without addressing style",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:  r2AccountID,
				Bucket:     "files-app",
				AccessMode: storage.R2AccessModePrivate,
				S3: storage.R2S3Compatibility{
					Enabled: true,
				},
			},
		},
		{
			name: "secret without access key",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:  r2AccountID,
				Bucket:     "files-app",
				AccessMode: storage.R2AccessModePrivate,
				S3:         validS3,
				SecretKey:  "secret",
			},
		},
		{
			name: "invalid object prefix",
			descriptor: storage.R2PolicyDescriptor{
				AccountID:    r2AccountID,
				Bucket:       "files-app",
				AccessMode:   storage.R2AccessModePrivate,
				S3:           validS3,
				ObjectPrefix: "objects/../secret",
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateR2PolicyDescriptor(tc.descriptor)
			if !errors.Is(err, storage.ErrR2PolicyInvalid) {
				t.Fatalf("ValidateR2PolicyDescriptor() error = %v, want ErrR2PolicyInvalid", err)
			}
		})
	}
}

func TestR2ObjectKeyPlanningRejectsInvalidKeys(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name     string
		key      string
		prefixes []string
	}{
		{name: "empty key", key: ""},
		{name: "parent segment", key: "objects/../secret"},
		{name: "backslash", key: `objects\secret`},
		{name: "invalid prefix", key: "file.txt", prefixes: []string{"objects/."}},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			_, err := storage.PlanR2ObjectKey(tc.key, tc.prefixes...)
			if !errors.Is(err, storage.ErrR2PolicyInvalid) {
				t.Fatalf("PlanR2ObjectKey() error = %v, want ErrR2PolicyInvalid", err)
			}
		})
	}
}
