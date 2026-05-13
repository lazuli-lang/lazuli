package payments

import (
	"errors"
	"strings"
	"testing"
)

func TestPayPalCredentialsNormalizeValidateAndRedact(t *testing.T) {
	t.Parallel()

	credentials := NormalizePayPalCredentials(PayPalCredentials{
		ClientID:     " client-id-placeholder ",
		ClientSecret: " secret-placeholder ",
		Environment:  " production ",
	})
	if credentials.ClientID != "client-id-placeholder" ||
		credentials.ClientSecret != "secret-placeholder" ||
		credentials.Environment != PayPalEnvironmentLive ||
		credentials.BaseURL != PayPalLiveBaseURL {
		t.Fatalf("NormalizePayPalCredentials() = %#v, want trimmed live defaults", credentials)
	}
	if err := ValidatePayPalCredentials(credentials); err != nil {
		t.Fatalf("ValidatePayPalCredentials(valid) error = %v", err)
	}
	if got := RedactPayPalClientID(credentials.ClientID); !strings.HasPrefix(got, "clie...") || !strings.HasSuffix(got, "lder") {
		t.Fatalf("RedactPayPalClientID() = %q, want prefix and suffix retained", got)
	}
	if got := RedactPayPalClientSecret(credentials.ClientSecret); got != "[redacted]" {
		t.Fatalf("RedactPayPalClientSecret() = %q, want [redacted]", got)
	}
	if got := RedactPayPalBaseURL("https://user:pass@api.example.test/v1?token=value#frag"); got != "https://api.example.test/v1" {
		t.Fatalf("RedactPayPalBaseURL() = %q, want safe URL", got)
	}

	tests := []struct {
		name        string
		credentials PayPalCredentials
	}{
		{name: "missing client id", credentials: PayPalCredentials{ClientSecret: "secret-placeholder"}},
		{name: "missing client secret", credentials: PayPalCredentials{ClientID: "client-id-placeholder"}},
		{name: "bad environment", credentials: PayPalCredentials{ClientID: "client-id-placeholder", ClientSecret: "secret-placeholder", Environment: "stage"}},
		{name: "bad base url", credentials: PayPalCredentials{ClientID: "client-id-placeholder", ClientSecret: "secret-placeholder", BaseURL: "http://api.example.test?token=value"}},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if err := ValidatePayPalCredentials(tt.credentials); !errors.Is(err, ErrInvalidPayPalDescriptor) {
				t.Fatalf("ValidatePayPalCredentials() error = %v, want ErrInvalidPayPalDescriptor", err)
			}
		})
	}
}

