package sourcemap

import (
	"reflect"
	"testing"
)

type stackTestFrame struct {
	file string
	line int
}

func (f stackTestFrame) SourceFile() string {
	return f.file
}

func (f stackTestFrame) SourceLine() int {
	return f.line
}

func TestLookupPathReturnsSourceMapEntry(t *testing.T) {
	sourceMap := sourcemapMustNew(t, []Entry{
		{ID: 2, Path: "features/order.lzi", LineOffsets: []uint32{0, 6}},
		{ID: 1, Path: "features/customer.lzi", LineOffsets: []uint32{0, 8}},
	})

	entry, ok := sourceMap.LookupPath("features/customer.lzi")
	if !ok {
		t.Fatal("LookupPath did not find source path")
	}
	if entry.ID != 1 || entry.Path != "features/customer.lzi" {
		t.Fatalf("LookupPath entry = %+v, want customer entry", entry)
	}
}

func TestResolveFileLineMapsStackPathToLocation(t *testing.T) {
	sourceMap := sourcemapMustNew(t, []Entry{
		{ID: 7, Path: "features/customer.lzi", LineOffsets: []uint32{0, 12, 30, 50}},
	})

	loc, ok := sourceMap.ResolveFileLine(`C:\work\capsule\features\customer.lzi`, 3)
	if !ok {
		t.Fatal("ResolveFileLine did not find stack path")
	}
	if loc.FileID != 7 || loc.Path != "features/customer.lzi" || loc.Line != 3 || loc.Column != 1 {
		t.Fatalf("ResolveFileLine location = %+v, want features/customer.lzi:3:1", loc)
	}
}

func TestResolveFileLineRejectsUnknownOrOutOfRangePairs(t *testing.T) {
	sourceMap := sourcemapMustNew(t, []Entry{
		{ID: 1, Path: "features/customer.lzi", LineOffsets: []uint32{0, 10}},
	})

	tests := []struct {
		name string
		file string
		line int
	}{
		{name: "missing file", file: "features/order.lzi", line: 1},
		{name: "zero line", file: "features/customer.lzi", line: 0},
		{name: "negative line", file: "features/customer.lzi", line: -1},
		{name: "line past EOF", file: "features/customer.lzi", line: 2},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, ok := sourceMap.ResolveFileLine(tt.file, tt.line); ok {
				t.Fatalf("ResolveFileLine(%q, %d) ok = true, want false", tt.file, tt.line)
			}
		})
	}
}

func TestResolveFrameAcceptsSmallInterface(t *testing.T) {
	sourceMap := sourcemapMustNew(t, []Entry{
		{ID: 3, Path: "features/customer.lzi", LineOffsets: []uint32{0, 4, 9}},
	})

	loc, ok := sourceMap.ResolveFrame(stackTestFrame{
		file: "features/customer.lzi",
		line: 2,
	})
	if !ok {
		t.Fatal("ResolveFrame did not resolve interface frame")
	}
	if loc.FileID != 3 || loc.Line != 2 || loc.Column != 1 {
		t.Fatalf("ResolveFrame location = %+v, want file 3 line 2 column 1", loc)
	}
}

func TestResolveStackFiltersAndPreservesStackOrder(t *testing.T) {
	sourceMap := sourcemapMustNew(t, []Entry{
		{ID: 1, Path: "features/customer.lzi", LineOffsets: []uint32{0, 8, 20}},
		{ID: 2, Path: "features/order.lzi", LineOffsets: []uint32{0, 5, 11}},
	})
	frames := []StackFrame{
		{Function: "runtime.gopanic", File: "runtime/panic.go", Line: 100},
		{Function: "customer.Handle", File: "features/customer.lzi", Line: 2},
		{Function: "order.Handle", File: "/srv/app/features/order.lzi", Line: 1},
	}

	mapped := sourceMap.ResolveStack(frames)

	if len(mapped) != 2 {
		t.Fatalf("ResolveStack length = %d, want 2", len(mapped))
	}
	gotFunctions := []string{mapped[0].Frame.Function, mapped[1].Frame.Function}
	if !reflect.DeepEqual(gotFunctions, []string{"customer.Handle", "order.Handle"}) {
		t.Fatalf("resolved functions = %v, want customer then order", gotFunctions)
	}
	if mapped[0].Location.FileID != 1 || mapped[0].Location.Line != 2 {
		t.Fatalf("first location = %+v, want customer line 2", mapped[0].Location)
	}
	if mapped[1].Location.FileID != 2 || mapped[1].Location.Line != 1 {
		t.Fatalf("second location = %+v, want order line 1", mapped[1].Location)
	}
}

func TestResolveFileLineRejectsAmbiguousSuffixMatch(t *testing.T) {
	sourceMap := sourcemapMustNew(t, []Entry{
		{ID: 1, Path: "features/customer.lzi", LineOffsets: []uint32{0, 8}},
		{ID: 2, Path: "customer.lzi", LineOffsets: []uint32{0, 8}},
	})

	if _, ok := sourceMap.ResolveFileLine("/srv/app/features/customer.lzi", 1); ok {
		t.Fatal("ResolveFileLine resolved ambiguous suffix, want false")
	}
}
