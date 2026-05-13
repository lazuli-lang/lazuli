package notifications

import (
	"errors"
	"fmt"
	"net/url"
	"path"
	"strconv"
	"strings"
	"unicode/utf8"
)

const (
	DiscordWebhookMaxContentChars       = 2000
	DiscordWebhookMaxUsernameChars      = 80
	DiscordWebhookMaxEmbeds             = 10
	DiscordWebhookMaxEmbedTitleChars    = 256
	DiscordWebhookMaxEmbedDescChars     = 4096
	DiscordWebhookMaxEmbedFields        = 25
	DiscordWebhookMaxEmbedFieldName     = 256
	DiscordWebhookMaxEmbedFieldValue    = 1024
	DiscordWebhookMaxEmbedFooterChars   = 2048
	DiscordWebhookMaxEmbedAuthorChars   = 256
	DiscordWebhookMaxEmbedTotalChars    = 6000
	DiscordWebhookMaxThreadNameChars    = 100
	DiscordWebhookMaxMetadataValueChars = 256
)

var (
	ErrInvalidDiscordWebhookURL        = errors.New("notifications: invalid discord webhook url")
	ErrInvalidDiscordWebhookDescriptor = errors.New("notifications: invalid discord webhook descriptor")
	ErrInvalidDiscordWebhookPayload    = errors.New("notifications: invalid discord webhook payload")
)

// DiscordWebhookDescriptor describes a Discord webhook notification without
// binding to any HTTP client or provider runtime.
type DiscordWebhookDescriptor struct {
	WebhookURL string
	Username   string
	AvatarURL  string
	ThreadID   string
	ThreadName string
	Content    string
	Embeds     []DiscordWebhookEmbed
	Metadata   DiscordWebhookMetadata
}

// DiscordWebhookMetadata carries deterministic correlation data for adapters.
type DiscordWebhookMetadata struct {
	Notification string
	Tenant       string
	Idempotency  string
}

// DiscordWebhookEmbed is the provider-neutral subset of Discord embed payload
// fields useful for planning size and shape before a webhook send exists.
type DiscordWebhookEmbed struct {
	Title       string
	Description string
	URL         string
	Color       int
	Author      string
	Footer      string
	Fields      []DiscordWebhookEmbedField
}

// DiscordWebhookEmbedField is one embed field.
type DiscordWebhookEmbedField struct {
	Name   string
	Value  string
	Inline bool
}

// DiscordWebhookPayloadPlan is the deterministic dry-run output for a webhook
// descriptor. Payload is the JSON-shaped request body an adapter may marshal.
type DiscordWebhookPayloadPlan struct {
	Descriptor         DiscordWebhookDescriptor
	Payload            map[string]any
	RedactedWebhookURL string
	ContentChars       int
	EmbedChars         int
	TotalChars         int
}

// ValidateDiscordWebhookURL checks the stable Discord webhook URL shape.
func ValidateDiscordWebhookURL(raw string) error {
	_, err := normalizeDiscordWebhookURL(raw)
	return err
}

// RedactDiscordWebhookURL removes the secret token segment from a Discord
// webhook URL while preserving enough shape for logs and diagnostics.
func RedactDiscordWebhookURL(raw string) string {
	raw = strings.TrimSpace(raw)
	u, err := url.Parse(raw)
	if err != nil || u == nil {
		return "[redacted]"
	}
	parts := strings.Split(strings.Trim(u.EscapedPath(), "/"), "/")
	if len(parts) >= 4 && parts[0] == "api" && parts[1] == "webhooks" {
		parts[3] = "[redacted]"
		u.RawQuery = ""
		u.Fragment = ""
		u.User = nil
		return u.Scheme + "://" + u.Host + "/" + strings.Join(parts, "/")
	}
	u.RawQuery = ""
	u.Fragment = ""
	u.User = nil
	return u.String()
}

// ValidateDiscordWebhookDescriptor checks a descriptor without mutating it.
func ValidateDiscordWebhookDescriptor(desc DiscordWebhookDescriptor) error {
	_, err := NormalizeDiscordWebhookDescriptor(desc)
	return err
}

