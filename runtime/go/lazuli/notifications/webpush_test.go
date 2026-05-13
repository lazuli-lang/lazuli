package notifications

import (
	"bytes"
	"crypto/ecdh"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestWebPushVAPIDValidationAndRedaction(t *testing.T) {
	t.Parallel()

	vapid := validWebPushVAPID(t)
	if err := ValidateWebPushVAPIDMetadata(WebPushVAPIDMetadata{
		PublicKey:  " " + vapid.PublicKey + " ",
		PrivateKey: " " + vapid.PrivateKey + " ",
		Subject:    " mailto:ops@example.com ",
	}); err != nil {
		t.Fatalf("ValidateWebPushVAPIDMetadata(valid) error = %v", err)
	}

	normalized := NormalizeWebPushVAPIDMetadata(WebPushVAPIDMetadata{
		PublicKey:  " " + vapid.PublicKey + " ",
		PrivateKey: " " + vapid.PrivateKey + " ",
		Subject:    " https://example.com/push ",
	})
	if normalized.PublicKey != vapid.PublicKey || normalized.PrivateKey != vapid.PrivateKey || normalized.Subject != "https://example.com/push" {
		t.Fatalf("NormalizeWebPushVAPIDMetadata() = %#v, want trimmed metadata", normalized)
	}
	if got := RedactWebPushVAPIDPrivateKey(vapid.PrivateKey); got != "[redacted]" {
		t.Fatalf("RedactWebPushVAPIDPrivateKey() = %q, want [redacted]", got)
	}
	if got := RedactWebPushVAPIDPublicKey(vapid.PublicKey); !strings.HasPrefix(got, vapid.PublicKey[:6]+"...") || !strings.HasSuffix(got, vapid.PublicKey[len(vapid.PublicKey)-6:]) {
		t.Fatalf("RedactWebPushVAPIDPublicKey() = %q, want prefix and suffix retained", got)
	}

	tests := []struct {
		name string
		meta WebPushVAPIDMetadata
		want error
	}{
		{
			name: "invalid public key",
			meta: WebPushVAPIDMetadata{
				PublicKey:  "not-base64",
				PrivateKey: vapid.PrivateKey,
				Subject:    "mailto:ops@example.com",
			},
			want: ErrInvalidWebPushVAPID,
		},
		{
			name: "invalid private key",
			meta: WebPushVAPIDMetadata{
				PublicKey:  vapid.PublicKey,
				PrivateKey: base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{1}, 31)),
				Subject:    "mailto:ops@example.com",
			},
			want: ErrInvalidWebPushVAPID,
		},
		{
			name: "invalid subject",
			meta: WebPushVAPIDMetadata{
				PublicKey:  vapid.PublicKey,
				PrivateKey: vapid.PrivateKey,
				Subject:    "http://example.com",
			},
			want: ErrInvalidWebPushSubject,
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if err := ValidateWebPushVAPIDMetadata(tt.meta); !errors.Is(err, tt.want) {
				t.Fatalf("ValidateWebPushVAPIDMetadata() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestValidateWebPushSubject(t *testing.T) {
	t.Parallel()

	valid := []string{
		"mailto:ops@example.com",
		"https://example.com/push-contact",
	}
	for _, subject := range valid {
		subject := subject
		t.Run(subject, func(t *testing.T) {
			t.Parallel()

			if err := ValidateWebPushSubject(subject); err != nil {
				t.Fatalf("ValidateWebPushSubject(%q) error = %v", subject, err)
			}
		})
	}

	invalid := []string{
		"",
		"ops@example.com",
		"mailto:",
		"mailto:ops example.com",
		"http://example.com",
		"https://user@example.com/contact",
		"https://example.com/contact#fragment",
	}
	for _, subject := range invalid {
		subject := subject
		t.Run("invalid "+subject, func(t *testing.T) {
			t.Parallel()

			if err := ValidateWebPushSubject(subject); !errors.Is(err, ErrInvalidWebPushSubject) {
				t.Fatalf("ValidateWebPushSubject(%q) error = %v, want ErrInvalidWebPushSubject", subject, err)
			}
		})
	}
}

func TestWebPushSubscriptionValidationAndEndpointRedaction(t *testing.T) {
	t.Parallel()

	subscription := validWebPushSubscription(t)
	if err := ValidateWebPushSubscriptionDescriptor(WebPushSubscriptionDescriptor{
		Endpoint: " " + subscription.Endpoint + " ",
		P256DH:   " " + subscription.P256DH + " ",
		Auth:     " " + subscription.Auth + " ",
	}); err != nil {
		t.Fatalf("ValidateWebPushSubscriptionDescriptor(valid) error = %v", err)
	}
	if got := RedactWebPushSubscriptionEndpoint(subscription.Endpoint + "?token=secret"); got != "https://push.example.test/send/..." {
		t.Fatalf("RedactWebPushSubscriptionEndpoint() = %q, want host and first path segment", got)
	}
	if got := RedactWebPushSubscriptionEndpoint("not a url"); got != "[redacted]" {
		t.Fatalf("RedactWebPushSubscriptionEndpoint(invalid) = %q, want [redacted]", got)
	}

	tests := []struct {
		name string
		desc WebPushSubscriptionDescriptor
	}{
		{
			name: "http endpoint",
			desc: WebPushSubscriptionDescriptor{
				Endpoint: "http://push.example.test/send/token",
				P256DH:   subscription.P256DH,
				Auth:     subscription.Auth,
			},
		},
		{
			name: "invalid p256dh",
			desc: WebPushSubscriptionDescriptor{
				Endpoint: subscription.Endpoint,
				P256DH:   "not-base64",
				Auth:     subscription.Auth,
			},
		},
		{
			name: "invalid auth",
			desc: WebPushSubscriptionDescriptor{
				Endpoint: subscription.Endpoint,
				P256DH:   subscription.P256DH,
				Auth:     "",
			},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if err := ValidateWebPushSubscriptionDescriptor(tt.desc); !errors.Is(err, ErrInvalidWebPushSubscription) {
				t.Fatalf("ValidateWebPushSubscriptionDescriptor() error = %v, want ErrInvalidWebPushSubscription", err)
			}
		})
	}
}

func TestWebPushOptionsNormalizeAndValidate(t *testing.T) {
	t.Parallel()

	normalized := NormalizeWebPushOptions(WebPushOptions{Topic: " incident-1 "})
	if normalized.TTL != defaultWebPushTTL || normalized.Urgency != WebPushUrgencyNormal || normalized.Topic != "incident-1" {
		t.Fatalf("NormalizeWebPushOptions() = %#v, want defaults and trimmed topic", normalized)
	}
	if err := ValidateWebPushOptions(WebPushOptions{
		TTL:     30 * time.Second,
		Urgency: WebPushUrgencyHigh,
		Topic:   "incident-1",
	}); err != nil {
		t.Fatalf("ValidateWebPushOptions(valid) error = %v", err)
	}

	tests := []struct {
		name string
		opts WebPushOptions
	}{
		{name: "negative ttl", opts: WebPushOptions{TTL: -time.Second}},
		{name: "fractional ttl", opts: WebPushOptions{TTL: 1500 * time.Millisecond}},
		{name: "bad urgency", opts: WebPushOptions{Urgency: WebPushUrgency("urgent")}},
		{name: "topic too long", opts: WebPushOptions{Topic: strings.Repeat("a", WebPushMaxTopicRunes+1)}},
		{name: "topic with space", opts: WebPushOptions{Topic: "incident 1"}},
		{name: "topic unicode", opts: WebPushOptions{Topic: "incidént"}},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if err := ValidateWebPushOptions(tt.opts); !errors.Is(err, ErrInvalidWebPushOptions) {
				t.Fatalf("ValidateWebPushOptions() error = %v, want ErrInvalidWebPushOptions", err)
			}
		})
	}
}

