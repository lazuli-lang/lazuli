package email

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestParseBounceEventPayloadNormalizesCommonFields(t *testing.T) {
	t.Parallel()

	payload := map[string]any{
		"provider": "adapter",
		"event": map[string]any{
			"id": json.Number("42"),
		},
		"message": map[string]any{
			"id": "msg-123",
		},
		"recipient": map[string]any{
			"email": "User <user@example.com>",
		},
		"bounce": map[string]any{
			"type":   "permanent",
			"reason": "550 5.1.1 user unknown",
		},
		"smtp": map[string]string{
			"status":     "5.1.1",
			"diagnostic": "smtp; 550 5.1.1 no such user",
		},
		"timestamp": "2026-05-12T15:04:05Z",
	}

	event, err := ParseBounceEventPayload(payload)
	if err != nil {
		t.Fatalf("ParseBounceEventPayload: %v", err)
	}
	if event.Provider != "adapter" || event.ID != "42" || event.MessageID != "msg-123" {
		t.Fatalf("event ids = %+v", event)
	}
	if event.Recipient != "user@example.com" {
		t.Fatalf("Recipient = %q, want user@example.com", event.Recipient)
	}
	if event.Type != BounceTypeHard {
		t.Fatalf("Type = %q, want hard", event.Type)
	}
	if event.Reason != BounceReasonInvalidRecipient {
		t.Fatalf("Reason = %q, want invalid_recipient", event.Reason)
	}
	if !event.OccurredAt.Equal(time.Date(2026, 5, 12, 15, 4, 5, 0, time.UTC)) {
		t.Fatalf("OccurredAt = %s", event.OccurredAt)
	}
	if decision := DecideBounceSuppression(event); !decision.Suppress || decision.Reason != "hard_bounce" {
		t.Fatalf("DecideBounceSuppression = %+v, want hard suppression", decision)
	}
}

func TestClassifyBounceReason(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		reason     string
		status     string
		diagnostic string
		want       BounceReason
	}{
		{
			name:   "invalid recipient status",
			status: "5.1.1",
			want:   BounceReasonInvalidRecipient,
		},
		{
			name:       "mailbox full diagnostic",
			diagnostic: "452 4.2.2 mailbox full",
			want:       BounceReasonMailboxFull,
		},
		{
			name:   "rate limited",
			reason: "temporarily deferred due to rate limit",
			want:   BounceReasonRateLimited,
		},
		{
			name:       "blocklist",
			diagnostic: "message blocked due to IP reputation",
			want:       BounceReasonBlocklist,
		},
		{
			name:   "permanent fallback",
			status: "5.7.1",
			want:   BounceReasonPermanent,
		},
		{
			name:   "temporary fallback",
			status: "4.4.1",
			want:   BounceReasonTemporary,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := ClassifyBounceReason(tt.reason, tt.status, tt.diagnostic)
			if got != tt.want {
				t.Fatalf("ClassifyBounceReason() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestDecideBounceSuppression(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		event    BounceEvent
		suppress bool
		reason   string
	}{
		{
			name: "hard bounce suppresses",
			event: BounceEvent{
				Recipient: "user@example.com",
				Type:      BounceTypeHard,
			},
			suppress: true,
			reason:   "hard_bounce",
		},
		{
			name: "soft bounce does not suppress",
			event: BounceEvent{
				Recipient:      "user@example.com",
				Type:           BounceTypeSoft,
				ProviderReason: "mailbox full",
			},
			reason: "soft_bounce",
		},
		{
			name: "invalid recipient reason suppresses",
			event: BounceEvent{
				Recipient:      "user@example.com",
				ProviderReason: "recipient address rejected: user unknown",
			},
			suppress: true,
			reason:   "invalid_recipient",
		},
		{
			name: "mailbox full reason does not suppress without hard type",
			event: BounceEvent{
				Recipient:      "user@example.com",
				ProviderReason: "mailbox full",
				Status:         "5.2.2",
			},
			reason: "mailbox_full",
		},
		{
			name: "permanent status suppresses",
			event: BounceEvent{
				Recipient: "user@example.com",
				Status:    "5.7.1",
			},
			suppress: true,
			reason:   "permanent_failure",
		},
		{
			name: "temporary status does not suppress",
			event: BounceEvent{
				Recipient: "user@example.com",
				Status:    "4.4.1",
			},
			reason: "temporary_failure",
		},
		{
			name: "invalid event does not suppress",
			event: BounceEvent{
				Recipient: "not an address",
				Type:      BounceTypeHard,
			},
			reason: "invalid_event",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := DecideBounceSuppression(tt.event)
			if got.Suppress != tt.suppress || got.Reason != tt.reason {
				t.Fatalf("DecideBounceSuppression() = %+v, want suppress=%v reason=%q", got, tt.suppress, tt.reason)
			}
			if ShouldSuppressBounce(tt.event) != tt.suppress {
				t.Fatalf("ShouldSuppressBounce() = %v, want %v", ShouldSuppressBounce(tt.event), tt.suppress)
			}
		})
	}
}

func TestBouncePayloadHelpersResolvePathsAndTimestamps(t *testing.T) {
	t.Parallel()

	payload := map[string]any{
		"payload": map[string]any{
			"event": map[string]any{
				"id": json.Number("99"),
			},
			"timestamp": int64(1_768_236_245_000),
		},
	}

	id, ok := BouncePayloadString(payload, "payload.event.id")
	if !ok || id != "99" {
		t.Fatalf("BouncePayloadString() = %q, %v; want 99, true", id, ok)
	}

	got, ok, err := BouncePayloadTime(payload, "payload.timestamp")
	if err != nil {
		t.Fatalf("BouncePayloadTime: %v", err)
	}
	if !ok {
		t.Fatalf("BouncePayloadTime ok = false, want true")
	}
	if !got.Equal(time.UnixMilli(1_768_236_245_000).UTC()) {
		t.Fatalf("BouncePayloadTime = %s", got)
	}
}

func TestParseBounceEventPayloadRejectsInvalidRecipient(t *testing.T) {
	t.Parallel()

	_, err := ParseBounceEventPayload(map[string]any{
		"recipient": "not an address",
	})
	if !errors.Is(err, ErrInvalidBouncePayload) {
		t.Fatalf("ParseBounceEventPayload error = %v, want ErrInvalidBouncePayload", err)
	}
	if !strings.Contains(err.Error(), "recipient") {
		t.Fatalf("ParseBounceEventPayload error = %q, want recipient field", err)
	}
}

func TestNormalizeBounceEventRejectsMissingRecipient(t *testing.T) {
	t.Parallel()

	_, err := NormalizeBounceEvent(BounceEvent{})
	if !errors.Is(err, ErrInvalidBounceEvent) {
		t.Fatalf("NormalizeBounceEvent error = %v, want ErrInvalidBounceEvent", err)
	}
	if !strings.Contains(err.Error(), "recipient") {
		t.Fatalf("NormalizeBounceEvent error = %q, want recipient field", err)
	}
}
