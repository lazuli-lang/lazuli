package config

import (
	"reflect"
	"sort"
	"strings"
	"unicode"
)

const (
	// DefaultInspectMask is the replacement used for redacted inspection values.
	DefaultInspectMask = "[REDACTED]"
)

var defaultInspectRedactKeys = []string{
	"access_token",
	"api_key",
	"api_secret",
	"api_token",
	"auth_token",
	"authorization",
	"aws_secret_access_key",
	"bearer_token",
	"client_secret",
	"connection_string",
	"cookie",
	"credential",
	"credentials",
	"csrf_token",
	"database_url",
	"db_url",
	"dsn",
	"encryption_key",
	"hmac_secret",
	"id_token",
	"jwt",
	"oauth_secret",
	"oauth_token",
	"password",
	"password_hash",
	"private_key",
	"refresh_token",
	"secret",
	"secret_access_key",
	"secret_key",
	"secret_token",
	"session",
	"session_cookie",
	"session_id",
	"session_token",
	"signing_key",
	"token",
	"webhook_secret",
}

// InspectionEntry is one flattened, display-safe configuration value.
type InspectionEntry struct {
	Key        string `json:"key"`
	Value      any    `json:"value"`
	Source     string `json:"source,omitempty"`
	Provenance string `json:"provenance,omitempty"`
	Redacted   bool   `json:"redacted,omitempty"`
}

// InspectOptions controls inspection redaction and source metadata.
type InspectOptions struct {
	// Source is the broad origin of inspected values, such as "env" or "file".
	Source string
	// Provenance is the specific origin, such as an env var or file path.
	Provenance string
	// RedactKeys are config/env key names whose values are replaced. Nil uses
	// the default sensitive key set. A non-nil empty slice disables key-based
	// redaction. Matching is case-insensitive and ignores separators.
	RedactKeys []string
	// Mask replaces sensitive values. Empty uses DefaultInspectMask.
	Mask string
}

// FlattenMap returns a copy of values with nested string-keyed maps flattened
// into dot-separated keys.
func FlattenMap(values map[string]any) map[string]any {
	out := make(map[string]any)
	flattenValue(out, "", values)
	return out
}

// InspectMap flattens values, redacts sensitive keys, and returns entries in a
// stable order.
func InspectMap(values map[string]any, options InspectOptions) []InspectionEntry {
	flat := FlattenMap(values)
	settings := newInspectSettings(options)
	source := strings.TrimSpace(options.Source)
	provenance := strings.TrimSpace(options.Provenance)

	entries := make([]InspectionEntry, 0, len(flat))
	for key, value := range flat {
		entry := InspectionEntry{
			Key:        key,
			Value:      value,
			Source:     source,
			Provenance: provenance,
		}
		if shouldRedactInspectKey(key, settings.redactKeys) {
			entry.Value = settings.mask
			entry.Redacted = true
		}
		entries = append(entries, entry)
	}
	SortInspectionEntries(entries)
	return entries
}

// InspectValues returns parsed env Values as redacted, stably sorted inspection
// entries.
//
// When options.Source is empty, Source is "env" for loaded values and
// "default" for defaulted values. When options.Provenance is empty, Provenance
// is the originating env var name.
func InspectValues(values Values, options InspectOptions) []InspectionEntry {
	settings := newInspectSettings(options)
	sourceOverride := strings.TrimSpace(options.Source)
	provenanceOverride := strings.TrimSpace(options.Provenance)

	entries := make([]InspectionEntry, 0, len(values))
	for key, value := range values {
		source := sourceOverride
		if source == "" {
			source = "env"
			if value.Defaulted {
				source = "default"
			}
		}
		provenance := provenanceOverride
		if provenance == "" {
			provenance = value.Env
		}

		entry := InspectionEntry{
			Key:        key,
			Value:      value.Parsed,
			Source:     source,
			Provenance: provenance,
		}
		if value.Redact ||
			shouldRedactInspectKey(key, settings.redactKeys) ||
			shouldRedactInspectKey(value.Env, settings.redactKeys) {
			entry.Value = settings.mask
			entry.Redacted = true
		}
		entries = append(entries, entry)
	}
	SortInspectionEntries(entries)
	return entries
}

