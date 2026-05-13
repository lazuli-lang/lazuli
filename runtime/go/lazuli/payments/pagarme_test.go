package payments_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/payments"
)

func TestPagarMeDescriptorMetadata(t *testing.T) {
	t.Parallel()

	descriptor := payments.PagarMeDescriptor()
	if descriptor.Name != payments.PagarMeProviderName || descriptor.DisplayName != payments.PagarMeProviderDisplayName {
		t.Fatalf("descriptor identity = %+v", descriptor)
	}
	if descriptor.DefaultBaseURL != payments.PagarMeDefaultBaseURL {
		t.Fatalf("DefaultBaseURL = %q, want %q", descriptor.DefaultBaseURL, payments.PagarMeDefaultBaseURL)
	}
	if descriptor.IdempotencyHeader != payments.PagarMeHeaderIdempotencyKey {
		t.Fatalf("IdempotencyHeader = %q, want %q", descriptor.IdempotencyHeader, payments.PagarMeHeaderIdempotencyKey)
	}
	if descriptor.MaxInstallments != payments.PagarMeMaxInstallments {
		t.Fatalf("MaxInstallments = %d, want %d", descriptor.MaxInstallments, payments.PagarMeMaxInstallments)
	}
}

func TestNormalizePagarMeConfig(t *testing.T) {
	t.Parallel()

	config, err := payments.NormalizePagarMeConfig(payments.PagarMeConfig{
		APIKey:      " invalid-placeholder-api-key ",
		AccountID:   " acct-placeholder ",
		Environment: " test ",
		BaseURL:     " HTTPS://api.pagar.me/core/v5/ ",
	})
	if err != nil {
		t.Fatalf("NormalizePagarMeConfig() error = %v", err)
	}
	if config.APIKey != "invalid-placeholder-api-key" {
		t.Fatalf("APIKey was not trimmed: %+v", config)
	}
	if config.AccountID != "acct-placeholder" {
		t.Fatalf("AccountID was not trimmed: %+v", config)
	}
	if config.Environment != payments.PagarMeEnvironmentSandbox {
		t.Fatalf("Environment = %q, want sandbox", config.Environment)
	}
	if config.BaseURL != payments.PagarMeDefaultBaseURL {
		t.Fatalf("BaseURL = %q, want default form", config.BaseURL)
	}
}

func TestNormalizePagarMeConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config payments.PagarMeConfig
		want   error
	}{
		{
			name: "missing api key",
			config: payments.PagarMeConfig{
				AccountID: "acct-placeholder",
			},
			want: payments.ErrPagarMeAPIKeyMissing,
		},
		{
			name: "api key whitespace",
			config: payments.PagarMeConfig{
				APIKey: "invalid placeholder",
			},
			want: payments.ErrPagarMeAPIKeyInvalid,
		},
		{
			name: "account whitespace",
			config: payments.PagarMeConfig{
				APIKey:    "invalid-placeholder-api-key",
				AccountID: "acct placeholder",
			},
			want: payments.ErrPagarMeAccountInvalid,
		},
		{
			name: "bad environment",
			config: payments.PagarMeConfig{
				APIKey:      "invalid-placeholder-api-key",
				Environment: "staging",
			},
			want: payments.ErrPagarMeEnvironmentInvalid,
		},
		{
			name: "bad base url",
			config: payments.PagarMeConfig{
				APIKey:  "invalid-placeholder-api-key",
				BaseURL: "ftp://api.pagar.me",
			},
			want: payments.ErrPagarMeBaseURLInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := tt.config.Validate()
			if !errors.Is(err, payments.ErrPagarMeConfigInvalid) {
				t.Fatalf("Validate() error = %v, want ErrPagarMeConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestPagarMeEnvironmentAndBaseURLNormalization(t *testing.T) {
	t.Parallel()

	cases := []struct {
		input string
		want  payments.PagarMeEnvironment
	}{
		{"", payments.PagarMeEnvironmentProduction},
		{" prod ", payments.PagarMeEnvironmentProduction},
		{"live", payments.PagarMeEnvironmentProduction},
		{"test", payments.PagarMeEnvironmentSandbox},
		{"sandbox", payments.PagarMeEnvironmentSandbox},
		{"custom", payments.PagarMeEnvironmentCustom},
	}
	for _, tc := range cases {
		t.Run(tc.input, func(t *testing.T) {
			t.Parallel()

			got, err := payments.NormalizePagarMeEnvironment(tc.input)
			if err != nil {
				t.Fatalf("NormalizePagarMeEnvironment(%q) error = %v", tc.input, err)
			}
			if got != tc.want {
				t.Fatalf("NormalizePagarMeEnvironment(%q) = %q, want %q", tc.input, got, tc.want)
			}
		})
	}

	baseURL, err := payments.NormalizePagarMeBaseURL(" https://example.test/pagarme/ ")
	if err != nil {
		t.Fatalf("NormalizePagarMeBaseURL() error = %v", err)
	}
	if baseURL != "https://example.test/pagarme" {
		t.Fatalf("NormalizePagarMeBaseURL() = %q", baseURL)
	}
	if err := payments.ValidatePagarMeBaseURL("https://user:pass@example.test"); !errors.Is(err, payments.ErrPagarMeBaseURLInvalid) {
		t.Fatalf("ValidatePagarMeBaseURL(credentials) error = %v, want ErrPagarMeBaseURLInvalid", err)
	}
}

func TestNormalizePagarMePaymentMetadata(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		in   payments.PagarMePaymentMetadata
		want payments.PagarMePaymentMetadata
	}{
		{
			name: "credit installments",
			in: payments.PagarMePaymentMetadata{
				Method:       payments.PagarMePaymentMethodCreditCard,
				CaptureMode:  payments.CaptureModeManual,
				Installments: 3,
			},
			want: payments.PagarMePaymentMetadata{
				Method:       payments.PagarMePaymentMethodCreditCard,
				CaptureMode:  payments.CaptureModeManual,
				Installments: 3,
			},
		},
		{
			name: "pix defaults",
			in: payments.PagarMePaymentMetadata{
				Method: payments.PagarMePaymentMethodPix,
			},
			want: payments.PagarMePaymentMetadata{
				Method:       payments.PagarMePaymentMethodPix,
				CaptureMode:  payments.CaptureModeAutomatic,
				Installments: 1,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, err := payments.NormalizePagarMePaymentMetadata(tt.in)
			if err != nil {
				t.Fatalf("NormalizePagarMePaymentMetadata() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizePagarMePaymentMetadata() = %+v, want %+v", got, tt.want)
			}
		})
	}
}

func TestNormalizePagarMePaymentMetadataRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []payments.PagarMePaymentMetadata{
		{Method: "cash"},
		{Method: payments.PagarMePaymentMethodCreditCard, CaptureMode: "later"},
		{Method: payments.PagarMePaymentMethodCreditCard, Installments: payments.PagarMeMaxInstallments + 1},
		{Method: payments.PagarMePaymentMethodPix, Installments: 2},
		{Method: payments.PagarMePaymentMethodBoleto, CaptureMode: payments.CaptureModeManual},
	}

	for _, tc := range tests {
		t.Run(string(tc.Method)+"/"+string(tc.CaptureMode), func(t *testing.T) {
			t.Parallel()

			if err := tc.Validate(); !errors.Is(err, payments.ErrPagarMePaymentMetadataInvalid) {
				t.Fatalf("Validate() error = %v, want ErrPagarMePaymentMetadataInvalid", err)
			}
		})
	}
}

func TestPagarMeIdempotencyMetadata(t *testing.T) {
	t.Parallel()

	got, err := payments.PagarMeCreatePaymentIntentIdempotencyKey("tenant:1", "txn:1")
	if err != nil {
		t.Fatalf("PagarMeCreatePaymentIntentIdempotencyKey() error = %v", err)
	}
	want := `payments:create_intent:provider=pagarme:tenant=tenant\:1:transaction=txn\:1:subject=txn\:1`
	if got != want {
		t.Fatalf("PagarMeCreatePaymentIntentIdempotencyKey() = %q, want %q", got, want)
	}

	metadata, err := payments.NormalizePagarMeIdempotencyMetadata(" request-key ")
	if err != nil {
		t.Fatalf("NormalizePagarMeIdempotencyMetadata() error = %v", err)
	}
	if metadata.Header != payments.PagarMeHeaderIdempotencyKey || metadata.Key != "request-key" {
		t.Fatalf("NormalizePagarMeIdempotencyMetadata() = %+v", metadata)
	}

	got, err = payments.FormatPagarMeIdempotencyKey(payments.CaptureKey("tenant", "txn", "pay-placeholder").WithProvider("other"))
	if err != nil {
		t.Fatalf("FormatPagarMeIdempotencyKey() error = %v", err)
	}
	if !strings.Contains(got, "provider=pagarme") || strings.Contains(got, "provider=other") {
		t.Fatalf("FormatPagarMeIdempotencyKey() provider scope = %q", got)
	}
}

func TestPagarMeIdempotencyKeyValidationAndLengthLimit(t *testing.T) {
	t.Parallel()

	if _, err := payments.PagarMeRefundPaymentIdempotencyKey("tenant", "txn", " "); !errors.Is(err, payments.ErrPagarMeIdempotencyKeyMissing) {
		t.Fatalf("PagarMeRefundPaymentIdempotencyKey(missing subject) error = %v, want ErrPagarMeIdempotencyKeyMissing", err)
	}
	if _, err := payments.NormalizePagarMeIdempotencyKey("key\nvalue"); !errors.Is(err, payments.ErrPagarMeIdempotencyKeyInvalid) {
		t.Fatalf("NormalizePagarMeIdempotencyKey(control) error = %v, want ErrPagarMeIdempotencyKeyInvalid", err)
	}

	longSubject := strings.Repeat("x", 400)
	got, err := payments.FormatPagarMeIdempotencyKey(payments.RefundKey("tenant", "txn", longSubject))
	if err != nil {
		t.Fatalf("FormatPagarMeIdempotencyKey(long) error = %v", err)
	}
	if len(got) > payments.PagarMeMaxIdempotencyKeyLength {
		t.Fatalf("hashed idempotency key length = %d, want <= %d", len(got), payments.PagarMeMaxIdempotencyKeyLength)
	}
	if !strings.HasPrefix(got, "payments:pagarme:sha256:") {
		t.Fatalf("hashed idempotency key = %q, want sha256 prefix", got)
	}
	gotAgain, err := payments.FormatPagarMeIdempotencyKey(payments.RefundKey("tenant", "txn", longSubject))
	if err != nil {
		t.Fatalf("FormatPagarMeIdempotencyKey(long again) error = %v", err)
	}
	if gotAgain != got {
		t.Fatalf("hashed idempotency key changed: %q != %q", gotAgain, got)
	}
}

func TestPagarMeRedactionAndSafeSummary(t *testing.T) {
	t.Parallel()

	apiKey := "invalid-placeholder-api-key"
	accountID := "acct-placeholder"
	idempotencyKey := "payments:create_intent:provider=pagarme:tenant=tenant:subject=txn"

	summary, err := payments.BuildPagarMeSafeSummary(
		payments.PagarMeConfig{
			APIKey:      apiKey,
			AccountID:   accountID,
			Environment: payments.PagarMeEnvironmentSandbox,
		},
		payments.PagarMePaymentMetadata{
			Method: payments.PagarMePaymentMethodCreditCard,
		},
		payments.PagarMeIdempotencyMetadata{Key: idempotencyKey},
	)
	if err != nil {
		t.Fatalf("BuildPagarMeSafeSummary() error = %v", err)
	}
	if summary.Provider != payments.PagarMeProviderName {
		t.Fatalf("Provider = %q, want pagarme", summary.Provider)
	}
	if summary.APIKey == apiKey || summary.AccountID == accountID || summary.IdempotencyKey == idempotencyKey {
		t.Fatalf("summary contains unredacted metadata: %+v", summary)
	}
	if !strings.HasPrefix(summary.APIKey, "inva...") {
		t.Fatalf("redacted API key = %q", summary.APIKey)
	}
	if summary.BaseURL != payments.PagarMeDefaultBaseURL {
		t.Fatalf("BaseURL = %q, want safe normalized URL", summary.BaseURL)
	}
	if summary.CaptureMode != payments.CaptureModeAutomatic || summary.Installments != 1 {
		t.Fatalf("payment defaults = %+v", summary)
	}

	if got := payments.RedactPagarMeURL("https://user:pass@example.test/path?secret=1"); got != "redacted" {
		t.Fatalf("RedactPagarMeURL(credentials) = %q, want redacted", got)
	}
}
