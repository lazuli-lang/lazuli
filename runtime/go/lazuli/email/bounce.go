package email

import (
	"encoding/json"
	"errors"
	"fmt"
	netmail "net/mail"
	"strconv"
	"strings"
	"time"
)

// BounceType is the provider-neutral durability of a bounce.
type BounceType string

const (
	BounceTypeUnknown BounceType = "unknown"
	BounceTypeHard    BounceType = "hard"
	BounceTypeSoft    BounceType = "soft"
)

// BounceReason is the provider-neutral classification for a bounce cause.
type BounceReason string

const (
	BounceReasonUnknown          BounceReason = "unknown"
	BounceReasonInvalidRecipient BounceReason = "invalid_recipient"
	BounceReasonDNS              BounceReason = "dns"
	BounceReasonMailboxFull      BounceReason = "mailbox_full"
	BounceReasonMessageTooLarge  BounceReason = "message_too_large"
	BounceReasonPolicy           BounceReason = "policy"
	BounceReasonContent          BounceReason = "content"
	BounceReasonSpam             BounceReason = "spam"
	BounceReasonBlocklist        BounceReason = "blocklist"
	BounceReasonRateLimited      BounceReason = "rate_limited"
	BounceReasonTemporary        BounceReason = "temporary_failure"
	BounceReasonPermanent        BounceReason = "permanent_failure"
)

var (
	// ErrInvalidBounceEvent is wrapped by malformed normalized bounce events.
	ErrInvalidBounceEvent = errors.New("email: invalid bounce event")
	// ErrInvalidBouncePayload is wrapped by malformed provider webhook payload maps.
	ErrInvalidBouncePayload = errors.New("email: invalid bounce payload")
)

// BounceEvent is a normalized email bounce event that provider adapters can
// emit from webhook payloads.
type BounceEvent struct {
	Provider string
	ID       string

	MessageID string
	Recipient string

	Type           BounceType
	Reason         BounceReason
	ProviderReason string
	Status         string
	Diagnostic     string

	OccurredAt time.Time
}

// SuppressionDecision records whether a bounce should suppress future sends to
// the recipient and the normalized reason for that choice.
type SuppressionDecision struct {
	Suppress bool
	Reason   string
}

// NormalizeBounceEvent trims provider text, normalizes bounce type/reason, and
// validates Recipient as a mailbox address.
func NormalizeBounceEvent(event BounceEvent) (BounceEvent, error) {
	event.Provider = strings.TrimSpace(event.Provider)
	event.ID = strings.TrimSpace(event.ID)
	event.MessageID = strings.TrimSpace(event.MessageID)
	event.ProviderReason = strings.TrimSpace(event.ProviderReason)
	event.Status = strings.TrimSpace(event.Status)
	event.Diagnostic = strings.TrimSpace(event.Diagnostic)
	event.Type = NormalizeBounceType(string(event.Type))

	recipient, err := normalizeBounceRecipient(event.Recipient)
	if err != nil {
		return BounceEvent{}, bounceEventError("recipient", err)
	}
	event.Recipient = recipient

	if event.Reason == "" || event.Reason == BounceReasonUnknown {
		event.Reason = ClassifyBounceReason(event.ProviderReason, event.Status, event.Diagnostic)
	} else {
		event.Reason = normalizeKnownBounceReason(event.Reason)
	}

	return event, nil
}

// NormalizeBounceType maps provider labels onto the hard/soft/unknown catalog.
func NormalizeBounceType(raw string) BounceType {
	switch normalizeBounceToken(raw) {
	case "hard", "permanent", "permanent_failure", "failed", "failure":
		return BounceTypeHard
	case "soft", "temporary", "temporary_failure", "transient", "deferred", "delayed":
		return BounceTypeSoft
	default:
		return BounceTypeUnknown
	}
}

