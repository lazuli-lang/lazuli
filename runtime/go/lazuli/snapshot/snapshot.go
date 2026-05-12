// Package snapshot provides deterministic JSON serializers for snapshot tests
// and diagnostics.
package snapshot

import (
	"bytes"
	"encoding/json"
	"strings"
	"time"
)

const (
	// DefaultRedaction is the replacement used for redacted fields when no
	// custom redaction value is configured.
	DefaultRedaction = "[REDACTED]"
)

// Option configures snapshot serialization.
type Option func(*options)

type options struct {
	redactFields   map[string]struct{}
	redaction      any
	timeNormalizer func(time.Time) any
}

// WithRedactedFields replaces JSON object fields matching names with the
// configured redaction value. Field matching is case-insensitive and uses the
// field names after JSON tags are applied.
func WithRedactedFields(names ...string) Option {
	return func(options *options) {
		if len(names) == 0 {
			return
		}
		if options.redactFields == nil {
			options.redactFields = make(map[string]struct{}, len(names))
		}
		for _, name := range names {
			name = strings.ToLower(strings.TrimSpace(name))
			if name == "" {
				continue
			}
			options.redactFields[name] = struct{}{}
		}
	}
}

// WithRedaction configures the JSON value used for redacted fields.
func WithRedaction(value any) Option {
	return func(options *options) {
		options.redaction = value
	}
}

// WithNormalizedTimes renders RFC3339 timestamp strings in UTC using
// time.RFC3339Nano. This normalizes time.Time values after their JSON form is
// produced and also applies to other JSON strings that parse as RFC3339.
func WithNormalizedTimes() Option {
	return WithTimeNormalizer(func(t time.Time) any {
		return t.UTC().Format(time.RFC3339Nano)
	})
}

// WithTimeNormalizer applies normalize to every JSON string that parses as an
// RFC3339 timestamp. A nil normalize function leaves timestamps unchanged.
func WithTimeNormalizer(normalize func(time.Time) any) Option {
	return func(options *options) {
		options.timeNormalizer = normalize
	}
}

// Marshal returns v as deterministic, indented JSON with a trailing newline.
//
// Values are first encoded with encoding/json, then decoded into a JSON tree so
// redaction and time normalization operate on the same field names and values
// callers assert in snapshots. Map keys are sorted by encoding/json during the
// final marshal.
func Marshal(v any, opts ...Option) ([]byte, error) {
	options := applyOptions(opts)

	data, err := json.Marshal(v)
	if err != nil {
		return nil, err
	}

	var value any
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	if err := decoder.Decode(&value); err != nil {
		return nil, err
	}

	value = normalizeValue(value, options)
	var out bytes.Buffer
	encoder := json.NewEncoder(&out)
	encoder.SetEscapeHTML(false)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(value); err != nil {
		return nil, err
	}
	return out.Bytes(), nil
}

// String returns v as deterministic, indented JSON text with a trailing newline.
func String(v any, opts ...Option) (string, error) {
	data, err := Marshal(v, opts...)
	if err != nil {
		return "", err
	}
	return string(data), nil
}

// Compare serializes got and returns a diff against want. The returned diff is
// empty when the normalized snapshot strings match. Compare performs no file
// reads or writes.
func Compare(want string, got any, opts ...Option) (string, error) {
	gotSnapshot, err := String(got, opts...)
	if err != nil {
		return "", err
	}
	return Diff(want, gotSnapshot), nil
}

// Diff returns a line-oriented diff between want and got. The returned string is
// empty when the two inputs match after line-ending normalization and ignoring a
// single final newline.
func Diff(want, got string) string {
	wantLines := splitLines(want)
	gotLines := splitLines(got)
	if equalLines(wantLines, gotLines) {
		return ""
	}

	ops := diffOps(wantLines, gotLines)
	var out strings.Builder
	out.WriteString("--- want\n+++ got\n")
	for _, op := range ops {
		out.WriteByte(op.kind)
		out.WriteByte(' ')
		out.WriteString(op.line)
		out.WriteByte('\n')
	}
	return out.String()
}

func applyOptions(opts []Option) options {
	options := options{redaction: DefaultRedaction}
	for _, opt := range opts {
		if opt != nil {
			opt(&options)
		}
	}
	return options
}

func normalizeValue(value any, options options) any {
	switch typed := value.(type) {
	case map[string]any:
		for key, child := range typed {
			if _, ok := options.redactFields[strings.ToLower(key)]; ok {
				typed[key] = options.redaction
				continue
			}
			typed[key] = normalizeValue(child, options)
		}
		return typed
	case []any:
		for i, child := range typed {
			typed[i] = normalizeValue(child, options)
		}
		return typed
	case string:
		if options.timeNormalizer == nil {
			return typed
		}
		t, ok := parseTime(typed)
		if !ok {
			return typed
		}
		return options.timeNormalizer(t)
	default:
		return value
	}
}

func parseTime(value string) (time.Time, bool) {
	t, err := time.Parse(time.RFC3339Nano, value)
	if err != nil {
		return time.Time{}, false
	}
	return t, true
}

func splitLines(s string) []string {
	s = strings.ReplaceAll(s, "\r\n", "\n")
	s = strings.ReplaceAll(s, "\r", "\n")
	s = strings.TrimSuffix(s, "\n")
	if s == "" {
		return nil
	}
	return strings.Split(s, "\n")
}

func equalLines(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

type diffOp struct {
	kind byte
	line string
}

func diffOps(want, got []string) []diffOp {
	lcs := make([][]int, len(want)+1)
	for i := range lcs {
		lcs[i] = make([]int, len(got)+1)
	}
	for i := len(want) - 1; i >= 0; i-- {
		for j := len(got) - 1; j >= 0; j-- {
			if want[i] == got[j] {
				lcs[i][j] = lcs[i+1][j+1] + 1
			} else if lcs[i+1][j] >= lcs[i][j+1] {
				lcs[i][j] = lcs[i+1][j]
			} else {
				lcs[i][j] = lcs[i][j+1]
			}
		}
	}

	ops := make([]diffOp, 0, len(want)+len(got))
	i, j := 0, 0
	for i < len(want) && j < len(got) {
		switch {
		case want[i] == got[j]:
			ops = append(ops, diffOp{kind: ' ', line: want[i]})
			i++
			j++
		case lcs[i+1][j] >= lcs[i][j+1]:
			ops = append(ops, diffOp{kind: '-', line: want[i]})
			i++
		default:
			ops = append(ops, diffOp{kind: '+', line: got[j]})
			j++
		}
	}
	for i < len(want) {
		ops = append(ops, diffOp{kind: '-', line: want[i]})
		i++
	}
	for j < len(got) {
		ops = append(ops, diffOp{kind: '+', line: got[j]})
		j++
	}
	return ops
}