// SortInspectionEntries sorts entries by key, then source and provenance.
func SortInspectionEntries(entries []InspectionEntry) {
	sort.SliceStable(entries, func(i, j int) bool {
		if entries[i].Key != entries[j].Key {
			return entries[i].Key < entries[j].Key
		}
		if entries[i].Source != entries[j].Source {
			return entries[i].Source < entries[j].Source
		}
		return entries[i].Provenance < entries[j].Provenance
	})
}

// ShouldRedactKey reports whether key matches the default sensitive config/env
// key set.
func ShouldRedactKey(key string) bool {
	return shouldRedactInspectKey(key, newInspectSettings(InspectOptions{}).redactKeys)
}

type inspectSettings struct {
	mask       string
	redactKeys map[string]struct{}
}

func newInspectSettings(options InspectOptions) inspectSettings {
	settings := inspectSettings{
		mask: DefaultInspectMask,
	}
	if options.Mask != "" {
		settings.mask = options.Mask
	}

	keys := defaultInspectRedactKeys
	if options.RedactKeys != nil {
		keys = options.RedactKeys
	}
	settings.redactKeys = make(map[string]struct{}, len(keys))
	for _, key := range keys {
		if normalized := normalizeInspectKey(key); normalized != "" {
			settings.redactKeys[normalized] = struct{}{}
		}
	}
	return settings
}

func flattenValue(out map[string]any, prefix string, value any) {
	entries, ok := stringMapEntries(value)
	if !ok {
		if prefix != "" {
			out[prefix] = value
		}
		return
	}
	if len(entries) == 0 {
		if prefix != "" {
			out[prefix] = value
		}
		return
	}

	for _, entry := range entries {
		key := entry.key
		if prefix != "" {
			key = prefix + "." + key
		}
		flattenValue(out, key, entry.value)
	}
}

type stringMapEntry struct {
	key   string
	value any
}

func stringMapEntries(value any) ([]stringMapEntry, bool) {
	if value == nil {
		return nil, false
	}
	rv := reflect.ValueOf(value)
	if rv.Kind() != reflect.Map || rv.Type().Key().Kind() != reflect.String {
		return nil, false
	}
	if rv.IsNil() {
		return nil, true
	}

	keys := rv.MapKeys()
	sort.Slice(keys, func(i, j int) bool {
		return keys[i].String() < keys[j].String()
	})

	entries := make([]stringMapEntry, 0, len(keys))
	for _, key := range keys {
		entries = append(entries, stringMapEntry{
			key:   key.String(),
			value: rv.MapIndex(key).Interface(),
		})
	}
	return entries, true
}

func shouldRedactInspectKey(key string, redactKeys map[string]struct{}) bool {
	normalized := normalizeInspectKey(key)
	if normalized == "" {
		return false
	}
	if _, ok := redactKeys[normalized]; ok {
		return true
	}

	parts := inspectKeyParts(key)
	for i := range parts {
		if _, ok := redactKeys[strings.Join(parts[i:], "")]; ok {
			return true
		}
	}
	return false
}

func normalizeInspectKey(key string) string {
	var b strings.Builder
	for _, r := range key {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(unicode.ToLower(r))
		}
	}
	return b.String()
}

func inspectKeyParts(key string) []string {
	var parts []string
	var b strings.Builder
	for _, r := range key {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(unicode.ToLower(r))
			continue
		}
		if b.Len() > 0 {
			parts = append(parts, b.String())
			b.Reset()
		}
	}
	if b.Len() > 0 {
		parts = append(parts, b.String())
	}
	return parts
}