// ClassifyBounceReason maps provider reason text, SMTP status, and diagnostic
// text to a provider-neutral reason catalog.
func ClassifyBounceReason(reason, status, diagnostic string) BounceReason {
	text := strings.ToLower(strings.Join([]string{reason, status, diagnostic}, " "))
	status = strings.TrimSpace(status)

	switch {
	case hasBounceStatusPrefix(status, "5.1.") ||
		containsAnyBounceText(text, "invalid recipient", "recipient invalid", "user unknown", "unknown user", "no such user", "address not found", "mailbox unavailable", "recipient address rejected", "does not exist"):
		return BounceReasonInvalidRecipient
	case containsAnyBounceText(text, "domain not found", "host not found", "no mx", "dns", "unrouteable", "unroutable"):
		return BounceReasonDNS
	case hasBounceStatusPrefix(status, "4.2.2", "5.2.2") ||
		containsAnyBounceText(text, "mailbox full", "mailbox is full", "quota", "over quota", "exceeded storage"):
		return BounceReasonMailboxFull
	case hasBounceStatusPrefix(status, "4.3.2", "5.3.4") ||
		containsAnyBounceText(text, "message too large", "exceeds size", "maximum message size", "size limit"):
		return BounceReasonMessageTooLarge
	case containsAnyBounceText(text, "rate limit", "rate-limit", "throttle", "too many messages", "too many requests", "try again later", "greylist", "graylist"):
		return BounceReasonRateLimited
	case containsAnyBounceText(text, "blocklist", "blacklist", "blocked", "listed at", "reputation"):
		return BounceReasonBlocklist
	case containsAnyBounceText(text, "spam", "abuse", "complaint"):
		return BounceReasonSpam
	case containsAnyBounceText(text, "content", "malware", "virus", "attachment rejected"):
		return BounceReasonContent
	case containsAnyBounceText(text, "policy", "prohibited", "forbidden", "unauthorized", "dmarc", "spf", "dkim"):
		return BounceReasonPolicy
	case strings.HasPrefix(status, "4."):
		return BounceReasonTemporary
	case strings.HasPrefix(status, "5."):
		return BounceReasonPermanent
	default:
		return BounceReasonUnknown
	}
}

// DecideBounceSuppression returns a conservative suppression decision for a
// normalized or raw BounceEvent.
func DecideBounceSuppression(event BounceEvent) SuppressionDecision {
	event, err := NormalizeBounceEvent(event)
	if err != nil {
		return SuppressionDecision{Reason: "invalid_event"}
	}

	switch event.Type {
	case BounceTypeHard:
		return SuppressionDecision{Suppress: true, Reason: "hard_bounce"}
	case BounceTypeSoft:
		return SuppressionDecision{Reason: "soft_bounce"}
	}

	switch event.Reason {
	case BounceReasonInvalidRecipient, BounceReasonDNS, BounceReasonPermanent:
		return SuppressionDecision{Suppress: true, Reason: string(event.Reason)}
	case BounceReasonMailboxFull, BounceReasonMessageTooLarge, BounceReasonRateLimited, BounceReasonTemporary:
		return SuppressionDecision{Reason: string(event.Reason)}
	}

	if strings.HasPrefix(strings.TrimSpace(event.Status), "5.") {
		return SuppressionDecision{Suppress: true, Reason: "permanent_smtp_status"}
	}
	return SuppressionDecision{Reason: "unknown"}
}

// ShouldSuppressBounce reports whether future sends to the event recipient
// should be suppressed.
func ShouldSuppressBounce(event BounceEvent) bool {
	return DecideBounceSuppression(event).Suppress
}

// ParseBounceEventPayload parses a provider-neutral webhook payload map into a
// normalized BounceEvent. Provider adapters can pre-map their raw webhook shape
// into these common fields, or rely on the supported aliases below.
func ParseBounceEventPayload(payload map[string]any) (BounceEvent, error) {
	if payload == nil {
		return BounceEvent{}, bouncePayloadError("", errors.New("payload is nil"))
	}

	event := BounceEvent{
		Provider:       firstBouncePayloadString(payload, "provider", "source"),
		ID:             firstBouncePayloadString(payload, "event_id", "event.id", "id"),
		MessageID:      firstBouncePayloadString(payload, "message_id", "message.id", "smtp_id", "smtp-id"),
		Recipient:      firstBouncePayloadString(payload, "recipient", "recipient.email", "email", "to", "payload.recipient", "payload.email"),
		Type:           NormalizeBounceType(firstBouncePayloadString(payload, "bounce_type", "bounce.type", "type", "severity")),
		ProviderReason: firstBouncePayloadString(payload, "reason", "bounce_reason", "bounce.reason", "error", "error.message"),
		Status:         firstBouncePayloadString(payload, "status", "smtp_status", "smtp.status", "status_code"),
		Diagnostic:     firstBouncePayloadString(payload, "diagnostic", "diagnostic_code", "smtp.diagnostic", "response"),
	}

	occurredAt, ok, err := firstBouncePayloadTime(payload, "occurred_at", "timestamp", "created_at", "event_time")
	if err != nil {
		return BounceEvent{}, bouncePayloadError("occurred_at", err)
	}
	if ok {
		event.OccurredAt = occurredAt
	}

	event, err = NormalizeBounceEvent(event)
	if err != nil {
		return BounceEvent{}, bouncePayloadError("recipient", err)
	}
	return event, nil
}

