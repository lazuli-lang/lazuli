// Package debug builds deterministic JSONL debug bundles for Lazuli tooling.
//
// EXPERIMENTAL: subject to change before 1.0.
package debug

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sort"
	"strings"
)

// EntryType identifies the payload shape of a debug bundle entry.
type EntryType string

const (
	EntryTypeExample EntryType = "example"
	EntryTypeError   EntryType = "error"
	EntryTypeProfile EntryType = "profile"
)

var (
	// ErrMissingEntryType is returned when an entry does not declare a type.
	ErrMissingEntryType = errors.New("lazuli/debug: entry type is required")
	// ErrInvalidEntryType is returned when an entry type is not part of the
	// debug bundle catalog.
	ErrInvalidEntryType = errors.New("lazuli/debug: invalid entry type")
	// ErrMissingEntryName is returned when an entry does not declare a name.
	ErrMissingEntryName = errors.New("lazuli/debug: entry name is required")
	// ErrInvalidErrorEnvelope is returned when ErrorEnvelope is not valid JSON.
	ErrInvalidErrorEnvelope = errors.New("lazuli/debug: invalid error envelope")
	// ErrNilWriter is returned when WriteJSONL receives a nil writer.
	ErrNilWriter = errors.New("lazuli/debug: writer is nil")
)

// Entry is one logical item in a JSONL debug bundle.
//
// Type and Name are required. The remaining fields are optional and are
// included only when set. Labels are caller-provided metadata; computed
// metadata such as ordinal and byte counts is added by the builder.
type Entry struct {
	Type EntryType
	Name string

	Intent       string
	Feature      string
	Kind         string
	Op           string
	Source       string
	LZISource    string
	IRSnippet    string
	CommonErrors []string

	ErrorEnvelope json.RawMessage
	DebugRoute    string

	ProfileSnippet string
	PatternID      string
	PatternVersion string

	Labels map[string]string
}

// EntryMetadata is emitted on each JSONL record.
type EntryMetadata struct {
	Ordinal      int               `json:"ordinal"`
	ContentBytes int               `json:"content_bytes"`
	LineBytes    int               `json:"line_bytes"`
	Labels       map[string]string `json:"labels,omitempty"`
}

// EntrySummary reports the byte accounting for one emitted JSONL record.
type EntrySummary struct {
	Type     EntryType
	Name     string
	Metadata EntryMetadata
}

// Summary reports byte accounting for a built JSONL bundle.
type Summary struct {
	EntryCount   int
	ContentBytes int
	TotalBytes   int
	Entries      []EntrySummary
}

// Builder accumulates entries before producing a deterministic JSONL bundle.
type Builder struct {
	entries []Entry
}

// NewBuilder returns an empty debug bundle builder.
func NewBuilder() *Builder {
	return &Builder{}
}

// ExampleEntry returns an example bundle entry.
func ExampleEntry(name, intent, lziSource, irSnippet string, commonErrors ...string) Entry {
	return Entry{
		Type:         EntryTypeExample,
		Name:         name,
		Intent:       intent,
		LZISource:    lziSource,
		IRSnippet:    irSnippet,
		CommonErrors: commonErrors,
	}
}

// ErrorEntry returns an error-context bundle entry.
func ErrorEntry(name string, envelope json.RawMessage, lziSource, irSnippet string) Entry {
	return Entry{
		Type:          EntryTypeError,
		Name:          name,
		ErrorEnvelope: append(json.RawMessage(nil), envelope...),
		LZISource:     lziSource,
		IRSnippet:     irSnippet,
	}
}

// ProfileEntry returns a profile-context bundle entry.
func ProfileEntry(name, profileSnippet string) Entry {
	return Entry{
		Type:           EntryTypeProfile,
		Name:           name,
		ProfileSnippet: profileSnippet,
	}
}

// Add appends entry to b. A nil builder ignores the entry.
func (b *Builder) Add(entry Entry) {
	if b == nil {
		return
	}
	b.entries = append(b.entries, entry)
}

// Build returns b's entries encoded as deterministic JSONL and its summary.
func (b *Builder) Build() ([]byte, Summary, error) {
	if b == nil {
		return BuildJSONL(nil)
	}
	return BuildJSONL(b.entries)
}

// Write writes b's JSONL bundle to w and returns its summary.
func (b *Builder) Write(w io.Writer) (Summary, error) {
	if b == nil {
		return WriteJSONL(w, nil)
	}
	return WriteJSONL(w, b.entries)
}

