package notifications

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

const testPagerDutyRoutingKey = "0123456789abcdef0123456789abcdef"

func TestPagerDutyRoutingKeyValidationAndRedaction(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name         string
		key          string
		wantValid    bool
		wantRedacted string
	}{
		{
			name:         "valid",
			key:          " " + testPagerDutyRoutingKey + " ",
			wantValid:    true,
			wantRedacted: "0123...cdef",
		},
		{
			name:         "wrong length",
			key:          "short",
			wantRedacted: "[redacted]",
		},
		{
			name:         "contains whitespace",
			key:          "0123456789abcdef 123456789abcde",
			wantRedacted: "[redacted]",
		},
		{
			name:         "contains control character",
			key:          "0123456789abcdef\x010123456789abcde",
			wantRedacted: "[redacted]",
		},
		{
			name:         "contains non ascii",
			key:          "0123456789abcdef0123456789abcde\u00e7",
			wantRedacted: "[redacted]",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := IsPagerDutyRoutingKey(tt.key); got != tt.wantValid {
				t.Fatalf("IsPagerDutyRoutingKey() = %v, want %v", got, tt.wantValid)
			}
			if got := RedactPagerDutyRoutingKey(tt.key); got != tt.wantRedacted {
				t.Fatalf("RedactPagerDutyRoutingKey() = %q, want %q", got, tt.wantRedacted)
			}
		})
	}
}

func TestPagerDutyEventActionAndSeverityValidation(t *testing.T) {
	t.Parallel()

	for _, action := range []string{"trigger", "ACKNOWLEDGE", " resolve "} {
		if !IsPagerDutyEventAction(action) {
			t.Fatalf("IsPagerDutyEventAction(%q) = false, want true", action)
		}
	}
	if IsPagerDutyEventAction("close") {
		t.Fatalf("IsPagerDutyEventAction(close) = true, want false")
	}

	for _, severity := range []string{"critical", "ERROR", " warning ", "info"} {
		if !IsPagerDutySeverity(severity) {
			t.Fatalf("IsPagerDutySeverity(%q) = false, want true", severity)
		}
	}
	if IsPagerDutySeverity("notice") {
		t.Fatalf("IsPagerDutySeverity(notice) = true, want false")
	}
}

func TestNormalizePagerDutyEventDescriptorTrimsAndCopies(t *testing.T) {
	t.Parallel()

	details := map[string]any{" incident ": " INC-1 "}
	normalized := NormalizePagerDutyEventDescriptor(PagerDutyEventDescriptor{
		RoutingKey:    " " + testPagerDutyRoutingKey + " ",
		EventAction:   " TRIGGER ",
		DedupKey:      " incident-1 ",
		Summary:       " CPU high ",
		Source:        " api-1 ",
		Severity:      " WARNING ",
		Component:     " api ",
		Group:         " core ",
		Class:         " saturation ",
		Client:        " Lazuli ",
		ClientURL:     " https://example.com/incidents/1?token=secret ",
		CustomDetails: details,
	})

	if normalized.EventAction != PagerDutyEventActionTrigger || normalized.Severity != PagerDutySeverityWarning {
		t.Fatalf("normalized action/severity = %q/%q", normalized.EventAction, normalized.Severity)
	}
	if normalized.RoutingKey != testPagerDutyRoutingKey || normalized.Summary != "CPU high" || normalized.Source != "api-1" {
		t.Fatalf("normalized descriptor = %#v", normalized)
	}
	if _, ok := normalized.CustomDetails[" incident "]; ok {
		t.Fatalf("custom_details retained untrimmed key: %#v", normalized.CustomDetails)
	}
	if got := normalized.CustomDetails["incident"]; got != " INC-1 " {
		t.Fatalf("custom_details incident = %#v, want original value", got)
	}

	details[" incident "] = "mutated"
	if got := normalized.CustomDetails["incident"]; got != " INC-1 " {
		t.Fatalf("NormalizePagerDutyEventDescriptor retained mutable map = %#v", got)
	}
}

