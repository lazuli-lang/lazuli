package payments_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/payments"
)

func TestNormalizeWebhookSignatureHeaderName(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		header  string
		want    string
		wantErr bool
	}{
		{name: "canonicalizes mixed case", header: " x-provider-signature ", want: "X-Provider-Signature"},
		{name: "keeps token punctuation", header: "X.Provider_Signature", want: "X.provider_signature"},
		{name: "empty", header: " ", wantErr: true},
		{name: "space", header: "X Provider Signature", wantErr: true},
		{name: "unicode", header: "X-Signature-Ç", wantErr: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, err := payments.NormalizeWebhookSignatureHeaderName(tt.header)
			if tt.wantErr {
				if !errors.Is(err, payments.ErrWebhookSignatureHeaderInvalid) {
					t.Fatalf("NormalizeWebhookSignatureHeaderName() error = %v, want header invalid", err)
				}
				return
			}
			if err != nil {
				t.Fatalf("NormalizeWebhookSignatureHeaderName() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeWebhookSignatureHeaderName() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestWebhookSignatureAlgorithmMetadata(t *testing.T) {
	t.Parallel()

	algorithm, err := payments.NormalizeWebhookSignatureAlgorithm(" HMAC_SHA256 ")
	if err != nil {
		t.Fatalf("NormalizeWebhookSignatureAlgorithm() error = %v", err)
	}
	if algorithm != payments.WebhookSignatureAlgorithmHMACSHA256 {
		t.Fatalf("algorithm = %q, want hmac-sha256", algorithm)
	}

	metadata, ok := algorithm.Metadata()
	if !ok {
		t.Fatal("Metadata() ok = false")
	}
	if metadata.Family != "hmac" || metadata.Hash != "sha256" || !metadata.Keyed || metadata.Asymmetric {
		t.Fatalf("metadata = %+v, want keyed hmac sha256", metadata)
	}

	metadata, ok = payments.WebhookSignatureAlgorithmEd25519.Metadata()
	if !ok {
		t.Fatal("ed25519 Metadata() ok = false")
	}
	if !metadata.Asymmetric || metadata.Keyed || metadata.Hash != "" {
		t.Fatalf("ed25519 metadata = %+v", metadata)
	}

	if _, err := payments.NormalizeWebhookSignatureAlgorithm("md5"); !errors.Is(err, payments.ErrWebhookSignatureAlgorithmInvalid) {
		t.Fatalf("NormalizeWebhookSignatureAlgorithm(md5) error = %v, want algorithm invalid", err)
	}
}

func TestNormalizeWebhookSignatureTolerance(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		tolerance time.Duration
		want      time.Duration
		wantErr   bool
	}{
		{name: "default", want: payments.DefaultWebhookSignatureTolerance},
		{name: "minimum", tolerance: time.Second, want: time.Second},
		{name: "maximum", tolerance: 24 * time.Hour, want: 24 * time.Hour},
		{name: "too small", tolerance: time.Millisecond, wantErr: true},
		{name: "negative", tolerance: -time.Second, wantErr: true},
		{name: "too large", tolerance: 25 * time.Hour, wantErr: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, err := payments.NormalizeWebhookSignatureTolerance(tt.tolerance)
			if tt.wantErr {
				if !errors.Is(err, payments.ErrWebhookSignatureToleranceInvalid) {
					t.Fatalf("NormalizeWebhookSignatureTolerance() error = %v, want tolerance invalid", err)
				}
				return
			}
			if err != nil {
				t.Fatalf("NormalizeWebhookSignatureTolerance() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeWebhookSignatureTolerance() = %s, want %s", got, tt.want)
			}
		})
	}
}

func TestWebhookSecretAndURLRedaction(t *testing.T) {
	t.Parallel()

	if got := payments.RedactWebhookSecret(" invalid-placeholder-secret "); got != "inva...[redacted]...cret" {
		t.Fatalf("RedactWebhookSecret() = %q", got)
	}
	if got := payments.RedactWebhookSecret("short"); got != "[redacted]" {
		t.Fatalf("RedactWebhookSecret(short) = %q", got)
	}
	if got := payments.RedactWebhookSecret(" "); got != "" {
		t.Fatalf("RedactWebhookSecret(empty) = %q", got)
	}

	got := payments.RedactWebhookURL("https://user:pass@example.test/webhooks/pay?signature=abc&debug=1&api_key=placeholder")
	if strings.Contains(got, "user") || strings.Contains(got, "pass") || strings.Contains(got, "abc") || strings.Contains(got, "placeholder") {
		t.Fatalf("RedactWebhookURL leaked sensitive data: %q", got)
	}
	if got != "https://example.test/webhooks/pay?api_key=%5Bredacted%5D&debug=1&signature=%5Bredacted%5D" {
		t.Fatalf("RedactWebhookURL() = %q", got)
	}
}

func TestPlanWebhookSignatureValidationNormalizesMetadata(t *testing.T) {
	t.Parallel()

	plan, err := payments.PlanWebhookSignatureValidation(payments.WebhookSignatureValidationOptions{
		Provider:        " provider-a ",
		SignatureHeader: " x-provider-signature ",
		TimestampHeader: " x-provider-timestamp ",
		Algorithm:       payments.WebhookSignatureAlgorithmHMACSHA256,
		Secret:          " invalid-placeholder-secret ",
		EndpointURL:     "/webhooks/pay?token=placeholder&mode=test",
		CanonicalPayload: payments.WebhookSignatureCanonicalPayload{
			TimestampSeparator: ".",
			SignedHeaders:      []string{"x-provider-id", "X-Provider-Id", " x-provider-signature "},
		},
		ProviderEventHint: " payment.updated ",
	})
	if err != nil {
		t.Fatalf("PlanWebhookSignatureValidation() error = %v", err)
	}

	if plan.Provider != "provider-a" {
		t.Fatalf("Provider = %q", plan.Provider)
	}
	if plan.SignatureHeader != "X-Provider-Signature" || plan.TimestampHeader != "X-Provider-Timestamp" {
		t.Fatalf("headers = %q/%q", plan.SignatureHeader, plan.TimestampHeader)
	}
	if plan.Tolerance != payments.DefaultWebhookSignatureTolerance {
		t.Fatalf("Tolerance = %s, want default", plan.Tolerance)
	}
	if plan.Secret != "inva...[redacted]...cret" {
		t.Fatalf("Secret = %q, want redacted", plan.Secret)
	}
	if plan.EndpointURL != "/webhooks/pay?mode=test&token=%5Bredacted%5D" {
		t.Fatalf("EndpointURL = %q", plan.EndpointURL)
	}
	if plan.CanonicalPayload.Body != payments.WebhookSignaturePayloadRawBody {
		t.Fatalf("CanonicalPayload.Body = %q", plan.CanonicalPayload.Body)
	}
	if plan.CanonicalPayload.TimestampHeader != "X-Provider-Timestamp" {
		t.Fatalf("CanonicalPayload.TimestampHeader = %q", plan.CanonicalPayload.TimestampHeader)
	}
	wantHeaders := []string{"X-Provider-Id", "X-Provider-Signature"}
	if !reflect.DeepEqual(plan.CanonicalPayload.SignedHeaders, wantHeaders) {
		t.Fatalf("SignedHeaders = %#v, want %#v", plan.CanonicalPayload.SignedHeaders, wantHeaders)
	}
	if plan.AlgorithmMetadata.Family != "hmac" || plan.AlgorithmMetadata.Hash != "sha256" {
		t.Fatalf("AlgorithmMetadata = %+v", plan.AlgorithmMetadata)
	}
	if plan.ProviderEventHint != "payment.updated" {
		t.Fatalf("ProviderEventHint = %q", plan.ProviderEventHint)
	}
}

func TestValidateWebhookSignaturePlanRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		options payments.WebhookSignatureValidationOptions
		want    error
	}{
		{
			name: "missing signature header",
			options: payments.WebhookSignatureValidationOptions{
				Algorithm: payments.WebhookSignatureAlgorithmHMACSHA256,
				Secret:    "invalid-placeholder-secret",
			},
			want: payments.ErrWebhookSignatureHeaderInvalid,
		},
		{
			name: "missing secret",
			options: payments.WebhookSignatureValidationOptions{
				SignatureHeader: "X-Signature",
				Algorithm:       payments.WebhookSignatureAlgorithmHMACSHA256,
			},
			want: payments.ErrWebhookSignatureSecretMissing,
		},
		{
			name: "unsupported algorithm",
			options: payments.WebhookSignatureValidationOptions{
				SignatureHeader: "X-Signature",
				Algorithm:       payments.WebhookSignatureAlgorithm("rsa-sha256"),
				Secret:          "invalid-placeholder-secret",
			},
			want: payments.ErrWebhookSignatureAlgorithmInvalid,
		},
		{
			name: "invalid canonical payload body",
			options: payments.WebhookSignatureValidationOptions{
				SignatureHeader: "X-Signature",
				Algorithm:       payments.WebhookSignatureAlgorithmHMACSHA256,
				Secret:          "invalid-placeholder-secret",
				CanonicalPayload: payments.WebhookSignatureCanonicalPayload{
					Body: payments.WebhookSignaturePayloadBody("json_compacted"),
				},
			},
			want: payments.ErrWebhookSignaturePayloadUnsupported,
		},
		{
			name: "invalid signed header",
			options: payments.WebhookSignatureValidationOptions{
				SignatureHeader: "X-Signature",
				Algorithm:       payments.WebhookSignatureAlgorithmHMACSHA256,
				Secret:          "invalid-placeholder-secret",
				CanonicalPayload: payments.WebhookSignatureCanonicalPayload{
					SignedHeaders: []string{"bad header"},
				},
			},
			want: payments.ErrWebhookSignatureCanonicalPayload,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := payments.ValidateWebhookSignaturePlan(tt.options)
			if !errors.Is(err, payments.ErrWebhookSignaturePlanInvalid) {
				t.Fatalf("ValidateWebhookSignaturePlan() error = %v, want plan invalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidateWebhookSignaturePlan() error = %v, want %v", err, tt.want)
			}
		})
	}
}
