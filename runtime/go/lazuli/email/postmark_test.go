package email

import (
	"errors"
	"strings"
	"testing"
)

func TestPostmarkProviderDescriptorIsStable(t *testing.T) {
	t.Parallel()

	descriptor := PostmarkProviderDescriptor()
	if descriptor.Name != PostmarkProviderName || descriptor.DisplayName != PostmarkProviderDisplayName {
		t.Fatalf("descriptor identity = %+v", descriptor)
	}
	if descriptor.DefaultBaseURL != PostmarkDefaultBaseURL {
		t.Fatalf("DefaultBaseURL = %q, want %q", descriptor.DefaultBaseURL, PostmarkDefaultBaseURL)
	}
	if descriptor.DefaultMessageStream != PostmarkDefaultMessageStream {
		t.Fatalf("DefaultMessageStream = %q, want %q", descriptor.DefaultMessageStream, PostmarkDefaultMessageStream)
	}
	if descriptor.ServerTokenHeader != PostmarkServerTokenHeader {
		t.Fatalf("ServerTokenHeader = %q, want %q", descriptor.ServerTokenHeader, PostmarkServerTokenHeader)
	}
	if descriptor.IdempotencyMetadataKey != PostmarkIdempotencyMetadataKey {
		t.Fatalf("IdempotencyMetadataKey = %q, want %q", descriptor.IdempotencyMetadataKey, PostmarkIdempotencyMetadataKey)
	}
}

func TestNormalizePostmarkConfigAppliesDefaultsAndTrims(t *testing.T) {
	t.Parallel()

	config, err := NormalizePostmarkConfig(PostmarkConfig{
		ServerToken: " token-123 ",
		Sender:      " Acme <noreply@example.com> ",
	})
	if err != nil {
		t.Fatalf("NormalizePostmarkConfig() error = %v", err)
	}
	if config.ServerToken != "token-123" {
		t.Fatalf("ServerToken was not trimmed: %q", config.ServerToken)
	}
	if config.MessageStream != PostmarkDefaultMessageStream {
		t.Fatalf("MessageStream = %q, want default outbound", config.MessageStream)
	}
	if config.APIBaseURL != PostmarkDefaultBaseURL {
		t.Fatalf("APIBaseURL = %q, want %q", config.APIBaseURL, PostmarkDefaultBaseURL)
	}
	if config.Sender != `"Acme" <noreply@example.com>` {
		t.Fatalf("Sender = %q, want parsed mailbox", config.Sender)
	}
}

func TestValidatePostmarkConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config PostmarkConfig
		want   error
	}{
		{
			name: "server token",
			config: PostmarkConfig{
				ServerToken: "bad token",
				Sender:      "noreply@example.com",
			},
			want: ErrPostmarkServerTokenInvalid,
		},
		{
			name: "message stream",
			config: PostmarkConfig{
				ServerToken:   "token-123",
				MessageStream: "broadcast stream",
				Sender:        "noreply@example.com",
			},
			want: ErrPostmarkMessageStreamInvalid,
		},
		{
			name: "base url",
			config: PostmarkConfig{
				ServerToken: "token-123",
				APIBaseURL:  "ftp://api.postmarkapp.com",
				Sender:      "noreply@example.com",
			},
			want: ErrPostmarkBaseURLInvalid,
		},
		{
			name: "sender",
			config: PostmarkConfig{
				ServerToken: "token-123",
				Sender:      "not an address",
			},
			want: ErrPostmarkSenderInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := tt.config.Validate()
			if !errors.Is(err, ErrPostmarkConfigInvalid) {
				t.Fatalf("Validate() error = %v, want ErrPostmarkConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestPostmarkBaseURLStreamAndSenderHelpers(t *testing.T) {
	t.Parallel()

	stream, err := NormalizePostmarkMessageStream(" Broadcasts_1 ")
	if err != nil {
		t.Fatalf("NormalizePostmarkMessageStream() error = %v", err)
	}
	if stream != "Broadcasts_1" {
		t.Fatalf("NormalizePostmarkMessageStream() = %q", stream)
	}
	if err := ValidatePostmarkMessageStream("bad.stream"); !errors.Is(err, ErrPostmarkMessageStreamInvalid) {
		t.Fatalf("ValidatePostmarkMessageStream() error = %v, want ErrPostmarkMessageStreamInvalid", err)
	}

	baseURL, err := NormalizePostmarkBaseURL(" HTTPS://API.PostmarkApp.COM/email/ ")
	if err != nil {
		t.Fatalf("NormalizePostmarkBaseURL(custom) error = %v", err)
	}
	if baseURL != "https://api.postmarkapp.com/email" {
		t.Fatalf("NormalizePostmarkBaseURL(custom) = %q", baseURL)
	}
	if err := ValidatePostmarkBaseURL("https://user:pass@api.postmarkapp.com"); !errors.Is(err, ErrPostmarkBaseURLInvalid) {
		t.Fatalf("ValidatePostmarkBaseURL(credentials) error = %v, want ErrPostmarkBaseURLInvalid", err)
	}

	sender, err := NormalizePostmarkSender(`"Ops Team" <ops@example.com>`)
	if err != nil {
		t.Fatalf("NormalizePostmarkSender() error = %v", err)
	}
	if sender != `"Ops Team" <ops@example.com>` {
		t.Fatalf("NormalizePostmarkSender() = %q", sender)
	}
	if err := ValidatePostmarkSender("ops@@example.com"); !errors.Is(err, ErrPostmarkSenderInvalid) {
		t.Fatalf("ValidatePostmarkSender(invalid) error = %v, want ErrPostmarkSenderInvalid", err)
	}
}

func TestPlanPostmarkMetadataNormalizesTagMetadataAndIdempotency(t *testing.T) {
	t.Parallel()

	plan, err := PlanPostmarkMetadata(PostmarkMetadataInput{
		Tag: " welcome ",
		Metadata: map[string]string{
			"tenant":   " lazuli ",
			"campaign": " spring ",
		},
		IdempotencyKey: " tenant:welcome:123 ",
	})
	if err != nil {
		t.Fatalf("PlanPostmarkMetadata() error = %v", err)
	}
	if plan.Tag != "welcome" {
		t.Fatalf("Tag = %q, want trimmed tag", plan.Tag)
	}
	if plan.IdempotencyKey != "tenant:welcome:123" {
		t.Fatalf("IdempotencyKey = %q", plan.IdempotencyKey)
	}
	if plan.Metadata["tenant"] != "lazuli" || plan.Metadata["campaign"] != "spring" {
		t.Fatalf("Metadata = %+v, want trimmed values", plan.Metadata)
	}
	if plan.Metadata[PostmarkIdempotencyMetadataKey] != "tenant:welcome:123" {
		t.Fatalf("idempotency metadata = %q", plan.Metadata[PostmarkIdempotencyMetadataKey])
	}

	fields := plan.RequestFields()
	if fields["Tag"] != "welcome" {
		t.Fatalf("Tag field = %+v", fields["Tag"])
	}
	metadata, ok := fields["Metadata"].(map[string]string)
	if !ok {
		t.Fatalf("Metadata field type = %T, want map[string]string", fields["Metadata"])
	}
	if metadata["campaign"] != "spring" || metadata[PostmarkIdempotencyMetadataKey] != "tenant:welcome:123" {
		t.Fatalf("Metadata field = %+v", metadata)
	}
}

func TestPlanPostmarkMetadataRejectsInvalidValues(t *testing.T) {
	t.Parallel()

	_, err := PlanPostmarkMetadata(PostmarkMetadataInput{
		Tag: strings.Repeat("x", PostmarkMaxTagLength+1),
		Metadata: map[string]string{
			"bad.key": "value",
		},
		IdempotencyKey: "key\nvalue",
	})
	if !errors.Is(err, ErrPostmarkTagInvalid) {
		t.Fatalf("PlanPostmarkMetadata() error = %v, want ErrPostmarkTagInvalid", err)
	}
	if !errors.Is(err, ErrPostmarkMetadataInvalid) {
		t.Fatalf("PlanPostmarkMetadata() error = %v, want ErrPostmarkMetadataInvalid", err)
	}
	if !errors.Is(err, ErrPostmarkIdempotencyKeyInvalid) {
		t.Fatalf("PlanPostmarkMetadata() error = %v, want ErrPostmarkIdempotencyKeyInvalid", err)
	}
}

func TestPostmarkIdempotencyMetadataValidationAndHashing(t *testing.T) {
	t.Parallel()

	meta, err := PlanPostmarkIdempotencyMetadata(" message:123 ")
	if err != nil {
		t.Fatalf("PlanPostmarkIdempotencyMetadata() error = %v", err)
	}
	if meta.Provider != PostmarkProviderName || meta.MetadataKey != PostmarkIdempotencyMetadataKey || meta.Key != "message:123" {
		t.Fatalf("metadata = %+v", meta)
	}

	if _, err := NormalizePostmarkIdempotencyKey(" "); !errors.Is(err, ErrPostmarkIdempotencyKeyMissing) {
		t.Fatalf("NormalizePostmarkIdempotencyKey(empty) error = %v, want ErrPostmarkIdempotencyKeyMissing", err)
	}

	longKey := strings.Repeat("x", PostmarkMaxIdempotencyKeyLength+100)
	hashed, err := NormalizePostmarkIdempotencyKey(longKey)
	if err != nil {
		t.Fatalf("NormalizePostmarkIdempotencyKey(long) error = %v", err)
	}
	if len(hashed) > PostmarkMaxIdempotencyKeyLength {
		t.Fatalf("hashed key length = %d, want <= %d", len(hashed), PostmarkMaxIdempotencyKeyLength)
	}
	if !strings.HasPrefix(hashed, postmarkHashedIdempotencyKeyPrefix) {
		t.Fatalf("hashed key = %q, want sha256 prefix", hashed)
	}
	hashedAgain, err := NormalizePostmarkIdempotencyKey(longKey)
	if err != nil {
		t.Fatalf("NormalizePostmarkIdempotencyKey(long again) error = %v", err)
	}
	if hashedAgain != hashed {
		t.Fatalf("hashed key changed: %q != %q", hashedAgain, hashed)
	}
}

func TestPostmarkRedactedSummary(t *testing.T) {
	t.Parallel()

	summary := PostmarkConfig{
		ServerToken:   "token-123",
		MessageStream: "outbound",
		APIBaseURL:    "https://user:pass@api.postmarkapp.com",
		Sender:        "Mail <mail@example.com>",
	}.RedactedSummary()

	if summary.Provider != PostmarkProviderName {
		t.Fatalf("Provider = %q", summary.Provider)
	}
	if summary.ServerToken != "redacted" {
		t.Fatalf("ServerToken = %q, want redacted", summary.ServerToken)
	}
	if strings.Contains(summary.APIBaseURL, "pass") || !strings.Contains(summary.APIBaseURL, "redacted@") {
		t.Fatalf("APIBaseURL was not redacted: %q", summary.APIBaseURL)
	}
	if summary.Sender != `"Mail" <***@example.com>` {
		t.Fatalf("Sender = %q, want local part redacted", summary.Sender)
	}
}