func TestPlanPagerDutyTriggerPayload(t *testing.T) {
	t.Parallel()

	details := map[string]any{"runbook": "https://example.com/runbooks/cpu"}
	plan, err := PlanPagerDutyEventPayload(PagerDutyEventDescriptor{
		RoutingKey:    " " + testPagerDutyRoutingKey + " ",
		EventAction:   " trigger ",
		DedupKey:      " cpu-high:api-1 ",
		Summary:       " CPU high ",
		Source:        " api-1 ",
		Severity:      " critical ",
		Component:     " api ",
		Group:         " core ",
		Class:         " saturation ",
		Client:        " Lazuli ",
		ClientURL:     " https://example.com/incidents/1?token=secret#timeline ",
		CustomDetails: details,
	})
	if err != nil {
		t.Fatalf("PlanPagerDutyEventPayload() error = %v", err)
	}

	if plan.RedactedRoutingKey != "0123...cdef" {
		t.Fatalf("RedactedRoutingKey = %q", plan.RedactedRoutingKey)
	}
	if plan.RedactedClientURL != "https://example.com/incidents/1" {
		t.Fatalf("RedactedClientURL = %q", plan.RedactedClientURL)
	}
	if plan.EventAction != PagerDutyEventActionTrigger || plan.DedupKey != "cpu-high:api-1" {
		t.Fatalf("plan action/dedup = %q/%q", plan.EventAction, plan.DedupKey)
	}
	if plan.Severity != PagerDutySeverityCritical || plan.Source != "api-1" || plan.Component != "api" {
		t.Fatalf("plan metadata = severity %q source %q component %q", plan.Severity, plan.Source, plan.Component)
	}

	var payload map[string]any
	if err := json.Unmarshal(plan.PayloadJSON, &payload); err != nil {
		t.Fatalf("unmarshal payload JSON: %v", err)
	}
	if got := payload["routing_key"]; got != testPagerDutyRoutingKey {
		t.Fatalf("payload.routing_key = %v, want routing key", got)
	}
	if got := payload["event_action"]; got != PagerDutyEventActionTrigger {
		t.Fatalf("payload.event_action = %v, want trigger", got)
	}
	triggerPayload, ok := payload["payload"].(map[string]any)
	if !ok {
		t.Fatalf("payload.payload = %T, want object", payload["payload"])
	}
	if got := triggerPayload["summary"]; got != "CPU high" {
		t.Fatalf("payload.summary = %v, want CPU high", got)
	}
	if got := triggerPayload["severity"]; got != PagerDutySeverityCritical {
		t.Fatalf("payload.severity = %v, want critical", got)
	}
	customDetails, ok := triggerPayload["custom_details"].(map[string]any)
	if !ok || customDetails["runbook"] != "https://example.com/runbooks/cpu" {
		t.Fatalf("payload.custom_details = %#v", triggerPayload["custom_details"])
	}

	details["runbook"] = "mutated"
	plannedTriggerPayload := plan.Payload["payload"].(map[string]any)
	plannedDetails := plannedTriggerPayload["custom_details"].(map[string]any)
	if got := plannedDetails["runbook"]; got != "https://example.com/runbooks/cpu" {
		t.Fatalf("plan retained mutable custom_details = %v", got)
	}
}

func TestPlanPagerDutyAcknowledgeAndResolvePayload(t *testing.T) {
	t.Parallel()

	for _, action := range []string{PagerDutyEventActionAcknowledge, PagerDutyEventActionResolve} {
		action := action
		t.Run(action, func(t *testing.T) {
			t.Parallel()

			plan, err := PlanPagerDutyEventPayload(PagerDutyEventDescriptor{
				RoutingKey:  testPagerDutyRoutingKey,
				EventAction: action,
				DedupKey:    "incident-1",
				Summary:     "not included",
				Source:      "not included",
				Severity:    PagerDutySeverityCritical,
				Client:      "Lazuli",
			})
			if err != nil {
				t.Fatalf("PlanPagerDutyEventPayload(%s) error = %v", action, err)
			}
			if _, ok := plan.Payload["payload"]; ok {
				t.Fatalf("payload included trigger payload for %s: %#v", action, plan.Payload)
			}
			if got := plan.Payload["dedup_key"]; got != "incident-1" {
				t.Fatalf("payload.dedup_key = %v, want incident-1", got)
			}
			if got := plan.Payload["client"]; got != "Lazuli" {
				t.Fatalf("payload.client = %v, want Lazuli", got)
			}
		})
	}
}

func TestValidatePagerDutyEventDescriptorRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		desc PagerDutyEventDescriptor
		want error
	}{
		{
			name: "bad routing key",
			desc: PagerDutyEventDescriptor{
				RoutingKey:  "short",
				EventAction: PagerDutyEventActionTrigger,
				Summary:     "CPU high",
				Source:      "api-1",
				Severity:    PagerDutySeverityCritical,
			},
			want: ErrPagerDutyRoutingKeyInvalid,
		},
		{
			name: "bad action",
			desc: PagerDutyEventDescriptor{
				RoutingKey:  testPagerDutyRoutingKey,
				EventAction: "close",
			},
			want: ErrPagerDutyEventActionInvalid,
		},
		{
			name: "trigger missing summary",
			desc: PagerDutyEventDescriptor{
				RoutingKey:  testPagerDutyRoutingKey,
				EventAction: PagerDutyEventActionTrigger,
				Source:      "api-1",
				Severity:    PagerDutySeverityCritical,
			},
			want: ErrPagerDutyPayloadInvalid,
		},
		{
			name: "trigger missing source",
			desc: PagerDutyEventDescriptor{
				RoutingKey:  testPagerDutyRoutingKey,
				EventAction: PagerDutyEventActionTrigger,
				Summary:     "CPU high",
				Severity:    PagerDutySeverityCritical,
			},
			want: ErrPagerDutyPayloadInvalid,
		},
		{
			name: "trigger bad severity",
			desc: PagerDutyEventDescriptor{
				RoutingKey:  testPagerDutyRoutingKey,
				EventAction: PagerDutyEventActionTrigger,
				Summary:     "CPU high",
				Source:      "api-1",
				Severity:    "notice",
			},
			want: ErrPagerDutyPayloadInvalid,
		},
		{
			name: "ack missing dedup",
			desc: PagerDutyEventDescriptor{
				RoutingKey:  testPagerDutyRoutingKey,
				EventAction: PagerDutyEventActionAcknowledge,
			},
			want: ErrPagerDutyPayloadInvalid,
		},
		{
			name: "dedup too long",
			desc: PagerDutyEventDescriptor{
				RoutingKey:  testPagerDutyRoutingKey,
				EventAction: PagerDutyEventActionResolve,
				DedupKey:    strings.Repeat("a", PagerDutyMaxDedupKeyRunes+1),
			},
			want: ErrPagerDutyPayloadInvalid,
		},
		{
			name: "client url with credentials",
			desc: PagerDutyEventDescriptor{
				RoutingKey:  testPagerDutyRoutingKey,
				EventAction: PagerDutyEventActionTrigger,
				Summary:     "CPU high",
				Source:      "api-1",
				Severity:    PagerDutySeverityCritical,
				ClientURL:   "https://user:pass@example.com/incidents/1",
			},
			want: ErrPagerDutyURLInvalid,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := ValidatePagerDutyEventDescriptor(tt.desc)
			if !errors.Is(err, tt.want) {
				t.Fatalf("ValidatePagerDutyEventDescriptor() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestRedactPagerDutyURL(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		raw  string
		want string
	}{
		{
			name: "empty",
			raw:  " ",
			want: "",
		},
		{
			name: "query and fragment removed",
			raw:  "https://example.com/incidents/1?token=secret#timeline",
			want: "https://example.com/incidents/1",
		},
		{
			name: "credentials removed",
			raw:  "https://user:pass@example.com/incidents/1",
			want: "https://example.com/incidents/1",
		},
		{
			name: "invalid parse",
			raw:  "%",
			want: "[redacted]",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := RedactPagerDutyURL(tt.raw); got != tt.want {
				t.Fatalf("RedactPagerDutyURL() = %q, want %q", got, tt.want)
			}
		})
	}
}
