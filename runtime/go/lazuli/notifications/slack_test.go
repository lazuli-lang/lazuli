package notifications

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

func TestSlackWebhookURLValidationAndRedaction(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name         string
		raw          string
		wantValid    bool
		wantRedacted string
	}{
		{
			name:         "standard Slack URL",
			raw:          " https://hooks.slack.com/services/T000/B000/secret ",
			wantValid:    true,
			wantRedacted: "https://hooks.slack.com/services/...",
		},
		{
			name:         "gov Slack URL",
			raw:          "https://hooks.slack-gov.com/services/T000/B000/secret",
			wantValid:    true,
			wantRedacted: "https://hooks.slack-gov.com/services/...",
		},
		{
			name:         "http rejected",
			raw:          "http://hooks.slack.com/services/T000/B000/secret",
			wantRedacted: "[redacted]",
		},
		{
			name:         "wrong host rejected",
			raw:          "https://example.com/services/T000/B000/secret",
			wantRedacted: "[redacted]",
		},
		{
			name:         "query rejected",
			raw:          "https://hooks.slack.com/services/T000/B000/secret?debug=true",
			wantRedacted: "[redacted]",
		},
		{
			name:         "short path rejected",
			raw:          "https://hooks.slack.com/services/T000/B000",
			wantRedacted: "[redacted]",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := IsSlackWebhookURL(tt.raw); got != tt.wantValid {
				t.Fatalf("IsSlackWebhookURL() = %v, want %v", got, tt.wantValid)
			}
			if got := RedactSlackWebhookURL(tt.raw); got != tt.wantRedacted {
				t.Fatalf("RedactSlackWebhookURL() = %q, want %q", got, tt.wantRedacted)
			}
		})
	}
}

func TestValidateSlackWebhookDescriptor(t *testing.T) {
	t.Parallel()

	valid := SlackWebhookDescriptor{
		WebhookURL: "https://hooks.slack.com/services/T000/B000/secret",
		Channel:    " #ops ",
		Username:   " Lazuli ",
		IconEmoji:  ":bell:",
	}
	normalized := NormalizeSlackWebhookDescriptor(valid)
	if normalized.Channel != "#ops" || normalized.Username != "Lazuli" {
		t.Fatalf("NormalizeSlackWebhookDescriptor() = %#v, want trimmed channel and username", normalized)
	}
	if err := ValidateSlackWebhookDescriptor(valid); err != nil {
		t.Fatalf("ValidateSlackWebhookDescriptor(valid) error = %v", err)
	}

	err := ValidateSlackWebhookDescriptor(SlackWebhookDescriptor{
		WebhookURL: "https://hooks.slack.com/services/T000/B000/secret",
		IconEmoji:  "bell",
		IconURL:    "http://example.com/icon.png",
	})
	if !errors.Is(err, ErrSlackWebhookInvalid) {
		t.Fatalf("ValidateSlackWebhookDescriptor() error = %v, want ErrSlackWebhookInvalid", err)
	}
	for _, want := range []string{
		"icon_emoji and icon_url are mutually exclusive",
		"icon_emoji must use :name: shape",
		"icon_url must be https",
	} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("ValidateSlackWebhookDescriptor() error = %q, want fragment %q", err.Error(), want)
		}
	}

	if err := ValidateSlackWebhookDescriptor(SlackWebhookDescriptor{}); !errors.Is(err, ErrSlackWebhookURLInvalid) {
		t.Fatalf("ValidateSlackWebhookDescriptor(empty) error = %v, want ErrSlackWebhookURLInvalid", err)
	}
}

