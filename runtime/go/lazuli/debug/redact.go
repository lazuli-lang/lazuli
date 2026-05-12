// Package debug provides helpers for producing bounded, log-safe Lazuli
// debug envelopes.
package debug

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"reflect"
	"strings"
	"unicode"
)

const (
	// RedactedValue is the default replacement for values whose key is sensitive.
	RedactedValue = "[REDACTED]"
	// DefaultMaxStringLen is the default maximum redacted string length.
	DefaultMaxStringLen = 1024
	// DefaultMaxSliceLen is the default maximum redacted slice length.
	DefaultMaxSliceLen = 64

	truncatedSliceKey = "_truncated"
	omittedItemsKey   = "omitted"
	truncationSuffix  = "..."
)

var defaultRedactKeys = []string{
	"access_token",
	"api_key",
	"api_secret",
	"api_token",
	"auth_token",
	"authorization",
	"bearer_token",
	"client_secret",
	"connection_string",
	"cookie",
	"credential",
	"credentials",
	"csrf_token",
	"database_url",
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

// RedactionConfig controls how debug envelope values are redacted and bounded.
type RedactionConfig struct {
	// MaxStringLen caps string output. Values less than or equal to zero use the
	// default 1024-rune cap.
	MaxStringLen int
	// MaxSliceLen caps slice output. Values less than or equal to zero use the
	// default 64-item cap. When a slice is truncated, the last returned item is a
	// JSON object describing the omitted item count.
	MaxSliceLen int
	// RedactKeys are JSON object field names whose values are replaced. Nil uses
	// the default sensitive key set. A non-nil empty slice disables key redaction.
	// Matching is case-insensitive and ignores non-letter/digit separators.
	RedactKeys []string
	// Mask replaces sensitive values. Empty uses RedactedValue.
	Mask string
}

// Redact returns a copy of value with sensitive fields redacted and large
// strings/slices truncated.
//
// The helper is intended for JSON-shaped debug envelopes. Maps with string keys
// stay maps so encoding/json can produce deterministic key order.
func Redact(value any, config *RedactionConfig) any {
	settings := newRedactionSettings(config)
	return redactValue(value, settings)
}

// MarshalRedactedJSON redacts value and marshals it with encoding/json.
//
// The standard encoder sorts map keys, so map-shaped envelopes have stable JSON
// output as long as their values are deterministic.
func MarshalRedactedJSON(value any, config *RedactionConfig) ([]byte, error) {
	return json.Marshal(Redact(value, config))
}

type redactionSettings struct {
	maxStringLen int
	maxSliceLen  int
	mask         string
	redactKeys   map[string]struct{}
}

func newRedactionSettings(config *RedactionConfig) redactionSettings {
	settings := redactionSettings{
		maxStringLen: DefaultMaxStringLen,
		maxSliceLen:  DefaultMaxSliceLen,
		mask:         RedactedValue,
	}
	if config != nil {
		if config.MaxStringLen > 0 {
			settings.maxStringLen = config.MaxStringLen
		}
		if config.MaxSliceLen > 0 {
			settings.maxSliceLen = config.MaxSliceLen
		}
		if config.Mask != "" {
			settings.mask = config.Mask
		}
	}

	keys := defaultRedactKeys
	if config != nil && config.RedactKeys != nil {
		keys = config.RedactKeys
	}
	settings.redactKeys = make(map[string]struct{}, len(keys))
	for _, key := range keys {
		if normalized := normalizeRedactKey(key); normalized != "" {
			settings.redactKeys[normalized] = struct{}{}
		}
	}
	return settings
}

func redactValue(value any, settings redactionSettings) any {
	switch typed := value.(type) {
	case nil:
		return nil
	case json.Number:
		return typed
	case string:
		return truncateString(typed, settings.maxStringLen)
	case json.RawMessage:
		if decoded, ok := decodeJSONValue(typed); ok {
			return redactValue(decoded, settings)
		}
		return truncateString(string(typed), settings.maxStringLen)
	case []byte:
		return truncateString(string(typed), settings.maxStringLen)
	case error:
		return truncateString(typed.Error(), settings.maxStringLen)
	case map[string]any:
		return redactStringMap(typed, settings)
	case []any:
		return redactAnySlice(typed, settings)
	default:
		return redactReflectValue(reflect.ValueOf(value), settings)
	}
}

func redactStringMap(in map[string]any, settings redactionSettings) map[string]any {
	if in == nil {
		return nil
	}

	out := make(map[string]any, len(in))
	for key, value := range in {
		if shouldRedactKey(key, settings) {
			out[key] = settings.mask
			continue
		}
		out[key] = redactValue(value, settings)
	}
	return out
}

func redactAnySlice(in []any, settings redactionSettings) []any {
	if in == nil {
		return nil
	}

	limit, truncated := sliceOutputLimit(len(in), settings.maxSliceLen)
	out := make([]any, 0, limit)
	for i := 0; i < limit; i++ {
		out = append(out, redactValue(in[i], settings))
	}
	if truncated {
		out = append(out, truncationMarker(len(in)-limit))
	}
	return out
}

func redactReflectValue(value reflect.Value, settings redactionSettings) any {
	if !value.IsValid() {
		return nil
	}
	for value.Kind() == reflect.Pointer || value.Kind() == reflect.Interface {
		if value.IsNil() {
			return nil
		}
		value = value.Elem()
	}

	switch value.Kind() {
	case reflect.String:
		return truncateString(value.String(), settings.maxStringLen)
	case reflect.Bool, reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64,
		reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr,
		reflect.Float32, reflect.Float64:
		if value.CanInterface() {
			return value.Interface()
		}
		return fmt.Sprint(value)
	case reflect.Map:
		return redactReflectMap(value, settings)
	case reflect.Slice:
		if value.IsNil() {
			return nil
		}
		if value.Type().Elem().Kind() == reflect.Uint8 {
			return truncateString(string(value.Bytes()), settings.maxStringLen)
		}
		return redactReflectSlice(value, settings)
	case reflect.Array:
		return redactReflectSlice(value, settings)
	case reflect.Struct:
		return redactStruct(value, settings)
	default:
		if value.CanInterface() {
			return value.Interface()
		}
		return fmt.Sprint(value)
	}
}

func redactReflectMap(value reflect.Value, settings redactionSettings) any {
	if value.IsNil() {
		return nil
	}
	if value.Type().Key().Kind() != reflect.String {
		return redactJSONRoundTrip(value.Interface(), settings)
	}

	out := make(map[string]any, value.Len())
	for _, keyValue := range value.MapKeys() {
		key := keyValue.String()
		if shouldRedactKey(key, settings) {
			out[key] = settings.mask
			continue
		}
		out[key] = redactReflectValue(value.MapIndex(keyValue), settings)
	}
	return out
}

func redactReflectSlice(value reflect.Value, settings redactionSettings) []any {
	limit, truncated := sliceOutputLimit(value.Len(), settings.maxSliceLen)
	out := make([]any, 0, limit)
	for i := 0; i < limit; i++ {
		out = append(out, redactReflectValue(value.Index(i), settings))
	}
	if truncated {
		out = append(out, truncationMarker(value.Len()-limit))
	}
	return out
}

func redactStruct(value reflect.Value, settings redactionSettings) any {
	if value.CanInterface() {
		if redacted, ok := redactJSONRoundTripOK(value.Interface(), settings); ok {
			return redacted
		}
	}

	out := make(map[string]any)
	valueType := value.Type()
	for i := 0; i < value.NumField(); i++ {
		field := valueType.Field(i)
		if field.PkgPath != "" {
			continue
		}
		key, ok := jsonFieldName(field)
		if !ok {
			continue
		}
		if shouldRedactKey(key, settings) {
			out[key] = settings.mask
			continue
		}
		out[key] = redactReflectValue(value.Field(i), settings)
	}
	return out
}

func redactJSONRoundTrip(value any, settings redactionSettings) any {
	redacted, ok := redactJSONRoundTripOK(value, settings)
	if ok {
		return redacted
	}
	return fmt.Sprint(value)
}

func redactJSONRoundTripOK(value any, settings redactionSettings) (any, bool) {
	payload, err := json.Marshal(value)
	if err != nil {
		return nil, false
	}
	decoded, ok := decodeJSONValue(payload)
	if !ok {
		return nil, false
	}
	return redactValue(decoded, settings), true
}

func decodeJSONValue(payload []byte) (any, bool) {
	decoder := json.NewDecoder(bytes.NewReader(payload))
	decoder.UseNumber()

	var decoded any
	if err := decoder.Decode(&decoded); err != nil {
		return nil, false
	}
	var extra any
	if err := decoder.Decode(&extra); err == nil || err != io.EOF {
		return nil, false
	}
	return decoded, true
}

func jsonFieldName(field reflect.StructField) (string, bool) {
	tag := field.Tag.Get("json")
	if tag == "-" {
		return "", false
	}
	if comma := strings.IndexByte(tag, ','); comma >= 0 {
		tag = tag[:comma]
	}
	if tag != "" {
		return tag, true
	}
	return field.Name, true
}

func sliceOutputLimit(length, max int) (int, bool) {
	if length <= max {
		return length, false
	}
	if max <= 1 {
		return 0, true
	}
	return max - 1, true
}

func truncationMarker(omitted int) map[string]any {
	return map[string]any{
		truncatedSliceKey: true,
		omittedItemsKey:   omitted,
	}
}

func truncateString(value string, max int) string {
	runes := []rune(value)
	if len(runes) <= max {
		return value
	}
	suffix := []rune(truncationSuffix)
	if max <= len(suffix) {
		return string(runes[:max])
	}
	return string(runes[:max-len(suffix)]) + truncationSuffix
}

func shouldRedactKey(key string, settings redactionSettings) bool {
	_, ok := settings.redactKeys[normalizeRedactKey(key)]
	return ok
}

func normalizeRedactKey(key string) string {
	var b strings.Builder
	for _, r := range key {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(unicode.ToLower(r))
		}
	}
	return b.String()
}
