package version

import (
	"errors"
	"fmt"
)

var (
	// ErrInvalidDeprecationRule is returned when a deprecation rule cannot be
	// evaluated consistently.
	ErrInvalidDeprecationRule = errors.New("lazuli/version: invalid deprecation rule")
)

// DeprecationSeverity is the severity assigned to a compatibility warning.
type DeprecationSeverity uint8

const (
	DeprecationSeverityError DeprecationSeverity = iota + 1
	DeprecationSeverityWarning
	DeprecationSeverityInfo
)

// String renders s as a stable lowercase token.
func (s DeprecationSeverity) String() string {
	switch s {
	case DeprecationSeverityError:
		return "error"
	case DeprecationSeverityWarning:
		return "warning"
	case DeprecationSeverityInfo:
		return "info"
	default:
		return "unknown"
	}
}

// ReplacementLink points users to replacement documentation or APIs.
type ReplacementLink struct {
	Label string
	URL   string
}

// DeprecationRule describes a framework feature that is deprecated for a
// bounded Lazuli minor-version window.
type DeprecationRule struct {
	// ID is a stable identifier for the deprecated surface.
	ID string
	// Subject is the deprecated feature, API, or behavior.
	Subject string
	// Message is optional human-readable guidance for the warning.
	Message string
	// DeprecatedSince is the first Lazuli minor version that should warn.
	DeprecatedSince MinorPin
	// RemoveAfter is the last Lazuli minor version that should only warn. Later
	// versions evaluate the rule as an error.
	RemoveAfter MinorPin
	// Severity is used during the warning window. Zero defaults to warning.
	Severity DeprecationSeverity
	// ReplacementLinks point users at replacement guidance.
	ReplacementLinks []ReplacementLink
}

// CompatibilityWarning is the result of evaluating an active deprecation rule
// against a current Lazuli schema version.
type CompatibilityWarning struct {
	Rule     DeprecationRule
	Current  SchemaVersion
	Severity DeprecationSeverity
	Removed  bool
}

// Validate reports structural problems that would make the rule ambiguous.
func (r DeprecationRule) Validate() error {
	if !isValidDeprecationSeverity(r.Severity) {
		return fmt.Errorf("%w: invalid severity %d", ErrInvalidDeprecationRule, r.Severity)
	}
	if compareMinorPins(r.RemoveAfter, r.DeprecatedSince) < 0 {
		return fmt.Errorf("%w: remove after %s is before deprecated since %s", ErrInvalidDeprecationRule, r.RemoveAfter, r.DeprecatedSince)
	}
	return nil
}

// EvaluateDeprecation evaluates rule against current. The returned bool is
// false when current is before DeprecatedSince and no warning should be emitted.
func EvaluateDeprecation(current SchemaVersion, rule DeprecationRule) (CompatibilityWarning, bool, error) {
	if err := rule.Validate(); err != nil {
		return CompatibilityWarning{}, false, err
	}

	currentPin := current.MinorPin()
	if compareMinorPins(currentPin, rule.DeprecatedSince) < 0 {
		return CompatibilityWarning{}, false, nil
	}

	removed := compareMinorPins(currentPin, rule.RemoveAfter) > 0
	severity := normalizeDeprecationSeverity(rule.Severity)
	if removed {
		severity = DeprecationSeverityError
	}

	return CompatibilityWarning{
		Rule:     rule,
		Current:  current,
		Severity: severity,
		Removed:  removed,
	}, true, nil
}

// EvaluateDeprecations evaluates rules against current and returns only active
// compatibility warnings.
func EvaluateDeprecations(current SchemaVersion, rules []DeprecationRule) ([]CompatibilityWarning, error) {
	warnings := make([]CompatibilityWarning, 0, len(rules))
	for _, rule := range rules {
		warning, active, err := EvaluateDeprecation(current, rule)
		if err != nil {
			return nil, err
		}
		if active {
			warnings = append(warnings, warning)
		}
	}
	return warnings, nil
}

func normalizeDeprecationSeverity(severity DeprecationSeverity) DeprecationSeverity {
	if severity == 0 {
		return DeprecationSeverityWarning
	}
	return severity
}

func isValidDeprecationSeverity(severity DeprecationSeverity) bool {
	switch severity {
	case 0, DeprecationSeverityError, DeprecationSeverityWarning, DeprecationSeverityInfo:
		return true
	default:
		return false
	}
}

func compareMinorPins(a, b MinorPin) int {
	if a.Major != b.Major {
		if a.Major < b.Major {
			return -1
		}
		return 1
	}
	if a.Minor != b.Minor {
		if a.Minor < b.Minor {
			return -1
		}
		return 1
	}
	return 0
}
