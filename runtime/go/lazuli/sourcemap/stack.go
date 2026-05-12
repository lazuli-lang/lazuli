package sourcemap

import (
	"path"
	"strings"
)

// SourceFrame is the minimal stack-frame shape needed for source-map lookup.
//
// Packages that parse stack traces can implement this interface without making
// sourcemap depend on their concrete frame type.
type SourceFrame interface {
	SourceFile() string
	SourceLine() int
}

// StackFrame is a simple parsed stack frame that can be resolved through a
// SourceMap.
type StackFrame struct {
	Function string `json:"function,omitempty"`
	File     string `json:"file,omitempty"`
	Line     int    `json:"line,omitempty"`
}

// SourceFile returns the frame file path.
func (f StackFrame) SourceFile() string {
	return f.File
}

// SourceLine returns the frame line number.
func (f StackFrame) SourceLine() int {
	return f.Line
}

// MappedStackFrame pairs a parsed stack frame with its Lazuli source location.
type MappedStackFrame struct {
	Frame    StackFrame `json:"frame"`
	Location Location   `json:"location"`
}

// LookupPath returns the entry for sourcePath.
func (m SourceMap) LookupPath(sourcePath string) (Entry, bool) {
	entries, err := normalizeEntries(m.Entries)
	if err != nil {
		return Entry{}, false
	}
	for _, entry := range entries {
		if entry.Path == sourcePath {
			return entry, true
		}
	}
	return Entry{}, false
}

// ResolveFileLine resolves a parsed stack file:line pair to a Lazuli source
// location. The returned column is the first column on the resolved line.
func (m SourceMap) ResolveFileLine(file string, line int) (Location, bool) {
	entry, ok := m.lookupStackEntry(file)
	if !ok {
		return Location{}, false
	}
	return stackLocationForEntry(entry, line)
}

// ResolveFrame resolves frame to a Lazuli source location.
func (m SourceMap) ResolveFrame(frame SourceFrame) (Location, bool) {
	if frame == nil {
		return Location{}, false
	}
	return m.ResolveFileLine(frame.SourceFile(), frame.SourceLine())
}

// ResolveStackFrame resolves frame and returns it with its Lazuli source
// location.
func (m SourceMap) ResolveStackFrame(frame StackFrame) (MappedStackFrame, bool) {
	location, ok := m.ResolveFrame(frame)
	if !ok {
		return MappedStackFrame{}, false
	}
	return MappedStackFrame{
		Frame:    frame,
		Location: location,
	}, true
}

// ResolveStack returns the frames that resolve to m, preserving stack order.
// Unmatched frames are omitted.
func (m SourceMap) ResolveStack(frames []StackFrame) []MappedStackFrame {
	mapped := make([]MappedStackFrame, 0, len(frames))
	for _, frame := range frames {
		mappedFrame, ok := m.ResolveStackFrame(frame)
		if ok {
			mapped = append(mapped, mappedFrame)
		}
	}
	return mapped
}

func (m SourceMap) lookupStackEntry(file string) (Entry, bool) {
	stackPath := cleanStackPath(file)
	if stackPath == "" {
		return Entry{}, false
	}

	entries, err := normalizeEntries(m.Entries)
	if err != nil {
		return Entry{}, false
	}
	for _, entry := range entries {
		if entry.Path == stackPath {
			return entry, true
		}
	}

	var match Entry
	matched := false
	for _, entry := range entries {
		if strings.HasSuffix(stackPath, "/"+entry.Path) {
			if matched {
				return Entry{}, false
			}
			match = entry
			matched = true
		}
	}
	if !matched {
		return Entry{}, false
	}
	return match, true
}

func cleanStackPath(file string) string {
	file = strings.TrimSpace(file)
	if file == "" {
		return ""
	}
	file = strings.ReplaceAll(file, "\\", "/")
	return path.Clean(file)
}

func stackLocationForEntry(entry Entry, line int) (Location, bool) {
	if line <= 0 {
		return Location{}, false
	}
	if err := validateLineOffsets(entry.LineOffsets); err != nil {
		return Location{}, false
	}
	if line > len(entry.LineOffsets)-1 {
		return Location{}, false
	}
	return Location{
		FileID: entry.ID,
		Path:   entry.Path,
		Line:   uint32(line),
		Column: 1,
	}, true
}
