// Package sourcemap resolves Lazuli source byte spans to source file
// locations.
package sourcemap

import (
	"encoding/json"
	"errors"
	"fmt"
	"path"
	"sort"
	"strings"
	"unicode"
)

// ErrInvalidSourceMap is returned when a source map cannot be validated.
var ErrInvalidSourceMap = errors.New("lazuli/sourcemap: invalid source map")

// FileID identifies one source file in a SourceMap.
type FileID uint16

// Span is a half-open byte span in a source file. Start is inclusive and End
// is exclusive.
type Span struct {
	Start uint32 `json:"start"`
	End   uint32 `json:"end"`
}

// Location is the resolved source position for a span or byte offset.
type Location struct {
	FileID FileID `json:"file_id"`
	Path   string `json:"path"`
	Line   uint32 `json:"line"`
	Column uint32 `json:"column"`
}

// String renders loc as path:line:column when a path is available.
func (loc Location) String() string {
	if loc.Path == "" {
		return ""
	}
	if loc.Line == 0 {
		return loc.Path
	}
	if loc.Column == 0 {
		return fmt.Sprintf("%s:%d", loc.Path, loc.Line)
	}
	return fmt.Sprintf("%s:%d:%d", loc.Path, loc.Line, loc.Column)
}

// Entry describes one source file in a SourceMap.
//
// LineOffsets stores the byte offset of each line start followed by the EOF
// byte offset. Empty files are represented as []uint32{0, 0}.
type Entry struct {
	ID          FileID   `json:"id"`
	Path        string   `json:"path"`
	LineOffsets []uint32 `json:"line_offsets"`
}

// LineColumn returns the 1-based line and byte column for offset.
func (e Entry) LineColumn(offset uint32) (uint32, uint32, bool) {
	if err := validateLineOffsets(e.LineOffsets); err != nil {
		return 0, 0, false
	}
	return lineColumnForOffset(e.LineOffsets, offset)
}

// Contains reports whether offset is within the entry's known byte range.
func (e Entry) Contains(offset uint32) bool {
	_, _, ok := e.LineColumn(offset)
	return ok
}

// SourceMap is a sidecar map from file IDs to source file line offsets.
//
// The JSON shape mirrors the D1 source map companion:
// {"files":[{"id":1,"path":"features/customer.lzi","line_offsets":[...]}]}.
type SourceMap struct {
	Entries []Entry `json:"files"`
}

// New returns a validated source map. Entries are copied and sorted by file
// ID, then path, so downstream JSON and lookup behavior are deterministic.
func New(entries []Entry) (SourceMap, error) {
	normalized, err := normalizeEntries(entries)
	if err != nil {
		return SourceMap{}, err
	}
	return SourceMap{Entries: normalized}, nil
}

// Validate checks that m is sorted-validatable and has unique file IDs and
// paths. Unsorted entries are valid; validation sorts a copy before checking.
func (m SourceMap) Validate() error {
	_, err := normalizeEntries(m.Entries)
	return err
}

// Lookup returns the entry for fileID.
func (m SourceMap) Lookup(fileID FileID) (Entry, bool) {
	entries, err := normalizeEntries(m.Entries)
	if err != nil {
		return Entry{}, false
	}
	index := sort.Search(len(entries), func(i int) bool {
		return entries[i].ID >= fileID
	})
	if index == len(entries) || entries[index].ID != fileID {
		return Entry{}, false
	}
	return entries[index], true
}

// Resolve returns the source location for span's start offset in fileID.
func (m SourceMap) Resolve(fileID FileID, span Span) (Location, bool) {
	if span.End < span.Start {
		return Location{}, false
	}
	entry, ok := m.Lookup(fileID)
	if !ok || !entry.Contains(span.End) {
		return Location{}, false
	}
	line, column, ok := entry.LineColumn(span.Start)
	if !ok {
		return Location{}, false
	}
	return Location{
		FileID: fileID,
		Path:   entry.Path,
		Line:   line,
		Column: column,
	}, true
}

// LookupSpan is an alias for Resolve.
func (m SourceMap) LookupSpan(fileID FileID, span Span) (Location, bool) {
	return m.Resolve(fileID, span)
}

// ResolveRange returns the source location for the start of the half-open byte
// range [start, end).
func (m SourceMap) ResolveRange(fileID FileID, start, end uint32) (Location, bool) {
	return m.Resolve(fileID, Span{Start: start, End: end})
}

// LineColumn returns the source location for offset in fileID.
func (m SourceMap) LineColumn(fileID FileID, offset uint32) (Location, bool) {
	entry, ok := m.Lookup(fileID)
	if !ok {
		return Location{}, false
	}
	line, column, ok := entry.LineColumn(offset)
	if !ok {
		return Location{}, false
	}
	return Location{
		FileID: fileID,
		Path:   entry.Path,
		Line:   line,
		Column: column,
	}, true
}

// MarshalJSON validates and emits m in deterministic file ID order.
func (m SourceMap) MarshalJSON() ([]byte, error) {
	entries, err := normalizeEntries(m.Entries)
	if err != nil {
		return nil, err
	}
	type sourceMapJSON struct {
		Files []Entry `json:"files"`
	}
	return json.Marshal(sourceMapJSON{Files: entries})
}

