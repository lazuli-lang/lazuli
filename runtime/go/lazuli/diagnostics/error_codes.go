package diagnostics

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"unicode"
)

var (
	// ErrInvalidErrorCode reports an error code registry entry that is missing
	// required metadata or does not use the canonical code namespace shape.
	ErrInvalidErrorCode = errors.New("lazuli/diagnostics: invalid error code")

	// ErrDuplicateErrorCode reports multiple registry entries for the same code.
	ErrDuplicateErrorCode = errors.New("lazuli/diagnostics: duplicate error code")

	// ErrUnknownErrorCode reports a required lookup for a code outside a registry.
	ErrUnknownErrorCode = errors.New("lazuli/diagnostics: unknown error code")
)

// ErrorCodeDefinition is one framework-owned error code registration.
type ErrorCodeDefinition struct {
	Code             Code
	Namespace        Family
	Severity         Severity
	Owner            string
	DocumentationURL string
}

// ErrorCodeEntry is kept as a descriptive alias for registry callers that use
// entry terminology.
type ErrorCodeEntry = ErrorCodeDefinition

// ErrorCodeRegistry is a validated registry of error code definitions.
type ErrorCodeRegistry struct {
	definitions []ErrorCodeDefinition
	byCode      map[Code]ErrorCodeDefinition
}

// NewErrorCodeRegistry builds a validated registry. Entries are stored in
// deterministic code order regardless of input order.
func NewErrorCodeRegistry(definitions ...ErrorCodeDefinition) (*ErrorCodeRegistry, error) {
	normalized, err := normalizeErrorCodeDefinitions(definitions)
	if err != nil {
		return nil, err
	}
	return newErrorCodeRegistryFromNormalized(normalized), nil
}

// ValidateErrorCodeDefinitions checks registry entries without mutating the
// input slice.
func ValidateErrorCodeDefinitions(definitions []ErrorCodeDefinition) error {
	_, err := normalizeErrorCodeDefinitions(definitions)
	return err
}

// SortedErrorCodeDefinitions returns a validated, normalized, deterministically
// sorted copy of definitions.
func SortedErrorCodeDefinitions(definitions []ErrorCodeDefinition) ([]ErrorCodeDefinition, error) {
	normalized, err := normalizeErrorCodeDefinitions(definitions)
	if err != nil {
		return nil, err
	}
	return normalized, nil
}

// ValidateErrorCodeNamespace checks that code uses namespace and the canonical
// namespace-NNN shape.
func ValidateErrorCodeNamespace(code Code, namespace Family) error {
	cleanCode := Code(strings.TrimSpace(string(code)))
	cleanNamespace := Family(strings.TrimSpace(string(namespace)))

	var errs []error
	if err := validateErrorCodeShape(cleanCode); err != nil {
		errs = append(errs, err)
	}
	if err := validateErrorCodeNamespaceToken(cleanNamespace); err != nil {
		errs = append(errs, err)
	}
	if len(errs) == 0 && cleanCode.Family() != cleanNamespace {
		errs = append(errs, fmt.Errorf("code namespace is %q, want %q", cleanCode.Family(), cleanNamespace))
	}
	if err := errors.Join(errs...); err != nil {
		return fmt.Errorf("%w: %w", ErrInvalidErrorCode, err)
	}
	return nil
}

// Register adds one validated entry. The zero value registry is usable.
func (r *ErrorCodeRegistry) Register(definition ErrorCodeDefinition) error {
	if r == nil {
		return fmt.Errorf("%w: registry is nil", ErrInvalidErrorCode)
	}

	clean, err := normalizeErrorCodeDefinition(definition, -1)
	if err != nil {
		return err
	}
	r.ensureIndex()
	if _, ok := r.byCode[clean.Code]; ok {
		return fmt.Errorf("%w: %s", ErrDuplicateErrorCode, clean.Code)
	}

	r.byCode[clean.Code] = clean
	r.definitions = append(r.definitions, clean)
	sortErrorCodeDefinitions(r.definitions)
	return nil
}

// Lookup returns the definition for code.
func (r *ErrorCodeRegistry) Lookup(code Code) (ErrorCodeDefinition, bool) {
	if r == nil || r.byCode == nil {
		return ErrorCodeDefinition{}, false
	}
	definition, ok := r.byCode[Code(strings.TrimSpace(string(code)))]
	return definition, ok
}

// LookupRequired returns the definition for code or ErrUnknownErrorCode.
func (r *ErrorCodeRegistry) LookupRequired(code Code) (ErrorCodeDefinition, error) {
	if definition, ok := r.Lookup(code); ok {
		return definition, nil
	}
	return ErrorCodeDefinition{}, fmt.Errorf("%w: %s", ErrUnknownErrorCode, strings.TrimSpace(string(code)))
}

// Definitions returns registry entries in deterministic code order.
func (r *ErrorCodeRegistry) Definitions() []ErrorCodeDefinition {
	if r == nil || len(r.definitions) == 0 {
		return nil
	}
	return append([]ErrorCodeDefinition(nil), r.definitions...)
}

// Codes returns registry codes in deterministic code order.
func (r *ErrorCodeRegistry) Codes() []Code {
	if r == nil || len(r.definitions) == 0 {
		return nil
	}
	codes := make([]Code, 0, len(r.definitions))
	for _, definition := range r.definitions {
		codes = append(codes, definition.Code)
	}
	return codes
}

