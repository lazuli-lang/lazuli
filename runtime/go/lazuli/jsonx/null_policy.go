package jsonx

import (
	"errors"
	"fmt"
	"strings"
	"unicode/utf8"
)

var (
	// ErrInvalidNullPolicy is returned when generated JSON field metadata names
	// an unsupported null policy.
	ErrInvalidNullPolicy = errors.New("lazuli/jsonx: invalid null policy")

	// ErrInvalidFieldMetadata is returned when generated JSON field metadata is
	// incomplete or internally inconsistent.
	ErrInvalidFieldMetadata = errors.New("lazuli/jsonx: invalid field metadata")

	// ErrInvalidFieldState is returned when a generated serializer reports an
	// impossible field value state.
	ErrInvalidFieldState = errors.New("lazuli/jsonx: invalid field state")

	// ErrJSONFieldRequired is returned when a required JSON field is absent.
	ErrJSONFieldRequired = errors.New("lazuli/jsonx: json field required")

	// ErrJSONNullDisallowed is returned when a non-nullable JSON field is null.
	ErrJSONNullDisallowed = errors.New("lazuli/jsonx: json null disallowed")
)

// NullPolicy controls how generated serializers handle null and empty values.
type NullPolicy string

const (
	// NullPolicyDefault includes non-empty values and writes null only for
	// fields marked nullable.
	NullPolicyDefault NullPolicy = ""

	// NullPolicyOmitEmpty omits absent, null, or empty values.
	NullPolicyOmitEmpty NullPolicy = "omitempty"

	// NullPolicyInclude includes empty values and writes null for nullable
	// fields.
	NullPolicyInclude NullPolicy = "include"

	// NullPolicyNull writes JSON null for null values and for empty values on
	// nullable fields.
	NullPolicyNull NullPolicy = "null"

	// NullPolicyNullAsEmpty asks generated serializers to treat JSON null as the
	// type's empty representation.
	NullPolicyNullAsEmpty NullPolicy = "null_as_empty"
)

// NormalizeNullPolicy trims and canonicalizes policy names accepted by Lazuli
// decorators.
func NormalizeNullPolicy(policy NullPolicy) (NullPolicy, error) {
	value := strings.ToLower(strings.TrimSpace(string(policy)))
	value = strings.ReplaceAll(value, "-", "_")

	switch value {
	case "":
		return NullPolicyDefault, nil
	case "omitempty", "omit", "omit_empty":
		return NullPolicyOmitEmpty, nil
	case string(NullPolicyInclude):
		return NullPolicyInclude, nil
	case string(NullPolicyNull):
		return NullPolicyNull, nil
	case string(NullPolicyNullAsEmpty):
		return NullPolicyNullAsEmpty, nil
	default:
		return "", fmt.Errorf("%w: %q", ErrInvalidNullPolicy, string(policy))
	}
}

// Validate reports whether policy is supported.
func (p NullPolicy) Validate() error {
	_, err := NormalizeNullPolicy(p)
	return err
}

// String returns the canonical policy name.
func (p NullPolicy) String() string {
	normalized, err := NormalizeNullPolicy(p)
	if err != nil {
		return string(p)
	}
	if normalized == NullPolicyDefault {
		return "default"
	}
	return string(normalized)
}

// FieldMetadata describes one generated JSON field.
type FieldMetadata struct {
	// Name is an optional Go or IR field name used only in diagnostics.
	Name string

	// JSONName is the object member name written to or read from JSON.
	JSONName string

	// Policy controls null and empty value handling for this field.
	Policy NullPolicy

	// Required reports whether the field must be present in JSON.
	Required bool

	// Nullable reports whether JSON null is a valid field value.
	Nullable bool
}

// Normalize returns metadata with trimmed names and a canonical policy.
func (m FieldMetadata) Normalize() (FieldMetadata, error) {
	m.Name = strings.TrimSpace(m.Name)
	m.JSONName = strings.TrimSpace(m.JSONName)

	policy, err := NormalizeNullPolicy(m.Policy)
	if err != nil {
		return FieldMetadata{}, fmt.Errorf("%w: %s: %w", ErrInvalidFieldMetadata, fieldMetadataLabel(m), err)
	}
	m.Policy = policy
	return m, nil
}

