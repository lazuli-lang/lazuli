// Package config provides small runtime helpers for loading Lazuli
// application configuration from environment variables.
package config

import (
	"errors"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"
)

// Kind identifies the parser used for a schema field.
type Kind string

const (
	// String parses the field as a string.
	String Kind = "string"
	// Int parses the field as an int.
	Int Kind = "int"
	// Bool parses the field as a bool accepted by strconv.ParseBool.
	Bool Kind = "bool"
	// Duration parses the field as a time.Duration accepted by time.ParseDuration.
	Duration Kind = "duration"
)

var (
	// ErrInvalidField indicates an invalid schema field definition.
	ErrInvalidField = errors.New("lazuli/config: invalid env field")
	// ErrRequired indicates a required field was absent or empty.
	ErrRequired = errors.New("lazuli/config: required env field missing")
	// ErrInvalidValue indicates an environment value or default could not be parsed.
	ErrInvalidValue = errors.New("lazuli/config: invalid env value")
)

// Field describes one environment-backed config value.
//
// Values are keyed by Name. Env is the environment variable to read; if Env is
// empty, Name is used. Type defaults to String. Default is used only when the
// variable is absent and HasDefault is true. Redact marks the value as sensitive
// for logs and diagnostics.
type Field struct {
	Name       string
	Env        string
	Type       Kind
	Required   bool
	Default    string
	HasDefault bool
	Redact     bool
}

// Schema is the ordered list of environment fields to load.
type Schema []Field

// LookupFunc reads an environment variable by name.
type LookupFunc func(string) (string, bool)

// Values contains parsed configuration values keyed by Field.Name.
type Values map[string]Value

// Value is a parsed config value with source and redaction metadata.
type Value struct {
	Name      string
	Env       string
	Type      Kind
	Raw       string
	Parsed    any
	Defaulted bool
	Redact    bool
}

// FieldError reports a schema or value error for a specific field.
//
// Use errors.Is against ErrInvalidField, ErrRequired, and ErrInvalidValue to
// classify the failure.
type FieldError struct {
	Name string
	Env  string
	Err  error
}

// Error returns a stable, human-readable field error.
func (e *FieldError) Error() string {
	if e == nil {
		return "<nil>"
	}
	target := e.Name
	if target == "" {
		target = e.Env
	}
	if e.Env != "" && e.Env != target {
		target = fmt.Sprintf("%s env %s", target, e.Env)
	}
	if target == "" {
		return fmt.Sprintf("lazuli/config: env field: %v", e.Err)
	}
	return fmt.Sprintf("lazuli/config: env field %s: %v", target, e.Err)
}