func TestPayPalIntentAndCaptureDescriptors(t *testing.T) {
	t.Parallel()

	intent := NormalizePayPalIntentDescriptor(PayPalIntentDescriptor{
		CaptureMode:     " manual ",
		TransactionID:   " tx-1 ",
		PaymentIntentID: " pi-1 ",
		OrderID:         " order-1 ",
	})
	if intent.CaptureMode != CaptureModeManual || intent.PayPalIntent() != "AUTHORIZE" || intent.TransactionID != "tx-1" {
		t.Fatalf("NormalizePayPalIntentDescriptor() = %#v, want manual AUTHORIZE", intent)
	}
	if err := ValidatePayPalIntentDescriptor(intent); err != nil {
		t.Fatalf("ValidatePayPalIntentDescriptor(valid) error = %v", err)
	}
	automatic := NormalizePayPalIntentDescriptor(PayPalIntentDescriptor{TransactionID: "tx-1"})
	if automatic.CaptureMode != CaptureModeAutomatic || automatic.PayPalIntent() != "CAPTURE" {
		t.Fatalf("automatic intent = %#v / %q, want automatic CAPTURE", automatic, automatic.PayPalIntent())
	}
	if err := ValidatePayPalIntentDescriptor(PayPalIntentDescriptor{}); !errors.Is(err, ErrInvalidPayPalDescriptor) {
		t.Fatalf("ValidatePayPalIntentDescriptor(empty) error = %v, want ErrInvalidPayPalDescriptor", err)
	}
	if err := ValidatePayPalIntentDescriptor(PayPalIntentDescriptor{CaptureMode: "later", TransactionID: "tx-1"}); !errors.Is(err, ErrInvalidPayPalDescriptor) {
		t.Fatalf("ValidatePayPalIntentDescriptor(bad mode) error = %v, want ErrInvalidPayPalDescriptor", err)
	}

	capture := NormalizePayPalCaptureDescriptor(PayPalCaptureDescriptor{
		PaymentID:       " pay-1 ",
		OrderID:         " order-1 ",
		AuthorizationID: " auth-1 ",
		CaptureID:       " cap-1 ",
		FinalCapture:    true,
	})
	if capture.PaymentID != "pay-1" || capture.OrderID != "order-1" || !capture.FinalCapture {
		t.Fatalf("NormalizePayPalCaptureDescriptor() = %#v, want trimmed identifiers", capture)
	}
	if err := ValidatePayPalCaptureDescriptor(capture); err != nil {
		t.Fatalf("ValidatePayPalCaptureDescriptor(valid) error = %v", err)
	}
	if err := ValidatePayPalCaptureDescriptor(PayPalCaptureDescriptor{AuthorizationID: "auth-1"}); !errors.Is(err, ErrInvalidPayPalDescriptor) {
		t.Fatalf("ValidatePayPalCaptureDescriptor(auth only) error = %v, want ErrInvalidPayPalDescriptor", err)
	}
}

func TestPayPalWebhookAndIdempotencyDescriptors(t *testing.T) {
	t.Parallel()

	webhook := NormalizePayPalWebhookDescriptor(PayPalWebhookDescriptor{
		WebhookID:      " wh-1 ",
		TransmissionID: " transmission-1 ",
		EventID:        " event-1 ",
		EventType:      WebhookEventPaymentUpdated,
	})
	if webhook.WebhookID != "wh-1" || webhook.TransmissionID != "transmission-1" || webhook.EventID != "event-1" {
		t.Fatalf("NormalizePayPalWebhookDescriptor() = %#v, want trimmed identifiers", webhook)
	}
	if err := ValidatePayPalWebhookDescriptor(webhook); err != nil {
		t.Fatalf("ValidatePayPalWebhookDescriptor(valid) error = %v", err)
	}
	if err := ValidatePayPalWebhookDescriptor(PayPalWebhookDescriptor{WebhookID: "wh-1"}); !errors.Is(err, ErrInvalidPayPalDescriptor) {
		t.Fatalf("ValidatePayPalWebhookDescriptor(missing delivery id) error = %v, want ErrInvalidPayPalDescriptor", err)
	}

	idempotency := NormalizePayPalIdempotencyDescriptor(PayPalIdempotencyDescriptor{
		Operation:     OperationCapture,
		RequestID:     " request-1 ",
		TransactionID: " tx-1 ",
		Subject:       " pay-1 ",
	})
	if idempotency.RequestID != "request-1" || idempotency.TransactionID != "tx-1" || idempotency.Subject != "pay-1" {
		t.Fatalf("NormalizePayPalIdempotencyDescriptor() = %#v, want trimmed identifiers", idempotency)
	}
	if err := ValidatePayPalIdempotencyDescriptor(idempotency); err != nil {
		t.Fatalf("ValidatePayPalIdempotencyDescriptor(valid) error = %v", err)
	}
	if got := idempotency.PayPalIdempotencyKey().String(); got != "payments:capture:provider=paypal:transaction=tx-1:subject=pay-1" {
		t.Fatalf("PayPalIdempotencyKey() = %q, want provider-scoped key", got)
	}
	if err := ValidatePayPalIdempotencyDescriptor(PayPalIdempotencyDescriptor{Operation: OperationCapture}); !errors.Is(err, ErrInvalidPayPalDescriptor) {
		t.Fatalf("ValidatePayPalIdempotencyDescriptor(missing request) error = %v, want ErrInvalidPayPalDescriptor", err)
	}
	if err := ValidatePayPalIdempotencyDescriptor(PayPalIdempotencyDescriptor{Operation: "void", RequestID: "request-1"}); !errors.Is(err, ErrInvalidPayPalDescriptor) {
		t.Fatalf("ValidatePayPalIdempotencyDescriptor(bad operation) error = %v, want ErrInvalidPayPalDescriptor", err)
	}
}

