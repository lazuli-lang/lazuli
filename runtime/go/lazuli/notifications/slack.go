package notifications

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"strings"
	"unicode/utf8"
)

const (
	// SlackWebhookMaxTextRunes is Slack's documented message text ceiling.
	SlackWebhookMaxTextRunes = 40000
	// SlackWebhookMaxBlocks is Slack's block count ceiling for messages.
	SlackWebhookMaxBlocks = 50
	// SlackWebhookMaxBlockTextRunes is the common block text object ceiling.
	SlackWebhookMaxBlockTextRunes = 3000
)

var (
	// ErrSlackWebhookInvalid is returned when Slack webhook metadata or payload
	// cannot be normalized into a sendable plan.
	ErrSlackWebhookInvalid = errors.New("notifications: invalid slack webhook")
	// ErrSlackWebhookURLInvalid is returned when a webhook URL does not match
	// Slack's incoming webhook URL shape.
	ErrSlackWebhookURLInvalid = errors.New("notifications: invalid slack webhook url")
	// ErrSlackWebhookPayloadInvalid is returned when Slack payload content is
	// empty or exceeds deterministic planning limits.
	ErrSlackWebhookPayloadInvalid = errors.New("notifications: invalid slack webhook payload")
)

// SlackWebhookDescriptor is provider-neutral Slack incoming webhook metadata.
// WebhookURL is secret-bearing and should be logged with RedactSlackWebhookURL.
type SlackWebhookDescriptor struct {
	WebhookURL string
	Channel    string
	Username   string
	IconEmoji  string
	IconURL    string
}

// SlackWebhookBlock is the minimal normalized block shape needed for payload
// size planning. Extra carries adapter-specific block fields without coupling
// this package to a Slack SDK.
type SlackWebhookBlock struct {
	Type  string
	Text  string
	Extra map[string]any
}

// SlackWebhookMessage is the provider-neutral input for dry-run payload
// planning. IdempotencyKey is metadata for callers/stores; it is not emitted
// into the Slack JSON payload.
type SlackWebhookMessage struct {
	Text           string
	Blocks         []SlackWebhookBlock
	IdempotencyKey string
}

// SlackWebhookPlan is the deterministic result of planning a Slack webhook
// delivery. PayloadJSON is ready for an adapter to write to an HTTP request.
type SlackWebhookPlan struct {
	Descriptor     SlackWebhookDescriptor
	RedactedURL    string
	Payload        map[string]any
	PayloadJSON    []byte
	TextRunes      int
	BlockTextRunes int
	BlockCount     int
	IdempotencyKey string
}

// NormalizeSlackWebhookDescriptor trims Slack webhook metadata and returns a
// copy. It does not validate required fields.
func NormalizeSlackWebhookDescriptor(descriptor SlackWebhookDescriptor) SlackWebhookDescriptor {
	return SlackWebhookDescriptor{
		WebhookURL: strings.TrimSpace(descriptor.WebhookURL),
		Channel:    strings.TrimSpace(descriptor.Channel),
		Username:   strings.TrimSpace(descriptor.Username),
		IconEmoji:  strings.TrimSpace(descriptor.IconEmoji),
		IconURL:    strings.TrimSpace(descriptor.IconURL),
	}
}

// ValidateSlackWebhookDescriptor checks incoming webhook metadata without
// mutating the descriptor.
func ValidateSlackWebhookDescriptor(descriptor SlackWebhookDescriptor) error {
	normalized := NormalizeSlackWebhookDescriptor(descriptor)
	var errs []error
	if !IsSlackWebhookURL(normalized.WebhookURL) {
		errs = append(errs, ErrSlackWebhookURLInvalid)
	}
	if normalized.IconEmoji != "" && normalized.IconURL != "" {
		errs = append(errs, fmt.Errorf("%w: icon_emoji and icon_url are mutually exclusive", ErrSlackWebhookInvalid))
	}
	if normalized.IconEmoji != "" && !strings.HasPrefix(normalized.IconEmoji, ":") {
		errs = append(errs, fmt.Errorf("%w: icon_emoji must use :name: shape", ErrSlackWebhookInvalid))
	}
	if normalized.IconURL != "" && !isHTTPSURL(normalized.IconURL) {
		errs = append(errs, fmt.Errorf("%w: icon_url must be https", ErrSlackWebhookInvalid))
	}
	return errors.Join(errs...)
}

// IsSlackWebhookURL reports whether raw matches Slack incoming webhook URL
// shape without exposing the secret path segment.
func IsSlackWebhookURL(raw string) bool {
	u, ok := parseSlackWebhookURL(raw)
	return ok && u != nil
}

// RedactSlackWebhookURL returns a log-safe representation of a Slack incoming
// webhook URL. Invalid URLs are replaced entirely.
func RedactSlackWebhookURL(raw string) string {
	u, ok := parseSlackWebhookURL(raw)
	if !ok {
		return "[redacted]"
	}
	return u.Scheme + "://" + u.Host + "/services/..."
}

// NormalizeSlackWebhookMessage trims top-level message fields and block text
// while preserving Extra maps by shallow copy.
func NormalizeSlackWebhookMessage(message SlackWebhookMessage) SlackWebhookMessage {
	blocks := make([]SlackWebhookBlock, 0, len(message.Blocks))
	for _, block := range message.Blocks {
		blocks = append(blocks, SlackWebhookBlock{
			Type:  strings.TrimSpace(block.Type),
			Text:  strings.TrimSpace(block.Text),
			Extra: cloneSlackWebhookExtra(block.Extra),
		})
	}
	return SlackWebhookMessage{
		Text:           strings.TrimSpace(message.Text),
		Blocks:         blocks,
		IdempotencyKey: strings.TrimSpace(message.IdempotencyKey),
	}
}

