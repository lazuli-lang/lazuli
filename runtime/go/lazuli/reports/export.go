// Package reports provides deterministic export helpers for generated reports.
package reports

import (
	"bytes"
	"context"
	"encoding"
	"encoding/csv"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
)

var (
	// ErrNilWriter is returned when an export helper is used without an io.Writer.
	ErrNilWriter = errors.New("lazuli/reports: writer is nil")

	// ErrNoColumns is returned when an export has no column definitions.
	ErrNoColumns = errors.New("lazuli/reports: no columns")

	// ErrInvalidColumn is wrapped when a column definition is unusable.
	ErrInvalidColumn = errors.New("lazuli/reports: invalid column")

	// ErrDuplicateColumn is wrapped when two column definitions use the same key.
	ErrDuplicateColumn = errors.New("lazuli/reports: duplicate column")

	// ErrNilRowStream is returned when a streaming export is used without a row stream.
	ErrNilRowStream = errors.New("lazuli/reports: row stream is nil")
)

// Column describes one exported report column.
//
// Key is the stable row-map key and JSON object field name. Header is the CSV
// header label; when empty, Key is used.
type Column struct {
	Key    string `json:"key"`
	Header string `json:"header,omitempty"`
}

// Row is a report row keyed by Column.Key.
//
// Export helpers read values in the order of the supplied column definitions,
// so Row map iteration order never affects CSV or JSON output.
type Row map[string]any

// RowStream pushes rows into a streaming export.
//
// Implementations should stop and return the yield error immediately when
// yield returns a non-nil error.
type RowStream func(context.Context, func(Row) error) error

// CSVOption configures CSV exports.
type CSVOption func(*csvOptions)

type csvOptions struct {
	guardInjection bool
}

// WithCSVInjectionGuard prefixes dangerous string cells with a single quote.
//
// The guard is intentionally opt-in because it changes cell text. It applies to
// text-like values whose first non-space character could be interpreted as a
// spreadsheet formula trigger.
func WithCSVInjectionGuard(enabled bool) CSVOption {
	return func(options *csvOptions) {
		options.guardInjection = enabled
	}
}

// WriteCSV writes rows as CSV using columns for header labels and row ordering.
func WriteCSV(ctx context.Context, w io.Writer, columns []Column, rows []Row, opts ...CSVOption) error {
	return StreamCSV(ctx, w, columns, sliceRowStream(rows), opts...)
}

// StreamCSV streams rows as CSV using columns for header labels and row ordering.
func StreamCSV(ctx context.Context, w io.Writer, columns []Column, stream RowStream, opts ...CSVOption) error {
	if w == nil {
		return ErrNilWriter
	}
	if stream == nil {
		return ErrNilRowStream
	}
	if err := ValidateColumns(columns); err != nil {
		return err
	}
	ctx = contextOrBackground(ctx)
	options := applyCSVOptions(opts)

	cw := csv.NewWriter(w)
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := cw.Write(csvHeaders(columns)); err != nil {
		return fmt.Errorf("lazuli/reports: write csv header: %w", err)
	}
	if err := flushCSV(cw, "header"); err != nil {
		return err
	}

	err := stream(ctx, func(row Row) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		record, err := csvRecord(columns, row, options)
		if err != nil {
			return err
		}
		if err := cw.Write(record); err != nil {
			return fmt.Errorf("lazuli/reports: write csv row: %w", err)
		}
		if err := flushCSV(cw, "row"); err != nil {
			return err
		}
		return ctx.Err()
	})
	if err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	return flushCSV(cw, "csv")
}

// WriteJSON writes rows as a JSON array of objects in column order.
func WriteJSON(ctx context.Context, w io.Writer, columns []Column, rows []Row) error {
	return StreamJSON(ctx, w, columns, sliceRowStream(rows))
}

// StreamJSON streams rows as a JSON array of objects in column order.
func StreamJSON(ctx context.Context, w io.Writer, columns []Column, stream RowStream) error {
	if w == nil {
		return ErrNilWriter
	}
	if stream == nil {
		return ErrNilRowStream
	}
	if err := ValidateColumns(columns); err != nil {
		return err
	}
	ctx = contextOrBackground(ctx)

	if err := ctx.Err(); err != nil {
		return err
	}
	if _, err := io.WriteString(w, "["); err != nil {
		return fmt.Errorf("lazuli/reports: write json array: %w", err)
	}

	first := true
	err := stream(ctx, func(row Row) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		payload, err := marshalOrderedRow(columns, row)
		if err != nil {
			return err
		}
		if !first {
			if _, err := io.WriteString(w, ","); err != nil {
				return fmt.Errorf("lazuli/reports: write json separator: %w", err)
			}
		}
		if _, err := w.Write(payload); err != nil {
			return fmt.Errorf("lazuli/reports: write json row: %w", err)
		}
		first = false
		return ctx.Err()
	})
	if err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if _, err := io.WriteString(w, "]\n"); err != nil {
		return fmt.Errorf("lazuli/reports: close json array: %w", err)
	}
	return nil
}