func TestPlanWebPushPayload(t *testing.T) {
	t.Parallel()

	vapid := validWebPushVAPID(t)
	subscription := validWebPushSubscription(t)
	data := map[string]any{"booking_id": "booking-1"}
	plan, err := PlanWebPushPayload(vapid, subscription, WebPushMessage{
		Title:          " Gate changed ",
		Body:           " Use entrance B ",
		Data:           data,
		IdempotencyKey: " req-1 ",
	}, WebPushOptions{
		TTL:     30 * time.Second,
		Urgency: WebPushUrgencyHigh,
		Topic:   "booking-1",
	})
	if err != nil {
		t.Fatalf("PlanWebPushPayload() error = %v", err)
	}

	if plan.Headers["TTL"] != "30" || plan.Headers["Urgency"] != "high" || plan.Headers["Topic"] != "booking-1" {
		t.Fatalf("Headers = %#v, want TTL/Urgency/Topic", plan.Headers)
	}
	if plan.RedactedEndpoint != "https://push.example.test/send/..." {
		t.Fatalf("RedactedEndpoint = %q, want endpoint redaction", plan.RedactedEndpoint)
	}
	if plan.RedactedPrivateKey != "[redacted]" {
		t.Fatalf("RedactedPrivateKey = %q, want [redacted]", plan.RedactedPrivateKey)
	}
	if plan.IdempotencyKey != "req-1" {
		t.Fatalf("IdempotencyKey = %q, want req-1", plan.IdempotencyKey)
	}
	if plan.PayloadBytes != len(plan.Payload) || !bytes.Equal(plan.Payload, plan.PayloadJSON) {
		t.Fatalf("payload lengths/json mismatch: bytes=%d len=%d json=%q payload=%q", plan.PayloadBytes, len(plan.Payload), plan.PayloadJSON, plan.Payload)
	}

	var payload map[string]any
	if err := json.Unmarshal(plan.PayloadJSON, &payload); err != nil {
		t.Fatalf("unmarshal payload JSON: %v", err)
	}
	if payload["title"] != "Gate changed" || payload["body"] != "Use entrance B" {
		t.Fatalf("payload title/body = %#v, want trimmed values", payload)
	}
	payloadData, ok := payload["data"].(map[string]any)
	if !ok || payloadData["booking_id"] != "booking-1" {
		t.Fatalf("payload.data = %#v, want booking id", payload["data"])
	}
	data["booking_id"] = "mutated"
	if payloadData["booking_id"] != "booking-1" {
		t.Fatalf("plan retained mutable data map = %#v, want booking-1", payloadData)
	}
}

