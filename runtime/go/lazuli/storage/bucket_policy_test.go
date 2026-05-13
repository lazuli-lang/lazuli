package storage_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestBucketPolicyBuilders(t *testing.T) {
	t.Parallel()

	public := storage.PublicBucket("assets")
	if public.Name != "assets" {
		t.Fatalf("public Name = %q, want assets", public.Name)
	}
	if public.Visibility != storage.VisibilityPublic {
		t.Fatalf("public Visibility = %s, want public", public.Visibility)
	}
	if public.SignedTTL != 0 {
		t.Fatalf("public SignedTTL = %s, want zero", public.SignedTTL)
	}
	if err := public.Validate(); err != nil {
		t.Fatalf("public Validate() error = %v", err)
	}

	private := storage.PrivateBucket("imports")
	if private.Visibility != storage.VisibilityPrivate {
		t.Fatalf("private Visibility = %s, want private", private.Visibility)
	}
	if err := storage.ValidateBucketPolicy(private); err != nil {
		t.Fatalf("private ValidateBucketPolicy() error = %v", err)
	}

	signed := storage.SignedBucket("exports", 15*time.Minute)
	if signed.Visibility != storage.VisibilitySigned {
		t.Fatalf("signed Visibility = %s, want signed", signed.Visibility)
	}
	if signed.SignedTTL != 15*time.Minute {
		t.Fatalf("signed SignedTTL = %s, want 15m", signed.SignedTTL)
	}
	if err := signed.Validate(); err != nil {
		t.Fatalf("signed Validate() error = %v", err)
	}
}

func TestBucketPolicyTenantPrefixAndLabels(t *testing.T) {
	t.Parallel()

	policy := storage.SignedBucket(
		"files",
		10*time.Minute,
		storage.BucketPrefix("/objects/"),
		storage.TenantPrefix("tenants"),
		storage.AllowedMimeClasses(storage.MimeClassImage, storage.MimeClassText),
		storage.LifecycleLabels(" Hot ", "archive_30d"),
	)

	if err := policy.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	prefix, err := policy.ObjectPrefix("tenant-a")
	if err != nil {
		t.Fatalf("ObjectPrefix() error = %v", err)
	}
	if prefix != "objects/tenants/tenant-a/" {
		t.Fatalf("ObjectPrefix() = %q, want objects/tenants/tenant-a/", prefix)
	}

	if got := policy.Namespace.Normalize().Prefix; got != "objects" {
		t.Fatalf("normalized prefix = %q, want objects", got)
	}
	if got := policy.LifecycleLabels[0].String(); got != "hot" {
		t.Fatalf("LifecycleLabels[0] = %q, want hot", got)
	}
}

func TestBucketPolicyAllowedMimeClasses(t *testing.T) {
	t.Parallel()

	policy := storage.PrivateBucket(
		"media",
		storage.AllowedMimeClasses(storage.MimeClassImage, storage.MimeClassVideo),
	)

	if !policy.AllowsContentType("image/png; charset=utf-8") {
		t.Fatal("image/png was rejected")
	}
	if !policy.AllowsMimeType(storage.MimeType{Family: "video", Subtype: "mp4"}) {
		t.Fatal("video/mp4 was rejected")
	}
	if policy.AllowsContentType("application/pdf") {
		t.Fatal("application/pdf was accepted")
	}

	unrestricted := storage.PublicBucket("static")
	if !unrestricted.AllowsContentType("application/octet-stream") {
		t.Fatal("unrestricted bucket rejected application/octet-stream")
	}
}

func TestBucketPolicyObjectPrefixRequiresTenant(t *testing.T) {
	t.Parallel()

	policy := storage.PrivateBucket("files", storage.TenantPrefix("tenants"))
	if _, err := policy.ObjectPrefix(""); !errors.Is(err, storage.ErrBucketPolicyInvalid) {
		t.Fatalf("empty tenant error = %v, want ErrBucketPolicyInvalid", err)
	}
	if _, err := policy.ObjectPrefix("tenant/a"); !errors.Is(err, storage.ErrBucketPolicyInvalid) {
		t.Fatalf("invalid tenant error = %v, want ErrBucketPolicyInvalid", err)
	}

	global := storage.PrivateBucket("files", storage.BucketPrefix("objects"))
	prefix, err := global.ObjectPrefix("")
	if err != nil {
		t.Fatalf("global ObjectPrefix() error = %v", err)
	}
	if prefix != "objects/" {
		t.Fatalf("global ObjectPrefix() = %q, want objects/", prefix)
	}
}

func TestBucketPolicyValidationRejectsInvalidPolicies(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name   string
		policy storage.BucketPolicy
	}{
		{
			name:   "empty name",
			policy: storage.PrivateBucket(""),
		},
		{
			name: "unknown visibility",
			policy: storage.BucketPolicy{
				Name:       "files",
				Visibility: storage.FileVisibility(99),
			},
		},
		{
			name:   "signed without ttl",
			policy: storage.SignedBucket("files", 0),
		},
		{
			name: "private with ttl",
			policy: storage.BucketPolicy{
				Name:       "files",
				Visibility: storage.VisibilityPrivate,
				SignedTTL:  time.Minute,
			},
		},
		{
			name:   "invalid namespace prefix",
			policy: storage.PrivateBucket("files", storage.BucketPrefix("../objects")),
		},
		{
			name:   "tenant scoped without prefix",
			policy: storage.BucketPolicy{Name: "files", Visibility: storage.VisibilityPrivate, Namespace: storage.BucketNamespace{TenantScoped: true}},
		},
		{
			name:   "unknown mime class",
			policy: storage.PrivateBucket("files", storage.AllowedMimeClasses(storage.MimeClass("spreadsheet"))),
		},
		{
			name:   "duplicate mime class",
			policy: storage.PrivateBucket("files", storage.AllowedMimeClasses(storage.MimeClassImage, storage.MimeClass(" image "))),
		},
		{
			name:   "invalid lifecycle label",
			policy: storage.PrivateBucket("files", storage.LifecycleLabels("delete after 30d")),
		},
		{
			name:   "duplicate lifecycle label",
			policy: storage.PrivateBucket("files", storage.LifecycleLabels("hot", " Hot ")),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateBucketPolicy(tc.policy)
			if !errors.Is(err, storage.ErrBucketPolicyInvalid) {
				t.Fatalf("ValidateBucketPolicy() error = %v, want ErrBucketPolicyInvalid", err)
			}
		})
	}
}

func TestMimeClassHelpers(t *testing.T) {
	t.Parallel()

	if got := storage.MimeClassImage.String(); got != "image" {
		t.Fatalf("MimeClassImage.String() = %q, want image", got)
	}
	if got := storage.MimeClass("spreadsheet").String(); got != "unknown" {
		t.Fatalf("unknown MimeClass.String() = %q, want unknown", got)
	}
	if !storage.MimeClassText.Matches(storage.MimeType{Family: "text", Subtype: "plain"}) {
		t.Fatal("MimeClassText did not match text/plain")
	}
	if storage.MimeClassAudio.Matches(storage.MimeType{Family: "image", Subtype: "png"}) {
		t.Fatal("MimeClassAudio matched image/png")
	}
}