// UnmarshalJSON decodes, validates, and sorts a source map.
func (m *SourceMap) UnmarshalJSON(data []byte) error {
	type sourceMapJSON struct {
		Files []Entry `json:"files"`
	}
	var decoded sourceMapJSON
	if err := json.Unmarshal(data, &decoded); err != nil {
		return invalidSourceMap("decode: %v", err)
	}
	normalized, err := normalizeEntries(decoded.Files)
	if err != nil {
		return err
	}
	m.Entries = normalized
	return nil
}

func normalizeEntries(entries []Entry) ([]Entry, error) {
	normalized := make([]Entry, len(entries))
	for i, entry := range entries {
		normalized[i] = cloneEntry(entry)
	}
	sort.Slice(normalized, func(i, j int) bool {
		return entryLess(normalized[i], normalized[j])
	})

	seenIDs := make(map[FileID]struct{}, len(normalized))
	seenPaths := make(map[string]FileID, len(normalized))
	for _, entry := range normalized {
		if err := validateEntry(entry); err != nil {
			return nil, invalidSourceMap("entry %d: %v", entry.ID, err)
		}
		if _, ok := seenIDs[entry.ID]; ok {
			return nil, invalidSourceMap("duplicate file id %d", entry.ID)
		}
		seenIDs[entry.ID] = struct{}{}
		if previous, ok := seenPaths[entry.Path]; ok {
			return nil, invalidSourceMap("path %q reused by file ids %d and %d", entry.Path, previous, entry.ID)
		}
		seenPaths[entry.Path] = entry.ID
	}
	return normalized, nil
}

func cloneEntry(entry Entry) Entry {
	if entry.LineOffsets != nil {
		entry.LineOffsets = append([]uint32(nil), entry.LineOffsets...)
	}
	return entry
}

func entryLess(a, b Entry) bool {
	if a.ID != b.ID {
		return a.ID < b.ID
	}
	if a.Path != b.Path {
		return a.Path < b.Path
	}
	minLen := len(a.LineOffsets)
	if len(b.LineOffsets) < minLen {
		minLen = len(b.LineOffsets)
	}
	for i := 0; i < minLen; i++ {
		if a.LineOffsets[i] != b.LineOffsets[i] {
			return a.LineOffsets[i] < b.LineOffsets[i]
		}
	}
	return len(a.LineOffsets) < len(b.LineOffsets)
}

func validateEntry(entry Entry) error {
	if err := validateSourcePath(entry.Path); err != nil {
		return fmt.Errorf("path: %w", err)
	}
	if err := validateLineOffsets(entry.LineOffsets); err != nil {
		return err
	}
	return nil
}

func validateSourcePath(sourcePath string) error {
	if sourcePath == "" {
		return errors.New("path is required")
	}
	if strings.TrimSpace(sourcePath) != sourcePath {
		return errors.New("path must not have leading or trailing whitespace")
	}
	if strings.Contains(sourcePath, "\\") {
		return errors.New("path must use forward slashes")
	}
	if strings.Contains(sourcePath, "://") {
		return errors.New("path must be relative")
	}
	if path.IsAbs(sourcePath) {
		return errors.New("path must be relative")
	}
	for _, r := range sourcePath {
		if unicode.IsControl(r) {
			return errors.New("path must not contain control characters")
		}
	}

	cleaned := path.Clean(sourcePath)
	if cleaned == "." || cleaned == ".." || strings.HasPrefix(cleaned, "../") {
		return errors.New("path must not traverse outside the capsule")
	}
	if cleaned != sourcePath {
		return errors.New("path must be canonical")
	}
	return nil
}

func validateLineOffsets(offsets []uint32) error {
	if len(offsets) < 2 {
		return errors.New("line_offsets must include first line and EOF offsets")
	}
	if offsets[0] != 0 {
		return errors.New("line_offsets must start at 0")
	}
	for i := 1; i < len(offsets); i++ {
		if offsets[i] < offsets[i-1] {
			return fmt.Errorf("line_offsets[%d] must not be before line_offsets[%d]", i, i-1)
		}
		if offsets[i] == offsets[i-1] && i != len(offsets)-1 {
			return fmt.Errorf("line_offsets[%d] duplicates the previous offset before EOF", i)
		}
	}
	return nil
}

func lineColumnForOffset(offsets []uint32, offset uint32) (uint32, uint32, bool) {
	if offset > offsets[len(offsets)-1] {
		return 0, 0, false
	}
	lineIndex := sort.Search(len(offsets), func(i int) bool {
		return offsets[i] > offset
	}) - 1
	if lineIndex < 0 {
		return 0, 0, false
	}
	if lineIndex >= len(offsets)-1 {
		lineIndex = len(offsets) - 2
	}
	return uint32(lineIndex + 1), offset - offsets[lineIndex] + 1, true
}

func invalidSourceMap(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidSourceMap, fmt.Sprintf(format, args...))
}
