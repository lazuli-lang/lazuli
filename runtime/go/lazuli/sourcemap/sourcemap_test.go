package sourcemap

import (
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestNewSortsAndCopiesEntries(t *testing.T) {
	entries := []Entry{
		{ID: 2, Path: "features/order.lzi", LineOffsets: []uint32{0, 11}},
		{ID: 1, Path: "features/customer.lzi", LineOffsets: []uint32{0, 8}},
	}

	sourceMap, err := New(entries)
	if err != nil {
		t.Fatalf("New error = %v", err)
	}

	gotIDs := []FileID{sourceMap.Entries[0].ID, sourceMap.Entries[1].ID}
	if !reflect.DeepEqual(gotIDs, []FileID{1, 2}) {
		t.Fatalf("entry IDs = %v, want [1 2]", gotIDs)
	}

	entries[1].Path = "features/changed.lzi"
	entries[1].LineOffsets[1] = 99
	if sourceMap.Entries[0].Path != "features/customer.lzi" {
		t.Fatalf("stored path = %q, want original path", sourceMap.Entries[0].Path)
	}
	if got := sourceMap.Entries[0].LineOffsets[1]; got != 8 {
		t.Fatalf("stored line offset = %d, want 8", got)
	}
}

func TestResolveMapsSpanToLocation(t *testing.T) {
	sourceMap := sourcemapMustNew(t, []Entry{
		{ID: 7, Path: "features/customer.lzi", LineOffsets: []uint32{0, 12, 30, 50}},
	})

	loc, ok := sourceMap.Resolve(7, Span{Start: 34, End: 40})
	if !ok {
		t.Fatal("Resolve did not find span")
	}
	if loc.FileID != 7 || loc.Path != "features/customer.lzi" || loc.Line != 3 || loc.Column != 5 {
		t.Fatalf("location = %+v, want features/customer.lzi:3:5", loc)
	}
	if got := loc.String(); got != "features/customer.lzi:3:5" {
		t.Fatalf("String = %q, want source location", got)
	}

	loc, ok = sourceMap.LookupSpan(7, Span{Start: 12, End: 15})
	if !ok {
		t.Fatal("LookupSpan did not find span")
	}
	if loc.Line != 2 || loc.Column != 1 {
		t.Fatalf("LookupSpan location = %+v, want line 2 column 1", loc)
	}
}

func TestLineColumnHandlesBoundaries(t *testing.T) {
	entry := Entry{ID: 1, Path: "features/customer.lzi", LineOffsets: []uint32{0, 6, 6}}

	line, column, ok := entry.LineColumn(6)
	if !ok {
		t.Fatal("Entry.LineColumn did not resolve EOF offset")
	}
	if line != 2 || column != 1 {
		t.Fatalf("Entry.LineColumn = %d:%d, want 2:1", line, column)
	}

	sourceMap := sourcemapMustNew(t, []Entry{entry})
	loc, ok := sourceMap.LineColumn(1, 0)
	if !ok {
		t.Fatal("SourceMap.LineColumn did not resolve first byte")
	}
	if loc.Line != 1 || loc.Column != 1 {
		t.Fatalf("SourceMap.LineColumn = %+v, want line 1 column 1", loc)
	}
	if _, ok := sourceMap.LineColumn(1, 7); ok {
		t.Fatal("SourceMap.LineColumn resolved offset beyond EOF")
	}
}

func TestResolveRejectsInvalidSpans(t *testing.T) {
	sourceMap := sourcemapMustNew(t, []Entry{
		{ID: 1, Path: "features/customer.lzi", LineOffsets: []uint32{0, 10}},
	})

	tests := []struct {
		name   string
		fileID FileID
		span   Span
	}{
		{name: "missing file", fileID: 2, span: Span{Start: 0, End: 1}},
		{name: "reversed span", fileID: 1, span: Span{Start: 6, End: 5}},
		{name: "end beyond EOF", fileID: 1, span: Span{Start: 0, End: 11}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, ok := sourceMap.Resolve(tt.fileID, tt.span); ok {
				t.Fatalf("Resolve(%d, %+v) ok = true, want false", tt.fileID, tt.span)
			}
		})
	}
}

