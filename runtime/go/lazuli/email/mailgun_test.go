package email

import (
	"errors"
	"strings"
	"testing"
)

func TestMailgunProviderDescriptorIsStable(t *testing.T) {
	t.Parallel()

	descriptor := MailgunProviderDescriptor()
	if descriptor.Name != MailgunProviderName || descriptor.DisplayName != MailgunProviderDisplayName {
		t.Fatalf("descriptor identity = %+v", descriptor)
	}
	if descriptor.DefaultRegion != MailgunRegionUS {
		t.Fatalf("DefaultRegion = %q, want %q", descriptor.DefaultRegion, MailgunRegionUS)
	}
	if descriptor.DefaultUSBaseURL != MailgunDefaultUSBaseURL || descriptor.DefaultEUBaseURL != MailgunDefaultEUBaseURL {
		t.Fatalf("base URLs = %+v", descriptor)
	}
	if descriptor.IdempotencyHeader != MailgunIdempotencyHeader {
		t.Fatalf("IdempotencyHeader = %q, want %q", descriptor.IdempotencyHeader, MailgunIdempotencyHeader)
	}
}

func TestNormalizeMailgunConfigAppliesDefaultsAndTrims(t *testing.T) {
	t.Parallel()

	config, err := NormalizeMailgunConfig(MailgunConfig{
		Domain: " Mg.Example.COM. ",
		APIKey: " key-123 ",
		Sender: " Acme <noreply@mg.example.com> ",
	})
	if err != nil {
		t.Fatalf("NormalizeMailgunConfig() error = %v", err)
	}
	if config.Domain != "mg.example.com" {
		t.Fatalf("Domain = %q, want normalized domain", config.Domain)
	}
	if config.Region != MailgunRegionUS {
		t.Fatalf("Region = %q, want default US", config.Region)
	}
	if config.APIBaseURL != MailgunDefaultUSBaseURL {
		t.Fatalf("APIBaseURL = %q, want %q", config.APIBaseURL, MailgunDefaultUSBaseURL)
	}
	if config.APIKey != "key-123" {
		t.Fatalf("APIKey was not trimmed: %q", config.APIKey)
	}
	if config.Sender != `"Acme" <noreply@mg.example.com>` {
		t.Fatalf("Sender = %q, want parsed mailbox", config.Sender)
	}
}

func TestValidateMailgunConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config MailgunConfig
		want   error
	}{
		{
			name: "domain",
			config: MailgunConfig{
				Domain: "localhost",
				Sender: "noreply@example.com",
			},
			want: ErrMailgunDomainInvalid,
		},
		{
			name: "region",
			config: MailgunConfig{
				Domain: "example.com",
				Region: "ap",
				Sender: "noreply@example.com",
			},
			want: ErrMailgunRegionInvalid,
		},
		{
			name: "base url",
			config: MailgunConfig{
				Domain:     "example.com",
				APIBaseURL: "ftp://api.mailgun.net",
				Sender:     "noreply@example.com",
			},
			want: ErrMailgunBaseURLInvalid,
		},
		{
			name: "api key",
			config: MailgunConfig{
				Domain: "example.com",
				APIKey: "bad key",
				Sender: "noreply@example.com",
			},
			want: ErrMailgunAPIKeyInvalid,
		},
		{
			name: "sender",
			config: MailgunConfig{
				Domain: "example.com",
				Sender: "not an address",
			},
			want: ErrMailgunSenderInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := tt.config.Validate()
			if !errors.Is(err, ErrMailgunConfigInvalid) {
				t.Fatalf("Validate() error = %v, want ErrMailgunConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestMailgunDomainRegionAndBaseURLHelpers(t *testing.T) {
	t.Parallel()

	domain, err := NormalizeMailgunDomain(" Mail.Example.TEST. ")
	if err != nil {
		t.Fatalf("NormalizeMailgunDomain() error = %v", err)
	}
	if domain != "mail.example.test" {
		t.Fatalf("NormalizeMailgunDomain() = %q", domain)
	}
	if err := ValidateMailgunDomain("-bad.example"); !errors.Is(err, ErrMailgunDomainInvalid) {
		t.Fatalf("ValidateMailgunDomain() error = %v, want ErrMailgunDomainInvalid", err)
	}

	region, err := NormalizeMailgunRegion(" EU ")
	if err != nil {
		t.Fatalf("NormalizeMailgunRegion() error = %v", err)
	}
	if region != MailgunRegionEU {
		t.Fatalf("NormalizeMailgunRegion() = %q, want EU", region)
	}

	baseURL, err := NormalizeMailgunBaseURL("", MailgunRegionEU)
	if err != nil {
		t.Fatalf("NormalizeMailgunBaseURL(default EU) error = %v", err)
	}
	if baseURL != MailgunDefaultEUBaseURL {
		t.Fatalf("NormalizeMailgunBaseURL(default EU) = %q", baseURL)
	}

	baseURL, err = NormalizeMailgunBaseURL(" HTTPS://API.Mailgun.NET/v3/ ", MailgunRegionUS)
	if err != nil {
		t.Fatalf("NormalizeMailgunBaseURL(custom) error = %v", err)
	}
	if baseURL != "https://api.mailgun.net/v3" {
		t.Fatalf("NormalizeMailgunBaseURL(custom) = %q", baseURL)
	}
	if err := ValidateMailgunBaseURL("https://user:pass@api.mailgun.net", MailgunRegionUS); !errors.Is(err, ErrMailgunBaseURLInvalid) {
		t.Fatalf("ValidateMailgunBaseURL(credentials) error = %v, want ErrMailgunBaseURLInvalid", err)
	}
}

func TestNormalizeMailgunSenderUsesNetMail(t *testing.T) {
	t.Parallel()

	sender, err := NormalizeMailgunSender(`"Ops Team" <ops@example.com>`)
	if err != nil {
		t.Fatalf("NormalizeMailgunSender() error = %v", err)
	}
	if sender != `"Ops Team" <ops@example.com>` {
		t.Fatalf("NormalizeMailgunSender() = %q", sender)
	}
	if err := ValidateMailgunSender("ops@@example.com"); !errors.Is(err, ErrMailgunSenderInvalid) {
		t.Fatalf("ValidateMailgunSender(invalid) error = %v, want ErrMailgunSenderInvalid", err)
	}
}

func TestPlanMailgunMetadataNormalizesTagsHeadersAndIdempotency(t *testing.T) {
	t.Parallel()

	plan, err := PlanMailgunMetadata(MailgunMetadataInput{
		Tags: []string{" welcome ", "trial", "welcome"},
		Headers: map[string]string{
			"X-Campaign": " spring ",
		},
		IdempotencyKey: " tenant:welcome:123 ",
	})
	if err != nil {
		t.Fatalf("PlanMailgunMetadata() error = %v", err)
	}
	if got := strings.Join(plan.Tags, ","); got != "welcome,trial" {
		t.Fatalf("Tags = %q, want normalized unique tags", got)
	}
	if plan.IdempotencyKey != "tenant:welcome:123" {
		t.Fatalf("IdempotencyKey = %q", plan.IdempotencyKey)
	}
	if plan.Headers["X-Campaign"] != "spring" {
		t.Fatalf("X-Campaign header = %q", plan.Headers["X-Campaign"])
	}
	if plan.Headers[MailgunIdempotencyHeader] != "tenant:welcome:123" {
		t.Fatalf("idempotency header = %q", plan.Headers[MailgunIdempotencyHeader])
	}

	fields := plan.RequestFields()
	if got := strings.Join(fields["o:tag"], ","); got != "welcome,trial" {
		t.Fatalf("o:tag fields = %q", got)
	}
	if fields["h:X-Campaign"][0] != "spring" {
		t.Fatalf("h:X-Campaign field = %+v", fields["h:X-Campaign"])
	}
	if fields["h:"+MailgunIdempotencyHeader][0] != "tenant:welcome:123" {
		t.Fatalf("idempotency field = %+v", fields["h:"+MailgunIdempotencyHeader])
	}
}

func TestPlanMailgunMetadataRejectsInvalidValues(t *testing.T) {
	t.Parallel()

	_, err := PlanMailgunMetadata(MailgunMetadataInput{
		Tags: []string{strings.Repeat("x", MailgunMaxTagLength+1)},
		Headers: map[string]string{
			"Bad Header": "value",
		},
		IdempotencyKey: "key\nvalue",
	})
	if !errors.Is(err, ErrMailgunTagInvalid) {
		t.Fatalf("PlanMailgunMetadata() error = %v, want ErrMailgunTagInvalid", err)
	}
	if !errors.Is(err, ErrMailgunHeaderInvalid) {
		t.Fatalf("PlanMailgunMetadata() error = %v, want ErrMailgunHeaderInvalid", err)
	}
	if !errors.Is(err, ErrMailgunIdempotencyKeyInvalid) {
		t.Fatalf("PlanMailgunMetadata() error = %v, want ErrMailgunIdempotencyKeyInvalid", err)
	}
}

func TestMailgunIdempotencyMetadataValidationAndHashing(t *testing.T) {
	t.Parallel()

	meta, err := PlanMailgunIdempotencyMetadata(" message:123 ")
	if err != nil {
		t.Fatalf("PlanMailgunIdempotencyMetadata() error = %v", err)
	}
	if meta.Provider != MailgunProviderName || meta.Header != MailgunIdempotencyHeader || meta.Key != "message:123" {
		t.Fatalf("metadata = %+v", meta)
	}

	if _, err := NormalizeMailgunIdempotencyKey(" "); !errors.Is(err, ErrMailgunIdempotencyKeyMissing) {
		t.Fatalf("NormalizeMailgunIdempotencyKey(empty) error = %v, want ErrMailgunIdempotencyKeyMissing", err)
	}

	longKey := strings.Repeat("x", MailgunMaxIdempotencyKeyLength+100)
	hashed, err := NormalizeMailgunIdempotencyKey(longKey)
	if err != nil {
		t.Fatalf("NormalizeMailgunIdempotencyKey(long) error = %v", err)
	}
	if len(hashed) > MailgunMaxIdempotencyKeyLength {
		t.Fatalf("hashed key length = %d, want <= %d", len(hashed), MailgunMaxIdempotencyKeyLength)
	}
	if !strings.HasPrefix(hashed, mailgunHashedIdempotencyKeyPrefix) {
		t.Fatalf("hashed key = %q, want sha256 prefix", hashed)
	}
	hashedAgain, err := NormalizeMailgunIdempotencyKey(longKey)
	if err != nil {
		t.Fatalf("NormalizeMailgunIdempotencyKey(long again) error = %v", err)
	}
	if hashedAgain != hashed {
		t.Fatalf("hashed key changed: %q != %q", hashedAgain, hashed)
	}
}

func TestMailgunRedactedSummary(t *testing.T) {
	t.Parallel()

	summary := MailgunConfig{
		Domain:     "example.com",
		Region:     MailgunRegionEU,
		APIBaseURL: "https://user:pass@api.eu.mailgun.net",
		APIKey:     "key-123",
		Sender:     "Mail <mail@example.com>",
	}.RedactedSummary()

	if summary.Provider != MailgunProviderName {
		t.Fatalf("Provider = %q", summary.Provider)
	}
	if summary.APIKey != "redacted" {
		t.Fatalf("APIKey = %q, want redacted", summary.APIKey)
	}
	if strings.Contains(summary.APIBaseURL, "pass") || !strings.Contains(summary.APIBaseURL, "redacted@") {
		t.Fatalf("APIBaseURL was not redacted: %q", summary.APIBaseURL)
	}
}
