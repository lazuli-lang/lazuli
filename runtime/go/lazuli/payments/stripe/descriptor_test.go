package stripe_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/payments"
	"lazuli.dev/runtime/lazuli/payments/stripe"
)

func TestDescriptorMetadataIsStable(t *testing.T) {
	t.Parallel()

	descriptor := stripe.Descriptor()
	if descriptor.Name != stripe.ProviderName || descriptor.DisplayName != stripe.ProviderDisplayName {
		t.Fatalf("descriptor identity = %+v", descriptor)
	}
	if descriptor.DefaultBaseURL != stripe.DefaultBaseURL {
		t.Fatalf("DefaultBaseURL = %q, want %q", descriptor.DefaultBaseURL, stripe.DefaultBaseURL)
	}
	if descriptor.IdempotencyHeader != stripe.HeaderIdempotencyKey {
		t.Fatalf("IdempotencyHeader = %q, want %q", descriptor.IdempotencyHeader, stripe.HeaderIdempotencyKey)
	}
	if descriptor.WebhookSignatureHeader != stripe.HeaderSignature {
		t.Fatalf("WebhookSignatureHeader = %q, want %q", descriptor.WebhookSignatureHeader, stripe.HeaderSignature)
	}
	if !descriptor.Supports(stripe.CapabilityCreatePaymentIntent | stripe.CapabilityRefundPayment) {
		t.Fatalf("descriptor capabilities = 0x%x, want create and refund", uint64(descriptor.Capabilities))
	}
	if !descriptor.SupportsWebhookEvent(stripe.WebhookEventPaymentIntentSucceeded) {
		t.Fatalf("descriptor webhook events = %+v, want payment intent succeeded", descriptor.WebhookEvents)
	}

	descriptor.WebhookEvents[0] = "mutated"
	if got := stripe.Descriptor().WebhookEvents[0]; got == "mutated" {
		t.Fatal("Descriptor() returned a mutable shared webhook event slice")
	}
}

func TestWebhookEventNamesMapToPaymentEventTypes(t *testing.T) {
	t.Parallel()

	cases := []struct {
		event stripe.WebhookEventName
		want  payments.WebhookEventType
	}{
		{stripe.WebhookEventPaymentIntentCreated, payments.WebhookEventPaymentCreated},
		{stripe.WebhookEventPaymentIntentSucceeded, payments.WebhookEventPaymentUpdated},
		{stripe.WebhookEventPaymentIntentFailed, payments.WebhookEventPaymentFailed},
		{stripe.WebhookEventCheckoutSessionExpired, payments.WebhookEventPaymentExpired},
		{stripe.WebhookEventRefundCreated, payments.WebhookEventRefundCreated},
		{stripe.WebhookEventRefundFailed, payments.WebhookEventRefundUpdated},
	}

	for _, tc := range cases {
		t.Run(string(tc.event), func(t *testing.T) {
			t.Parallel()

			got, ok := tc.event.NormalizedType()
			if !ok {
				t.Fatalf("%q NormalizedType() ok = false", tc.event)
			}
			if got != tc.want {
				t.Fatalf("%q NormalizedType() = %q, want %q", tc.event, got, tc.want)
			}
			if !tc.event.Valid() {
				t.Fatalf("%q Valid() = false", tc.event)
			}
		})
	}

	if stripe.WebhookEventName("payment_intent.processing").Valid() {
		t.Fatal("unknown event should not be valid")
	}
}

func TestCapabilityFlags(t *testing.T) {
	t.Parallel()

	capabilities := stripe.CapabilityCreatePaymentIntent | stripe.CapabilityRefundPayment
	if !capabilities.Has(stripe.CapabilityCreatePaymentIntent) {
		t.Fatal("capabilities should include create payment intent")
	}
	if !capabilities.HasAny(stripe.CapabilityCapturePayment | stripe.CapabilityRefundPayment) {
		t.Fatal("capabilities should include one candidate")
	}
	if capabilities.Has(stripe.CapabilityCapturePayment) {
		t.Fatal("capabilities should not include capture")
	}
	if capabilities.Has(0) || capabilities.HasAny(0) {
		t.Fatal("zero capability checks should be false")
	}
	if err := stripe.DefaultCapabilities.Validate(); err != nil {
		t.Fatalf("DefaultCapabilities.Validate() error = %v", err)
	}
	if err := stripe.Capability(1 << 40).Validate(); !errors.Is(err, stripe.ErrCapabilityUnsupported) {
		t.Fatalf("unsupported capability error = %v, want ErrCapabilityUnsupported", err)
	}
}

