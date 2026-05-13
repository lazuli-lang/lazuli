package version

import (
	"errors"
	"fmt"
	"sort"
	"strings"
)

var (
	// ErrInvalidCompatibilityRange is returned when a compatibility range cannot
	// be evaluated consistently.
	ErrInvalidCompatibilityRange = errors.New("lazuli/version: invalid compatibility range")
	// ErrInvalidCompatibilityShim is returned when compatibility shim metadata
	// is incomplete or ambiguous.
	ErrInvalidCompatibilityShim = errors.New("lazuli/version: invalid compatibility shim")
	// ErrDuplicateCompatibilityShim is returned when two shims share the same
	// stable identifier.
	ErrDuplicateCompatibilityShim = errors.New("lazuli/version: duplicate compatibility shim")
)

// RuntimeCompatibilitySeverity is the severity assigned to a runtime
// compatibility warning.
type RuntimeCompatibilitySeverity uint8

const (
	RuntimeCompatibilitySeverityError RuntimeCompatibilitySeverity = iota + 1
	RuntimeCompatibilitySeverityWarning
	RuntimeCompatibilitySeverityInfo
)

// String renders s as a stable lowercase token.
func (s RuntimeCompatibilitySeverity) String() string {
	switch s {
	case RuntimeCompatibilitySeverityError:
		return "error"
	case RuntimeCompatibilitySeverityWarning:
		return "warning"
	case RuntimeCompatibilitySeverityInfo:
		return "info"
	default:
		return "unknown"
	}
}

// CompatibilityRange is an inclusive Lazuli minor-version range.
type CompatibilityRange struct {
	Min MinorPin
	Max MinorPin
}

// NewCompatibilityRange returns a validated inclusive compatibility range.
func NewCompatibilityRange(min, max MinorPin) (CompatibilityRange, error) {
	r := CompatibilityRange{Min: min, Max: max}
	if err := r.Validate(); err != nil {
		return CompatibilityRange{}, err
	}
	return r, nil
}

// Validate reports structural problems that would make the range ambiguous.
func (r CompatibilityRange) Validate() error {
	if compareMinorPins(r.Max, r.Min) < 0 {
		return fmt.Errorf("%w: max %s is before min %s", ErrInvalidCompatibilityRange, r.Max, r.Min)
	}
	return nil
}

// Contains reports whether pin is inside the inclusive range.
func (r CompatibilityRange) Contains(pin MinorPin) bool {
	return compareMinorPins(pin, r.Min) >= 0 && compareMinorPins(pin, r.Max) <= 0
}

// String returns a stable human-readable range.
func (r CompatibilityRange) String() string {
	if r.Min == r.Max {
		return r.Min.String()
	}
	return r.Min.String() + "-" + r.Max.String()
}

// CompatibilityReference points users to compatibility-layer documentation or
// migration notes.
type CompatibilityReference struct {
	Label string
	URL   string
}

// CompatibilityShim describes one runtime shim that preserves older API
// behavior for a bounded Lazuli API/runtime minor-version window.
type CompatibilityShim struct {
	// ID is a stable identifier for the shim.
	ID string
	// Subject is the API surface or behavior preserved by the shim.
	Subject string
	// Message is optional human-readable guidance for the warning.
	Message string
	// APIRange is the Lazuli API-version range this shim supports.
	APIRange CompatibilityRange
	// RuntimeRange is the Lazuli runtime-version range where this shim is valid.
	RuntimeRange CompatibilityRange
	// Severity is used when the shim is active. Zero defaults to warning.
	Severity RuntimeCompatibilitySeverity
	// References point users at compatibility or migration guidance.
	References []CompatibilityReference
}

// Validate reports structural problems that would make the shim ambiguous.
func (s CompatibilityShim) Validate() error {
	s = normalizeCompatibilityShim(s)
	if s.ID == "" {
		return fmt.Errorf("%w: id is required", ErrInvalidCompatibilityShim)
	}
	if !isValidRuntimeCompatibilitySeverity(s.Severity) {
		return fmt.Errorf("%w: invalid severity %d", ErrInvalidCompatibilityShim, s.Severity)
	}
	if err := s.APIRange.Validate(); err != nil {
		return fmt.Errorf("%w: api range: %w", ErrInvalidCompatibilityShim, err)
	}
	if err := s.RuntimeRange.Validate(); err != nil {
		return fmt.Errorf("%w: runtime range: %w", ErrInvalidCompatibilityShim, err)
	}
	return nil
}

// RuntimeCompatibilityWarning is emitted when a non-native API/runtime pairing
// is supported through a compatibility shim.
type RuntimeCompatibilityWarning struct {
	Shim           CompatibilityShim
	APIVersion     MinorPin
	RuntimeVersion SchemaVersion
	Severity       RuntimeCompatibilitySeverity
}