// Validate reports whether metadata is usable by generated serializers.
func (m FieldMetadata) Validate() error {
	_, err := normalizeFieldMetadata(m)
	return err
}

// ValidateField validates one generated JSON field metadata value.
func ValidateField(field FieldMetadata) error {
	return field.Validate()
}

// ValidateFields validates a generated JSON object field list and rejects
// duplicate JSON names.
func ValidateFields(fields []FieldMetadata) error {
	var errs []error
	seen := make(map[string]int, len(fields))

	for i, field := range fields {
		normalized, err := normalizeFieldMetadata(field)
		if err != nil {
			errs = append(errs, fmt.Errorf("field[%d]: %w", i, err))
			continue
		}

		if first, ok := seen[normalized.JSONName]; ok {
			errs = append(errs, fmt.Errorf("%w: field[%d] JSONName %q duplicates field[%d]", ErrInvalidFieldMetadata, i, normalized.JSONName, first))
			continue
		}
		seen[normalized.JSONName] = i
	}

	return errors.Join(errs...)
}

// FieldState reports the value state observed by generated serializers. Exactly
// one of Absent, Null, or Empty should be set for special handling; the zero
// state means a present, non-null, non-empty value.
type FieldState struct {
	Absent bool
	Null   bool
	Empty  bool
}

// Validate reports whether the state describes one coherent field value.
func (s FieldState) Validate() error {
	count := 0
	if s.Absent {
		count++
	}
	if s.Null {
		count++
	}
	if s.Empty {
		count++
	}
	if count > 1 {
		return fmt.Errorf("%w: choose only one of absent, null, or empty", ErrInvalidFieldState)
	}
	return nil
}

// FieldDecision tells a generated serializer how to emit one JSON field.
type FieldDecision uint8

const (
	// FieldDecisionInclude writes the field with its current value.
	FieldDecisionInclude FieldDecision = iota

	// FieldDecisionOmit skips the field.
	FieldDecisionOmit

	// FieldDecisionNull writes the field with JSON null.
	FieldDecisionNull

	// FieldDecisionNullAsEmpty writes the field using the type's empty JSON
	// representation rather than JSON null.
	FieldDecisionNullAsEmpty
)

// String returns a stable decision name.
func (d FieldDecision) String() string {
	switch d {
	case FieldDecisionInclude:
		return "include"
	case FieldDecisionOmit:
		return "omit"
	case FieldDecisionNull:
		return "null"
	case FieldDecisionNullAsEmpty:
		return "null_as_empty"
	default:
		return fmt.Sprintf("FieldDecision(%d)", d)
	}
}

// Include reports whether the field should be written with its current value.
func (d FieldDecision) Include() bool {
	return d == FieldDecisionInclude
}

// Omit reports whether the field should be skipped.
func (d FieldDecision) Omit() bool {
	return d == FieldDecisionOmit
}

// WriteNull reports whether the field should be written as JSON null.
func (d FieldDecision) WriteNull() bool {
	return d == FieldDecisionNull
}

// NullAsEmpty reports whether the field should be written as its empty JSON
// representation instead of JSON null.
func (d FieldDecision) NullAsEmpty() bool {
	return d == FieldDecisionNullAsEmpty
}

// DecideField returns the generated serializer action for one field value.
func DecideField(field FieldMetadata, state FieldState) (FieldDecision, error) {
	if err := state.Validate(); err != nil {
		return FieldDecisionOmit, err
	}

	normalized, err := normalizeFieldMetadata(field)
	if err != nil {
		return FieldDecisionOmit, err
	}

	switch {
	case state.Absent:
		if normalized.Required {
			return FieldDecisionOmit, fmt.Errorf("%w: %s", ErrJSONFieldRequired, fieldMetadataLabel(normalized))
		}
		return FieldDecisionOmit, nil
	case state.Null:
		return decideNullField(normalized)
	case state.Empty:
		return decideEmptyField(normalized), nil
	default:
		return FieldDecisionInclude, nil
	}
}

// NullDecision tells a generated decoder how to handle an incoming JSON null.
type NullDecision uint8