// NormalizeDiscordWebhookDescriptor returns a validated copy of desc.
func NormalizeDiscordWebhookDescriptor(desc DiscordWebhookDescriptor) (DiscordWebhookDescriptor, error) {
	clean := desc
	clean.WebhookURL = strings.TrimSpace(clean.WebhookURL)
	clean.Username = strings.TrimSpace(clean.Username)
	clean.AvatarURL = strings.TrimSpace(clean.AvatarURL)
	clean.ThreadID = strings.TrimSpace(clean.ThreadID)
	clean.ThreadName = strings.TrimSpace(clean.ThreadName)
	clean.Content = strings.TrimSpace(clean.Content)
	clean.Metadata = DiscordWebhookMetadata{
		Notification: strings.TrimSpace(desc.Metadata.Notification),
		Tenant:       strings.TrimSpace(desc.Metadata.Tenant),
		Idempotency:  strings.TrimSpace(desc.Metadata.Idempotency),
	}
	clean.Embeds = make([]DiscordWebhookEmbed, 0, len(desc.Embeds))
	for _, embed := range desc.Embeds {
		clean.Embeds = append(clean.Embeds, normalizeDiscordWebhookEmbed(embed))
	}

	var errs []error
	if normalizedURL, err := normalizeDiscordWebhookURL(clean.WebhookURL); err != nil {
		errs = append(errs, err)
	} else {
		clean.WebhookURL = normalizedURL
	}
	if clean.AvatarURL != "" {
		if err := validateDiscordHTTPSURL(clean.AvatarURL, "avatar_url"); err != nil {
			errs = append(errs, err)
		}
	}
	if clean.ThreadID != "" && !isDiscordSnowflake(clean.ThreadID) {
		errs = append(errs, fmt.Errorf("%w: thread_id must be a numeric snowflake", ErrInvalidDiscordWebhookDescriptor))
	}
	if discordWebhookRuneCount(clean.ThreadName) > DiscordWebhookMaxThreadNameChars {
		errs = append(errs, fmt.Errorf("%w: thread_name exceeds %d characters", ErrInvalidDiscordWebhookDescriptor, DiscordWebhookMaxThreadNameChars))
	}
	if clean.ThreadID != "" && clean.ThreadName != "" {
		errs = append(errs, fmt.Errorf("%w: thread_id and thread_name are mutually exclusive", ErrInvalidDiscordWebhookDescriptor))
	}
	if discordWebhookRuneCount(clean.Username) > DiscordWebhookMaxUsernameChars {
		errs = append(errs, fmt.Errorf("%w: username exceeds %d characters", ErrInvalidDiscordWebhookDescriptor, DiscordWebhookMaxUsernameChars))
	}
	if clean.Metadata.Idempotency != "" && discordWebhookRuneCount(clean.Metadata.Idempotency) > DiscordWebhookMaxMetadataValueChars {
		errs = append(errs, fmt.Errorf("%w: idempotency exceeds %d characters", ErrInvalidDiscordWebhookDescriptor, DiscordWebhookMaxMetadataValueChars))
	}
	if len(clean.Embeds) > DiscordWebhookMaxEmbeds {
		errs = append(errs, fmt.Errorf("%w: embeds exceeds %d", ErrInvalidDiscordWebhookPayload, DiscordWebhookMaxEmbeds))
	}
	if clean.Content == "" && len(clean.Embeds) == 0 {
		errs = append(errs, fmt.Errorf("%w: content or embeds required", ErrInvalidDiscordWebhookPayload))
	}
	errs = append(errs, validateDiscordWebhookSizes(clean)...)

	if err := errors.Join(errs...); err != nil {
		return DiscordWebhookDescriptor{}, err
	}
	return clean, nil
}

// PlanDiscordWebhookPayload validates desc and returns the dry-run request
// shape an adapter can marshal later. It does not perform HTTP.
func PlanDiscordWebhookPayload(desc DiscordWebhookDescriptor) (DiscordWebhookPayloadPlan, error) {
	clean, err := NormalizeDiscordWebhookDescriptor(desc)
	if err != nil {
		return DiscordWebhookPayloadPlan{}, err
	}

	payload := map[string]any{}
	if clean.Content != "" {
		payload["content"] = clean.Content
	}
	if clean.Username != "" {
		payload["username"] = clean.Username
	}
	if clean.AvatarURL != "" {
		payload["avatar_url"] = clean.AvatarURL
	}
	if clean.ThreadName != "" {
		payload["thread_name"] = clean.ThreadName
	}
	if len(clean.Embeds) > 0 {
		payload["embeds"] = discordWebhookEmbedPayloads(clean.Embeds)
	}

	contentChars := discordWebhookRuneCount(clean.Content)
	embedChars := discordWebhookEmbedChars(clean.Embeds)
	return DiscordWebhookPayloadPlan{
		Descriptor:         clean,
		Payload:            payload,
		RedactedWebhookURL: RedactDiscordWebhookURL(clean.WebhookURL),
		ContentChars:       contentChars,
		EmbedChars:         embedChars,
		TotalChars:         contentChars + embedChars,
	}, nil
}

