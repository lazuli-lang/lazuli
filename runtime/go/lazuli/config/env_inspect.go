package config

import (
	"errors"
	"os"
	"sort"
	"strings"
)

const (
	// EnvSourceEnv identifies values read directly from an environment variable.
	EnvSourceEnv = "env"
	// EnvSourceDefault identifies values supplied by a Field default.
	EnvSourceDefault = "default"
	// EnvSourceMissing identifies declared fields with no env value or default.
	EnvSourceMissing = "missing"
)

// EnvInspectOptions controls environment inspection filtering and redaction.
type EnvInspectOptions struct {
	// Prefixes filters inspected fields by environment variable prefix. Empty
	// includes every declared field.
	Prefixes []string
	// RedactKeys are config/env key names whose values are replaced. Nil uses
	// the default sensitive key set. A non-nil empty slice disables key-based
	// redaction. Field.Redact always redacts the field value.
	RedactKeys []string
	// Mask replaces sensitive values. Empty uses DefaultInspectMask.
	Mask string
}

// EnvInspectionEntry is one declared environment-backed config field.
type EnvInspectionEntry struct {
	Name     string `json:"name"`
	Env      string `json:"env"`
	Type     Kind   `json:"type,omitempty"`
	Required bool   `json:"required,omitempty"`
	Source   string `json:"source"`
	Value    any    `json:"value,omitempty"`
	Missing  bool   `json:"missing,omitempty"`
	Redacted bool   `json:"redacted,omitempty"`
}

// EnvInspectionSummary contains deterministic counts for an env inspection.
type EnvInspectionSummary struct {
	Total           int `json:"total"`
	Required        int `json:"required"`
	Optional        int `json:"optional"`
	Present         int `json:"present"`
	Missing         int `json:"missing"`
	MissingRequired int `json:"missing_required,omitempty"`
	FromEnv         int `json:"from_env"`
	FromDefault     int `json:"from_default"`
	Redacted        int `json:"redacted,omitempty"`
}

// EnvInspectionReport is the display-safe result of inspecting a Schema.
type EnvInspectionReport struct {
	Entries []EnvInspectionEntry `json:"entries"`
	Summary EnvInspectionSummary `json:"summary"`
}

// InspectEnv inspects declared environment-backed config fields without
// querying unrelated environment variables.
//
// Missing required values are reported in the summary, not returned as
// ErrRequired. Invalid schema fields and invalid present/default values are
// returned as joined errors while valid entries are still reported.
func InspectEnv(schema Schema, lookup LookupFunc, options EnvInspectOptions) (EnvInspectionReport, error) {
	if lookup == nil {
		lookup = os.LookupEnv
	}

	settings := newInspectSettings(InspectOptions{
		RedactKeys: options.RedactKeys,
		Mask:       options.Mask,
	})
	prefixes := cleanEnvPrefixes(options.Prefixes)

	seen := make(map[string]struct{}, len(schema))
	entries := make([]EnvInspectionEntry, 0, len(schema))
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

		if !matchesEnvPrefix(normalized.Env, prefixes) {
			continue
		}

		entry, raw := inspectEnvField(normalized, lookup, settings)
		if entry.Source != EnvSourceMissing {
			parsed, err := parseValue(normalized.Type, raw)
			if err != nil {
				errs = append(errs, fieldError(normalized, ErrInvalidValue, err.Error()))
			} else if !entry.Redacted {
				entry.Value = parsed
			}
		}
		entries = append(entries, entry)
	}

	SortEnvInspectionEntries(entries)
	return EnvInspectionReport{
		Entries: entries,
		Summary: summarizeEnvInspection(entries),
	}, errors.Join(errs...)
}

// RequiredEnvVars returns the required environment variable names declared by
// schema in stable sorted order.
func RequiredEnvVars(schema Schema) ([]string, error) {
	vars := make([]string, 0, len(schema))
	seenNames := make(map[string]struct{}, len(schema))
	seenVars := make(map[string]struct{}, len(schema))
	var errs []error

	for _, field := range schema {
		normalized, err := normalizeField(field)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if _, ok := seenNames[normalized.Name]; ok {
			errs = append(errs, fieldError(normalized, ErrInvalidField, "duplicate field name"))
			continue
		}
		seenNames[normalized.Name] = struct{}{}
		if !normalized.Required {
			continue
		}
		if _, ok := seenVars[normalized.Env]; ok {
			continue
		}
		seenVars[normalized.Env] = struct{}{}
		vars = append(vars, normalized.Env)
	}

	sort.Strings(vars)
	return vars, errors.Join(errs...)
}

