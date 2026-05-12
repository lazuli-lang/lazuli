// Package examplesbundle builds deterministic JSONL records for curated Lazuli
// examples.
//
// EXPERIMENTAL: subject to change before 1.0.
package examplesbundle

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"path"
	"sort"
	"strings"
	"unicode"
)

const contentHashPrefix = "sha256:"

var (
	// ErrInvalidEntry is wrapped by structural validation failures.
	ErrInvalidEntry = errors.New("lazuli/examplesbundle: invalid entry")
	// ErrWriterRequired is returned when WriteJSONL receives a nil writer.
	ErrWriterRequired = errors.New("lazuli/examplesbundle: writer required")
)

// Example describes one curated .lzi example before bundling.
//
// Build and NewEntry normalize metadata, sort list fields, and compute the
// content hash from LZISource. LZISource itself is preserved byte-for-byte.
type Example struct {
	Name         string
	Intent       string
	SourcePath   string
	Tags         []string
	LZISource    string
	IRSnippet    string
	CommonErrors []string
}

// Entry is the deterministic JSONL record emitted for one curated example.
type Entry struct {
	Name         string   `json:"name"`
	Intent       string   `json:"intent"`
	SourcePath   string   `json:"source_path"`
	Tags         []string `json:"tags"`
	ContentHash  string   `json:"content_hash"`
	LZISource    string   `json:"lzi_source"`
	IRSnippet    string   `json:"ir_snippet,omitempty"`
	CommonErrors []string `json:"common_errors,omitempty"`
}

// FieldError reports a validation failure for a specific bundle entry field.
type FieldError struct {
	Field string
	Err   error
}

// Error returns a stable, human-readable field validation error.
func (e *FieldError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Field == "" {
		return e.Err.Error()
	}
	return e.Field + ": " + e.Err.Error()
}

