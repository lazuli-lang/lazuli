// Package version implements Lazuli language-version policy helpers.
package version

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
)

const (
	// CodePinMismatch reports a missing or minor-mismatched lazuli_version pin.
	CodePinMismatch = "LAZULI-VERSION-001"
	// CodeMigrationPathMissing reports a mismatched pin with no migration path.
	CodeMigrationPathMissing = "LAZULI-VERSION-002"
	// CodePatchPinRejected reports a patch-level lazuli_version pin.
	CodePatchPinRejected = "LAZULI-VERSION-003"
)

var (
	// ErrInvalidSchemaSemver is returned when an LZIR schema version is not
	// strict MAJOR.MINOR.PATCH semver.
	ErrInvalidSchemaSemver = errors.New("lazuli/version: invalid schema semver")
	// ErrInvalidMinorPin is returned when a lazuli_version pin is not
	// strict MAJOR.MINOR form.
	ErrInvalidMinorPin = errors.New("lazuli/version: invalid minor pin")
	// ErrPatchPinRejected is returned when a lazuli_version pin includes PATCH.
	ErrPatchPinRejected = errors.New("lazuli/version: patch-level pin rejected")
)

// SchemaVersion is the strict MAJOR.MINOR.PATCH version used by LZIR_SCHEMA.
type SchemaVersion struct {
	Major int
	Minor int
	Patch int
}

// MinorPin is the MINOR-granularity lazuli_version pin from app.lzi.
type MinorPin struct {
	Major int
	Minor int
}

// MigrationPathFunc reports whether a migration path exists from one minor pin
// to another.
type MigrationPathFunc func(from, to MinorPin) bool

// CheckResult is the release-policy verdict for a lazuli_version pin.
//
// Code is empty when the pin matches the schema minor. Non-empty values are one
// of the LAZULI-VERSION-* diagnostic code constants in this package.
type CheckResult struct {
	Code   string
	Pin    MinorPin
	Schema SchemaVersion
}

// OK reports whether the check produced no diagnostic code.
func (r CheckResult) OK() bool {
	return r.Code == ""
}

// ParseSchemaSemver parses strict MAJOR.MINOR.PATCH semver used by LZIR_SCHEMA.
func ParseSchemaSemver(raw string) (SchemaVersion, error) {
	parts := strings.Split(strings.TrimSpace(raw), ".")
	if len(parts) != 3 {
		return SchemaVersion{}, fmt.Errorf("%w: expected MAJOR.MINOR.PATCH", ErrInvalidSchemaSemver)
	}

	major, err := parseVersionSegment(parts[0])
	if err != nil {
		return SchemaVersion{}, fmt.Errorf("%w: major: %w", ErrInvalidSchemaSemver, err)
	}
	minor, err := parseVersionSegment(parts[1])
	if err != nil {
		return SchemaVersion{}, fmt.Errorf("%w: minor: %w", ErrInvalidSchemaSemver, err)
	}
	patch, err := parseVersionSegment(parts[2])
	if err != nil {
		return SchemaVersion{}, fmt.Errorf("%w: patch: %w", ErrInvalidSchemaSemver, err)
	}

	return SchemaVersion{Major: major, Minor: minor, Patch: patch}, nil
}

// ParseMinorPin parses strict MAJOR.MINOR lazuli_version pins.
func ParseMinorPin(raw string) (MinorPin, error) {
	parts := strings.Split(strings.TrimSpace(raw), ".")
	switch len(parts) {
	case 2:
		major, err := parseVersionSegment(parts[0])
		if err != nil {
			return MinorPin{}, fmt.Errorf("%w: major: %w", ErrInvalidMinorPin, err)
		}
		minor, err := parseVersionSegment(parts[1])
		if err != nil {
			return MinorPin{}, fmt.Errorf("%w: minor: %w", ErrInvalidMinorPin, err)
		}
		return MinorPin{Major: major, Minor: minor}, nil
	case 3:
		return MinorPin{}, fmt.Errorf("%w: %w", ErrInvalidMinorPin, ErrPatchPinRejected)
	default:
		return MinorPin{}, fmt.Errorf("%w: expected MAJOR.MINOR", ErrInvalidMinorPin)
	}
}

// CheckPin parses and compares a lazuli_version pin against the schema semver.
//
// PATCH mismatches are tolerated because lazuli_version pins are minor-only. If
// hasMigrationPath is nil, mismatches produce CodePinMismatch. If it is set and
// returns false, mismatches produce CodeMigrationPathMissing.
func CheckPin(pinRaw, schemaRaw string, hasMigrationPath MigrationPathFunc) (CheckResult, error) {
	schema, err := ParseSchemaSemver(schemaRaw)
	if err != nil {
		return CheckResult{}, err
	}

	if strings.TrimSpace(pinRaw) == "" {
		return CheckResult{Code: CodePinMismatch, Schema: schema}, nil
	}

	pin, err := ParseMinorPin(pinRaw)
	if err != nil {
		if errors.Is(err, ErrPatchPinRejected) {
			return CheckResult{Code: CodePatchPinRejected, Schema: schema}, nil
		}
		return CheckResult{}, err
	}

	result := CheckResult{Pin: pin, Schema: schema}
	if pin.Matches(schema) {
		return result, nil
	}
	if hasMigrationPath != nil && !hasMigrationPath(pin, schema.MinorPin()) {
		result.Code = CodeMigrationPathMissing
		return result, nil
	}
	result.Code = CodePinMismatch
	return result, nil
}

// MinorPin returns the schema version at lazuli_version pin granularity.
func (v SchemaVersion) MinorPin() MinorPin {
	return MinorPin{Major: v.Major, Minor: v.Minor}
}

// String returns MAJOR.MINOR.PATCH form.
func (v SchemaVersion) String() string {
	return fmt.Sprintf("%d.%d.%d", v.Major, v.Minor, v.Patch)
}

// Matches reports whether p matches schema at MINOR granularity.
func (p MinorPin) Matches(schema SchemaVersion) bool {
	return p.Major == schema.Major && p.Minor == schema.Minor
}

// String returns MAJOR.MINOR form.
func (p MinorPin) String() string {
	return fmt.Sprintf("%d.%d", p.Major, p.Minor)
}

func parseVersionSegment(raw string) (int, error) {
	if raw == "" {
		return 0, errors.New("empty segment")
	}
	if len(raw) > 1 && raw[0] == '0' {
		return 0, errors.New("leading zero")
	}
	for _, r := range raw {
		if r < '0' || r > '9' {
			return 0, errors.New("non-numeric segment")
		}
	}
	value, err := strconv.Atoi(raw)
	if err != nil {
		return 0, err
	}
	return value, nil
}