// BouncePayloadValue resolves a dot-separated field path from payload. A
// leading "payload." segment is ignored for compatibility with webhook paths.
func BouncePayloadValue(payload map[string]any, path string) (any, bool) {
	path = strings.TrimSpace(path)
	if payload == nil || path == "" {
		return nil, false
	}

	parts := strings.Split(path, ".")
	if len(parts) > 0 && parts[0] == "payload" && len(parts) > 1 {
		if value, ok := bouncePayloadValueParts(payload, parts[1:]); ok {
			return value, true
		}
	}
	return bouncePayloadValueParts(payload, parts)
}

func bouncePayloadValueParts(payload map[string]any, parts []string) (any, bool) {
	var current any = payload
	for _, part := range parts {
		if part == "" {
			return nil, false
		}
		switch node := current.(type) {
		case map[string]any:
			next, ok := node[part]
			if !ok {
				return nil, false
			}
			current = next
		case map[string]string:
			next, ok := node[part]
			if !ok {
				return nil, false
			}
			current = next
		default:
			return nil, false
		}
	}

	return current, true
}

// BouncePayloadString resolves a field path and stringifies scalar values.
func BouncePayloadString(payload map[string]any, path string) (string, bool) {
	value, ok := BouncePayloadValue(payload, path)
	if !ok {
		return "", false
	}
	return stringifyBouncePayloadValue(value)
}

// BouncePayloadTime resolves a field path and parses common webhook timestamp
// values: time.Time, RFC3339 strings, Unix seconds, and Unix milliseconds.
func BouncePayloadTime(payload map[string]any, path string) (time.Time, bool, error) {
	value, ok := BouncePayloadValue(payload, path)
	if !ok {
		return time.Time{}, false, nil
	}
	timestamp, err := parseBouncePayloadTime(value)
	if err != nil {
		return time.Time{}, true, err
	}
	return timestamp, true, nil
}

func normalizeBounceRecipient(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", errors.New("recipient is required")
	}
	if containsControl(raw) {
		return "", errors.New("recipient contains control characters")
	}
	parsed, err := netmail.ParseAddress(raw)
	if err != nil {
		return "", fmt.Errorf("invalid recipient %q", raw)
	}
	return parsed.Address, nil
}

func normalizeKnownBounceReason(reason BounceReason) BounceReason {
	switch BounceReason(normalizeBounceToken(string(reason))) {
	case BounceReasonInvalidRecipient:
		return BounceReasonInvalidRecipient
	case BounceReasonDNS:
		return BounceReasonDNS
	case BounceReasonMailboxFull:
		return BounceReasonMailboxFull
	case BounceReasonMessageTooLarge:
		return BounceReasonMessageTooLarge
	case BounceReasonPolicy:
		return BounceReasonPolicy
	case BounceReasonContent:
		return BounceReasonContent
	case BounceReasonSpam:
		return BounceReasonSpam
	case BounceReasonBlocklist:
		return BounceReasonBlocklist
	case BounceReasonRateLimited:
		return BounceReasonRateLimited
	case BounceReasonTemporary:
		return BounceReasonTemporary
	case BounceReasonPermanent:
		return BounceReasonPermanent
	default:
		return BounceReasonUnknown
	}
}

func normalizeBounceToken(raw string) string {
	raw = strings.TrimSpace(strings.ToLower(raw))
	raw = strings.ReplaceAll(raw, "-", "_")
	raw = strings.ReplaceAll(raw, " ", "_")
	return raw
}

func hasBounceStatusPrefix(status string, prefixes ...string) bool {
	status = strings.TrimSpace(status)
	for _, prefix := range prefixes {
		if strings.HasPrefix(status, prefix) {
			return true
		}
	}
	return false
}

func containsAnyBounceText(text string, needles ...string) bool {
	for _, needle := range needles {
		if strings.Contains(text, needle) {
			return true
		}
	}
	return false
}

func firstBouncePayloadString(payload map[string]any, paths ...string) string {
	for _, path := range paths {
		if value, ok := BouncePayloadString(payload, path); ok {
			value = strings.TrimSpace(value)
			if value != "" {
				return value
			}
		}
	}
	return ""
}

func firstBouncePayloadTime(payload map[string]any, paths ...string) (time.Time, bool, error) {
	for _, path := range paths {
		value, ok, err := BouncePayloadTime(payload, path)
		if err != nil || ok {
			return value, ok, err
		}
	}
	return time.Time{}, false, nil
}