// ValidateSlackWebhookMessage checks Slack payload content without mutating the
// message.
func ValidateSlackWebhookMessage(message SlackWebhookMessage) error {
	normalized := NormalizeSlackWebhookMessage(message)
	var errs []error
	textRunes := utf8.RuneCountInString(normalized.Text)
	if normalized.Text == "" && len(normalized.Blocks) == 0 {
		errs = append(errs, fmt.Errorf("%w: text or blocks required", ErrSlackWebhookPayloadInvalid))
	}
	if textRunes > SlackWebhookMaxTextRunes {
		errs = append(errs, fmt.Errorf("%w: text exceeds %d runes", ErrSlackWebhookPayloadInvalid, SlackWebhookMaxTextRunes))
	}
	if len(normalized.Blocks) > SlackWebhookMaxBlocks {
		errs = append(errs, fmt.Errorf("%w: blocks exceed %d", ErrSlackWebhookPayloadInvalid, SlackWebhookMaxBlocks))
	}
	for i, block := range normalized.Blocks {
		if block.Type == "" {
			errs = append(errs, fmt.Errorf("%w: block %d type required", ErrSlackWebhookPayloadInvalid, i))
		}
		blockTextRunes := utf8.RuneCountInString(block.Text)
		if blockTextRunes > SlackWebhookMaxBlockTextRunes {
			errs = append(errs, fmt.Errorf("%w: block %d text exceeds %d runes", ErrSlackWebhookPayloadInvalid, i, SlackWebhookMaxBlockTextRunes))
		}
	}
	return errors.Join(errs...)
}

// PlanSlackWebhookPayload validates and normalizes Slack webhook metadata and
// content, then returns the JSON payload an adapter would send. It never sends
// HTTP and never includes the idempotency key in the Slack payload.
func PlanSlackWebhookPayload(
	descriptor SlackWebhookDescriptor,
	message SlackWebhookMessage,
) (SlackWebhookPlan, error) {
	descriptor = NormalizeSlackWebhookDescriptor(descriptor)
	message = NormalizeSlackWebhookMessage(message)

	if err := errors.Join(
		ValidateSlackWebhookDescriptor(descriptor),
		ValidateSlackWebhookMessage(message),
	); err != nil {
		return SlackWebhookPlan{}, err
	}

	payload := map[string]any{}
	if message.Text != "" {
		payload["text"] = message.Text
	}
	if descriptor.Channel != "" {
		payload["channel"] = descriptor.Channel
	}
	if descriptor.Username != "" {
		payload["username"] = descriptor.Username
	}
	if descriptor.IconEmoji != "" {
		payload["icon_emoji"] = descriptor.IconEmoji
	}
	if descriptor.IconURL != "" {
		payload["icon_url"] = descriptor.IconURL
	}
	if len(message.Blocks) > 0 {
		payload["blocks"] = slackWebhookBlocksPayload(message.Blocks)
	}

	body, err := json.Marshal(payload)
	if err != nil {
		return SlackWebhookPlan{}, fmt.Errorf("%w: marshal payload: %v", ErrSlackWebhookPayloadInvalid, err)
	}

	textRunes, blockTextRunes := slackWebhookRunePlan(message)
	return SlackWebhookPlan{
		Descriptor:     descriptor,
		RedactedURL:    RedactSlackWebhookURL(descriptor.WebhookURL),
		Payload:        payload,
		PayloadJSON:    body,
		TextRunes:      textRunes,
		BlockTextRunes: blockTextRunes,
		BlockCount:     len(message.Blocks),
		IdempotencyKey: message.IdempotencyKey,
	}, nil
}

func parseSlackWebhookURL(raw string) (*url.URL, bool) {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil || u == nil {
		return nil, false
	}
	if u.Scheme != "https" || u.User != nil || u.RawQuery != "" || u.Fragment != "" {
		return nil, false
	}
	if u.Host != "hooks.slack.com" && u.Host != "hooks.slack-gov.com" {
		return nil, false
	}
	parts := strings.Split(strings.Trim(u.EscapedPath(), "/"), "/")
	if len(parts) != 4 || parts[0] != "services" {
		return nil, false
	}
	for _, part := range parts[1:] {
		if part == "" {
			return nil, false
		}
	}
	return u, true
}

func isHTTPSURL(raw string) bool {
	u, err := url.Parse(strings.TrimSpace(raw))
	return err == nil && u != nil && u.Scheme == "https" && u.Host != ""
}

func cloneSlackWebhookExtra(extra map[string]any) map[string]any {
	if extra == nil {
		return nil
	}
	out := make(map[string]any, len(extra))
	for key, value := range extra {
		out[key] = value
	}
	return out
}

func slackWebhookBlocksPayload(blocks []SlackWebhookBlock) []map[string]any {
	out := make([]map[string]any, 0, len(blocks))
	for _, block := range blocks {
		item := cloneSlackWebhookExtra(block.Extra)
		if item == nil {
			item = map[string]any{}
		}
		item["type"] = block.Type
		if block.Text != "" {
			item["text"] = map[string]any{
				"type": "mrkdwn",
				"text": block.Text,
			}
		}
		out = append(out, item)
	}
	return out
}

func slackWebhookRunePlan(message SlackWebhookMessage) (int, int) {
	blockTextRunes := 0
	for _, block := range message.Blocks {
		blockTextRunes += utf8.RuneCountInString(block.Text)
	}
	return utf8.RuneCountInString(message.Text), blockTextRunes
}
