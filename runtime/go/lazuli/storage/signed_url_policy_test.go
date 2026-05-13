package storage_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestSignedURLPolicyValidatesRequest(t *testing.T) {
	t.Parallel()

	policy := storage.SignedURLUploadPolicy(5*time.Minute, storage.ImageAny())
	request := storage.SignedURLRequest{
		Method:      storage.SignedURLMethodPUT,
		TTL:         2 * time.Minute,
		ContentType: "image/png;charset=utf-8",
	}
	if err := policy.ValidateRequest(request); err != nil {
		t.Fatalf("ValidateRequest(valid) error = %v", err)
	}
	if !policy.AllowsMethod("put") {
		t.Fatal("AllowsMethod did not normalize lowercase method")
	}
	if !policy.AllowsContentType("image/webp") {
		t.Fatal("AllowsContentType did not match image wildcard")
	}

	request.Method = storage.SignedURLMethodGET
	err := policy.ValidateRequest(request)
	if !errors.Is(err, storage.ErrSignedURLPolicyInvalid) {
		t.Fatalf("ValidateRequest(disallowed method) error = %v, want ErrSignedURLPolicyInvalid", err)
	}

	request.Method = storage.SignedURLMethodPUT
	request.TTL = 6 * time.Minute
	err = policy.ValidateRequest(request)
	if !errors.Is(err, storage.ErrSignedURLPolicyInvalid) {
		t.Fatalf("ValidateRequest(ttl over max) error = %v, want ErrSignedURLPolicyInvalid", err)
	}

	request.TTL = time.Minute
	request.ContentType = "application/pdf"
	err = policy.ValidateRequest(request)
	if !errors.Is(err, storage.ErrFileMimeRejected) {
		t.Fatalf("ValidateRequest(disallowed content type) error = %v, want ErrFileMimeRejected", err)
	}
}

func TestValidateSignedURLPolicyRejectsInvalidShape(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name   string
		policy storage.SignedURLPolicy
	}{
		{
			name: "negative max age",
			policy: storage.SignedURLPolicy{
				MaxAge: -time.Second,
			},
		},
		{
			name: "invalid method token",
			policy: storage.SignedURLPolicy{
				Methods: []string{"GE T"},
			},
		},
		{
			name: "duplicate method",
			policy: storage.SignedURLPolicy{
				Methods: []string{"get", storage.SignedURLMethodGET},
			},
		},
		{
			name: "invalid content type constraint",
			policy: storage.SignedURLPolicy{
				ContentTypes: []storage.MimeType{{Family: "image", Subtype: ""}},
			},
		},
		{
			name: "invalid response content type",
			policy: storage.SignedURLPolicy{
				ResponseHeaders: storage.SignedURLResponseHeaders{ContentType: "not a media type"},
			},
		},
		{
			name: "invalid response header value",
			policy: storage.SignedURLPolicy{
				ResponseHeaders: storage.SignedURLResponseHeaders{CacheControl: "public\r\nx: y"},
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateSignedURLPolicy(tc.policy)
			if !errors.Is(err, storage.ErrSignedURLPolicyInvalid) {
				t.Fatalf("ValidateSignedURLPolicy() error = %v, want ErrSignedURLPolicyInvalid", err)
			}
		})
	}
}

func TestValidateSignedURLTTL(t *testing.T) {
	t.Parallel()

	if err := storage.ValidateSignedURLTTL(time.Minute, 0); err != nil {
		t.Fatalf("ValidateSignedURLTTL(no max age) error = %v", err)
	}
	if err := storage.ValidateSignedURLTTL(time.Minute, time.Minute); err != nil {
		t.Fatalf("ValidateSignedURLTTL(equal max age) error = %v", err)
	}

	cases := []struct {
		name   string
		ttl    time.Duration
		maxAge time.Duration
	}{
		{name: "zero ttl", ttl: 0, maxAge: time.Minute},
		{name: "negative max age", ttl: time.Minute, maxAge: -time.Second},
		{name: "over max age", ttl: 2 * time.Minute, maxAge: time.Minute},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateSignedURLTTL(tc.ttl, tc.maxAge)
			if !errors.Is(err, storage.ErrSignedURLPolicyInvalid) {
				t.Fatalf("ValidateSignedURLTTL() error = %v, want ErrSignedURLPolicyInvalid", err)
			}
		})
	}
}

func TestSignedURLResponseHeadersValues(t *testing.T) {
	t.Parallel()

	expires := time.Date(2026, 5, 12, 12, 30, 0, 0, time.FixedZone("BRT", -3*60*60))
	headers := storage.SignedURLResponseHeaders{
		CacheControl:       "private, max-age=60",
		ContentDisposition: `attachment; filename="report.csv"`,
		ContentEncoding:    "gzip",
		ContentLanguage:    "en-US",
		ContentType:        "text/csv",
		Expires:            expires,
	}
	if headers.Empty() {
		t.Fatal("Empty() = true, want false")
	}
	if err := headers.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	values := headers.Values()
	if values["Cache-Control"] != "private, max-age=60" {
		t.Fatalf("Cache-Control = %q, want private max-age", values["Cache-Control"])
	}
	if values["Content-Disposition"] != `attachment; filename="report.csv"` {
		t.Fatalf("Content-Disposition = %q, want attachment filename", values["Content-Disposition"])
	}
	if values["Content-Encoding"] != "gzip" {
		t.Fatalf("Content-Encoding = %q, want gzip", values["Content-Encoding"])
	}
	if values["Content-Language"] != "en-US" {
		t.Fatalf("Content-Language = %q, want en-US", values["Content-Language"])
	}
	if values["Content-Type"] != "text/csv" {
		t.Fatalf("Content-Type = %q, want text/csv", values["Content-Type"])
	}
	if want := "Tue, 12 May 2026 15:30:00 GMT"; values["Expires"] != want {
		t.Fatalf("Expires = %q, want %q", values["Expires"], want)
	}

	values["Cache-Control"] = "mutated"
	if got := headers.Values()["Cache-Control"]; got != "private, max-age=60" {
		t.Fatalf("Values returned shared map, got Cache-Control %q", got)
	}
}

func TestSignedURLPolicyHelpersCopyInputs(t *testing.T) {
	t.Parallel()

	accept := []storage.MimeType{storage.ImageAny()}
	policy := storage.SignedURLUploadPolicy(time.Minute, accept...)
	accept[0] = storage.App("pdf")

	if !policy.AllowsContentType("image/png") {
		t.Fatal("SignedURLUploadPolicy did not copy content type constraints")
	}
	if policy.AllowsContentType("application/pdf") {
		t.Fatal("SignedURLUploadPolicy content type constraints changed after caller mutation")
	}
}

func TestSignedURLPolicyResponseHeaderValues(t *testing.T) {
	t.Parallel()

	policy := storage.SignedURLDownloadPolicy(time.Minute, storage.TextMime("csv")).
		WithResponseHeaders(storage.SignedURLResponseHeaders{ContentType: "text/csv"})

	if err := policy.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if got := policy.ResponseHeaderValues()["Content-Type"]; got != "text/csv" {
		t.Fatalf("ResponseHeaderValues()[Content-Type] = %q, want text/csv", got)
	}
}