// BuildJSONL returns entries encoded as deterministic JSONL.
//
// Entries are normalized and sorted by type, name, Lazuli source metadata, and
// payload content. Each emitted record includes metadata with its 1-based
// ordinal, content byte count, and final line byte count including the trailing
// newline.
func BuildJSONL(entries []Entry) ([]byte, Summary, error) {
	normalized, err := normalizeEntries(entries)
	if err != nil {
		return nil, Summary{}, err
	}

	var out bytes.Buffer
	summary := Summary{
		EntryCount: len(normalized),
		Entries:    make([]EntrySummary, 0, len(normalized)),
	}

	for i, entry := range normalized {
		record := recordFromEntry(entry, i+1)
		line, metadata, err := marshalRecord(record)
		if err != nil {
			return nil, Summary{}, fmt.Errorf("lazuli/debug: encode entry %d: %w", i+1, err)
		}

		out.Write(line)
		out.WriteByte('\n')

		summary.ContentBytes += metadata.ContentBytes
		summary.TotalBytes += metadata.LineBytes
		summary.Entries = append(summary.Entries, EntrySummary{
			Type:     entry.Type,
			Name:     entry.Name,
			Metadata: cloneMetadata(metadata),
		})
	}

	return out.Bytes(), summary, nil
}

// WriteJSONL writes entries encoded as deterministic JSONL to w.
func WriteJSONL(w io.Writer, entries []Entry) (Summary, error) {
	if w == nil {
		return Summary{}, ErrNilWriter
	}

	data, summary, err := BuildJSONL(entries)
	if err != nil {
		return Summary{}, err
	}
	n, err := w.Write(data)
	if err != nil {
		return Summary{}, err
	}
	if n != len(data) {
		return Summary{}, io.ErrShortWrite
	}
	return summary, nil
}

type bundleRecord struct {
	Type EntryType `json:"type"`
	Name string    `json:"name"`

	Intent       string   `json:"intent,omitempty"`
	Feature      string   `json:"feature,omitempty"`
	Kind         string   `json:"kind,omitempty"`
	Op           string   `json:"op,omitempty"`
	Source       string   `json:"source,omitempty"`
	LZISource    string   `json:"lzi_source,omitempty"`
	IRSnippet    string   `json:"ir_snippet,omitempty"`
	CommonErrors []string `json:"common_errors,omitempty"`

	ErrorEnvelope json.RawMessage `json:"error_envelope,omitempty"`
	DebugRoute    string          `json:"debug_route,omitempty"`

	ProfileSnippet string `json:"profile_snippet,omitempty"`
	PatternID      string `json:"pattern_id,omitempty"`
	PatternVersion string `json:"pattern_version,omitempty"`

	Metadata EntryMetadata `json:"metadata"`
}

func normalizeEntries(entries []Entry) ([]Entry, error) {
	normalized := make([]Entry, len(entries))
	for i, entry := range entries {
		next, err := normalizeEntry(entry)
		if err != nil {
			return nil, fmt.Errorf("lazuli/debug: entry %d: %w", i+1, err)
		}
		normalized[i] = next
	}

	sort.Slice(normalized, func(i, j int) bool {
		return compareEntries(normalized[i], normalized[j]) < 0
	})
	return normalized, nil
}

func normalizeEntry(entry Entry) (Entry, error) {
	entry.Type = EntryType(strings.TrimSpace(string(entry.Type)))
	switch entry.Type {
	case "":
		return Entry{}, ErrMissingEntryType
	case EntryTypeExample, EntryTypeError, EntryTypeProfile:
	default:
		return Entry{}, fmt.Errorf("%w: %q", ErrInvalidEntryType, entry.Type)
	}

	entry.Name = strings.TrimSpace(entry.Name)
	if entry.Name == "" {
		return Entry{}, ErrMissingEntryName
	}

	entry.Feature = strings.TrimSpace(entry.Feature)
	entry.Kind = strings.TrimSpace(entry.Kind)
	entry.Op = strings.TrimSpace(entry.Op)
	entry.Source = strings.TrimSpace(entry.Source)
	entry.DebugRoute = strings.TrimSpace(entry.DebugRoute)
	entry.PatternID = strings.TrimSpace(entry.PatternID)
	entry.PatternVersion = strings.TrimSpace(entry.PatternVersion)
	entry.CommonErrors = normalizeStringList(entry.CommonErrors)
	entry.Labels = normalizeLabels(entry.Labels)

	if len(entry.ErrorEnvelope) > 0 {
		envelope, err := normalizeRawJSON(entry.ErrorEnvelope)
		if err != nil {
			return Entry{}, fmt.Errorf("%w: %v", ErrInvalidErrorEnvelope, err)
		}
		entry.ErrorEnvelope = envelope
	}

	return entry, nil
}