func TestPlanPayPalDescriptorSafeSummary(t *testing.T) {
	t.Parallel()

	plan, err := PlanPayPalDescriptor(
		PayPalCredentials{
			ClientID:     " client-id-placeholder ",
			ClientSecret: " secret-placeholder ",
			Environment:  PayPalEnvironmentSandbox,
			BaseURL:      "https://api-m.sandbox.paypal.com/",
		},
		PayPalIntentDescriptor{
			CaptureMode:     CaptureModeManual,
			TransactionID:   " tx-1 ",
			PaymentIntentID: " pi-1 ",
			OrderID:         " order-1 ",
		},
		PayPalCaptureDescriptor{
			PaymentID:       " pay-1 ",
			AuthorizationID: " auth-1 ",
			CaptureID:       " cap-1 ",
			FinalCapture:    true,
		},
		PayPalWebhookDescriptor{
			WebhookID:      " wh-1 ",
			TransmissionID: " transmission-1 ",
			EventID:        " event-1 ",
			EventType:      WebhookEventPaymentUpdated,
		},
		PayPalIdempotencyDescriptor{
			Operation:     OperationCapture,
			RequestID:     " request-1 ",
			TransactionID: " tx-1 ",
			Subject:       " pay-1 ",
		},
	)
	if err != nil {
		t.Fatalf("PlanPayPalDescriptor() error = %v", err)
	}

	if plan.Credentials.ClientID != "client-id-placeholder" || plan.Intent.TransactionID != "tx-1" || plan.Capture.PaymentID != "pay-1" {
		t.Fatalf("PlanPayPalDescriptor() = %#v, want normalized descriptors", plan)
	}
	summary := plan.Summary
	if summary.Provider != "paypal" || summary.Environment != PayPalEnvironmentSandbox || summary.BaseURL != PayPalSandboxBaseURL {
		t.Fatalf("summary provider/env/base = %#v, want paypal sandbox base URL", summary)
	}
	if summary.ClientSecret != "[redacted]" || strings.Contains(summary.ClientID, "placeholder") || strings.Contains(summary.IdempotencyRequestID, "request-1") {
		t.Fatalf("summary leaked sensitive values: %#v", summary)
	}
	if summary.Intent != "AUTHORIZE" || summary.OrderID != "order-1" || summary.PaymentID != "pay-1" || summary.CaptureID != "cap-1" {
		t.Fatalf("summary payment metadata = %#v, want intent/capture identifiers", summary)
	}
	if summary.WebhookID != "wh-1" || summary.TransmissionID != "transmission-1" || summary.EventID != "event-1" {
		t.Fatalf("summary webhook metadata = %#v, want identifiers", summary)
	}
	if summary.IdempotencyKey != "payments:capture:provider=paypal:transaction=tx-1:subject=pay-1" {
		t.Fatalf("summary IdempotencyKey = %q, want provider-scoped key", summary.IdempotencyKey)
	}
}

func TestSummarizePayPalDescriptorEmptyIdempotency(t *testing.T) {
	t.Parallel()

	summary := SummarizePayPalDescriptor(PayPalDescriptorPlan{
		Credentials: PayPalCredentials{
			ClientID:     "client-id-placeholder",
			ClientSecret: "secret-placeholder",
		},
		Intent: PayPalIntentDescriptor{TransactionID: "tx-1"},
	})
	if summary.IdempotencyKey != "" {
		t.Fatalf("summary IdempotencyKey = %q, want empty", summary.IdempotencyKey)
	}
}