func TestPlanWebPushPayloadRawBytes(t *testing.T) {
	t.Parallel()

	vapid := validWebPushVAPID(t)
	subscription := validWebPushSubscription(t)
	raw := []byte(`{"ready":true}`)
	plan, err := PlanWebPushPayload(vapid, subscription, WebPushMessage{
		Payload: raw,
	}, WebPushOptions{})
	if err != nil {
		t.Fatalf("PlanWebPushPayload(raw) error = %v", err)
	}
	if !bytes.Equal(plan.Payload, raw) {
		t.Fatalf("Payload = %q, want raw bytes", plan.Payload)
	}
	if len(plan.PayloadJSON) != 0 {
		t.Fatalf("PayloadJSON = %q, want empty for raw payload", plan.PayloadJSON)
	}
	raw[0] = '['
	if bytes.Equal(plan.Payload, raw) {
		t.Fatal("plan payload was mutated through original raw slice")
	}
}

func TestValidateWebPushMessage(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		message WebPushMessage
	}{
		{name: "empty message", message: WebPushMessage{}},
		{name: "raw too large", message: WebPushMessage{Payload: bytes.Repeat([]byte("a"), WebPushMaxPayloadBytes+1)}},
		{name: "json too large", message: WebPushMessage{Data: map[string]any{"body": strings.Repeat("a", WebPushMaxPayloadBytes)}}},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if err := ValidateWebPushMessage(tt.message); !errors.Is(err, ErrInvalidWebPushPayload) {
				t.Fatalf("ValidateWebPushMessage() error = %v, want ErrInvalidWebPushPayload", err)
			}
		})
	}
}

func validWebPushVAPID(t *testing.T) WebPushVAPIDMetadata {
	t.Helper()

	key, err := ecdh.P256().GenerateKey(bytes.NewReader(bytes.Repeat([]byte{1}, 64)))
	if err != nil {
		t.Fatalf("GenerateKey: %v", err)
	}
	return WebPushVAPIDMetadata{
		PublicKey:  base64.RawURLEncoding.EncodeToString(key.PublicKey().Bytes()),
		PrivateKey: base64.RawURLEncoding.EncodeToString(key.Bytes()),
		Subject:    "mailto:ops@example.com",
	}
}

func validWebPushSubscription(t *testing.T) WebPushSubscriptionDescriptor {
	t.Helper()

	key, err := ecdh.P256().GenerateKey(bytes.NewReader(bytes.Repeat([]byte{2}, 64)))
	if err != nil {
		t.Fatalf("GenerateKey: %v", err)
	}
	return WebPushSubscriptionDescriptor{
		Endpoint: "https://push.example.test/send/device-secret",
		P256DH:   base64.RawURLEncoding.EncodeToString(key.PublicKey().Bytes()),
		Auth:     base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{3}, 16)),
	}
}