const (
	// NullDecisionReject rejects the JSON null token for this field.
	NullDecisionReject NullDecision = iota

	// NullDecisionAccept accepts JSON null as a null field value.
	NullDecisionAccept

	// NullDecisionAsEmpty accepts JSON null as the field type's empty value.
	NullDecisionAsEmpty
)

// String returns a stable decision name.
func (d NullDecision) String() string {
	switch d {
	case NullDecisionReject:
		return "reject"
	case NullDecisionAccept:
		return "accept"
	case NullDecisionAsEmpty:
		return "null_as_empty"
	default:
		return fmt.Sprintf("NullDecision(%d)", d)
	}
}

// Accept reports whether JSON null is allowed for the field.
func (d NullDecision) Accept() bool {
	return d == NullDecisionAccept || d == NullDecisionAsEmpty
}

// NullAsEmpty reports whether JSON null should be decoded as the type's empty
// value.
func (d NullDecision) NullAsEmpty() bool {
	return d == NullDecisionAsEmpty
}

// DecideNull returns the generated decoder action for an incoming JSON null
// token.
func DecideNull(field FieldMetadata) (NullDecision, error) {
	normalized, err := normalizeFieldMetadata(field)
	if err != nil {
		return NullDecisionReject, err
	}

	if normalized.Policy == NullPolicyNullAsEmpty {
		return NullDecisionAsEmpty, nil
	}
	if normalized.Nullable {
		return NullDecisionAccept, nil
	}
	return NullDecisionReject, nil
}

func normalizeFieldMetadata(field FieldMetadata) (FieldMetadata, error) {
	normalized, err := field.Normalize()
	if err != nil {
		return FieldMetadata{}, err
	}

	label := fieldMetadataLabel(normalized)
	var errs []error
	if normalized.JSONName == "" {
		errs = append(errs, fmt.Errorf("%w: %s JSONName is required", ErrInvalidFieldMetadata, label))
	}
	if !validJSONFieldName(normalized.JSONName) {
		errs = append(errs, fmt.Errorf("%w: %s JSONName %q is invalid", ErrInvalidFieldMetadata, label, normalized.JSONName))
	}
	if normalized.Name != "" && !validJSONFieldName(normalized.Name) {
		errs = append(errs, fmt.Errorf("%w: %s Name %q is invalid", ErrInvalidFieldMetadata, label, normalized.Name))
	}
	if normalized.Required && normalized.Policy == NullPolicyOmitEmpty {
		errs = append(errs, fmt.Errorf("%w: %s required field cannot use omitempty policy", ErrInvalidFieldMetadata, label))
	}
	if !normalized.Nullable && normalized.Policy == NullPolicyNull {
		errs = append(errs, fmt.Errorf("%w: %s null policy requires a nullable field", ErrInvalidFieldMetadata, label))
	}

	return normalized, errors.Join(errs...)
}

func decideNullField(field FieldMetadata) (FieldDecision, error) {
	switch field.Policy {
	case NullPolicyOmitEmpty:
		return FieldDecisionOmit, nil
	case NullPolicyNullAsEmpty:
		return FieldDecisionNullAsEmpty, nil
	case NullPolicyNull:
		return FieldDecisionNull, nil
	default:
		if field.Nullable {
			return FieldDecisionNull, nil
		}
		return FieldDecisionOmit, fmt.Errorf("%w: %s", ErrJSONNullDisallowed, fieldMetadataLabel(field))
	}
}

func decideEmptyField(field FieldMetadata) FieldDecision {
	switch field.Policy {
	case NullPolicyOmitEmpty:
		return FieldDecisionOmit
	case NullPolicyNull:
		return FieldDecisionNull
	default:
		return FieldDecisionInclude
	}
}

func fieldMetadataLabel(field FieldMetadata) string {
	if field.Name != "" {
		return field.Name
	}
	if field.JSONName != "" {
		return field.JSONName
	}
	return "field"
}

func validJSONFieldName(name string) bool {
	if name == "" || !utf8.ValidString(name) {
		return false
	}
	for _, r := range name {
		if r < ' ' {
			return false
		}
	}
	return true
}