func TestNormalizeConfigAppliesDefaultsAndTrims(t *testing.T) {
	t.Parallel()

	config, err := stripe.NormalizeConfig(stripe.Config{
		SecretKey:       " sk_test_123 ",
		WebhookSecret:   " whsec_123 ",
		WebhookEndpoint: " /webhooks/stripe ",
	})
	if err != nil {
		t.Fatalf("NormalizeConfig() error = %v", err)
	}
	if config.SecretKey != "sk_test_123" || config.WebhookSecret != "whsec_123" {
		t.Fatalf("secrets were not trimmed: %+v", config)
	}
	if config.BaseURL != stripe.DefaultBaseURL {
		t.Fatalf("BaseURL = %q, want default", config.BaseURL)
	}
	if config.WebhookEndpoint != "/webhooks/stripe" {
		t.Fatalf("WebhookEndpoint = %q, want /webhooks/stripe", config.WebhookEndpoint)
	}
	if config.Capabilities != stripe.DefaultCapabilities {
		t.Fatalf("Capabilities = 0x%x, want default 0x%x", uint64(config.Capabilities), uint64(stripe.DefaultCapabilities))
	}
}

func TestValidateConfigAllowsScopedCapabilities(t *testing.T) {
	t.Parallel()

	if err := stripe.ValidateConfig(stripe.Config{
		SecretKey:    "sk_test_123",
		Capabilities: stripe.CapabilityCreatePaymentIntent,
	}); err != nil {
		t.Fatalf("ValidateConfig(api-only) error = %v", err)
	}

	if err := stripe.ValidateConfig(stripe.Config{
		WebhookSecret: "whsec_123",
		Capabilities:  stripe.CapabilityParseWebhookEvent,
	}); err != nil {
		t.Fatalf("ValidateConfig(webhook-only) error = %v", err)
	}
}

func TestValidateConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config stripe.Config
		want   error
	}{
		{
			name: "missing secret key",
			config: stripe.Config{
				WebhookSecret: "whsec_123",
			},
			want: stripe.ErrSecretKeyMissing,
		},
		{
			name: "missing webhook secret",
			config: stripe.Config{
				SecretKey: "sk_test_123",
			},
			want: stripe.ErrWebhookSecretMissing,
		},
		{
			name: "secret key whitespace",
			config: stripe.Config{
				SecretKey:     "sk test",
				WebhookSecret: "whsec_123",
			},
			want: stripe.ErrSecretKeyInvalid,
		},
		{
			name: "invalid base url",
			config: stripe.Config{
				SecretKey:     "sk_test_123",
				WebhookSecret: "whsec_123",
				BaseURL:       "ftp://api.stripe.com",
			},
			want: stripe.ErrBaseURLInvalid,
		},
		{
			name: "invalid webhook endpoint",
			config: stripe.Config{
				SecretKey:       "sk_test_123",
				WebhookSecret:   "whsec_123",
				WebhookEndpoint: "webhooks/stripe",
			},
			want: stripe.ErrWebhookEndpointInvalid,
		},
		{
			name: "unsupported capability",
			config: stripe.Config{
				SecretKey:     "sk_test_123",
				WebhookSecret: "whsec_123",
				Capabilities:  stripe.DefaultCapabilities | stripe.Capability(1<<40),
			},
			want: stripe.ErrCapabilityUnsupported,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := stripe.ValidateConfig(tt.config)
			if !errors.Is(err, stripe.ErrConfigInvalid) {
				t.Fatalf("ValidateConfig() error = %v, want ErrConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidateConfig() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestEndpointHelpersValidateAndJoinURLs(t *testing.T) {
	t.Parallel()

	baseURL, err := stripe.NormalizeBaseURL(" https://api.stripe.com/base/ ")
	if err != nil {
		t.Fatalf("NormalizeBaseURL() error = %v", err)
	}
	if baseURL != "https://api.stripe.com/base" {
		t.Fatalf("NormalizeBaseURL() = %q, want base without trailing slash", baseURL)
	}

	endpoint, err := stripe.APIEndpoint(baseURL, "/v1/payment_intents?limit=1")
	if err != nil {
		t.Fatalf("APIEndpoint() error = %v", err)
	}
	if endpoint != "https://api.stripe.com/base/v1/payment_intents?limit=1" {
		t.Fatalf("APIEndpoint() = %q", endpoint)
	}

	if err := stripe.ValidateBaseURL("https://user:pass@api.stripe.com"); !errors.Is(err, stripe.ErrBaseURLInvalid) {
		t.Fatalf("ValidateBaseURL(credentials) error = %v, want ErrBaseURLInvalid", err)
	}
	if _, err := stripe.APIEndpoint(stripe.DefaultBaseURL, "https://api.stripe.com/v1/payment_intents"); !errors.Is(err, stripe.ErrEndpointInvalid) {
		t.Fatalf("APIEndpoint(absolute path) error = %v, want ErrEndpointInvalid", err)
	}

	webhookEndpoint, err := stripe.NormalizeWebhookEndpoint("https://example.test/webhooks/stripe")
	if err != nil {
		t.Fatalf("NormalizeWebhookEndpoint(absolute) error = %v", err)
	}
	if webhookEndpoint != "https://example.test/webhooks/stripe" {
		t.Fatalf("NormalizeWebhookEndpoint() = %q", webhookEndpoint)
	}
	if err := stripe.ValidateWebhookEndpoint("/webhooks/stripe?debug=1"); !errors.Is(err, stripe.ErrWebhookEndpointInvalid) {
		t.Fatalf("ValidateWebhookEndpoint(query) error = %v, want ErrWebhookEndpointInvalid", err)
	}
}

func TestIdempotencyKeyHelpersUseStripeProvider(t *testing.T) {
	t.Parallel()

	got, err := stripe.CreatePaymentIntentIdempotencyKey("tenant:1", "txn:1")
	if err != nil {
		t.Fatalf("CreatePaymentIntentIdempotencyKey() error = %v", err)
	}
	want := `payments:create_intent:provider=stripe:tenant=tenant\:1:transaction=txn\:1:subject=txn\:1`
	if got != want {
		t.Fatalf("CreatePaymentIntentIdempotencyKey() = %q, want %q", got, want)
	}

	got, err = stripe.WebhookIdempotencyKey("evt_123")
	if err != nil {
		t.Fatalf("WebhookIdempotencyKey() error = %v", err)
	}
	if got != "payments:webhook:provider=stripe:subject=evt_123" {
		t.Fatalf("WebhookIdempotencyKey() = %q", got)
	}

	got, err = stripe.FormatIdempotencyKey(payments.CaptureKey("tenant", "txn", "pay_123").WithProvider("other"))
	if err != nil {
		t.Fatalf("FormatIdempotencyKey() error = %v", err)
	}
	if !strings.Contains(got, "provider=stripe") || strings.Contains(got, "provider=other") {
		t.Fatalf("FormatIdempotencyKey() provider scope = %q", got)
	}
}

func TestIdempotencyKeyValidationAndLengthLimit(t *testing.T) {
	t.Parallel()

	if _, err := stripe.CapturePaymentIdempotencyKey("tenant", "txn", " "); !errors.Is(err, stripe.ErrIdempotencyKeyMissing) {
		t.Fatalf("CapturePaymentIdempotencyKey(missing subject) error = %v, want ErrIdempotencyKeyMissing", err)
	}
	if _, err := stripe.NormalizeIdempotencyKey("key\nvalue"); !errors.Is(err, stripe.ErrIdempotencyKeyInvalid) {
		t.Fatalf("NormalizeIdempotencyKey(control) error = %v, want ErrIdempotencyKeyInvalid", err)
	}

	longSubject := strings.Repeat("x", 400)
	got, err := stripe.FormatIdempotencyKey(payments.RefundKey("tenant", "txn", longSubject))
	if err != nil {
		t.Fatalf("FormatIdempotencyKey(long) error = %v", err)
	}
	if len(got) > stripe.MaxIdempotencyKeyLength {
		t.Fatalf("hashed idempotency key length = %d, want <= %d", len(got), stripe.MaxIdempotencyKeyLength)
	}
	if !strings.HasPrefix(got, "payments:stripe:sha256:") {
		t.Fatalf("hashed idempotency key = %q, want sha256 prefix", got)
	}
	gotAgain, err := stripe.FormatIdempotencyKey(payments.RefundKey("tenant", "txn", longSubject))
	if err != nil {
		t.Fatalf("FormatIdempotencyKey(long again) error = %v", err)
	}
	if gotAgain != got {
		t.Fatalf("hashed idempotency key changed: %q != %q", gotAgain, got)
	}
}
