package notifications_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/notifications"
)

var (
	testTwilioAccountSID = "AC" + strings.Repeat("a", 32)
	testTwilioAuthToken  = "auth-token-" + strings.Repeat("b", 16)
)

func TestTwilioSMSDescriptorMetadataIsStable(t *testing.T) {
	t.Parallel()

	descriptor := notifications.TwilioSMSProviderDescriptor()
	if descriptor.ProviderName != notifications.TwilioProviderName {
		t.Fatalf("ProviderName = %q, want %q", descriptor.ProviderName, notifications.TwilioProviderName)
	}
	if descriptor.ProviderDisplayName != notifications.TwilioProviderDisplayName {
		t.Fatalf("ProviderDisplayName = %q, want %q", descriptor.ProviderDisplayName, notifications.TwilioProviderDisplayName)
	}
	if descriptor.Channel != notifications.ChannelSMS {
		t.Fatalf("Channel = %q, want %q", descriptor.Channel, notifications.ChannelSMS)
	}
	if descriptor.DefaultBaseURL != notifications.DefaultTwilioBaseURL {
		t.Fatalf("DefaultBaseURL = %q, want %q", descriptor.DefaultBaseURL, notifications.DefaultTwilioBaseURL)
	}
}

func TestNormalizeTwilioSMSConfigTrimsDefaultsAndRedacts(t *testing.T) {
	t.Parallel()

	config, err := notifications.NormalizeTwilioSMSConfig(notifications.TwilioSMSConfig{
		AccountSID: " " + testTwilioAccountSID + " ",
		AuthToken:  " " + testTwilioAuthToken + " ",
		FromNumber: " +14155550100 ",
	})
	if err != nil {
		t.Fatalf("NormalizeTwilioSMSConfig() error = %v", err)
	}
	if config.AccountSID != testTwilioAccountSID {
		t.Fatalf("AccountSID = %q, want trimmed", config.AccountSID)
	}
	if config.AuthToken != testTwilioAuthToken {
		t.Fatalf("AuthToken = %q, want trimmed", config.AuthToken)
	}
	if config.FromNumber != "+14155550100" {
		t.Fatalf("FromNumber = %q, want normalized E.164", config.FromNumber)
	}
	if config.BaseURL != notifications.DefaultTwilioBaseURL {
		t.Fatalf("BaseURL = %q, want default", config.BaseURL)
	}

	redacted := config.Redacted()
	if redacted.AccountSID == config.AccountSID || !strings.HasPrefix(redacted.AccountSID, "ACaa...") {
		t.Fatalf("redacted AccountSID = %q", redacted.AccountSID)
	}
	if redacted.AuthToken == config.AuthToken || !strings.HasSuffix(redacted.AuthToken, "bbbb") {
		t.Fatalf("redacted AuthToken = %q", redacted.AuthToken)
	}
}

func TestValidateTwilioSMSConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config notifications.TwilioSMSConfig
		want   error
	}{
		{
			name: "missing account sid",
			config: notifications.TwilioSMSConfig{
				AuthToken:  testTwilioAuthToken,
				FromNumber: "+14155550100",
			},
			want: notifications.ErrTwilioAccountSIDMissing,
		},
		{
			name: "bad account sid",
			config: notifications.TwilioSMSConfig{
				AccountSID: "SK-not-a-real-api-key",
				AuthToken:  testTwilioAuthToken,
				FromNumber: "+14155550100",
			},
			want: notifications.ErrTwilioAccountSIDInvalid,
		},
		{
			name: "missing auth token",
			config: notifications.TwilioSMSConfig{
				AccountSID: testTwilioAccountSID,
				FromNumber: "+14155550100",
			},
			want: notifications.ErrTwilioAuthTokenMissing,
		},
		{
			name: "auth token whitespace",
			config: notifications.TwilioSMSConfig{
				AccountSID: testTwilioAccountSID,
				AuthToken:  "auth token",
				FromNumber: "+14155550100",
			},
			want: notifications.ErrTwilioAuthTokenInvalid,
		},
		{
			name: "missing from number",
			config: notifications.TwilioSMSConfig{
				AccountSID: testTwilioAccountSID,
				AuthToken:  testTwilioAuthToken,
			},
			want: notifications.ErrTwilioFromNumberMissing,
		},
		{
			name: "bad from number",
			config: notifications.TwilioSMSConfig{
				AccountSID: testTwilioAccountSID,
				AuthToken:  testTwilioAuthToken,
				FromNumber: "415-555-0100",
			},
			want: notifications.ErrTwilioPhoneNumberInvalid,
		},
		{
			name: "bad base url",
			config: notifications.TwilioSMSConfig{
				AccountSID: testTwilioAccountSID,
				AuthToken:  testTwilioAuthToken,
				FromNumber: "+14155550100",
				BaseURL:    "ftp://api.twilio.com",
			},
			want: notifications.ErrTwilioBaseURLInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := notifications.ValidateTwilioSMSConfig(tt.config)
			if !errors.Is(err, notifications.ErrTwilioSMSConfigInvalid) {
				t.Fatalf("ValidateTwilioSMSConfig() error = %v, want ErrTwilioSMSConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidateTwilioSMSConfig() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestNormalizeE164PhoneNumber(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		number  string
		want    string
		wantErr bool
	}{
		{name: "valid", number: " +5511999999999 ", want: "+5511999999999"},
		{name: "country code zero", number: "+04155550100", wantErr: true},
		{name: "too long", number: "+1234567890123456", wantErr: true},
		{name: "separator", number: "+1 4155550100", wantErr: true},
		{name: "missing plus", number: "14155550100", wantErr: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, err := notifications.NormalizeE164PhoneNumber(tt.number)
			if tt.wantErr {
				if !errors.Is(err, notifications.ErrTwilioPhoneNumberInvalid) {
					t.Fatalf("NormalizeE164PhoneNumber() error = %v, want ErrTwilioPhoneNumberInvalid", err)
				}
				return
			}
			if err != nil {
				t.Fatalf("NormalizeE164PhoneNumber() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeE164PhoneNumber() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestEstimateTwilioSMSMessageSegments(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		body string
		want notifications.TwilioSMSMessageSegments
	}{
		{
			name: "empty",
			body: "",
			want: notifications.TwilioSMSMessageSegments{},
		},
		{
			name: "gsm single segment",
			body: strings.Repeat("a", 160),
			want: notifications.TwilioSMSMessageSegments{Encoding: "GSM-7", Units: 160, Segments: 1},
		},
		{
			name: "gsm multipart",
			body: strings.Repeat("a", 161),
			want: notifications.TwilioSMSMessageSegments{Encoding: "GSM-7", Units: 161, Segments: 2},
		},
		{
			name: "gsm extended chars count as escapes",
			body: "{}[]^~\\|€",
			want: notifications.TwilioSMSMessageSegments{Encoding: "GSM-7", Units: 18, Segments: 1},
		},
		{
			name: "ucs single segment",
			body: strings.Repeat("雪", 70),
			want: notifications.TwilioSMSMessageSegments{Encoding: "UCS-2", Units: 70, Segments: 1},
		},
		{
			name: "ucs multipart",
			body: strings.Repeat("雪", 71),
			want: notifications.TwilioSMSMessageSegments{Encoding: "UCS-2", Units: 71, Segments: 2},
		},
		{
			name: "ucs surrogate pair",
			body: "😀",
			want: notifications.TwilioSMSMessageSegments{Encoding: "UCS-2", Units: 2, Segments: 1},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := notifications.EstimateTwilioSMSMessageSegments(tt.body)
			if got != tt.want {
				t.Fatalf("EstimateTwilioSMSMessageSegments() = %+v, want %+v", got, tt.want)
			}
		})
	}
}

func TestPlanTwilioSMSRequestNormalizesWithoutHTTP(t *testing.T) {
	t.Parallel()

	plan, err := notifications.PlanTwilioSMSRequest(notifications.TwilioSMSConfig{
		AccountSID: testTwilioAccountSID,
		AuthToken:  testTwilioAuthToken,
		FromNumber: "+14155550100",
		BaseURL:    " https://api.twilio.com/ ",
	}, notifications.TwilioSMSMessage{
		To:                " +14155550199 ",
		Body:              " Hello from Lazuli ",
		StatusCallbackURL: " HTTPS://example.test/twilio/status?delivery=1 ",
		Idempotency: notifications.IdempotencyKey{
			Notification: "billing.invoice_due",
			Tenant:       "tenant-1",
			Key:          "invoice-123",
		},
	})
	if err != nil {
		t.Fatalf("PlanTwilioSMSRequest() error = %v", err)
	}

	if plan.Provider != notifications.TwilioProviderName || plan.Channel != notifications.ChannelSMS {
		t.Fatalf("plan identity = %+v", plan)
	}
	if plan.EndpointPath != "/2010-04-01/Accounts/"+testTwilioAccountSID+"/Messages.json" {
		t.Fatalf("EndpointPath = %q", plan.EndpointPath)
	}
	if plan.EndpointURL != notifications.DefaultTwilioBaseURL+plan.EndpointPath {
		t.Fatalf("EndpointURL = %q", plan.EndpointURL)
	}
	if plan.Body != "Hello from Lazuli" || plan.To != "+14155550199" {
		t.Fatalf("message was not normalized: %+v", plan)
	}
	if plan.StatusCallbackURL != "https://example.test/twilio/status?delivery=1" {
		t.Fatalf("StatusCallbackURL = %q", plan.StatusCallbackURL)
	}
	if plan.Idempotency.Provider != notifications.TwilioProviderName ||
		plan.Idempotency.Channel != notifications.ChannelSMS ||
		plan.Idempotency.Key.Key != "invoice-123" ||
		plan.Idempotency.MessageSHA256 == "" {
		t.Fatalf("idempotency metadata = %+v", plan.Idempotency)
	}
	if plan.RedactedConfig.AuthToken == testTwilioAuthToken {
		t.Fatal("RedactedConfig leaked auth token")
	}

	wantForm := []string{
		"Body=Hello from Lazuli",
		"From=+14155550100",
		"StatusCallback=https://example.test/twilio/status?delivery=1",
		"To=+14155550199",
	}
	if got := plan.FormValues(); !reflect.DeepEqual(got, wantForm) {
		t.Fatalf("FormValues() = %#v, want %#v", got, wantForm)
	}
}

func TestPlanTwilioSMSRequestRejectsInvalidMessage(t *testing.T) {
	t.Parallel()

	_, err := notifications.PlanTwilioSMSRequest(notifications.TwilioSMSConfig{
		AccountSID: testTwilioAccountSID,
		AuthToken:  testTwilioAuthToken,
		FromNumber: "+14155550100",
	}, notifications.TwilioSMSMessage{
		To:                "4155550199",
		Body:              " ",
		StatusCallbackURL: "ftp://example.test/twilio/status",
	})
	if !errors.Is(err, notifications.ErrTwilioPhoneNumberInvalid) {
		t.Fatalf("PlanTwilioSMSRequest() error = %v, want ErrTwilioPhoneNumberInvalid", err)
	}
	if !errors.Is(err, notifications.ErrTwilioMessageInvalid) {
		t.Fatalf("PlanTwilioSMSRequest() error = %v, want ErrTwilioMessageInvalid", err)
	}
	if !errors.Is(err, notifications.ErrTwilioStatusCallbackURLInvalid) {
		t.Fatalf("PlanTwilioSMSRequest() error = %v, want ErrTwilioStatusCallbackURLInvalid", err)
	}
}

func TestTwilioURLRedaction(t *testing.T) {
	t.Parallel()

	got := notifications.RedactTwilioURL("https://user:pass@example.test/path?token=secret#frag")
	if got != "https://example.test/path" {
		t.Fatalf("RedactTwilioURL() = %q, want sanitized URL", got)
	}
}