// RuntimeCompatibilityEvaluation is the deterministic compatibility verdict for
// one API pin against one runtime version.
type RuntimeCompatibilityEvaluation struct {
	APIVersion     MinorPin
	RuntimeVersion SchemaVersion
	Compatible     bool
	Native         bool
	Shims          []CompatibilityShim
	Warnings       []RuntimeCompatibilityWarning
}

// EvaluateRuntimeCompatibility evaluates apiVersion against runtimeVersion and
// the supplied shim catalog. Native minor matches are compatible without shims.
// Non-native matches are compatible only when at least one shim covers both the
// API and runtime minor ranges. Active shims and warnings are returned in stable
// order independent of input order.
func EvaluateRuntimeCompatibility(apiVersion MinorPin, runtimeVersion SchemaVersion, shims []CompatibilityShim) (RuntimeCompatibilityEvaluation, error) {
	evaluation := RuntimeCompatibilityEvaluation{
		APIVersion:     apiVersion,
		RuntimeVersion: runtimeVersion,
	}

	normalized, err := normalizeCompatibilityShims(shims)
	if err != nil {
		return evaluation, err
	}

	if apiVersion.Matches(runtimeVersion) {
		evaluation.Compatible = true
		evaluation.Native = true
		return evaluation, nil
	}

	runtimePin := runtimeVersion.MinorPin()
	for _, shim := range normalized {
		if !shim.APIRange.Contains(apiVersion) || !shim.RuntimeRange.Contains(runtimePin) {
			continue
		}
		evaluation.Shims = append(evaluation.Shims, shim)
		evaluation.Warnings = append(evaluation.Warnings, RuntimeCompatibilityWarning{
			Shim:           shim,
			APIVersion:     apiVersion,
			RuntimeVersion: runtimeVersion,
			Severity:       normalizeRuntimeCompatibilitySeverity(shim.Severity),
		})
	}

	evaluation.Compatible = len(evaluation.Shims) > 0
	return evaluation, nil
}

func normalizeCompatibilityShims(shims []CompatibilityShim) ([]CompatibilityShim, error) {
	normalized := make([]CompatibilityShim, 0, len(shims))
	seen := make(map[string]struct{}, len(shims))

	for i, shim := range shims {
		shim = normalizeCompatibilityShim(shim)
		if err := shim.Validate(); err != nil {
			return nil, fmt.Errorf("compatibility shim %d: %w", i, err)
		}
		if _, exists := seen[shim.ID]; exists {
			return nil, fmt.Errorf("%w %q", ErrDuplicateCompatibilityShim, shim.ID)
		}
		seen[shim.ID] = struct{}{}
		normalized = append(normalized, shim)
	}

	sort.Slice(normalized, func(i, j int) bool {
		return compatibilityShimLess(normalized[i], normalized[j])
	})
	return normalized, nil
}

func normalizeCompatibilityShim(shim CompatibilityShim) CompatibilityShim {
	shim.ID = strings.TrimSpace(shim.ID)
	shim.Subject = strings.TrimSpace(shim.Subject)
	shim.Message = strings.TrimSpace(shim.Message)
	if len(shim.References) > 0 {
		references := make([]CompatibilityReference, len(shim.References))
		for i, reference := range shim.References {
			references[i] = CompatibilityReference{
				Label: strings.TrimSpace(reference.Label),
				URL:   strings.TrimSpace(reference.URL),
			}
		}
		shim.References = references
	}
	return shim
}

func compatibilityShimLess(a, b CompatibilityShim) bool {
	if a.ID != b.ID {
		return a.ID < b.ID
	}
	if a.Subject != b.Subject {
		return a.Subject < b.Subject
	}
	if compareMinorPins(a.APIRange.Min, b.APIRange.Min) != 0 {
		return compareMinorPins(a.APIRange.Min, b.APIRange.Min) < 0
	}
	if compareMinorPins(a.APIRange.Max, b.APIRange.Max) != 0 {
		return compareMinorPins(a.APIRange.Max, b.APIRange.Max) < 0
	}
	if compareMinorPins(a.RuntimeRange.Min, b.RuntimeRange.Min) != 0 {
		return compareMinorPins(a.RuntimeRange.Min, b.RuntimeRange.Min) < 0
	}
	if compareMinorPins(a.RuntimeRange.Max, b.RuntimeRange.Max) != 0 {
		return compareMinorPins(a.RuntimeRange.Max, b.RuntimeRange.Max) < 0
	}
	if a.Message != b.Message {
		return a.Message < b.Message
	}
	return a.Severity < b.Severity
}

func normalizeRuntimeCompatibilitySeverity(severity RuntimeCompatibilitySeverity) RuntimeCompatibilitySeverity {
	if severity == 0 {
		return RuntimeCompatibilitySeverityWarning
	}
	return severity
}

func isValidRuntimeCompatibilitySeverity(severity RuntimeCompatibilitySeverity) bool {
	switch severity {
	case 0, RuntimeCompatibilitySeverityError, RuntimeCompatibilitySeverityWarning, RuntimeCompatibilitySeverityInfo:
		return true
	default:
		return false
	}
}