func normalizeErrorCodeDefinitions(definitions []ErrorCodeDefinition) ([]ErrorCodeDefinition, error) {
	normalized := make([]ErrorCodeDefinition, 0, len(definitions))
	seen := make(map[Code]int, len(definitions))

	var errs []error
	for i, definition := range definitions {
		clean, err := normalizeErrorCodeDefinition(definition, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if first, ok := seen[clean.Code]; ok {
			errs = append(errs, fmt.Errorf("%w: definition[%d] %s also appears at definition[%d]", ErrDuplicateErrorCode, i, clean.Code, first))
			continue
		}
		seen[clean.Code] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	sortErrorCodeDefinitions(normalized)
	return normalized, nil
}

func normalizeErrorCodeDefinition(definition ErrorCodeDefinition, index int) (ErrorCodeDefinition, error) {
	clean := ErrorCodeDefinition{
		Code:             Code(strings.TrimSpace(string(definition.Code))),
		Namespace:        Family(strings.TrimSpace(string(definition.Namespace))),
		Severity:         definition.Severity,
		Owner:            strings.TrimSpace(definition.Owner),
		DocumentationURL: strings.TrimSpace(definition.DocumentationURL),
	}

	var errs []error
	if err := ValidateErrorCodeNamespace(clean.Code, clean.Namespace); err != nil {
		errs = append(errs, err)
	}
	if !isKnownErrorCodeSeverity(clean.Severity) {
		errs = append(errs, errors.New("severity must be error, warning, info, or hint"))
	}
	if clean.Owner == "" {
		errs = append(errs, errors.New("owner is required"))
	} else if errorCodeHasControl(clean.Owner) {
		errs = append(errs, errors.New("owner contains control characters"))
	}
	if err := validateErrorCodeDocumentationURL(clean.DocumentationURL); err != nil {
		errs = append(errs, fmt.Errorf("documentation_url %w", err))
	}

	if err := errors.Join(errs...); err != nil {
		if index >= 0 {
			return ErrorCodeDefinition{}, fmt.Errorf("%w: definition[%d]: %w", ErrInvalidErrorCode, index, err)
		}
		return ErrorCodeDefinition{}, fmt.Errorf("%w: %w", ErrInvalidErrorCode, err)
	}
	return clean, nil
}

func newErrorCodeRegistryFromNormalized(definitions []ErrorCodeDefinition) *ErrorCodeRegistry {
	registry := &ErrorCodeRegistry{
		definitions: append([]ErrorCodeDefinition(nil), definitions...),
		byCode:      make(map[Code]ErrorCodeDefinition, len(definitions)),
	}
	for _, definition := range definitions {
		registry.byCode[definition.Code] = definition
	}
	return registry
}

func (r *ErrorCodeRegistry) ensureIndex() {
	if r.byCode == nil {
		r.byCode = make(map[Code]ErrorCodeDefinition, len(r.definitions))
		for _, definition := range r.definitions {
			r.byCode[definition.Code] = definition
		}
	}
}

func sortErrorCodeDefinitions(definitions []ErrorCodeDefinition) {
	sort.SliceStable(definitions, func(i, j int) bool {
		return definitions[i].Code < definitions[j].Code
	})
}

func validateErrorCodeShape(code Code) error {
	value := string(code)
	if value == "" {
		return errors.New("code is required")
	}

	index := strings.LastIndex(value, "-")
	if index <= 0 || index == len(value)-1 {
		return errors.New("code must use NAMESPACE-NNN shape")
	}
	if err := validateErrorCodeNamespaceToken(Family(value[:index])); err != nil {
		return err
	}

	sequence := value[index+1:]
	if len(sequence) != 3 {
		return errors.New("code sequence must be exactly three digits")
	}
	for _, r := range sequence {
		if r < '0' || r > '9' {
			return errors.New("code sequence must be numeric")
		}
	}
	return nil
}

func validateErrorCodeNamespaceToken(namespace Family) error {
	value := string(namespace)
	if value == "" {
		return errors.New("namespace is required")
	}
	if value == string(FamilyUnknown) {
		return errors.New("namespace must not be UNKNOWN")
	}
	if strings.TrimSpace(value) != value {
		return errors.New("namespace must not contain surrounding whitespace")
	}
	if errorCodeHasControl(value) {
		return errors.New("namespace contains control characters")
	}
	for _, segment := range strings.Split(value, "-") {
		if segment == "" {
			return errors.New("namespace must use non-empty hyphen-separated segments")
		}
		for _, r := range segment {
			if !isErrorCodeNamespaceRune(r) {
				return errors.New("namespace must contain only A-Z, 0-9, and hyphen")
			}
		}
	}
	return nil
}

func validateErrorCodeDocumentationURL(raw string) error {
	if raw == "" {
		return errors.New("is required")
	}
	if errorCodeHasControl(raw) {
		return errors.New("contains control characters")
	}

	parsed, err := url.Parse(raw)
	if err != nil {
		return fmt.Errorf("is invalid: %w", err)
	}
	if parsed.IsAbs() {
		switch parsed.Scheme {
		case "http", "https":
		default:
			return errors.New("must use http or https")
		}
		if parsed.Host == "" {
			return errors.New("must include host")
		}
		return nil
	}
	if strings.HasPrefix(raw, "/") && !strings.HasPrefix(raw, "//") && !strings.Contains(raw, "\\") {
		return nil
	}
	return errors.New("must be an absolute http(s) URL or root-relative path")
}

func isKnownErrorCodeSeverity(severity Severity) bool {
	switch severity {
	case SeverityError, SeverityWarning, SeverityInfo, SeverityHint:
		return true
	default:
		return false
	}
}

func isErrorCodeNamespaceRune(r rune) bool {
	return (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9')
}

func errorCodeHasControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}
