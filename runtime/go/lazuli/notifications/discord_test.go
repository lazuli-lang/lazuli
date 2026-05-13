package notifications

import (
	"errors"
	"strings"
	"testing"
)

const testDiscordWebhookURL = "https://discord.com/api/webhooks/123456789012345678/test_token"

func TestPlanDiscordWebhookPayloadNormalizesAndRedacts(t *testing.T) {
	t.Parallel()

	input := DiscordWebhookDescriptor{
		WebhookURL: " " + strings.ToUpper(testDiscordWebhookURL[:8]) + testDiscordWebhookURL[8:] + " ",
		Username:   " Lazuli ",
		AvatarURL:  " https://cdn.example.com/avatar.png ",
		ThreadID:   " 987654321098765432 ",
		Content:    " Gate changed ",
		Embeds: []DiscordWebhookEmbed{{
			Title:       " Update ",
			Description: " Use entrance B ",
			URL:         " https://example.com/bookings/1 ",
			Color:       0x2F855A,
			Author:      " Ops ",
			Footer:      " Booking 1 ",
			Fields: []DiscordWebhookEmbedField{{
				Name:   " Status ",
				Value:  " Ready ",
				Inline: true,
			}},
		}},
		Metadata: DiscordWebhookMetadata{
			Notification: " BookingGate ",
			Tenant:       " tenant-1 ",
			Idempotency:  " booking-1:gate ",
		},
	}

	plan, err := PlanDiscordWebhookPayload(input)
	if err != nil {
		t.Fatalf("PlanDiscordWebhookPayload() error = %v", err)
	}

	if plan.Descriptor.WebhookURL != testDiscordWebhookURL {
		t.Fatalf("WebhookURL = %q, want %q", plan.Descriptor.WebhookURL, testDiscordWebhookURL)
	}
	if plan.RedactedWebhookURL != "https://discord.com/api/webhooks/123456789012345678/[redacted]" {
		t.Fatalf("RedactedWebhookURL = %q", plan.RedactedWebhookURL)
	}
	if plan.Descriptor.Username != "Lazuli" || plan.Descriptor.ThreadID != "987654321098765432" {
		t.Fatalf("normalized descriptor = %#v", plan.Descriptor)
	}
	if plan.Descriptor.Metadata.Notification != "BookingGate" || plan.Descriptor.Metadata.Tenant != "tenant-1" || plan.Descriptor.Metadata.Idempotency != "booking-1:gate" {
		t.Fatalf("metadata = %#v", plan.Descriptor.Metadata)
	}
	if input.Username != " Lazuli " || input.Embeds[0].Fields[0].Name != " Status " {
		t.Fatalf("PlanDiscordWebhookPayload mutated input: %#v", input)
	}

	if got := plan.Payload["content"]; got != "Gate changed" {
		t.Fatalf("payload.content = %v, want Gate changed", got)
	}
	if got := plan.Payload["username"]; got != "Lazuli" {
		t.Fatalf("payload.username = %v, want Lazuli", got)
	}
	if _, ok := plan.Payload["thread_id"]; ok {
		t.Fatalf("payload included thread_id; thread_id belongs in request metadata")
	}
	embeds, ok := plan.Payload["embeds"].([]map[string]any)
	if !ok || len(embeds) != 1 {
		t.Fatalf("payload.embeds = %#v, want one embed", plan.Payload["embeds"])
	}
	if got := embeds[0]["title"]; got != "Update" {
		t.Fatalf("embed.title = %v, want Update", got)
	}
	if plan.ContentChars != len("Gate changed") || plan.EmbedChars == 0 || plan.TotalChars != plan.ContentChars+plan.EmbedChars {
		t.Fatalf("size plan = content %d embed %d total %d", plan.ContentChars, plan.EmbedChars, plan.TotalChars)
	}
}

func TestValidateDiscordWebhookURL(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		raw  string
		want error
	}{
		{name: "valid discord", raw: testDiscordWebhookURL},
		{name: "valid legacy host", raw: "https://discordapp.com/api/webhooks/123456789012345678/token"},
		{name: "empty", raw: "", want: ErrInvalidDiscordWebhookURL},
		{name: "http", raw: "http://discord.com/api/webhooks/123456789012345678/token", want: ErrInvalidDiscordWebhookURL},
		{name: "host", raw: "https://example.com/api/webhooks/123456789012345678/token", want: ErrInvalidDiscordWebhookURL},
		{name: "path", raw: "https://discord.com/webhooks/123456789012345678/token", want: ErrInvalidDiscordWebhookURL},
		{name: "id", raw: "https://discord.com/api/webhooks/not-id/token", want: ErrInvalidDiscordWebhookURL},
		{name: "query", raw: "https://discord.com/api/webhooks/123456789012345678/token?wait=true", want: ErrInvalidDiscordWebhookURL},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := ValidateDiscordWebhookURL(tc.raw)
			if !errors.Is(err, tc.want) {
				t.Fatalf("ValidateDiscordWebhookURL() error = %v, want %v", err, tc.want)
			}
		})
	}
}