func TestMarshalJSONUsesD1FilesShapeAndSortedEntries(t *testing.T) {
	sourceMap := SourceMap{Entries: []Entry{
		{ID: 2, Path: "features/order.lzi", LineOffsets: []uint32{0, 5}},
		{ID: 1, Path: "features/customer.lzi", LineOffsets: []uint32{0, 8}},
	}}

	got, err := json.Marshal(sourceMap)
	if err != nil {
		t.Fatalf("Marshal error = %v", err)
	}

	const want = `{"files":[{"id":1,"path":"features/customer.lzi","line_offsets":[0,8]},{"id":2,"path":"features/order.lzi","line_offsets":[0,5]}]}`
	if string(got) != want {
		t.Fatalf("Marshal = %s, want %s", got, want)
	}
}

func TestUnmarshalJSONValidatesAndSortsEntries(t *testing.T) {
	var sourceMap SourceMap
	err := json.Unmarshal([]byte(`{
		"files": [
			{"id": 3, "path": "features/z.lzi", "line_offsets": [0, 9]},
			{"id": 1, "path": "features/a.lzi", "line_offsets": [0, 4]}
		]
	}`), &sourceMap)
	if err != nil {
		t.Fatalf("Unmarshal error = %v", err)
	}

	gotIDs := []FileID{sourceMap.Entries[0].ID, sourceMap.Entries[1].ID}
	if !reflect.DeepEqual(gotIDs, []FileID{1, 3}) {
		t.Fatalf("entry IDs = %v, want [1 3]", gotIDs)
	}
}

func TestSourceMapValidationRejectsInvalidInput(t *testing.T) {
	tests := []struct {
		name    string
		entries []Entry
	}{
		{
			name: "duplicate file id",
			entries: []Entry{
				{ID: 1, Path: "features/a.lzi", LineOffsets: []uint32{0, 1}},
				{ID: 1, Path: "features/b.lzi", LineOffsets: []uint32{0, 1}},
			},
		},
		{
			name: "duplicate path",
			entries: []Entry{
				{ID: 1, Path: "features/a.lzi", LineOffsets: []uint32{0, 1}},
				{ID: 2, Path: "features/a.lzi", LineOffsets: []uint32{0, 1}},
			},
		},
		{
			name: "non canonical path",
			entries: []Entry{
				{ID: 1, Path: "features/../customer.lzi", LineOffsets: []uint32{0, 1}},
			},
		},
		{
			name: "missing offsets",
			entries: []Entry{
				{ID: 1, Path: "features/a.lzi"},
			},
		},
		{
			name: "decreasing offsets",
			entries: []Entry{
				{ID: 1, Path: "features/a.lzi", LineOffsets: []uint32{0, 4, 3}},
			},
		},
		{
			name: "interior duplicate offsets",
			entries: []Entry{
				{ID: 1, Path: "features/a.lzi", LineOffsets: []uint32{0, 4, 4, 8}},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := New(tt.entries)
			if !errors.Is(err, ErrInvalidSourceMap) {
				t.Fatalf("New error = %v, want ErrInvalidSourceMap", err)
			}
		})
	}
}

func TestSourceMapJSONRejectsInvalidInput(t *testing.T) {
	var sourceMap SourceMap
	err := json.Unmarshal([]byte(`{"files":[{"id":1,"path":"","line_offsets":[0,0]}]}`), &sourceMap)
	if !errors.Is(err, ErrInvalidSourceMap) {
		t.Fatalf("Unmarshal error = %v, want ErrInvalidSourceMap", err)
	}

	_, err = json.Marshal(SourceMap{Entries: []Entry{
		{ID: 1, Path: "features/a.lzi", LineOffsets: []uint32{1, 2}},
	}})
	if !errors.Is(err, ErrInvalidSourceMap) {
		t.Fatalf("Marshal error = %v, want ErrInvalidSourceMap", err)
	}
}

func TestValidationErrorOrderIsDeterministic(t *testing.T) {
	_, err := New([]Entry{
		{ID: 2, Path: "features/z.lzi", LineOffsets: []uint32{1, 2}},
		{ID: 1, Path: "../bad.lzi", LineOffsets: []uint32{0, 1}},
	})
	if !errors.Is(err, ErrInvalidSourceMap) {
		t.Fatalf("New error = %v, want ErrInvalidSourceMap", err)
	}
	if !strings.Contains(err.Error(), "entry 1: path") {
		t.Fatalf("New error = %v, want first sorted entry error", err)
	}
}

func sourcemapMustNew(t *testing.T, entries []Entry) SourceMap {
	t.Helper()

	sourceMap, err := New(entries)
	if err != nil {
		t.Fatalf("New error = %v", err)
	}
	return sourceMap
}