func TestPlanSlackWebhookPayload(t *testing.T) {
	t.Parallel()

	extra := map[string]any{"block_id": "incident-1"}
	message := SlackWebhookMessage{
		Text:           " Incident opened ",
		IdempotencyKey: " req-1 ",
		Blocks: []SlackWebhookBlock{
			{Type: " section ", Text: " Check runbook ", Extra: extra},
		},
	}
	plan, err := PlanSlackWebhookPayload(SlackWebhookDescriptor{
		WebhookURL: " https://hooks.slack.com/services/T000/B000/secret ",
		Channel:    " #alerts ",
		Username:   " Lazuli ",
		IconURL:    " https://example.com/icon.png ",
	}, message)
	if err != nil {
		t.Fatalf("PlanSlackWebhookPayload() error = %v", err)
	}

	if plan.RedactedURL != "https://hooks.slack.com/services/..." {
		t.Fatalf("RedactedURL = %q, want Slack redaction", plan.RedactedURL)
	}
	if plan.Descriptor.WebhookURL != "https://hooks.slack.com/services/T000/B000/secret" {
		t.Fatalf("Descriptor.WebhookURL = %q, want trimmed URL", plan.Descriptor.WebhookURL)
	}
	if plan.IdempotencyKey != "req-1" {
		t.Fatalf("IdempotencyKey = %q, want req-1", plan.IdempotencyKey)
	}
	if plan.TextRunes != len("Incident opened") {
		t.Fatalf("TextRunes = %d, want %d", plan.TextRunes, len("Incident opened"))
	}
	if plan.BlockTextRunes != len("Check runbook") || plan.BlockCount != 1 {
		t.Fatalf("block plan = (%d, %d), want (13, 1)", plan.BlockTextRunes, plan.BlockCount)
	}
	if _, ok := plan.Payload["idempotency_key"]; ok {
		t.Fatalf("Payload includes idempotency_key: %#v", plan.Payload)
	}

	var payload map[string]any
	if err := json.Unmarshal(plan.PayloadJSON, &payload); err != nil {
		t.Fatalf("unmarshal payload JSON: %v", err)
	}
	if got := payload["text"]; got != "Incident opened" {
		t.Fatalf("payload.text = %v, want Incident opened", got)
	}
	if got := payload["channel"]; got != "#alerts" {
		t.Fatalf("payload.channel = %v, want #alerts", got)
	}
	if got := payload["username"]; got != "Lazuli" {
		t.Fatalf("payload.username = %v, want Lazuli", got)
	}
	if got := payload["icon_url"]; got != "https://example.com/icon.png" {
		t.Fatalf("payload.icon_url = %v, want trimmed icon URL", got)
	}
	blocks, ok := payload["blocks"].([]any)
	if !ok || len(blocks) != 1 {
		t.Fatalf("payload.blocks = %#v, want one block", payload["blocks"])
	}
	block, ok := blocks[0].(map[string]any)
	if !ok {
		t.Fatalf("payload.blocks[0] = %T, want object", blocks[0])
	}
	if got := block["type"]; got != "section" {
		t.Fatalf("block.type = %v, want section", got)
	}
	if got := block["block_id"]; got != "incident-1" {
		t.Fatalf("block.block_id = %v, want incident-1", got)
	}
	text, ok := block["text"].(map[string]any)
	if !ok {
		t.Fatalf("block.text = %T, want object", block["text"])
	}
	if got := text["text"]; got != "Check runbook" {
		t.Fatalf("block.text.text = %v, want Check runbook", got)
	}

	extra["block_id"] = "mutated"
	if got := block["block_id"]; got != "incident-1" {
		t.Fatalf("plan retained mutable extra map = %v, want incident-1", got)
	}
}

func TestValidateSlackWebhookMessage(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		message SlackWebhookMessage
	}{
		{
			name:    "empty content",
			message: SlackWebhookMessage{},
		},
		{
			name: "text too long",
			message: SlackWebhookMessage{
				Text: strings.Repeat("a", SlackWebhookMaxTextRunes+1),
			},
		},
		{
			name: "too many blocks",
			message: SlackWebhookMessage{
				Blocks: make([]SlackWebhookBlock, SlackWebhookMaxBlocks+1),
			},
		},
		{
			name: "block text too long",
			message: SlackWebhookMessage{
				Blocks: []SlackWebhookBlock{{
					Type: "section",
					Text: strings.Repeat("a", SlackWebhookMaxBlockTextRunes+1),
				}},
			},
		},
		{
			name: "block type missing",
			message: SlackWebhookMessage{
				Blocks: []SlackWebhookBlock{{Text: "hello"}},
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if err := ValidateSlackWebhookMessage(tt.message); !errors.Is(err, ErrSlackWebhookPayloadInvalid) {
				t.Fatalf("ValidateSlackWebhookMessage() error = %v, want ErrSlackWebhookPayloadInvalid", err)
			}
		})
	}

	if err := ValidateSlackWebhookMessage(SlackWebhookMessage{
		Blocks: []SlackWebhookBlock{{Type: "section", Text: "hello"}},
	}); err != nil {
		t.Fatalf("ValidateSlackWebhookMessage(valid blocks only) error = %v", err)
	}
}