func TestRedactDiscordWebhookURL(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		raw  string
		want string
	}{
		{
			name: "valid",
			raw:  testDiscordWebhookURL,
			want: "https://discord.com/api/webhooks/123456789012345678/[redacted]",
		},
		{
			name: "query removed",
			raw:  testDiscordWebhookURL + "?wait=true",
			want: "https://discord.com/api/webhooks/123456789012345678/[redacted]",
		},
		{
			name: "invalid parse",
			raw:  "%",
			want: "[redacted]",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			if got := RedactDiscordWebhookURL(tc.raw); got != tc.want {
				t.Fatalf("RedactDiscordWebhookURL() = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestPlanDiscordWebhookPayloadRejectsInvalidDescriptor(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		desc DiscordWebhookDescriptor
		want error
	}{
		{
			name: "empty payload",
			desc: DiscordWebhookDescriptor{WebhookURL: testDiscordWebhookURL},
			want: ErrInvalidDiscordWebhookPayload,
		},
		{
			name: "content too large",
			desc: DiscordWebhookDescriptor{
				WebhookURL: testDiscordWebhookURL,
				Content:    strings.Repeat("a", DiscordWebhookMaxContentChars+1),
			},
			want: ErrInvalidDiscordWebhookPayload,
		},
		{
			name: "too many embeds",
			desc: DiscordWebhookDescriptor{
				WebhookURL: testDiscordWebhookURL,
				Content:    "ready",
				Embeds:     make([]DiscordWebhookEmbed, DiscordWebhookMaxEmbeds+1),
			},
			want: ErrInvalidDiscordWebhookPayload,
		},
		{
			name: "invalid avatar",
			desc: DiscordWebhookDescriptor{
				WebhookURL: testDiscordWebhookURL,
				AvatarURL:  "http://example.com/avatar.png",
				Content:    "ready",
			},
			want: ErrInvalidDiscordWebhookDescriptor,
		},
		{
			name: "invalid thread metadata",
			desc: DiscordWebhookDescriptor{
				WebhookURL: testDiscordWebhookURL,
				ThreadID:   "not-a-snowflake",
				Content:    "ready",
			},
			want: ErrInvalidDiscordWebhookDescriptor,
		},
		{
			name: "thread id and name",
			desc: DiscordWebhookDescriptor{
				WebhookURL: testDiscordWebhookURL,
				ThreadID:   "987654321098765432",
				ThreadName: "alerts",
				Content:    "ready",
			},
			want: ErrInvalidDiscordWebhookDescriptor,
		},
		{
			name: "field requires value",
			desc: DiscordWebhookDescriptor{
				WebhookURL: testDiscordWebhookURL,
				Embeds: []DiscordWebhookEmbed{{
					Fields: []DiscordWebhookEmbedField{{Name: "status"}},
				}},
			},
			want: ErrInvalidDiscordWebhookPayload,
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			_, err := PlanDiscordWebhookPayload(tc.desc)
			if !errors.Is(err, tc.want) {
				t.Fatalf("PlanDiscordWebhookPayload() error = %v, want %v", err, tc.want)
			}
		})
	}
}

func TestPlanDiscordWebhookPayloadReportsRuneCounts(t *testing.T) {
	t.Parallel()

	plan, err := PlanDiscordWebhookPayload(DiscordWebhookDescriptor{
		WebhookURL: testDiscordWebhookURL,
		Content:    "ola",
		Embeds: []DiscordWebhookEmbed{{
			Title:       "t",
			Description: "çç",
			Fields: []DiscordWebhookEmbedField{{
				Name:  "k",
				Value: "valor",
			}},
		}},
	})
	if err != nil {
		t.Fatalf("PlanDiscordWebhookPayload() error = %v", err)
	}

	if plan.ContentChars != 3 {
		t.Fatalf("ContentChars = %d, want 3", plan.ContentChars)
	}
	if plan.EmbedChars != 9 {
		t.Fatalf("EmbedChars = %d, want 9", plan.EmbedChars)
	}
	if plan.TotalChars != 12 {
		t.Fatalf("TotalChars = %d, want 12", plan.TotalChars)
	}
}
