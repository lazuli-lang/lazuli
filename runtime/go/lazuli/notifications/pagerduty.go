package notifications

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"strings"
	"unicode"
	"unicode/utf8"
)

const (
	PagerDutyEventActionTrigger     = "trigger"
	PagerDutyEventActionAcknowledge = "acknowledge"
	PagerDutyEventActionResolve     = "resolve"

	PagerDutySeverityCritical = "critical"
	PagerDutySeverityError    = "error"
	PagerDutySeverityWarning  = "warning"
	PagerDutySeverityInfo     = "info"

	PagerDutyRoutingKeyLength     = 32
	PagerDutyMaxDedupKeyRunes     = 255
	PagerDutyMaxPayloadFieldRunes = 1024
)

var (
	ErrPagerDutyRoutingKeyInvalid  = errors.New("notifications: invalid pagerduty routing key")
	ErrPagerDutyEventActionInvalid = errors.New("notifications: invalid pagerduty event action")
	ErrPagerDutyPayloadInvalid     = errors.New("notifications: invalid pagerduty payload")
	ErrPagerDutyURLInvalid         = errors.New("notifications: invalid pagerduty url")
)

// PagerDutyEventDescriptor describes a PagerDuty Events API event without
// binding this package to an HTTP client or provider runtime.
type PagerDutyEventDescriptor struct {
	RoutingKey    string
	EventAction   string
	DedupKey      string
	Summary       string
	Source        string
	Severity      string
	Component     string
	Group         string
	Class         string
	Client        string
	ClientURL     string
	CustomDetails map[string]any
}

// PagerDutyEventPlan is the deterministic result of planning a PagerDuty
// Events API payload. PayloadJSON is ready for an adapter to write later.
type PagerDutyEventPlan struct {
	Descriptor         PagerDutyEventDescriptor
	Payload            map[string]any
	PayloadJSON        []byte
	RedactedRoutingKey string
	RedactedClientURL  string
	EventAction        string
	DedupKey           string
	Severity           string
	Source             string
	Component          string
}

// NormalizePagerDutyEventDescriptor trims event metadata and returns a copy. It
// does not validate required fields.
func NormalizePagerDutyEventDescriptor(desc PagerDutyEventDescriptor) PagerDutyEventDescriptor {
	return PagerDutyEventDescriptor{
		RoutingKey:    strings.TrimSpace(desc.RoutingKey),
		EventAction:   strings.ToLower(strings.TrimSpace(desc.EventAction)),
		DedupKey:      strings.TrimSpace(desc.DedupKey),
		Summary:       strings.TrimSpace(desc.Summary),
		Source:        strings.TrimSpace(desc.Source),
		Severity:      strings.ToLower(strings.TrimSpace(desc.Severity)),
		Component:     strings.TrimSpace(desc.Component),
		Group:         strings.TrimSpace(desc.Group),
		Class:         strings.TrimSpace(desc.Class),
		Client:        strings.TrimSpace(desc.Client),
		ClientURL:     strings.TrimSpace(desc.ClientURL),
		CustomDetails: clonePagerDutyCustomDetails(desc.CustomDetails),
	}
}

// ValidatePagerDutyEventDescriptor checks PagerDuty event metadata without
// mutating the descriptor.
func ValidatePagerDutyEventDescriptor(desc PagerDutyEventDescriptor) error {
	desc = NormalizePagerDutyEventDescriptor(desc)

	var errs []error
	if !IsPagerDutyRoutingKey(desc.RoutingKey) {
		errs = append(errs, ErrPagerDutyRoutingKeyInvalid)
	}
	if !IsPagerDutyEventAction(desc.EventAction) {
		errs = append(errs, ErrPagerDutyEventActionInvalid)
	}
	if desc.DedupKey != "" && utf8.RuneCountInString(desc.DedupKey) > PagerDutyMaxDedupKeyRunes {
		errs = append(errs, fmt.Errorf("%w: dedup_key exceeds %d runes", ErrPagerDutyPayloadInvalid, PagerDutyMaxDedupKeyRunes))
	}
	if desc.ClientURL != "" && !isPagerDutyDiagnosticURL(desc.ClientURL) {
		errs = append(errs, fmt.Errorf("%w: client_url must be absolute http(s)", ErrPagerDutyURLInvalid))
	}

	switch desc.EventAction {
	case PagerDutyEventActionTrigger:
		errs = append(errs, validatePagerDutyTriggerPayload(desc)...)
	case PagerDutyEventActionAcknowledge, PagerDutyEventActionResolve:
		if desc.DedupKey == "" {
			errs = append(errs, fmt.Errorf("%w: dedup_key required for %s", ErrPagerDutyPayloadInvalid, desc.EventAction))
		}
	}

	return errors.Join(errs...)
}

// IsPagerDutyRoutingKey reports whether key has PagerDuty's stable 32-character
// integration key shape without exposing the secret value.
func IsPagerDutyRoutingKey(key string) bool {
	key = strings.TrimSpace(key)
	if len(key) != PagerDutyRoutingKeyLength {
		return false
	}
	for _, r := range key {
		if r > unicode.MaxASCII || unicode.IsSpace(r) || unicode.IsControl(r) {
			return false
		}
	}
	return true
}

