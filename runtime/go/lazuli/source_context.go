package lazuli

import (
	"context"
	"errors"
	"strconv"
	"strings"
)

var (
	// ErrSourceTagMalformed is returned when a serialized source tag cannot be parsed.
	ErrSourceTagMalformed = errors.New("lazuli: source tag malformed")

	// ErrSourceLocationMalformed is returned when a file:line:column source location cannot be parsed.
	ErrSourceLocationMalformed = errors.New("lazuli: source location malformed")
)

// SourceTag describes the generated Lazuli source operation currently running.
//
// EXPERIMENTAL: subject to change before 1.0.
type SourceTag struct {
	Feature string
	Kind    string
	Name    string
	File    string
	Line    int
	Column  int
}

type sourceContextKey struct{}

// WithSource returns a child context carrying tag as the active Lazuli source.
func WithSource(ctx context.Context, tag SourceTag) context.Context {
	return context.WithValue(ctx, sourceContextKey{}, tag)
}

// SourceFromContext reads the active Lazuli source tag from ctx.
func SourceFromContext(ctx context.Context) (SourceTag, bool) {
	if ctx == nil {
		return SourceTag{}, false
	}
	tag, ok := ctx.Value(sourceContextKey{}).(SourceTag)
	return tag, ok
}

// FormatSourceTag formats tag as "feature/kind/name@file:line:column".
func FormatSourceTag(tag SourceTag) string {
	return strings.TrimSpace(tag.Feature) + "/" +
		strings.TrimSpace(tag.Kind) + "/" +
		strings.TrimSpace(tag.Name) + "@" +
		FormatSourceLocation(tag.File, tag.Line, tag.Column)
}

// ParseSourceTag parses the string produced by FormatSourceTag.
func ParseSourceTag(raw string) (SourceTag, error) {
	head, location, ok := strings.Cut(strings.TrimSpace(raw), "@")
	if !ok {
		return SourceTag{}, ErrSourceTagMalformed
	}

	parts := strings.Split(head, "/")
	if len(parts) != 3 {
		return SourceTag{}, ErrSourceTagMalformed
	}

	tag := SourceTag{
		Feature: strings.TrimSpace(parts[0]),
		Kind:    strings.TrimSpace(parts[1]),
		Name:    strings.TrimSpace(parts[2]),
	}
	if tag.Feature == "" || tag.Kind == "" || tag.Name == "" {
		return SourceTag{}, ErrSourceTagMalformed
	}

	file, line, column, err := ParseSourceLocation(location)
	if err != nil {
		return SourceTag{}, errors.Join(ErrSourceTagMalformed, err)
	}
	tag.File = file
	tag.Line = line
	tag.Column = column
	return tag, nil
}

// FormatSourceLocation formats a resolved Lazuli source location as file:line:column.
func FormatSourceLocation(file string, line int, column int) string {
	return strings.TrimSpace(file) + ":" + strconv.Itoa(line) + ":" + strconv.Itoa(column)
}

// ParseSourceLocation parses a file:line:column source location.
func ParseSourceLocation(raw string) (file string, line int, column int, err error) {
	fileAndLine, columnText, ok := sourceContextCutLast(strings.TrimSpace(raw), ":")
	if !ok {
		return "", 0, 0, ErrSourceLocationMalformed
	}
	file, lineText, ok := sourceContextCutLast(fileAndLine, ":")
	if !ok {
		return "", 0, 0, ErrSourceLocationMalformed
	}

	file = strings.TrimSpace(file)
	line, lineErr := strconv.Atoi(strings.TrimSpace(lineText))
	column, columnErr := strconv.Atoi(strings.TrimSpace(columnText))
	if file == "" || lineErr != nil || columnErr != nil || line <= 0 || column <= 0 {
		return "", 0, 0, ErrSourceLocationMalformed
	}
	return file, line, column, nil
}

func sourceContextCutLast(s string, sep string) (before string, after string, ok bool) {
	index := strings.LastIndex(s, sep)
	if index < 0 {
		return "", "", false
	}
	return s[:index], s[index+len(sep):], true
}