func recordFromEntry(entry Entry, ordinal int) bundleRecord {
	return bundleRecord{
		Type:           entry.Type,
		Name:           entry.Name,
		Intent:         entry.Intent,
		Feature:        entry.Feature,
		Kind:           entry.Kind,
		Op:             entry.Op,
		Source:         entry.Source,
		LZISource:      entry.LZISource,
		IRSnippet:      entry.IRSnippet,
		CommonErrors:   append([]string(nil), entry.CommonErrors...),
		ErrorEnvelope:  append(json.RawMessage(nil), entry.ErrorEnvelope...),
		DebugRoute:     entry.DebugRoute,
		ProfileSnippet: entry.ProfileSnippet,
		PatternID:      entry.PatternID,
		PatternVersion: entry.PatternVersion,
		Metadata: EntryMetadata{
			Ordinal:      ordinal,
			ContentBytes: contentBytes(entry),
			Labels:       cloneLabels(entry.Labels),
		},
	}
}

func marshalRecord(record bundleRecord) ([]byte, EntryMetadata, error) {
	lineBytes := 0
	for i := 0; i < 16; i++ {
		record.Metadata.LineBytes = lineBytes
		line, err := marshalJSONLine(record)
		if err != nil {
			return nil, EntryMetadata{}, err
		}

		next := len(line) + 1
		if next == lineBytes {
			return line, cloneMetadata(record.Metadata), nil
		}
		lineBytes = next
	}

	return nil, EntryMetadata{}, errors.New("line byte count did not converge")
}

func marshalJSONLine(v any) ([]byte, error) {
	var buf bytes.Buffer
	encoder := json.NewEncoder(&buf)
	encoder.SetEscapeHTML(false)
	if err := encoder.Encode(v); err != nil {
		return nil, err
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n")), nil
}

func normalizeRawJSON(data json.RawMessage) (json.RawMessage, error) {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()

	var value any
	if err := decoder.Decode(&value); err != nil {
		return nil, err
	}
	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		if err == nil {
			return nil, errors.New("multiple JSON values")
		}
		return nil, err
	}

	normalized, err := marshalJSONLine(value)
	if err != nil {
		return nil, err
	}
	return append(json.RawMessage(nil), normalized...), nil
}

func normalizeStringList(values []string) []string {
	if len(values) == 0 {
		return nil
	}

	seen := make(map[string]struct{}, len(values))
	normalized := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		normalized = append(normalized, value)
	}
	sort.Strings(normalized)
	return normalized
}

func normalizeLabels(labels map[string]string) map[string]string {
	if len(labels) == 0 {
		return nil
	}

	normalized := make(map[string]string, len(labels))
	for key, value := range labels {
		key = strings.TrimSpace(key)
		if key == "" {
			continue
		}
		normalized[key] = value
	}
	if len(normalized) == 0 {
		return nil
	}
	return normalized
}

func contentBytes(entry Entry) int {
	total := len(entry.Intent) +
		len(entry.LZISource) +
		len(entry.IRSnippet) +
		len(entry.ErrorEnvelope) +
		len(entry.DebugRoute) +
		len(entry.ProfileSnippet)
	for _, value := range entry.CommonErrors {
		total += len(value)
	}
	return total
}

func compareEntries(a, b Entry) int {
	if diff := entryTypeOrder(a.Type) - entryTypeOrder(b.Type); diff != 0 {
		return diff
	}
	for _, pair := range [][2]string{
		{a.Name, b.Name},
		{a.Feature, b.Feature},
		{a.Kind, b.Kind},
		{a.Op, b.Op},
		{a.Source, b.Source},
		{a.PatternID, b.PatternID},
		{a.PatternVersion, b.PatternVersion},
		{a.Intent, b.Intent},
		{a.LZISource, b.LZISource},
		{a.IRSnippet, b.IRSnippet},
		{string(a.ErrorEnvelope), string(b.ErrorEnvelope)},
		{a.DebugRoute, b.DebugRoute},
		{a.ProfileSnippet, b.ProfileSnippet},
		{strings.Join(a.CommonErrors, "\x00"), strings.Join(b.CommonErrors, "\x00")},
		{labelsKey(a.Labels), labelsKey(b.Labels)},
	} {
		if cmp := strings.Compare(pair[0], pair[1]); cmp != 0 {
			return cmp
		}
	}
	return 0
}

func entryTypeOrder(entryType EntryType) int {
	switch entryType {
	case EntryTypeExample:
		return 0
	case EntryTypeError:
		return 1
	case EntryTypeProfile:
		return 2
	default:
		return 3
	}
}

func labelsKey(labels map[string]string) string {
	if len(labels) == 0 {
		return ""
	}

	keys := make([]string, 0, len(labels))
	for key := range labels {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	var b strings.Builder
	for _, key := range keys {
		b.WriteString(key)
		b.WriteByte('=')
		b.WriteString(labels[key])
		b.WriteByte(0)
	}
	return b.String()
}

func cloneLabels(labels map[string]string) map[string]string {
	if len(labels) == 0 {
		return nil
	}
	cloned := make(map[string]string, len(labels))
	for key, value := range labels {
		cloned[key] = value
	}
	return cloned
}

func cloneMetadata(metadata EntryMetadata) EntryMetadata {
	metadata.Labels = cloneLabels(metadata.Labels)
	return metadata
}
