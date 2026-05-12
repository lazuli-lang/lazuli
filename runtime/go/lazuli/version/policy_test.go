package version

import (
	"errors"
	"testing"
)

func TestParseSchemaSemver(t *testing.T) {
	got, err := ParseSchemaSemver("0.11.7")
	if err != nil {
		t.Fatalf("ParseSchemaSemver() error = %v", err)
	}
	want := SchemaVersion{Major: 0, Minor: 11, Patch: 7}
	if got != want {
		t.Fatalf("ParseSchemaSemver() = %#v, want %#v", got, want)
	}
	if got.String() != "0.11.7" {
		t.Fatalf("String() = %q, want 0.11.7", got.String())
	}
}

func TestParseSchemaSemverRejectsInvalidShape(t *testing.T) {
	for _, raw := range []string{"", "0.11", "0.11.0.1", "01.11.0", "0.x.0", "-1.11.0"} {
		_, err := ParseSchemaSemver(raw)
		if !errors.Is(err, ErrInvalidSchemaSemver) {
			t.Fatalf("ParseSchemaSemver(%q) error = %v, want ErrInvalidSchemaSemver", raw, err)
		}
	}
}

func TestParseMinorPin(t *testing.T) {
	got, err := ParseMinorPin("0.12")
	if err != nil {
		t.Fatalf("ParseMinorPin() error = %v", err)
	}
	want := MinorPin{Major: 0, Minor: 12}
	if got != want {
		t.Fatalf("ParseMinorPin() = %#v, want %#v", got, want)
	}
	if got.String() != "0.12" {
		t.Fatalf("String() = %q, want 0.12", got.String())
	}
}

func TestParseMinorPinRejectsPatchPins(t *testing.T) {
	_, err := ParseMinorPin("0.12.0")
	if !errors.Is(err, ErrPatchPinRejected) {
		t.Fatalf("ParseMinorPin() error = %v, want ErrPatchPinRejected", err)
	}
	if !errors.Is(err, ErrInvalidMinorPin) {
		t.Fatalf("ParseMinorPin() error = %v, want ErrInvalidMinorPin", err)
	}
}

func TestMinorPinMatchesSchemaIgnoringPatch(t *testing.T) {
	pin := MinorPin{Major: 0, Minor: 12}
	for _, schema := range []SchemaVersion{
		{Major: 0, Minor: 12, Patch: 0},
		{Major: 0, Minor: 12, Patch: 9},
	} {
		if !pin.Matches(schema) {
			t.Fatalf("%v should match schema %v", pin, schema)
		}
	}
	if pin.Matches(SchemaVersion{Major: 0, Minor: 13, Patch: 0}) {
		t.Fatal("0.12 pin matched 0.13.0 schema")
	}
	if pin.Matches(SchemaVersion{Major: 1, Minor: 12, Patch: 0}) {
		t.Fatal("0.12 pin matched 1.12.0 schema")
	}
}

func TestCheckPinDiagnosticCodesArePlainValues(t *testing.T) {
	var codes []string
	codes = append(codes, CodePinMismatch, CodeMigrationPathMissing, CodePatchPinRejected)

	want := []string{"LAZULI-VERSION-001", "LAZULI-VERSION-002", "LAZULI-VERSION-003"}
	for i, code := range codes {
		if code != want[i] {
			t.Fatalf("code[%d] = %q, want %q", i, code, want[i])
		}
	}
}

func TestCheckPinToleratesPatchMismatch(t *testing.T) {
	result, err := CheckPin("0.12", "0.12.9", nil)
	if err != nil {
		t.Fatalf("CheckPin() error = %v", err)
	}
	if !result.OK() {
		t.Fatalf("CheckPin() code = %q, want empty", result.Code)
	}
}

func TestCheckPinReportsPolicyDiagnostics(t *testing.T) {
	tests := []struct {
		name             string
		pin              string
		schema           string
		hasMigrationPath MigrationPathFunc
		wantCode         string
	}{
		{
			name:     "missing pin",
			pin:      "",
			schema:   "0.12.0",
			wantCode: CodePinMismatch,
		},
		{
			name:     "minor mismatch",
			pin:      "0.11",
			schema:   "0.12.0",
			wantCode: CodePinMismatch,
		},
		{
			name:     "major mismatch",
			pin:      "0.12",
			schema:   "1.0.0",
			wantCode: CodePinMismatch,
		},
		{
			name:     "patch pin rejected",
			pin:      "0.12.0",
			schema:   "0.12.0",
			wantCode: CodePatchPinRejected,
		},
		{
			name:   "missing migration path",
			pin:    "0.11",
			schema: "0.12.0",
			hasMigrationPath: func(from, to MinorPin) bool {
				if from != (MinorPin{Major: 0, Minor: 11}) || to != (MinorPin{Major: 0, Minor: 12}) {
					t.Fatalf("migration path checked from %#v to %#v", from, to)
				}
				return false
			},
			wantCode: CodeMigrationPathMissing,
		},
		{
			name:   "migration path exists",
			pin:    "0.11",
			schema: "0.12.0",
			hasMigrationPath: func(_, _ MinorPin) bool {
				return true
			},
			wantCode: CodePinMismatch,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := CheckPin(tt.pin, tt.schema, tt.hasMigrationPath)
			if err != nil {
				t.Fatalf("CheckPin() error = %v", err)
			}
			if result.Code != tt.wantCode {
				t.Fatalf("CheckPin() code = %q, want %q", result.Code, tt.wantCode)
			}
		})
	}
}