func stringifyBouncePayloadValue(value any) (string, bool) {
	switch v := value.(type) {
	case string:
		return v, true
	case []byte:
		return string(v), true
	case json.Number:
		return v.String(), true
	case fmt.Stringer:
		return v.String(), true
	case bool:
		return strconv.FormatBool(v), true
	case int:
		return strconv.FormatInt(int64(v), 10), true
	case int8:
		return strconv.FormatInt(int64(v), 10), true
	case int16:
		return strconv.FormatInt(int64(v), 10), true
	case int32:
		return strconv.FormatInt(int64(v), 10), true
	case int64:
		return strconv.FormatInt(v, 10), true
	case uint:
		return strconv.FormatUint(uint64(v), 10), true
	case uint8:
		return strconv.FormatUint(uint64(v), 10), true
	case uint16:
		return strconv.FormatUint(uint64(v), 10), true
	case uint32:
		return strconv.FormatUint(uint64(v), 10), true
	case uint64:
		return strconv.FormatUint(v, 10), true
	case float32:
		return strconv.FormatFloat(float64(v), 'f', -1, 32), true
	case float64:
		return strconv.FormatFloat(v, 'f', -1, 64), true
	default:
		return "", false
	}
}

func parseBouncePayloadTime(value any) (time.Time, error) {
	switch v := value.(type) {
	case time.Time:
		if v.IsZero() {
			return time.Time{}, errors.New("timestamp is zero")
		}
		return v.UTC(), nil
	case string:
		return parseBounceTimestampString(v)
	case []byte:
		return parseBounceTimestampString(string(v))
	case json.Number:
		return parseBounceTimestampString(v.String())
	case int:
		return parseBounceUnixTimestamp(int64(v))
	case int8:
		return parseBounceUnixTimestamp(int64(v))
	case int16:
		return parseBounceUnixTimestamp(int64(v))
	case int32:
		return parseBounceUnixTimestamp(int64(v))
	case int64:
		return parseBounceUnixTimestamp(v)
	case uint:
		return parseBounceUnixTimestamp(int64(v))
	case uint8:
		return parseBounceUnixTimestamp(int64(v))
	case uint16:
		return parseBounceUnixTimestamp(int64(v))
	case uint32:
		return parseBounceUnixTimestamp(int64(v))
	case uint64:
		if v > uint64(^uint(0)>>1) {
			return time.Time{}, errors.New("timestamp overflows int64")
		}
		return parseBounceUnixTimestamp(int64(v))
	case float32:
		return parseBounceTimestampString(strconv.FormatFloat(float64(v), 'f', -1, 32))
	case float64:
		return parseBounceTimestampString(strconv.FormatFloat(v, 'f', -1, 64))
	default:
		return time.Time{}, fmt.Errorf("unsupported timestamp type %T", value)
	}
}

func parseBounceTimestampString(raw string) (time.Time, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return time.Time{}, errors.New("timestamp is required")
	}
	if timestamp, err := time.Parse(time.RFC3339Nano, raw); err == nil {
		return timestamp.UTC(), nil
	}
	if unix, err := strconv.ParseInt(raw, 10, 64); err == nil {
		return parseBounceUnixTimestamp(unix)
	}
	return time.Time{}, fmt.Errorf("unsupported timestamp %q", raw)
}

func parseBounceUnixTimestamp(unix int64) (time.Time, error) {
	const millisecondsThreshold int64 = 1_000_000_000_000
	if unix >= millisecondsThreshold || unix <= -millisecondsThreshold {
		return time.UnixMilli(unix).UTC(), nil
	}
	return time.Unix(unix, 0).UTC(), nil
}

func bounceEventError(field string, err error) error {
	return &BounceError{Kind: ErrInvalidBounceEvent, Field: field, Err: err}
}

func bouncePayloadError(field string, err error) error {
	return &BounceError{Kind: ErrInvalidBouncePayload, Field: field, Err: err}
}

// BounceError adds a field path to bounce parsing and normalization failures.
type BounceError struct {
	Kind  error
	Field string
	Err   error
}

// Error implements error.
func (e *BounceError) Error() string {
	if e == nil {
		return "<nil>"
	}
	message := "email: bounce error"
	if e.Kind != nil {
		message = e.Kind.Error()
	}
	if e.Field != "" {
		message += ": " + e.Field
	}
	if e.Err != nil {
		message += ": " + e.Err.Error()
	}
	return message
}

// Is reports whether target matches the bounce error kind.
func (e *BounceError) Is(target error) bool {
	return e != nil && e.Kind != nil && target == e.Kind
}

// Unwrap exposes the lower-level parsing error, when one exists.
func (e *BounceError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}