// Unwrap exposes the classified error for errors.Is.
func (e *FieldError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

// LoadEnv loads and parses the declared schema with lookup.
//
// If lookup is nil, os.LookupEnv is used. Only declared fields are queried, so
// unrelated environment variables never cause validation errors.
func LoadEnv(schema Schema, lookup LookupFunc) (Values, error) {
	if lookup == nil {
		lookup = os.LookupEnv
	}

	values := make(Values, len(schema))
	seen := make(map[string]struct{}, len(schema))
	var errs []error

	for _, field := range schema {
		normalized, err := normalizeField(field)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if _, ok := seen[normalized.Name]; ok {
			errs = append(errs, fieldError(normalized, ErrInvalidField, "duplicate field name"))
			continue
		}
		seen[normalized.Name] = struct{}{}

		raw, ok := lookup(normalized.Env)
		defaulted := false
		if !ok && normalized.HasDefault {
			raw = normalized.Default
			ok = true
			defaulted = true
		}
		if !ok {
			if normalized.Required {
				errs = append(errs, fieldError(normalized, ErrRequired, "value is required"))
			}
			continue
		}
		if normalized.Required && raw == "" && !defaulted {
			errs = append(errs, fieldError(normalized, ErrRequired, "value is required"))
			continue
		}

		parsed, err := parseValue(normalized.Type, raw)
		if err != nil {
			errs = append(errs, fieldError(normalized, ErrInvalidValue, err.Error()))
			continue
		}
		values[normalized.Name] = Value{
			Name:      normalized.Name,
			Env:       normalized.Env,
			Type:      normalized.Type,
			Raw:       raw,
			Parsed:    parsed,
			Defaulted: defaulted,
			Redact:    normalized.Redact,
		}
	}

	return values, errors.Join(errs...)
}

// Get returns the parsed value and metadata for name.
func (v Values) Get(name string) (Value, bool) {
	value, ok := v[name]
	return value, ok
}

// Any returns the parsed value for name.
func (v Values) Any(name string) (any, bool) {
	value, ok := v[name]
	if !ok {
		return nil, false
	}
	return value.Parsed, true
}

// String returns the string value for name.
func (v Values) String(name string) (string, bool) {
	value, ok := v[name]
	if !ok {
		return "", false
	}
	parsed, ok := value.Parsed.(string)
	return parsed, ok
}

// Int returns the int value for name.
func (v Values) Int(name string) (int, bool) {
	value, ok := v[name]
	if !ok {
		return 0, false
	}
	parsed, ok := value.Parsed.(int)
	return parsed, ok
}

// Bool returns the bool value for name.
func (v Values) Bool(name string) (bool, bool) {
	value, ok := v[name]
	if !ok {
		return false, false
	}
	parsed, ok := value.Parsed.(bool)
	return parsed, ok
}

// Duration returns the time.Duration value for name.
func (v Values) Duration(name string) (time.Duration, bool) {
	value, ok := v[name]
	if !ok {
		return 0, false
	}
	parsed, ok := value.Parsed.(time.Duration)
	return parsed, ok
}

// Map returns parsed values without metadata.
func (v Values) Map() map[string]any {
	out := make(map[string]any, len(v))
	for name, value := range v {
		out[name] = value.Parsed
	}
	return out
}

// RedactedMap returns parsed values with redacted fields replaced by mask.
func (v Values) RedactedMap(mask string) map[string]any {
	out := make(map[string]any, len(v))
	for name, value := range v {
		if value.Redact {
			out[name] = mask
			continue
		}
		out[name] = value.Parsed
	}
	return out
}

func normalizeField(field Field) (Field, error) {
	if field.Name == "" {
		field.Name = field.Env
	}
	if field.Env == "" {
		field.Env = field.Name
	}
	if field.Type == "" {
		field.Type = String
	}
	if field.Name == "" || field.Env == "" {
		return field, fieldError(field, ErrInvalidField, "name or env is required")
	}
	if !validKind(field.Type) {
		return field, fieldError(field, ErrInvalidField, fmt.Sprintf("unsupported type %q", field.Type))
	}
	return field, nil
}

func validKind(kind Kind) bool {
	switch kind {
	case String, Int, Bool, Duration:
		return true
	default:
		return false
	}
}

func parseValue(kind Kind, raw string) (any, error) {
	switch kind {
	case String:
		return raw, nil
	case Int:
		value, err := strconv.Atoi(strings.TrimSpace(raw))
		if err != nil {
			return nil, err
		}
		return value, nil
	case Bool:
		value, err := strconv.ParseBool(strings.TrimSpace(raw))
		if err != nil {
			return nil, err
		}
		return value, nil
	case Duration:
		value, err := time.ParseDuration(strings.TrimSpace(raw))
		if err != nil {
			return nil, err
		}
		return value, nil
	default:
		return nil, fmt.Errorf("unsupported type %q", kind)
	}
}

func fieldError(field Field, kind error, detail string) *FieldError {
	err := kind
	if detail != "" {
		err = fmt.Errorf("%w: %s", kind, detail)
	}
	return &FieldError{
		Name: field.Name,
		Env:  field.Env,
		Err:  err,
	}
}
