package debug

import (
	"strconv"
	"strings"
	"unicode"
)

// StackFrame describes one parsed frame from a Go panic or runtime stack.
type StackFrame struct {
	Function string `json:"function,omitempty"`
	File     string `json:"file,omitempty"`
	Line     int    `json:"line,omitempty"`
	Column   int    `json:"column,omitempty"`
	LZI      bool   `json:"lzi,omitempty"`
}

// ParseStack extracts Go stack frames from stack.
//
// Frames are parsed from the standard Go two-line shape:
//
//	function(args...)
//	    file:line +0x...
//
// Malformed or partial frames are ignored. LZI is set when the parsed file
// path ends in ".lzi", which is how generated Go frames appear after //line
// directives point back to Lazuli source.
func ParseStack(stack []byte) []StackFrame {
	if len(stack) == 0 {
		return nil
	}

	lines := strings.Split(strings.ReplaceAll(string(stack), "\r\n", "\n"), "\n")
	frames := make([]StackFrame, 0)
	pendingFunction := ""

	for _, rawLine := range lines {
		if file, line, column, ok := stackParseLocationLine(rawLine); ok {
			if pendingFunction == "" {
				continue
			}
			frames = append(frames, StackFrame{
				Function: pendingFunction,
				File:     file,
				Line:     line,
				Column:   column,
				LZI:      stackIsLZIFile(file),
			})
			pendingFunction = ""
			continue
		}

		line := strings.TrimSpace(rawLine)
		if line == "" || strings.HasPrefix(line, "goroutine ") || strings.HasPrefix(line, "panic:") {
			pendingFunction = ""
			continue
		}
		if function, ok := stackParseFunctionLine(line); ok {
			pendingFunction = function
			continue
		}
		pendingFunction = ""
	}

	if len(frames) == 0 {
		return nil
	}
	return frames
}

func stackParseFunctionLine(line string) (string, bool) {
	if function, ok := strings.CutPrefix(line, "created by "); ok {
		if before, _, found := strings.Cut(function, " in goroutine "); found {
			function = before
		}
		function = strings.TrimSpace(function)
		return function, function != ""
	}

	index := strings.LastIndexByte(line, '(')
	if index <= 0 {
		return "", false
	}
	function := strings.TrimSpace(line[:index])
	return function, function != ""
}

func stackParseLocationLine(rawLine string) (file string, line int, column int, ok bool) {
	if rawLine == "" {
		return "", 0, 0, false
	}
	first, _ := stackFirstRune(rawLine)
	if !unicode.IsSpace(first) {
		return "", 0, 0, false
	}

	location := strings.TrimSpace(rawLine)
	if index := strings.Index(location, " +0x"); index >= 0 {
		location = strings.TrimSpace(location[:index])
	}
	file, line, column, ok = stackParseLocation(location)
	return file, line, column, ok
}

func stackParseLocation(location string) (file string, line int, column int, ok bool) {
	fileAndLine, last, ok := stackCutLast(location, ":")
	if !ok {
		return "", 0, 0, false
	}

	firstValue, err := strconv.Atoi(strings.TrimSpace(last))
	if err != nil || firstValue <= 0 {
		return "", 0, 0, false
	}

	file, maybeLine, hasColumn := stackCutLast(fileAndLine, ":")
	if hasColumn {
		lineValue, lineErr := strconv.Atoi(strings.TrimSpace(maybeLine))
		if lineErr == nil && lineValue > 0 {
			file = strings.TrimSpace(file)
			if file == "" {
				return "", 0, 0, false
			}
			return file, lineValue, firstValue, true
		}
	}

	file = strings.TrimSpace(fileAndLine)
	if file == "" {
		return "", 0, 0, false
	}
	return file, firstValue, 0, true
}

func stackCutLast(s string, sep string) (before string, after string, ok bool) {
	index := strings.LastIndex(s, sep)
	if index < 0 {
		return "", "", false
	}
	return s[:index], s[index+len(sep):], true
}

func stackIsLZIFile(file string) bool {
	return strings.HasSuffix(strings.ToLower(file), ".lzi")
}

func stackFirstRune(s string) (rune, bool) {
	for _, r := range s {
		return r, true
	}
	return 0, false
}
