package email

import (
	"errors"
	"strings"
	"testing"
)

func TestResendProviderDescriptorIsStable(t *testing.T) {
	t.Parallel()

	descriptor := ResendProviderDescriptor()
	if descriptor.Name != ResendProviderName || descriptor.DisplayName != ResendProviderDisplayName {
		t.Fatalf("descriptor identity = %+v", descriptor)
	}
	if descriptor.DefaultBaseURL != ResendDefaultBaseURL {
		t.Fatalf("DefaultBaseURL = %q, want %q", descriptor.DefaultBaseURL, ResendDefaultBaseURL)
	}
	if descriptor.APIKeyEnv != ResendAPIKeyEnv {
		t.Fatalf("APIKeyEnv = %q, want %q", descriptor.APIKeyEnv, ResendAPIKeyEnv)
	}
	if descriptor.IdempotencyHeader != ResendIdempotencyHeader {
		t.Fatalf("IdempotencyHeader = %q, want %q", descriptor.IdempotencyHeader, ResendIdempotencyHeader)
	}
}

func TestNormalizeResendConfigAppliesDefaultsAndTrims(t *testing.T) {
	t.Parallel()

	config, err := NormalizeResendConfig(ResendConfig{
		APIKey: " re_key_123 ",
		Sender: " Acme <noreply@example.com> ",
	})
	if err != nil {
		t.Fatalf("NormalizeResendConfig() error = %v", err)
	}
	if config.APIKey != "re_key_123" {
		t.Fatalf("APIKey was not trimmed: %q", config.APIKey)
	}
	if config.APIBaseURL != ResendDefaultBaseURL {
		t.Fatalf("APIBaseURL = %q, want %q", config.APIBaseURL, ResendDefaultBaseURL)
	}
	if config.Sender != `"Acme" <noreply@example.com>` {
		t.Fatalf("Sender = %q, want parsed mailbox", config.Sender)
	}
}

func TestValidateResendConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config ResendConfig
		want   error
	}{
		{
			name: "api key",
			config: ResendConfig{
				APIKey: "bad key",
				Sender: "noreply@example.com",
			},
			want: ErrResendAPIKeyInvalid,
		},
		{
			name: "base url",
			config: ResendConfig{
				APIBaseURL: "ftp://api.resend.com",
				Sender:     "noreply@example.com",
			},
			want: ErrResendBaseURLInvalid,
		},
		{
			name: "sender",
			config: ResendConfig{
				Sender: "not an address",
			},
			want: ErrResendSenderInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := tt.config.Validate()
			if !errors.Is(err, ErrResendConfigInvalid) {
				t.Fatalf("Validate() error = %v, want ErrResendConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestResendBaseURLAndSenderHelpers(t *testing.T) {
	t.Parallel()

	apiKey, err := NormalizeResendAPIKey(" re_key_123 ")
	if err != nil {
		t.Fatalf("NormalizeResendAPIKey() error = %v", err)
	}
	if apiKey != "re_key_123" {
		t.Fatalf("NormalizeResendAPIKey() = %q", apiKey)
	}
	if err := ValidateResendAPIKey("bad key"); !errors.Is(err, ErrResendAPIKeyInvalid) {
		t.Fatalf("ValidateResendAPIKey(invalid) error = %v, want ErrResendAPIKeyInvalid", err)
	}

	baseURL, err := NormalizeResendBaseURL(" HTTPS://API.Resend.COM/emails/ ")
	if err != nil {
		t.Fatalf("NormalizeResendBaseURL(custom) error = %v", err)
	}
	if baseURL != "https://api.resend.com/emails" {
		t.Fatalf("NormalizeResendBaseURL(custom) = %q", baseURL)
	}
	if err := ValidateResendBaseURL("https://user:pass@api.resend.com"); !errors.Is(err, ErrResendBaseURLInvalid) {
		t.Fatalf("ValidateResendBaseURL(credentials) error = %v, want ErrResendBaseURLInvalid", err)
	}

	sender, err := NormalizeResendSender(`"Ops Team" <ops@example.com>`)
	if err != nil {
		t.Fatalf("NormalizeResendSender() error = %v", err)
	}
	if sender != `"Ops Team" <ops@example.com>` {
		t.Fatalf("NormalizeResendSender() = %q", sender)
	}
	if err := ValidateResendSender("ops@@example.com"); !errors.Is(err, ErrResendSenderInvalid) {
		t.Fatalf("ValidateResendSender(invalid) error = %v, want ErrResendSenderInvalid", err)
	}
}

func TestPlanResendMetadataNormalizesAudienceTagsAndIdempotency(t *testing.T) {
	t.Parallel()

	plan, err := PlanResendMetadata(ResendMetadataInput{
		AudienceID: " audience_123 ",
		Tags: []ResendTag{
			{Name: " lifecycle ", Value: " welcome "},
			{Name: "tenant", Value: "acme"},
			{Name: "lifecycle", Value: "welcome"},
		},
		IdempotencyKey: " tenant:welcome:123 ",
	})
	if err != nil {
		t.Fatalf("PlanResendMetadata() error = %v", err)
	}
	if plan.AudienceID != "audience_123" {
		t.Fatalf("AudienceID = %q", plan.AudienceID)
	}
	if got := resendTagPairs(plan.Tags); got != "lifecycle=welcome,tenant=acme" {
		t.Fatalf("Tags = %q, want normalized unique sorted tags", got)
	}
	if plan.IdempotencyKey != "tenant:welcome:123" {
		t.Fatalf("IdempotencyKey = %q", plan.IdempotencyKey)
	}

	fields := plan.RequestFields()
	if fields["audience_id"][0] != "audience_123" {
		t.Fatalf("audience_id field = %+v", fields["audience_id"])
	}
	if fields[ResendIdempotencyHeader][0] != "tenant:welcome:123" {
		t.Fatalf("idempotency field = %+v", fields[ResendIdempotencyHeader])
	}
	if fields["tags.lifecycle"][0] != "welcome" || fields["tags.tenant"][0] != "acme" {
		t.Fatalf("tag fields = %+v", fields)
	}
}

func TestPlanResendMetadataRejectsInvalidValues(t *testing.T) {
	t.Parallel()

	_, err := PlanResendMetadata(ResendMetadataInput{
		AudienceID: "bad audience",
		Tags: []ResendTag{
			{Name: strings.Repeat("x", ResendMaxTagNameLength+1), Value: "value"},
			{Name: "valid", Value: "bad\nvalue"},
		},
		IdempotencyKey: "key\nvalue",
	})
	if !errors.Is(err, ErrResendAudienceInvalid) {
		t.Fatalf("PlanResendMetadata() error = %v, want ErrResendAudienceInvalid", err)
	}
	if !errors.Is(err, ErrResendTagInvalid) {
		t.Fatalf("PlanResendMetadata() error = %v, want ErrResendTagInvalid", err)
	}
	if !errors.Is(err, ErrResendIdempotencyKeyInvalid) {
		t.Fatalf("PlanResendMetadata() error = %v, want ErrResendIdempotencyKeyInvalid", err)
	}
}

func TestResendIdempotencyMetadataValidationAndHashing(t *testing.T) {
	t.Parallel()

	meta, err := PlanResendIdempotencyMetadata(" message:123 ")
	if err != nil {
		t.Fatalf("PlanResendIdempotencyMetadata() error = %v", err)
	}
	if meta.Provider != ResendProviderName || meta.Header != ResendIdempotencyHeader || meta.Key != "message:123" {
		t.Fatalf("metadata = %+v", meta)
	}

	if _, err := NormalizeResendIdempotencyKey(" "); !errors.Is(err, ErrResendIdempotencyKeyMissing) {
		t.Fatalf("NormalizeResendIdempotencyKey(empty) error = %v, want ErrResendIdempotencyKeyMissing", err)
	}

	longKey := strings.Repeat("x", ResendMaxIdempotencyKeyLength+100)
	hashed, err := NormalizeResendIdempotencyKey(longKey)
	if err != nil {
		t.Fatalf("NormalizeResendIdempotencyKey(long) error = %v", err)
	}
	if len(hashed) > ResendMaxIdempotencyKeyLength {
		t.Fatalf("hashed key length = %d, want <= %d", len(hashed), ResendMaxIdempotencyKeyLength)
	}
	if !strings.HasPrefix(hashed, resendHashedIdempotencyKeyPrefix) {
		t.Fatalf("hashed key = %q, want sha256 prefix", hashed)
	}
	hashedAgain, err := NormalizeResendIdempotencyKey(longKey)
	if err != nil {
		t.Fatalf("NormalizeResendIdempotencyKey(long again) error = %v", err)
	}
	if hashedAgain != hashed {
		t.Fatalf("hashed key changed: %q != %q", hashedAgain, hashed)
	}
}

func TestResendRedactedSummary(t *testing.T) {
	t.Parallel()

	summary := ResendConfig{
		APIBaseURL: "https://user:pass@api.resend.com",
		APIKey:     "re_key_123",
		Sender:     "Mail <mail@example.com>",
	}.RedactedSummary()

	if summary.Provider != ResendProviderName {
		t.Fatalf("Provider = %q", summary.Provider)
	}
	if summary.APIKey != "redacted" {
		t.Fatalf("APIKey = %q, want redacted", summary.APIKey)
	}
	if strings.Contains(summary.APIBaseURL, "pass") || !strings.Contains(summary.APIBaseURL, "redacted@") {
		t.Fatalf("APIBaseURL was not redacted: %q", summary.APIBaseURL)
	}
}

func resendTagPairs(tags []ResendTag) string {
	pairs := make([]string, 0, len(tags))
	for _, tag := range tags {
		pairs = append(pairs, tag.Name+"="+tag.Value)
	}
	return strings.Join(pairs, ",")
}
