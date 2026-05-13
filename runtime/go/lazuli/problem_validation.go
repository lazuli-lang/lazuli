package lazuli

import (
	"sort"
	"strings"
)

// ProblemValidationViolationsExtension is the RFC problem extension member
// used for validation failures.
const ProblemValidationViolationsExtension = "violations"

// ProblemValidationViolation is the normalized validation entry exposed in a
// Problem extension. The shape is intentionally small and deterministic so
// generated validators and request validators can share one client-facing
// payload contract.
type ProblemValidationViolation struct {
	Location  string `json:"location,omitempty"`
	Field     string `json:"field,omitempty"`
	Path      string `json:"path,omitempty"`
	Code      string `json:"code,omitempty"`
	Message   string `json:"message,omitempty"`
	InputType string `json:"input_type,omitempty"`
}

// ProblemValidationField describes a field-level validation failure before it
// is normalized into the "violations" Problem extension.
type ProblemValidationField struct {
	Field     string
	Path      string
	Code      string
	Message   string
	InputType string
}

// ValidationProblemExtensions returns a fresh Problem.Extensions map for a
// validation failure. Violations are copied, blank entries are omitted, and the
// result is sorted by location, field, path, code, message, then input type.
func ValidationProblemExtensions(violations ...ProblemValidationViolation) map[string]any {
	return map[string]any{
		"code":                               CodeValidationFailed,
		ProblemValidationViolationsExtension: normalizeProblemValidationViolations(violations),
	}
}

// ValidationProblemExtensionsFromFields converts field validation failures
// into the deterministic Problem extension shape.
func ValidationProblemExtensionsFromFields(fields ...ProblemValidationField) map[string]any {
	return ValidationProblemExtensions(ProblemValidationViolationsFromFields(fields...)...)
}

// ValidationProblemExtensionsFromViolations converts request validation
// violations into the deterministic Problem extension shape. Status is a
// response classification hint and is not exposed in the Problem JSON.
func ValidationProblemExtensionsFromViolations(violations ...ValidationViolation) map[string]any {
	return ValidationProblemExtensions(ProblemValidationViolationsFromViolations(violations...)...)
}

// ProblemValidationViolationsFromFields normalizes field validation failures
// into the Problem "violations" extension entry shape.
func ProblemValidationViolationsFromFields(fields ...ProblemValidationField) []ProblemValidationViolation {
	out := make([]ProblemValidationViolation, 0, len(fields))
	for _, field := range fields {
		out = append(out, ProblemValidationViolation{
			Field:     field.Field,
			Path:      field.Path,
			Code:      field.Code,
			Message:   field.Message,
			InputType: field.InputType,
		})
	}
	return normalizeProblemValidationViolations(out)
}

// ProblemValidationViolationsFromFieldErrors converts typed FieldError values
// into the Problem "violations" extension entry shape.
func ProblemValidationViolationsFromFieldErrors(fields ...*FieldError) []ProblemValidationViolation {
	out := make([]ProblemValidationViolation, 0, len(fields))
	for _, field := range fields {
		if field == nil {
			continue
		}
		code := field.Base.Code
		if code == "" {
			code = problemValidationFieldReasonCode(field.Reason)
		}
		out = append(out, ProblemValidationViolation{
			Field:     field.Field,
			Path:      field.Path,
			Code:      code,
			Message:   field.Base.Message,
			InputType: field.InputType,
		})
	}
	return normalizeProblemValidationViolations(out)
}

// ProblemValidationViolationsFromViolations converts request validation
// violations into the Problem "violations" extension entry shape.
func ProblemValidationViolationsFromViolations(violations ...ValidationViolation) []ProblemValidationViolation {
	out := make([]ProblemValidationViolation, 0, len(violations))
	for _, violation := range violations {
		out = append(out, ProblemValidationViolation{
			Location: violation.Location,
			Field:    violation.Field,
			Code:     violation.Code,
			Message:  violation.Message,
		})
	}
	return normalizeProblemValidationViolations(out)
}

func normalizeProblemValidationViolations(violations []ProblemValidationViolation) []ProblemValidationViolation {
	out := make([]ProblemValidationViolation, 0, len(violations))
	for _, violation := range violations {
		clean := ProblemValidationViolation{
			Location:  strings.TrimSpace(violation.Location),
			Field:     strings.TrimSpace(violation.Field),
			Path:      strings.TrimSpace(violation.Path),
			Code:      strings.TrimSpace(violation.Code),
			Message:   strings.TrimSpace(violation.Message),
			InputType: strings.TrimSpace(violation.InputType),
		}
		if problemValidationViolationEmpty(clean) {
			continue
		}
		out = append(out, clean)
	}
	sort.SliceStable(out, func(i, j int) bool {
		return compareProblemValidationViolation(out[i], out[j]) < 0
	})
	return out
}

func problemValidationViolationEmpty(violation ProblemValidationViolation) bool {
	return violation.Location == "" &&
		violation.Field == "" &&
		violation.Path == "" &&
		violation.Code == "" &&
		violation.Message == "" &&
		violation.InputType == ""
}

func compareProblemValidationViolation(left, right ProblemValidationViolation) int {
	for _, pair := range [][2]string{
		{left.Location, right.Location},
		{left.Field, right.Field},
		{left.Path, right.Path},
		{left.Code, right.Code},
		{left.Message, right.Message},
		{left.InputType, right.InputType},
	} {
		if cmp := strings.Compare(pair[0], pair[1]); cmp != 0 {
			return cmp
		}
	}
	return 0
}

func problemValidationFieldReasonCode(reason FieldReason) string {
	switch reason {
	case FieldReasonRequired:
		return "required"
	case FieldReasonInvalidFormat:
		return "invalid_format"
	case FieldReasonOutOfRange:
		return "out_of_range"
	case FieldReasonMismatch:
		return "mismatch"
	case FieldReasonUnknownEnum:
		return "unknown_enum"
	default:
		return "invalid"
	}
}