func normalizeDiscordWebhookURL(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", fmt.Errorf("%w: empty url", ErrInvalidDiscordWebhookURL)
	}
	u, err := url.Parse(raw)
	if err != nil || u == nil {
		return "", fmt.Errorf("%w: parse url", ErrInvalidDiscordWebhookURL)
	}
	if u.Scheme != "https" {
		return "", fmt.Errorf("%w: scheme must be https", ErrInvalidDiscordWebhookURL)
	}
	host := strings.ToLower(u.Hostname())
	if host != "discord.com" && host != "discordapp.com" {
		return "", fmt.Errorf("%w: host must be discord.com or discordapp.com", ErrInvalidDiscordWebhookURL)
	}
	if u.User != nil || u.RawQuery != "" || u.Fragment != "" {
		return "", fmt.Errorf("%w: url must not include credentials, query, or fragment", ErrInvalidDiscordWebhookURL)
	}
	parts := strings.Split(strings.Trim(u.EscapedPath(), "/"), "/")
	if len(parts) != 4 || parts[0] != "api" || parts[1] != "webhooks" {
		return "", fmt.Errorf("%w: path must be /api/webhooks/{id}/{token}", ErrInvalidDiscordWebhookURL)
	}
	id, err := url.PathUnescape(parts[2])
	if err != nil || !isDiscordSnowflake(id) {
		return "", fmt.Errorf("%w: webhook id must be a numeric snowflake", ErrInvalidDiscordWebhookURL)
	}
	token, err := url.PathUnescape(parts[3])
	if err != nil || strings.TrimSpace(token) == "" || strings.Contains(token, "/") {
		return "", fmt.Errorf("%w: webhook token required", ErrInvalidDiscordWebhookURL)
	}
	u.Host = host
	u.Path = path.Clean("/" + strings.Join(parts, "/"))
	u.RawPath = ""
	return u.String(), nil
}

func normalizeDiscordWebhookEmbed(embed DiscordWebhookEmbed) DiscordWebhookEmbed {
	clean := DiscordWebhookEmbed{
		Title:       strings.TrimSpace(embed.Title),
		Description: strings.TrimSpace(embed.Description),
		URL:         strings.TrimSpace(embed.URL),
		Color:       embed.Color,
		Author:      strings.TrimSpace(embed.Author),
		Footer:      strings.TrimSpace(embed.Footer),
		Fields:      make([]DiscordWebhookEmbedField, 0, len(embed.Fields)),
	}
	for _, field := range embed.Fields {
		clean.Fields = append(clean.Fields, DiscordWebhookEmbedField{
			Name:   strings.TrimSpace(field.Name),
			Value:  strings.TrimSpace(field.Value),
			Inline: field.Inline,
		})
	}
	return clean
}