// ValidateColumns checks that columns are non-empty and have unique keys.
func ValidateColumns(columns []Column) error {
	if len(columns) == 0 {
		return ErrNoColumns
	}

	seen := make(map[string]struct{}, len(columns))
	for i, column := range columns {
		key := strings.TrimSpace(column.Key)
		if key == "" || key != column.Key {
			return fmt.Errorf("%w: column %d key must be non-empty and trimmed", ErrInvalidColumn, i)
		}
		if _, ok := seen[key]; ok {
			return fmt.Errorf("%w: column %d key %q", ErrDuplicateColumn, i, key)
		}
		seen[key] = struct{}{}
	}
	return nil
}

func sliceRowStream(rows []Row) RowStream {
	return func(ctx context.Context, yield func(Row) error) error {
		for _, row := range rows {
			if err := ctx.Err(); err != nil {
				return err
			}
			if err := yield(row); err != nil {
				return err
			}
		}
		return ctx.Err()
	}
}

func applyCSVOptions(opts []CSVOption) csvOptions {
	options := csvOptions{}
	for _, opt := range opts {
		if opt != nil {
			opt(&options)
		}
	}
	return options
}

func csvHeaders(columns []Column) []string {
	headers := make([]string, len(columns))
	for i, column := range columns {
		headers[i] = column.Header
		if headers[i] == "" {
			headers[i] = column.Key
		}
	}
	return headers
}

func csvRecord(columns []Column, row Row, options csvOptions) ([]string, error) {
	record := make([]string, len(columns))
	for i, column := range columns {
		cell, err := csvCell(row[column.Key], options)
		if err != nil {
			return nil, fmt.Errorf("lazuli/reports: encode csv column %q: %w", column.Key, err)
		}
		record[i] = cell
	}
	return record, nil
}

func flushCSV(w *csv.Writer, label string) error {
	w.Flush()
	if err := w.Error(); err != nil {
		return fmt.Errorf("lazuli/reports: flush %s: %w", label, err)
	}
	return nil
}

func csvCell(value any, options csvOptions) (string, error) {
	text, wasText, err := stringifyCSVValue(value)
	if err != nil {
		return "", err
	}
	if options.guardInjection && wasText {
		text = guardCSVInjection(text)
	}
	return text, nil
}

func stringifyCSVValue(value any) (string, bool, error) {
	switch typed := value.(type) {
	case nil:
		return "", false, nil
	case string:
		return typed, true, nil
	case []byte:
		return string(typed), true, nil
	case encoding.TextMarshaler:
		data, err := typed.MarshalText()
		if err != nil {
			return "", false, err
		}
		return string(data), true, nil
	case fmt.Stringer:
		return typed.String(), true, nil
	default:
		return fmt.Sprint(value), false, nil
	}
}

func guardCSVInjection(text string) string {
	if text == "" {
		return text
	}
	trimmed := strings.TrimLeft(text, " \t\r\n")
	if trimmed == "" {
		return text
	}
	switch trimmed[0] {
	case '=', '+', '-', '@':
		return "'" + text
	default:
		return text
	}
}

func marshalOrderedRow(columns []Column, row Row) ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteByte('{')
	for i, column := range columns {
		if i > 0 {
			buf.WriteByte(',')
		}
		key, err := json.Marshal(column.Key)
		if err != nil {
			return nil, fmt.Errorf("lazuli/reports: encode json key %q: %w", column.Key, err)
		}
		value, err := json.Marshal(row[column.Key])
		if err != nil {
			return nil, fmt.Errorf("lazuli/reports: encode json column %q: %w", column.Key, err)
		}
		buf.Write(key)
		buf.WriteByte(':')
		buf.Write(value)
	}
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

func contextOrBackground(ctx context.Context) context.Context {
	if ctx == nil {
		return context.Background()
	}
	return ctx
}