// RedactPagerDutyRoutingKey keeps a short stable hint while removing the
// secret routing key value.
func RedactPagerDutyRoutingKey(key string) string {
	key = strings.TrimSpace(key)
	if !IsPagerDutyRoutingKey(key) {
		return "[redacted]"
	}
	return key[:4] + "..." + key[len(key)-4:]
}

// IsPagerDutyEventAction reports whether action is one of the Events API
// actions supported by PagerDuty.
func IsPagerDutyEventAction(action string) bool {
	switch strings.ToLower(strings.TrimSpace(action)) {
	case PagerDutyEventActionTrigger, PagerDutyEventActionAcknowledge, PagerDutyEventActionResolve:
		return true
	default:
		return false
	}
}

// IsPagerDutySeverity reports whether severity is a supported Events API
// trigger severity.
func IsPagerDutySeverity(severity string) bool {
	switch strings.ToLower(strings.TrimSpace(severity)) {
	case PagerDutySeverityCritical, PagerDutySeverityError, PagerDutySeverityWarning, PagerDutySeverityInfo:
		return true
	default:
		return false
	}
}

// RedactPagerDutyURL removes credentials, query parameters, and fragments from
// diagnostic URLs before they are logged.
func RedactPagerDutyURL(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	parsed, err := url.Parse(raw)
	if err != nil || parsed == nil {
		return "[redacted]"
	}
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String()
}

// PlanPagerDutyEventPayload validates desc and returns the dry-run request body
// an adapter can marshal later. It does not perform HTTP.
func PlanPagerDutyEventPayload(desc PagerDutyEventDescriptor) (PagerDutyEventPlan, error) {
	desc = NormalizePagerDutyEventDescriptor(desc)
	if err := ValidatePagerDutyEventDescriptor(desc); err != nil {
		return PagerDutyEventPlan{}, err
	}

	payload := map[string]any{
		"routing_key":  desc.RoutingKey,
		"event_action": desc.EventAction,
	}
	if desc.DedupKey != "" {
		payload["dedup_key"] = desc.DedupKey
	}
	if desc.Client != "" {
		payload["client"] = desc.Client
	}
	if desc.ClientURL != "" {
		payload["client_url"] = desc.ClientURL
	}
	if desc.EventAction == PagerDutyEventActionTrigger {
		payload["payload"] = pagerDutyTriggerPayload(desc)
	}

	body, err := json.Marshal(payload)
	if err != nil {
		return PagerDutyEventPlan{}, fmt.Errorf("%w: marshal payload: %v", ErrPagerDutyPayloadInvalid, err)
	}

	return PagerDutyEventPlan{
		Descriptor:         desc,
		Payload:            payload,
		PayloadJSON:        body,
		RedactedRoutingKey: RedactPagerDutyRoutingKey(desc.RoutingKey),
		RedactedClientURL:  RedactPagerDutyURL(desc.ClientURL),
		EventAction:        desc.EventAction,
		DedupKey:           desc.DedupKey,
		Severity:           desc.Severity,
		Source:             desc.Source,
		Component:          desc.Component,
	}, nil
}

func validatePagerDutyTriggerPayload(desc PagerDutyEventDescriptor) []error {
	var errs []error
	if desc.Summary == "" {
		errs = append(errs, fmt.Errorf("%w: summary required", ErrPagerDutyPayloadInvalid))
	}
	if desc.Source == "" {
		errs = append(errs, fmt.Errorf("%w: source required", ErrPagerDutyPayloadInvalid))
	}
	if !IsPagerDutySeverity(desc.Severity) {
		errs = append(errs, fmt.Errorf("%w: severity must be critical, error, warning, or info", ErrPagerDutyPayloadInvalid))
	}
	for _, field := range []struct {
		name  string
		value string
	}{
		{name: "summary", value: desc.Summary},
		{name: "source", value: desc.Source},
		{name: "component", value: desc.Component},
		{name: "group", value: desc.Group},
		{name: "class", value: desc.Class},
		{name: "client", value: desc.Client},
	} {
		if utf8.RuneCountInString(field.value) > PagerDutyMaxPayloadFieldRunes {
			errs = append(errs, fmt.Errorf("%w: %s exceeds %d runes", ErrPagerDutyPayloadInvalid, field.name, PagerDutyMaxPayloadFieldRunes))
		}
	}
	return errs
}

func pagerDutyTriggerPayload(desc PagerDutyEventDescriptor) map[string]any {
	payload := map[string]any{
		"summary":  desc.Summary,
		"source":   desc.Source,
		"severity": desc.Severity,
	}
	if desc.Component != "" {
		payload["component"] = desc.Component
	}
	if desc.Group != "" {
		payload["group"] = desc.Group
	}
	if desc.Class != "" {
		payload["class"] = desc.Class
	}
	if len(desc.CustomDetails) > 0 {
		payload["custom_details"] = clonePagerDutyCustomDetails(desc.CustomDetails)
	}
	return payload
}

func clonePagerDutyCustomDetails(details map[string]any) map[string]any {
	if len(details) == 0 {
		return nil
	}
	clone := make(map[string]any, len(details))
	for key, value := range details {
		clone[strings.TrimSpace(key)] = value
	}
	return clone
}

func isPagerDutyDiagnosticURL(raw string) bool {
	parsed, err := url.Parse(raw)
	if err != nil || parsed == nil {
		return false
	}
	return parsed.IsAbs() &&
		(parsed.Scheme == "http" || parsed.Scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil
}