func validateDiscordWebhookSizes(desc DiscordWebhookDescriptor) []error {
	var errs []error
	if discordWebhookRuneCount(desc.Content) > DiscordWebhookMaxContentChars {
		errs = append(errs, fmt.Errorf("%w: content exceeds %d characters", ErrInvalidDiscordWebhookPayload, DiscordWebhookMaxContentChars))
	}
	for i, embed := range desc.Embeds {
		if discordWebhookRuneCount(embed.Title) > DiscordWebhookMaxEmbedTitleChars {
			errs = append(errs, fmt.Errorf("%w: embed[%d].title exceeds %d characters", ErrInvalidDiscordWebhookPayload, i, DiscordWebhookMaxEmbedTitleChars))
		}
		if discordWebhookRuneCount(embed.Description) > DiscordWebhookMaxEmbedDescChars {
			errs = append(errs, fmt.Errorf("%w: embed[%d].description exceeds %d characters", ErrInvalidDiscordWebhookPayload, i, DiscordWebhookMaxEmbedDescChars))
		}
		if embed.URL != "" {
			if err := validateDiscordHTTPSURL(embed.URL, fmt.Sprintf("embed[%d].url", i)); err != nil {
				errs = append(errs, err)
			}
		}
		if embed.Color < 0 || embed.Color > 0xFFFFFF {
			errs = append(errs, fmt.Errorf("%w: embed[%d].color must be between 0 and 16777215", ErrInvalidDiscordWebhookPayload, i))
		}
		if discordWebhookRuneCount(embed.Author) > DiscordWebhookMaxEmbedAuthorChars {
			errs = append(errs, fmt.Errorf("%w: embed[%d].author exceeds %d characters", ErrInvalidDiscordWebhookPayload, i, DiscordWebhookMaxEmbedAuthorChars))
		}
		if discordWebhookRuneCount(embed.Footer) > DiscordWebhookMaxEmbedFooterChars {
			errs = append(errs, fmt.Errorf("%w: embed[%d].footer exceeds %d characters", ErrInvalidDiscordWebhookPayload, i, DiscordWebhookMaxEmbedFooterChars))
		}
		if len(embed.Fields) > DiscordWebhookMaxEmbedFields {
			errs = append(errs, fmt.Errorf("%w: embed[%d].fields exceeds %d", ErrInvalidDiscordWebhookPayload, i, DiscordWebhookMaxEmbedFields))
		}
		for j, field := range embed.Fields {
			if field.Name == "" || field.Value == "" {
				errs = append(errs, fmt.Errorf("%w: embed[%d].fields[%d] name and value required", ErrInvalidDiscordWebhookPayload, i, j))
			}
			if discordWebhookRuneCount(field.Name) > DiscordWebhookMaxEmbedFieldName {
				errs = append(errs, fmt.Errorf("%w: embed[%d].fields[%d].name exceeds %d characters", ErrInvalidDiscordWebhookPayload, i, j, DiscordWebhookMaxEmbedFieldName))
			}
			if discordWebhookRuneCount(field.Value) > DiscordWebhookMaxEmbedFieldValue {
				errs = append(errs, fmt.Errorf("%w: embed[%d].fields[%d].value exceeds %d characters", ErrInvalidDiscordWebhookPayload, i, j, DiscordWebhookMaxEmbedFieldValue))
			}
		}
	}
	if embedChars := discordWebhookEmbedChars(desc.Embeds); embedChars > DiscordWebhookMaxEmbedTotalChars {
		errs = append(errs, fmt.Errorf("%w: embeds total exceeds %d characters", ErrInvalidDiscordWebhookPayload, DiscordWebhookMaxEmbedTotalChars))
	}
	return errs
}

func validateDiscordHTTPSURL(raw, field string) error {
	u, err := url.Parse(raw)
	if err != nil || u == nil || u.Scheme != "https" || u.Host == "" || u.User != nil || u.Fragment != "" {
		return fmt.Errorf("%w: %s must be an https url without credentials or fragment", ErrInvalidDiscordWebhookDescriptor, field)
	}
	return nil
}

func discordWebhookEmbedChars(embeds []DiscordWebhookEmbed) int {
	total := 0
	for _, embed := range embeds {
		total += discordWebhookRuneCount(embed.Title)
		total += discordWebhookRuneCount(embed.Description)
		total += discordWebhookRuneCount(embed.Author)
		total += discordWebhookRuneCount(embed.Footer)
		for _, field := range embed.Fields {
			total += discordWebhookRuneCount(field.Name)
			total += discordWebhookRuneCount(field.Value)
		}
	}
	return total
}

func discordWebhookEmbedPayloads(embeds []DiscordWebhookEmbed) []map[string]any {
	out := make([]map[string]any, 0, len(embeds))
	for _, embed := range embeds {
		item := map[string]any{}
		if embed.Title != "" {
			item["title"] = embed.Title
		}
		if embed.Description != "" {
			item["description"] = embed.Description
		}
		if embed.URL != "" {
			item["url"] = embed.URL
		}
		if embed.Color != 0 {
			item["color"] = embed.Color
		}
		if embed.Author != "" {
			item["author"] = map[string]any{"name": embed.Author}
		}
		if embed.Footer != "" {
			item["footer"] = map[string]any{"text": embed.Footer}
		}
		if len(embed.Fields) > 0 {
			fields := make([]map[string]any, 0, len(embed.Fields))
			for _, field := range embed.Fields {
				fields = append(fields, map[string]any{
					"name":   field.Name,
					"value":  field.Value,
					"inline": field.Inline,
				})
			}
			item["fields"] = fields
		}
		out = append(out, item)
	}
	return out
}

func isDiscordSnowflake(s string) bool {
	if s == "" {
		return false
	}
	if len(s) > 20 {
		return false
	}
	if _, err := strconv.ParseUint(s, 10, 64); err != nil {
		return false
	}
	for _, r := range s {
		if r < '0' || r > '9' {
			return false
		}
	}
	return true
}

func discordWebhookRuneCount(s string) int {
	return utf8.RuneCountInString(s)
}
