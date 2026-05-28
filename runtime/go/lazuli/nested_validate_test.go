package lazuli

import (
	"encoding/json"
	"strings"
	"testing"
)

// Mirrors what codegen emits for a `record Installment { days: Integer,
// percentage: @semantic.Percentage }` used as a `Many<Installment>` field:
// the nested field carries the W1 `Percentage` carrier, and the record gets
// a `Validate()` method delegating to `lazuli.ValidateValue`.
type nvInstallment struct {
	Days       int64      `json:"days"`
	Percentage Percentage `json:"percentage"`
}

func (m nvInstallment) Validate() error { return ValidateValue(m) }

// A parent input carrying the `Many<Record>` JSONB collection.
type nvPlanInput struct {
	Name         string          `json:"name"`
	Installments []nvInstallment `json:"installments"`
}

func (m nvPlanInput) Validate() error { return ValidateValue(m) }

// GAP-R2 — an out-of-range `@semantic.Percentage` element of a `Many<Record>`
// collection is rejected at the wire decode boundary (the carrier's
// UnmarshalJSON fires per element via stdlib recursion).
func TestManyRecordPercentageRejectedOnDecode(t *testing.T) {
	body := []byte(`{"name":"p","installments":[{"days":30,"percentage":50},{"days":60,"percentage":250}]}`)
	var in nvPlanInput
	err := json.Unmarshal(body, &in)
	if err == nil {
		t.Fatalf("expected decode rejection for out-of-range nested percentage, got nil (in=%+v)", in)
	}
	if !strings.Contains(err.Error(), "out of range") {
		t.Fatalf("expected percentage range error, got: %v", err)
	}
}

// GAP-R2 — a valid `Many<Record>` collection decodes cleanly.
func TestManyRecordPercentageAcceptedOnDecode(t *testing.T) {
	body := []byte(`{"name":"p","installments":[{"days":30,"percentage":50},{"days":60,"percentage":50}]}`)
	var in nvPlanInput
	if err := json.Unmarshal(body, &in); err != nil {
		t.Fatalf("valid nested percentages should decode, got: %v", err)
	}
	if len(in.Installments) != 2 {
		t.Fatalf("expected 2 installments, got %d", len(in.Installments))
	}
}

// GAP-R2 — the explicit `Validate()` path (construction-time, not via JSON
// decode) also rejects an out-of-range nested element. This is the path the
// generated record `Validate()` method delegates to.
func TestManyRecordValidateRejectsOutOfRange(t *testing.T) {
	in := nvPlanInput{
		Name: "p",
		Installments: []nvInstallment{
			{Days: 30, Percentage: 50},
			{Days: 60, Percentage: Percentage(250)}, // out of range, bypassed UnmarshalJSON
		},
	}
	if err := in.Validate(); err == nil {
		t.Fatalf("expected ValidateValue to reject out-of-range nested percentage, got nil")
	}
}

// GAP-R2 — the explicit `Validate()` path accepts an in-range collection.
func TestManyRecordValidateAcceptsInRange(t *testing.T) {
	in := nvPlanInput{
		Name: "p",
		Installments: []nvInstallment{
			{Days: 30, Percentage: 50},
			{Days: 60, Percentage: 50},
		},
	}
	if err := in.Validate(); err != nil {
		t.Fatalf("expected ValidateValue to accept in-range collection, got: %v", err)
	}
}

// Defensive: a nil slice / empty struct validates without panic.
func TestValidateValueHandlesEmpty(t *testing.T) {
	if err := ValidateValue(nvPlanInput{}); err != nil {
		t.Fatalf("empty input should validate clean, got: %v", err)
	}
	if err := ValidateValue(struct{}{}); err != nil {
		t.Fatalf("empty struct should validate clean, got: %v", err)
	}
}