// MissingEnvVars returns required environment variable names that are absent
// without a default or explicitly empty, in stable sorted order.
func MissingEnvVars(schema Schema, lookup LookupFunc) ([]string, error) {
	if lookup == nil {
		lookup = os.LookupEnv
	}

	missing := make([]string, 0, len(schema))
	seenNames := make(map[string]struct{}, len(schema))
	seenVars := make(map[string]struct{}, len(schema))
	var errs []error

	for _, field := range schema {
		normalized, err := normalizeField(field)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if _, ok := seenNames[normalized.Name]; ok {
			errs = append(errs, fieldError(normalized, ErrInvalidField, "duplicate field name"))
			continue
		}
		seenNames[normalized.Name] = struct{}{}
		if !normalized.Required {
			continue
		}
		raw, ok := lookup(normalized.Env)
		if ok && raw != "" {
			continue
		}
		if !ok && normalized.HasDefault {
			continue
		}
		if _, ok := seenVars[normalized.Env]; ok {
			continue
		}
		seenVars[normalized.Env] = struct{}{}
		missing = append(missing, normalized.Env)
	}

	sort.Strings(missing)
	return missing, errors.Join(errs...)
}

// SortEnvInspectionEntries sorts entries by env var name, then config name.
func SortEnvInspectionEntries(entries []EnvInspectionEntry) {
	sort.SliceStable(entries, func(i, j int) bool {
		if entries[i].Env != entries[j].Env {
			return entries[i].Env < entries[j].Env
		}
		return entries[i].Name < entries[j].Name
	})
}

func inspectEnvField(field Field, lookup LookupFunc, settings inspectSettings) (EnvInspectionEntry, string) {
	entry := EnvInspectionEntry{
		Name:     field.Name,
		Env:      field.Env,
		Type:     field.Type,
		Required: field.Required,
		Source:   EnvSourceMissing,
		Missing:  true,
	}
	raw := ""

	envValue, ok := lookup(field.Env)
	if ok && (!field.Required || envValue != "") {
		entry.Source = EnvSourceEnv
		entry.Value = envValue
		entry.Missing = false
		raw = envValue
	} else if !ok && field.HasDefault {
		entry.Source = EnvSourceDefault
		entry.Value = field.Default
		entry.Missing = false
		raw = field.Default
	}

	if entry.Missing {
		return entry, raw
	}
	if field.Redact ||
		shouldRedactInspectKey(field.Name, settings.redactKeys) ||
		shouldRedactInspectKey(field.Env, settings.redactKeys) {
		entry.Value = settings.mask
		entry.Redacted = true
	}
	return entry, raw
}

func summarizeEnvInspection(entries []EnvInspectionEntry) EnvInspectionSummary {
	var summary EnvInspectionSummary
	summary.Total = len(entries)

	for _, entry := range entries {
		if entry.Required {
			summary.Required++
		} else {
			summary.Optional++
		}
		if entry.Missing {
			summary.Missing++
			if entry.Required {
				summary.MissingRequired++
			}
		} else {
			summary.Present++
		}
		switch entry.Source {
		case EnvSourceEnv:
			summary.FromEnv++
		case EnvSourceDefault:
			summary.FromDefault++
		}
		if entry.Redacted {
			summary.Redacted++
		}
	}

	return summary
}

func cleanEnvPrefixes(prefixes []string) []string {
	if len(prefixes) == 0 {
		return nil
	}
	cleaned := make([]string, 0, len(prefixes))
	for _, prefix := range prefixes {
		if prefix = strings.TrimSpace(prefix); prefix != "" {
			cleaned = append(cleaned, prefix)
		}
	}
	return cleaned
}

func matchesEnvPrefix(env string, prefixes []string) bool {
	if len(prefixes) == 0 {
		return true
	}
	for _, prefix := range prefixes {
		if strings.HasPrefix(env, prefix) {
			return true
		}
	}
	return false
}