// Unwrap exposes the classified validation error for errors.Is.
func (e *FieldError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

// NewEntry validates example and returns its canonical bundle entry.
func NewEntry(example Example) (Entry, error) {
	entry, err := normalizeExample(example)
	if err != nil {
		return Entry{}, err
	}
	entry.ContentHash = ContentHash(entry.LZISource)
	return entry, nil
}

// Build validates examples, computes their content hashes, and returns entries
// sorted by source path and name for deterministic JSONL emission.
func Build(examples []Example) ([]Entry, error) {
	entries := make([]Entry, 0, len(examples))
	var errs []error

	for i, example := range examples {
		entry, err := NewEntry(example)
		if err != nil {
			errs = append(errs, indexedError(i, err))
			continue
		}
		entries = append(entries, entry)
	}
	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	sortEntries(entries)
	if err := validateUniqueBundleEntries(entries); err != nil {
		return nil, err
	}
	return entries, nil
}

// ContentHash returns the SHA-256 digest used for Entry.ContentHash.
func ContentHash(content string) string {
	sum := sha256.Sum256([]byte(content))
	return contentHashPrefix + hex.EncodeToString(sum[:])
}

// MarshalJSONL returns deterministic JSONL for examples.
func MarshalJSONL(examples []Example) ([]byte, error) {
	var out bytes.Buffer
	if err := WriteJSONL(&out, examples); err != nil {
		return nil, err
	}
	return out.Bytes(), nil
}

// WriteJSONL writes deterministic JSONL for examples to w.
func WriteJSONL(w io.Writer, examples []Example) error {
	if w == nil {
		return ErrWriterRequired
	}

	entries, err := Build(examples)
	if err != nil {
		return err
	}

	encoder := json.NewEncoder(w)
	encoder.SetEscapeHTML(false)
	for _, entry := range entries {
		if err := encoder.Encode(entry); err != nil {
			return err
		}
	}
	return nil
}

func normalizeExample(example Example) (Entry, error) {
	var errs []error

	name := strings.TrimSpace(example.Name)
	if name == "" {
		errs = append(errs, invalidField("name", "value is required"))
	} else if hasControlRune(name) {
		errs = append(errs, invalidField("name", "control characters are not allowed"))
	}

	intent := strings.TrimSpace(example.Intent)
	if intent == "" {
		errs = append(errs, invalidField("intent", "value is required"))
	} else if hasControlRune(intent) {
		errs = append(errs, invalidField("intent", "control characters are not allowed"))
	}

	sourcePath, err := cleanSourcePath(example.SourcePath)
	if err != nil {
		errs = append(errs, invalidField("source_path", err.Error()))
	}

	tags, err := normalizeStringList("tags", example.Tags, true)
	if err != nil {
		errs = append(errs, err)
	}

	if strings.TrimSpace(example.LZISource) == "" {
		errs = append(errs, invalidField("lzi_source", "value is required"))
	}

	commonErrors, err := normalizeStringList("common_errors", example.CommonErrors, false)
	if err != nil {
		errs = append(errs, err)
	}

	if err := errors.Join(errs...); err != nil {
		return Entry{}, err
	}
	return Entry{
		Name:         name,
		Intent:       intent,
		SourcePath:   sourcePath,
		Tags:         tags,
		LZISource:    example.LZISource,
		IRSnippet:    example.IRSnippet,
		CommonErrors: commonErrors,
	}, nil
}

func cleanSourcePath(sourcePath string) (string, error) {
	sourcePath = strings.TrimSpace(sourcePath)
	if sourcePath == "" {
		return "", errors.New("value is required")
	}
	if strings.HasPrefix(sourcePath, "/") || strings.HasPrefix(sourcePath, "//") {
		return "", errors.New("absolute paths are not allowed")
	}
	if strings.Contains(sourcePath, "\\") {
		return "", errors.New("backslashes are not allowed")
	}
	if strings.Contains(sourcePath, ":") || strings.Contains(sourcePath, "://") {
		return "", errors.New("absolute URLs and drive paths are not allowed")
	}
	if strings.ContainsAny(sourcePath, "?#") {
		return "", errors.New("query strings and fragments are not allowed")
	}
	if hasControlRune(sourcePath) {
		return "", errors.New("control characters are not allowed")
	}

	parts := strings.Split(sourcePath, "/")
	for _, part := range parts {
		if part == ".." {
			return "", errors.New("path traversal is not allowed")
		}
	}

	cleaned := path.Clean(sourcePath)
	if cleaned == "." || cleaned == ".." || strings.HasPrefix(cleaned, "../") {
		return "", errors.New("path must be a safe relative file path")
	}
	if path.Ext(cleaned) != ".lzi" {
		return "", errors.New("path must point to a .lzi file")
	}
	return cleaned, nil
}

func normalizeStringList(field string, values []string, required bool) ([]string, error) {
	if len(values) == 0 {
		if required {
			return nil, invalidField(field, "at least one value is required")
		}
		return nil, nil
	}

	normalized := make([]string, 0, len(values))
	seen := make(map[string]int, len(values))
	var errs []error
	for i, value := range values {
		item := strings.TrimSpace(value)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		switch {
		case item == "":
			errs = append(errs, invalidField(itemField, "value is required"))
			continue
		case hasControlRune(item):
			errs = append(errs, invalidField(itemField, "control characters are not allowed"))
			continue
		}
		if first, ok := seen[item]; ok {
			errs = append(errs, invalidField(itemField, fmt.Sprintf("duplicate value also appears at %s[%d]", field, first)))
			continue
		}
		seen[item] = i
		normalized = append(normalized, item)
	}
	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	sort.Strings(normalized)
	return normalized, nil
}

func sortEntries(entries []Entry) {
	sort.Slice(entries, func(i, j int) bool {
		if entries[i].SourcePath != entries[j].SourcePath {
			return entries[i].SourcePath < entries[j].SourcePath
		}
		return entries[i].Name < entries[j].Name
	})
}

func hasControlRune(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func indexedError(index int, err error) error {
	if err == nil {
		return nil
	}
	return fmt.Errorf("entry[%d]: %w", index, err)
}

func invalidField(field string, message string) error {
	return &FieldError{
		Field: field,
		Err:   fmt.Errorf("%w: %s", ErrInvalidEntry, message),
	}
}
