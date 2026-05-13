package storage_test

import (
	"errors"
	"reflect"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestGCSPolicyDescriptorNormalizesAndValidates(t *testing.T) {
	t.Parallel()

	descriptor := storage.GCSPolicyDescriptor{
		Bucket:       " App-Uploads ",
		Location:     " southamerica_east1 ",
		StorageClass: "nearline",
		AccessMode:   "public-read",
		SignedURL: storage.GCSSignedURLCapability{
			Enabled:      true,
			Version:      " V4 ",
			MaxAge:       15 * time.Minute,
			Methods:      []string{"put", "GET", "put"},
			CredentialID: " service-account@example.iam.gserviceaccount.com ",
			SignerURL:    "https://signer.example.test/path?token=secret",
		},
		ObjectPrefix: "/objects/uploads/",
	}

	normalized := descriptor.Normalize()
	if normalized.Bucket != "app-uploads" {
		t.Fatalf("Normalize().Bucket = %q, want app-uploads", normalized.Bucket)
	}
	if normalized.Location != "SOUTHAMERICA-EAST1" {
		t.Fatalf("Normalize().Location = %q, want SOUTHAMERICA-EAST1", normalized.Location)
	}
	if normalized.StorageClass != storage.GCSStorageClassNearline {
		t.Fatalf("Normalize().StorageClass = %q, want NEARLINE", normalized.StorageClass)
	}
	if normalized.AccessMode != storage.GCSAccessModePublic {
		t.Fatalf("Normalize().AccessMode = %q, want public", normalized.AccessMode)
	}
	if !reflect.DeepEqual(normalized.SignedURL.Methods, []string{"GET", "PUT"}) {
		t.Fatalf("Normalize().SignedURL.Methods = %#v, want GET/PUT", normalized.SignedURL.Methods)
	}
	if normalized.ObjectPrefix != "objects/uploads" {
		t.Fatalf("Normalize().ObjectPrefix = %q, want objects/uploads", normalized.ObjectPrefix)
	}
	if err := descriptor.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	if !normalized.SignedURL.AllowsMethod("get") {
		t.Fatal("AllowsMethod did not accept normalized GET")
	}
	if normalized.SignedURL.AllowsMethod("delete") {
		t.Fatal("AllowsMethod accepted DELETE")
	}
}

func TestGCSNormalizationHelpers(t *testing.T) {
	t.Parallel()

	if got := storage.NormalizeGCSBucket(" My_Bucket "); got != "my_bucket" {
		t.Fatalf("NormalizeGCSBucket() = %q, want my_bucket", got)
	}
	if got := storage.NormalizeGCSLocation(" us_central1 "); got != "US-CENTRAL1" {
		t.Fatalf("NormalizeGCSLocation() = %q, want US-CENTRAL1", got)
	}
	if got := storage.NormalizeGCSStorageClass(""); got != storage.GCSStorageClassStandard {
		t.Fatalf("NormalizeGCSStorageClass(empty) = %q, want STANDARD", got)
	}
	if got := storage.NormalizeGCSStorageClass("multi-regional"); got != storage.GCSStorageClassStandard {
		t.Fatalf("NormalizeGCSStorageClass(alias) = %q, want STANDARD", got)
	}
	if got := storage.NormalizeGCSAccessMode("uniform"); got != storage.GCSAccessModePrivate {
		t.Fatalf("NormalizeGCSAccessMode(uniform) = %q, want private", got)
	}
}

func TestPlanGCSObjectKey(t *testing.T) {
	t.Parallel()

	plan, err := storage.PlanGCSObjectKey("/tenant-a/avatar.png", "/objects/", " users ")
	if err != nil {
		t.Fatalf("PlanGCSObjectKey() error = %v", err)
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

	descriptor := storage.GCSPolicyDescriptor{
		Bucket:       "files-app",
		Location:     "US",
		StorageClass: storage.GCSStorageClassStandard,
		AccessMode:   storage.GCSAccessModePrivate,
		ObjectPrefix: "objects",
	}
	planned, err := descriptor.PlanObjectKey("reports/2026.csv", "tenants", "tenant-a")
	if err != nil {
		t.Fatalf("PlanObjectKey() error = %v", err)
	}
	if planned.Full != "objects/tenants/tenant-a/reports/2026.csv" {
		t.Fatalf("PlanObjectKey().Full = %q, want objects/tenants/tenant-a/reports/2026.csv", planned.Full)
	}
}

func TestGCSPolicyRedactedSummary(t *testing.T) {
	t.Parallel()

	descriptor := storage.GCSPolicyDescriptor{
		Bucket:       "files-app",
		Location:     "US",
		StorageClass: "coldline",
		AccessMode:   "private",
		SignedURL: storage.GCSSignedURLCapability{
			Enabled:      true,
			Version:      "v4",
			MaxAge:       time.Minute,
			Methods:      []string{"GET"},
			CredentialID: "secret-key-id",
			SignerURL:    "https://signer.example.test/sign?token=secret",
		},
		ObjectPrefix: "/private/",
	}

	summary := descriptor.RedactedSummary()
	if summary.SignedURL.CredentialIDRedacted != "[redacted]" {
		t.Fatalf("CredentialIDRedacted = %q, want [redacted]", summary.SignedURL.CredentialIDRedacted)
	}
	if summary.SignedURL.SignerURLRedacted != "https://signer.example.test/[redacted]" {
		t.Fatalf("SignerURLRedacted = %q, want host-only redaction", summary.SignedURL.SignerURLRedacted)
	}
	if !reflect.DeepEqual(summary.SignedURL.Methods, []string{"GET"}) {
		t.Fatalf("summary methods = %#v, want GET", summary.SignedURL.Methods)
	}

	summary.SignedURL.Methods[0] = "PUT"
	if got := descriptor.RedactedSummary().SignedURL.Methods[0]; got != "GET" {
		t.Fatalf("RedactedSummary returned shared methods slice, got %q", got)
	}
}

func TestGCSValidationRejectsInvalidDescriptors(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name       string
		descriptor storage.GCSPolicyDescriptor
	}{
		{
			name: "invalid bucket",
			descriptor: storage.GCSPolicyDescriptor{
				Bucket:       "goog-files",
				Location:     "US",
				StorageClass: storage.GCSStorageClassStandard,
				AccessMode:   storage.GCSAccessModePrivate,
			},
		},
		{
			name: "missing location",
			descriptor: storage.GCSPolicyDescriptor{
				Bucket:       "files-app",
				StorageClass: storage.GCSStorageClassStandard,
				AccessMode:   storage.GCSAccessModePrivate,
			},
		},
		{
			name: "unknown storage class",
			descriptor: storage.GCSPolicyDescriptor{
				Bucket:       "files-app",
				Location:     "US",
				StorageClass: "GLACIER",
				AccessMode:   storage.GCSAccessModePrivate,
			},
		},
		{
			name: "unknown access mode",
			descriptor: storage.GCSPolicyDescriptor{
				Bucket:       "files-app",
				Location:     "US",
				StorageClass: storage.GCSStorageClassStandard,
				AccessMode:   "signed",
			},
		},
		{
			name: "disabled signed url with metadata",
			descriptor: storage.GCSPolicyDescriptor{
				Bucket:       "files-app",
				Location:     "US",
				StorageClass: storage.GCSStorageClassStandard,
				AccessMode:   storage.GCSAccessModePrivate,
				SignedURL: storage.GCSSignedURLCapability{
					Methods: []string{"GET"},
				},
			},
		},
		{
			name: "enabled signed url without credential",
			descriptor: storage.GCSPolicyDescriptor{
				Bucket:       "files-app",
				Location:     "US",
				StorageClass: storage.GCSStorageClassStandard,
				AccessMode:   storage.GCSAccessModePrivate,
				SignedURL: storage.GCSSignedURLCapability{
					Enabled: true,
					Version: "v4",
					MaxAge:  time.Minute,
					Methods: []string{"GET"},
				},
			},
		},
		{
			name: "invalid object prefix",
			descriptor: storage.GCSPolicyDescriptor{
				Bucket:       "files-app",
				Location:     "US",
				StorageClass: storage.GCSStorageClassStandard,
				AccessMode:   storage.GCSAccessModePrivate,
				ObjectPrefix: "objects/../secret",
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateGCSPolicyDescriptor(tc.descriptor)
			if !errors.Is(err, storage.ErrGCSPolicyInvalid) {
				t.Fatalf("ValidateGCSPolicyDescriptor() error = %v, want ErrGCSPolicyInvalid", err)
			}
		})
	}
}

func TestGCSObjectKeyPlanningRejectsInvalidKeys(t *testing.T) {
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

			_, err := storage.PlanGCSObjectKey(tc.key, tc.prefixes...)
			if !errors.Is(err, storage.ErrGCSPolicyInvalid) {
				t.Fatalf("PlanGCSObjectKey() error = %v, want ErrGCSPolicyInvalid", err)
			}
		})
	}
}
