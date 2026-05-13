package notifications_test

import (
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/notifications"
)

var testFCMPrivateKey = strings.Join([]string{
	"-----BEGIN " + "PRIVATE KEY-----",
	"abc123",
	"-----END " + "PRIVATE KEY-----",
}, "\n")

func TestFCMDescriptorMetadataIsStable(t *testing.T) {
	t.Parallel()

	descriptor := notifications.FCMProviderDescriptor()
	if descriptor.ProviderName != notifications.FCMProviderName {
		t.Fatalf("ProviderName = %q, want %q", descriptor.ProviderName, notifications.FCMProviderName)
	}
	if descriptor.ProviderDisplayName != notifications.FCMProviderDisplayName {
		t.Fatalf("ProviderDisplayName = %q, want %q", descriptor.ProviderDisplayName, notifications.FCMProviderDisplayName)
	}
	if descriptor.Channel != notifications.ChannelPush {
		t.Fatalf("Channel = %q, want %q", descriptor.Channel, notifications.ChannelPush)
	}
	if descriptor.DefaultBaseURL != notifications.DefaultFCMBaseURL {
		t.Fatalf("DefaultBaseURL = %q, want %q", descriptor.DefaultBaseURL, notifications.DefaultFCMBaseURL)
	}
	if descriptor.MessagesPathTemplate != notifications.FCMMessagesPathTemplate {
		t.Fatalf("MessagesPathTemplate = %q", descriptor.MessagesPathTemplate)
	}
	if descriptor.MaxTTL != notifications.MaxFCMTTL {
		t.Fatalf("MaxTTL = %s, want %s", descriptor.MaxTTL, notifications.MaxFCMTTL)
	}
}

func TestNormalizeFCMConfigTrimsDefaultsAndRedacts(t *testing.T) {
	t.Parallel()

	config, err := notifications.NormalizeFCMConfig(notifications.FCMConfig{
		ProjectID:           " Lazuli-App-1 ",
		ServiceAccountEmail: " Sender@lazuli-app-1.iam.gserviceaccount.com ",
		PrivateKey:          "\n" + testFCMPrivateKey + "\n",
	})
	if err != nil {
		t.Fatalf("NormalizeFCMConfig() error = %v", err)
	}
	if config.ProjectID != "lazuli-app-1" {
		t.Fatalf("ProjectID = %q, want normalized", config.ProjectID)
	}
	if config.ServiceAccountEmail != "sender@lazuli-app-1.iam.gserviceaccount.com" {
		t.Fatalf("ServiceAccountEmail = %q, want normalized", config.ServiceAccountEmail)
	}
	if config.BaseURL != notifications.DefaultFCMBaseURL {
		t.Fatalf("BaseURL = %q, want default", config.BaseURL)
	}

	redacted := config.Redacted()
	if redacted.ProjectID != config.ProjectID {
		t.Fatalf("redacted ProjectID = %q, want unchanged", redacted.ProjectID)
	}
	if redacted.ServiceAccountEmail == config.ServiceAccountEmail || !strings.HasSuffix(redacted.ServiceAccountEmail, "@lazuli-app-1.iam.gserviceaccount.com") {
		t.Fatalf("redacted ServiceAccountEmail = %q", redacted.ServiceAccountEmail)
	}
	if redacted.PrivateKey == config.PrivateKey || strings.Contains(redacted.PrivateKey, "PRIVATE KEY") {
		t.Fatalf("redacted PrivateKey = %q", redacted.PrivateKey)
	}
}

func TestValidateFCMConfigRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		config notifications.FCMConfig
		want   error
	}{
		{
			name: "missing project",
			config: notifications.FCMConfig{
				ServiceAccountEmail: "sender@lazuli-app-1.iam.gserviceaccount.com",
				PrivateKey:          testFCMPrivateKey,
			},
			want: notifications.ErrFCMProjectIDMissing,
		},
		{
			name: "bad project",
			config: notifications.FCMConfig{
				ProjectID:           "Bad_Project",
				ServiceAccountEmail: "sender@lazuli-app-1.iam.gserviceaccount.com",
				PrivateKey:          testFCMPrivateKey,
			},
			want: notifications.ErrFCMProjectIDInvalid,
		},
		{
			name: "missing email",
			config: notifications.FCMConfig{
				ProjectID:  "lazuli-app-1",
				PrivateKey: testFCMPrivateKey,
			},
			want: notifications.ErrFCMServiceAccountEmailMissing,
		},
		{
			name: "bad email",
			config: notifications.FCMConfig{
				ProjectID:           "lazuli-app-1",
				ServiceAccountEmail: "sender@example.test",
				PrivateKey:          testFCMPrivateKey,
			},
			want: notifications.ErrFCMServiceAccountEmailInvalid,
		},
		{
			name: "missing key",
			config: notifications.FCMConfig{
				ProjectID:           "lazuli-app-1",
				ServiceAccountEmail: "sender@lazuli-app-1.iam.gserviceaccount.com",
			},
			want: notifications.ErrFCMPrivateKeyMissing,
		},
		{
			name: "bad key",
			config: notifications.FCMConfig{
				ProjectID:           "lazuli-app-1",
				ServiceAccountEmail: "sender@lazuli-app-1.iam.gserviceaccount.com",
				PrivateKey:          "secret",
			},
			want: notifications.ErrFCMPrivateKeyInvalid,
		},
		{
			name: "bad base url",
			config: notifications.FCMConfig{
				ProjectID:           "lazuli-app-1",
				ServiceAccountEmail: "sender@lazuli-app-1.iam.gserviceaccount.com",
				PrivateKey:          testFCMPrivateKey,
				BaseURL:             "ftp://fcm.googleapis.com",
			},
			want: notifications.ErrFCMBaseURLInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := notifications.ValidateFCMConfig(tt.config)
			if !errors.Is(err, notifications.ErrFCMConfigInvalid) {
				t.Fatalf("ValidateFCMConfig() error = %v, want ErrFCMConfigInvalid", err)
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidateFCMConfig() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestNormalizeFCMTargetPlansTokenAndTopic(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		target notifications.FCMTarget
		want   notifications.FCMPlannedTarget
	}{
		{
			name:   "token",
			target: notifications.FCMTarget{Token: " device-token-1 "},
			want:   notifications.FCMPlannedTarget{Type: notifications.FCMTargetToken, Value: "device-token-1"},
		},
		{
			name:   "topic",
			target: notifications.FCMTarget{Topic: " /topics/tenant-updates "},
			want:   notifications.FCMPlannedTarget{Type: notifications.FCMTargetTopic, Value: "tenant-updates"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, err := notifications.NormalizeFCMTarget(tt.target)
			if err != nil {
				t.Fatalf("NormalizeFCMTarget() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeFCMTarget() = %#v, want %#v", got, tt.want)
			}
		})
	}
}

func TestNormalizeFCMTargetRejectsMissingAmbiguousOrMalformedTargets(t *testing.T) {
	t.Parallel()

	tests := []notifications.FCMTarget{
		{},
		{Token: "device-token", Topic: "tenant-updates"},
		{Token: "device token"},
		{Topic: "tenant updates"},
	}
	for _, target := range tests {
		target := target
		t.Run(strings.Join([]string{target.Token, target.Topic}, "|"), func(t *testing.T) {
			t.Parallel()

			_, err := notifications.NormalizeFCMTarget(target)
			if !errors.Is(err, notifications.ErrFCMTargetInvalid) {
				t.Fatalf("NormalizeFCMTarget() error = %v, want ErrFCMTargetInvalid", err)
			}
		})
	}
}

func TestNormalizeFCMTTLBoundsAndRounding(t *testing.T) {
	t.Parallel()

	got, err := notifications.NormalizeFCMTTL(1500 * time.Millisecond)
	if err != nil {
		t.Fatalf("NormalizeFCMTTL() error = %v", err)
	}
	if got != time.Second {
		t.Fatalf("NormalizeFCMTTL() = %s, want 1s", got)
	}

	for _, ttl := range []time.Duration{-time.Second, notifications.MaxFCMTTL + time.Second} {
		_, err := notifications.NormalizeFCMTTL(ttl)
		if !errors.Is(err, notifications.ErrFCMTTLInvalid) {
			t.Fatalf("NormalizeFCMTTL(%s) error = %v, want ErrFCMTTLInvalid", ttl, err)
		}
	}
}

func TestPlanFCMRequestNormalizesWithoutHTTP(t *testing.T) {
	t.Parallel()

	plan, err := notifications.PlanFCMRequest(notifications.FCMConfig{
		ProjectID:           "lazuli-app-1",
		ServiceAccountEmail: "sender@lazuli-app-1.iam.gserviceaccount.com",
		PrivateKey:          testFCMPrivateKey,
		BaseURL:             " https://fcm.googleapis.com/ ",
	}, notifications.FCMMessage{
		Target: notifications.FCMTarget{Topic: "topics/tenant-updates"},
		Title:  " Gate changed ",
		Body:   " Use entrance B ",
		Data: map[string]string{
			" booking_id ": " booking-1 ",
			"empty":        " ",
		},
		TTL: 10*time.Minute + 250*time.Millisecond,
		Android: notifications.FCMAndroidOptions{
			Priority:  " HIGH ",
			TTL:       time.Hour,
			ChannelID: " ops ",
		},
		APNS: notifications.FCMAPNSOptions{
			Headers:        map[string]string{" apns-priority ": " 10 "},
			AnalyticsLabel: " booking_apns ",
		},
		WebPush: notifications.FCMWebPushOptions{
			Headers:        map[string]string{" Urgency ": " high "},
			Link:           " HTTPS://example.test/bookings/1 ",
			AnalyticsLabel: " booking_update ",
		},
		Idempotency: notifications.IdempotencyKey{
			Notification: "booking.gate_changed",
			Tenant:       "tenant-1",
		},
	})
	if err != nil {
		t.Fatalf("PlanFCMRequest() error = %v", err)
	}

	if plan.Provider != notifications.FCMProviderName || plan.Channel != notifications.ChannelPush {
		t.Fatalf("plan identity = %+v", plan)
	}
	if plan.EndpointPath != "/v1/projects/lazuli-app-1/messages:send" {
		t.Fatalf("EndpointPath = %q", plan.EndpointPath)
	}
	if plan.EndpointURL != notifications.DefaultFCMBaseURL+plan.EndpointPath {
		t.Fatalf("EndpointURL = %q", plan.EndpointURL)
	}
	if plan.Message.Target.Type != notifications.FCMTargetTopic || plan.Message.Target.Value != "tenant-updates" {
		t.Fatalf("Target = %+v", plan.Message.Target)
	}
	if plan.Message.TTL != 10*time.Minute || plan.Message.TTLSeconds != 600 {
		t.Fatalf("TTL = %s/%d, want 10m/600", plan.Message.TTL, plan.Message.TTLSeconds)
	}
	if plan.Message.Android.Priority != "high" || plan.Message.Android.TTL != time.Hour || plan.Message.Android.ChannelID != "ops" {
		t.Fatalf("Android = %+v", plan.Message.Android)
	}
	if got, want := plan.Message.DataValues(), []string{"booking_id=booking-1"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("DataValues() = %#v, want %#v", got, want)
	}
	if got, want := plan.Message.APNSHeaderValues(), []string{"apns-priority=10"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("APNSHeaderValues() = %#v, want %#v", got, want)
	}
	if got, want := plan.Message.WebPushHeaderValues(), []string{"Urgency=high"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("WebPushHeaderValues() = %#v, want %#v", got, want)
	}
	if plan.Message.Idempotency.Provider != notifications.FCMProviderName ||
		plan.Message.Idempotency.Target != "tenant-updates" ||
		plan.Message.Idempotency.Key.Key == "" ||
		plan.Message.Idempotency.MessageSHA256 == "" {
		t.Fatalf("idempotency metadata = %+v", plan.Message.Idempotency)
	}
	if plan.RedactedConfig.PrivateKey == testFCMPrivateKey {
		t.Fatal("RedactedConfig leaked private key")
	}
	if plan.RedactedConfig.ServiceAccountEmail == "sender@lazuli-app-1.iam.gserviceaccount.com" {
		t.Fatal("RedactedConfig leaked service account email")
	}

	bodyJSON, err := json.Marshal(plan.RequestBody())
	if err != nil {
		t.Fatalf("Marshal(RequestBody()): %v", err)
	}
	for _, want := range []string{
		`"topic":"tenant-updates"`,
		`"title":"Gate changed"`,
		`"body":"Use entrance B"`,
		`"ttl":"600s"`,
		`"priority":"high"`,
		`"channel_id":"ops"`,
		`"link":"https://example.test/bookings/1"`,
	} {
		if !strings.Contains(string(bodyJSON), want) {
			t.Fatalf("RequestBody JSON = %s, missing %s", bodyJSON, want)
		}
	}
}

func TestPlanFCMRequestRejectsInvalidMessage(t *testing.T) {
	t.Parallel()

	_, err := notifications.PlanFCMRequest(notifications.FCMConfig{
		ProjectID:           "lazuli-app-1",
		ServiceAccountEmail: "sender@lazuli-app-1.iam.gserviceaccount.com",
		PrivateKey:          testFCMPrivateKey,
	}, notifications.FCMMessage{
		Target: notifications.FCMTarget{Token: "bad token"},
		TTL:    notifications.MaxFCMTTL + time.Second,
		Android: notifications.FCMAndroidOptions{
			Priority: "urgent",
		},
		WebPush: notifications.FCMWebPushOptions{
			Link: "ftp://example.test",
		},
	})
	if !errors.Is(err, notifications.ErrFCMTargetInvalid) {
		t.Fatalf("PlanFCMRequest() error = %v, want ErrFCMTargetInvalid", err)
	}
	if !errors.Is(err, notifications.ErrFCMTTLInvalid) {
		t.Fatalf("PlanFCMRequest() error = %v, want ErrFCMTTLInvalid", err)
	}
	if !errors.Is(err, notifications.ErrFCMMessageInvalid) {
		t.Fatalf("PlanFCMRequest() error = %v, want ErrFCMMessageInvalid", err)
	}
	if !errors.Is(err, notifications.ErrFCMOptionsInvalid) {
		t.Fatalf("PlanFCMRequest() error = %v, want ErrFCMOptionsInvalid", err)
	}
}

func TestFCMURLRedaction(t *testing.T) {
	t.Parallel()

	got := notifications.RedactFCMURL("https://user:pass@example.test/path?token=secret#frag")
	if got != "https://example.test/path" {
		t.Fatalf("RedactFCMURL() = %q, want sanitized URL", got)
	}
}
