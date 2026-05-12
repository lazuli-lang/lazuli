package reports

import (
	"context"
	"encoding/csv"
	"errors"
	"fmt"
	"io"
	"strings"
)

var (
	// ErrNilReader is returned when an import helper is used without an io.Reader.
	ErrNilReader = errors.New("lazuli/reports: reader is nil")

	// ErrMissingCSVImportHeader reports an empty CSV import.
	ErrMissingCSVImportHeader = errors.New("lazuli/reports: missing csv import header")

	// ErrDuplicateCSVImportHeader reports a repeated input CSV header.
	ErrDuplicateCSVImportHeader = errors.New("lazuli/reports: duplicate csv import header")

	// ErrMissingRequiredCSVImportColumn reports a required import column absent from the CSV header.
	ErrMissingRequiredCSVImportColumn = errors.New("lazuli/reports: missing required csv import column")

	// ErrUnknownCSVImportColumn reports an input CSV column not declared by the import schema.
	ErrUnknownCSVImportColumn = errors.New("lazuli/reports: unknown csv import column")

	// ErrInvalidCSVImportRow wraps CSV read failures and row validator failures.
	ErrInvalidCSVImportRow = errors.New("lazuli/reports: invalid csv import row")
)

// ImportColumn describes one accepted CSV import column.
//
// Key is the stable Row key. Header is the CSV header label; when empty, Key is
// used. Required marks headers that must be present before row validation runs.
type ImportColumn struct {
	Key      string `json:"key"`
	Header   string `json:"header,omitempty"`
	Required bool   `json:"required,omitempty"`
}

// CSVImportUnknownColumnBehavior configures how header mapping handles
// undeclared CSV columns.
type CSVImportUnknownColumnBehavior uint8

const (
	// RejectUnknownCSVImportColumns makes undeclared input columns validation errors.
	RejectUnknownCSVImportColumns CSVImportUnknownColumnBehavior = iota
	// IgnoreUnknownCSVImportColumns leaves undeclared input columns out of mapped rows.
	IgnoreUnknownCSVImportColumns
)

// CSVImportRowValidator validates one mapped CSV data row.
//
// rowNumber is the 1-based CSV file row number, so the first data row is 2.
// Return *CSVImportError, or errors.Join of them, to attach column-specific
// details to the final report.
type CSVImportRowValidator func(ctx context.Context, rowNumber int, row Row) error

// CSVImportOption configures CSV import validation helpers.
type CSVImportOption func(*csvImportOptions)

type csvImportOptions struct {
	unknownColumnBehavior CSVImportUnknownColumnBehavior
	validateRow           CSVImportRowValidator
}

// WithCSVImportUnknownColumns configures whether undeclared input columns are
// rejected or ignored. The default is RejectUnknownCSVImportColumns.
func WithCSVImportUnknownColumns(behavior CSVImportUnknownColumnBehavior) CSVImportOption {
	return func(options *csvImportOptions) {
		options.unknownColumnBehavior = behavior
	}
}

// WithCSVImportRowValidator runs validate for every mapped data row.
func WithCSVImportRowValidator(validate CSVImportRowValidator) CSVImportOption {
	return func(options *csvImportOptions) {
		options.validateRow = validate
	}
}

// CSVImportHeaderMap maps input CSV header positions to import column keys.
//
// Headers are normalized with strings.TrimSpace. IndexByKey and KeyByIndex use
// zero-based CSV record indexes.
type CSVImportHeaderMap struct {
	Headers        []string
	IndexByKey     map[string]int
	KeyByIndex     map[int]string
	UnknownHeaders []CSVImportUnknownHeader
}

// CSVImportUnknownHeader describes an undeclared CSV input header.
type CSVImportUnknownHeader struct {
	Index  int
	Header string
}

// Row converts a CSV record into a Row using the mapped import columns.
func (m CSVImportHeaderMap) Row(record []string) Row {
	row := make(Row, len(m.KeyByIndex))
	for index, key := range m.KeyByIndex {
		if index < len(record) {
			row[key] = record[index]
			continue
		}
		row[key] = ""
	}
	return row
}

// CSVImportErrorReport reports one or more CSV import validation errors.
type CSVImportErrorReport struct {
	Errors []*CSVImportError
}

// Error returns a stable human-readable import report summary.
func (r *CSVImportErrorReport) Error() string {
	if r == nil || len(r.Errors) == 0 {
		return "<nil>"
	}
	if len(r.Errors) == 1 {
		return r.Errors[0].Error()
	}
	return fmt.Sprintf("lazuli/reports: csv import validation failed (%d errors)", len(r.Errors))
}

// Unwrap exposes report entries for errors.Is and errors.As.
func (r *CSVImportErrorReport) Unwrap() []error {
	if r == nil || len(r.Errors) == 0 {
		return nil
	}
	errs := make([]error, 0, len(r.Errors))
	for _, err := range r.Errors {
		if err != nil {
			errs = append(errs, err)
		}
	}
	return errs
}

// CSVImportError reports one CSV import error with optional row and column
// coordinates. Row and Column are 1-based CSV file positions when set.
type CSVImportError struct {
	Row    int
	Column int
	Key    string
	Header string
	Err    error
}

// Error returns a stable human-readable import error.
func (e *CSVImportError) Error() string {
	if e == nil {
		return "<nil>"
	}

	var parts []string
	if e.Row > 0 {
		parts = append(parts, fmt.Sprintf("row %d", e.Row))
	}
	if e.Column > 0 {
		parts = append(parts, fmt.Sprintf("column %d", e.Column))
	}
	if e.Key != "" {
		parts = append(parts, fmt.Sprintf("key %q", e.Key))
	}
	if e.Header != "" {
		parts = append(parts, fmt.Sprintf("header %q", e.Header))
	}
	if len(parts) == 0 {
		return fmt.Sprintf("lazuli/reports: csv import: %v", e.Err)
	}
	return fmt.Sprintf("lazuli/reports: csv import %s: %v", strings.Join(parts, " "), e.Err)
}

// Unwrap exposes the classified cause for errors.Is and errors.As.
func (e *CSVImportError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

// ValidateImportColumns checks that import columns are non-empty and have
// unique keys and header labels.
func ValidateImportColumns(columns []ImportColumn) error {
	if len(columns) == 0 {
		return ErrNoColumns
	}

	seenKeys := make(map[string]struct{}, len(columns))
	seenHeaders := make(map[string]struct{}, len(columns))
	for i, column := range columns {
		key := strings.TrimSpace(column.Key)
		if key == "" || key != column.Key {
			return fmt.Errorf("%w: column %d key must be non-empty and trimmed", ErrInvalidColumn, i)
		}
		if _, ok := seenKeys[key]; ok {
			return fmt.Errorf("%w: column %d key %q", ErrDuplicateColumn, i, key)
		}
		seenKeys[key] = struct{}{}

		header := importColumnHeader(column)
		if _, ok := seenHeaders[header]; ok {
			return fmt.Errorf("%w: column %d header %q", ErrDuplicateColumn, i, header)
		}
		seenHeaders[header] = struct{}{}
	}
	return nil
}

// MapCSVImportHeaders maps input CSV headers to import columns and validates
// required and unknown column behavior before row processing.
func MapCSVImportHeaders(headers []string, columns []ImportColumn, opts ...CSVImportOption) (CSVImportHeaderMap, error) {
	if err := ValidateImportColumns(columns); err != nil {
		return CSVImportHeaderMap{}, err
	}
	options := applyCSVImportOptions(opts)

	expectedByHeader := make(map[string]ImportColumn, len(columns))
	for _, column := range columns {
		expectedByHeader[importColumnHeader(column)] = column
	}

	mapping := CSVImportHeaderMap{
		Headers:    make([]string, len(headers)),
		IndexByKey: make(map[string]int, len(columns)),
		KeyByIndex: make(map[int]string, len(columns)),
	}
	seenHeaders := make(map[string]int, len(headers))
	var errs []*CSVImportError

	for i, rawHeader := range headers {
		header := strings.TrimSpace(rawHeader)
		mapping.Headers[i] = header
		if header == "" {
			mapping.UnknownHeaders = append(mapping.UnknownHeaders, CSVImportUnknownHeader{
				Index:  i,
				Header: header,
			})
			if options.unknownColumnBehavior == RejectUnknownCSVImportColumns {
				errs = append(errs, csvImportError(1, i+1, "", "", ErrUnknownCSVImportColumn))
			}
			continue
		}
		if first, ok := seenHeaders[header]; ok {
			errs = append(errs, csvImportError(
				1,
				i+1,
				"",
				header,
				fmt.Errorf("%w: also appears at column %d", ErrDuplicateCSVImportHeader, first+1),
			))
			continue
		}
		seenHeaders[header] = i

		column, ok := expectedByHeader[header]
		if !ok {
			mapping.UnknownHeaders = append(mapping.UnknownHeaders, CSVImportUnknownHeader{
				Index:  i,
				Header: header,
			})
			if options.unknownColumnBehavior == RejectUnknownCSVImportColumns {
				errs = append(errs, csvImportError(1, i+1, "", header, ErrUnknownCSVImportColumn))
			}
			continue
		}
		mapping.IndexByKey[column.Key] = i
		mapping.KeyByIndex[i] = column.Key
	}

	for _, column := range columns {
		if !column.Required {
			continue
		}
		if _, ok := mapping.IndexByKey[column.Key]; ok {
			continue
		}
		errs = append(errs, &CSVImportError{
			Row:    1,
			Key:    column.Key,
			Header: importColumnHeader(column),
			Err:    ErrMissingRequiredCSVImportColumn,
		})
	}

	if err := csvImportErrorReport(errs); err != nil {
		return CSVImportHeaderMap{}, err
	}
	return mapping, nil
}

// ReadImportCSV reads, maps, and validates CSV rows using import columns.
func ReadImportCSV(ctx context.Context, r io.Reader, columns []ImportColumn, opts ...CSVImportOption) ([]Row, error) {
	if r == nil {
		return nil, ErrNilReader
	}
	if err := ValidateImportColumns(columns); err != nil {
		return nil, err
	}
	ctx = contextOrBackground(ctx)
	options := applyCSVImportOptions(opts)

	reader := csv.NewReader(r)
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	headers, err := reader.Read()
	if errors.Is(err, io.EOF) {
		return nil, csvImportErrorReport([]*CSVImportError{{
			Row: 1,
			Err: ErrMissingCSVImportHeader,
		}})
	}
	if err != nil {
		return nil, csvImportErrorReport([]*CSVImportError{csvImportReadError(err, 1)})
	}

	mapping, err := MapCSVImportHeaders(headers, columns, opts...)
	if err != nil {
		return nil, err
	}

	var rows []Row
	var errs []*CSVImportError
	rowNumber := 1
	for {
		if err := ctx.Err(); err != nil {
			return rows, err
		}

		record, err := reader.Read()
		if errors.Is(err, io.EOF) {
			break
		}
		rowNumber++
		if err != nil {
			errs = append(errs, csvImportReadError(err, rowNumber))
			continue
		}

		row := mapping.Row(record)
		rows = append(rows, row)
		if options.validateRow == nil {
			continue
		}
		if err := options.validateRow(ctx, rowNumber, row); err != nil {
			errs = append(errs, csvImportRowValidationErrors(rowNumber, mapping, err)...)
		}
	}

	return rows, csvImportErrorReport(errs)
}

func applyCSVImportOptions(opts []CSVImportOption) csvImportOptions {
	options := csvImportOptions{
		unknownColumnBehavior: RejectUnknownCSVImportColumns,
	}
	for _, opt := range opts {
		if opt != nil {
			opt(&options)
		}
	}
	return options
}

func importColumnHeader(column ImportColumn) string {
	header := strings.TrimSpace(column.Header)
	if header == "" {
		return column.Key
	}
	return header
}

func csvImportReadError(err error, rowNumber int) *CSVImportError {
	importErr := &CSVImportError{
		Row: rowNumber,
		Err: errors.Join(ErrInvalidCSVImportRow, err),
	}
	var parseErr *csv.ParseError
	if errors.As(err, &parseErr) {
		if parseErr.Line > 0 {
			importErr.Row = parseErr.Line
		}
		if parseErr.Column > 0 {
			importErr.Column = parseErr.Column
		}
	}
	return importErr
}

func csvImportRowValidationErrors(rowNumber int, mapping CSVImportHeaderMap, err error) []*CSVImportError {
	if err == nil {
		return nil
	}
	if joined, ok := err.(interface{ Unwrap() []error }); ok {
		children := joined.Unwrap()
		errs := make([]*CSVImportError, 0, len(children))
		for _, child := range children {
			errs = append(errs, csvImportRowValidationErrors(rowNumber, mapping, child)...)
		}
		return errs
	}

	var importErr *CSVImportError
	if errors.As(err, &importErr) {
		out := *importErr
		if out.Row == 0 {
			out.Row = rowNumber
		}
		if out.Column == 0 && out.Key != "" {
			if index, ok := mapping.IndexByKey[out.Key]; ok {
				out.Column = index + 1
				if out.Header == "" && index < len(mapping.Headers) {
					out.Header = mapping.Headers[index]
				}
			}
		}
		out.Err = ensureCSVImportRowError(out.Err)
		return []*CSVImportError{&out}
	}

	return []*CSVImportError{{
		Row: rowNumber,
		Err: ensureCSVImportRowError(err),
	}}
}

func ensureCSVImportRowError(err error) error {
	if err == nil {
		return ErrInvalidCSVImportRow
	}
	if errors.Is(err, ErrInvalidCSVImportRow) {
		return err
	}
	return errors.Join(ErrInvalidCSVImportRow, err)
}

func csvImportError(row, column int, key, header string, err error) *CSVImportError {
	return &CSVImportError{
		Row:    row,
		Column: column,
		Key:    key,
		Header: header,
		Err:    err,
	}
}

func csvImportErrorReport(errs []*CSVImportError) error {
	filtered := make([]*CSVImportError, 0, len(errs))
	for _, err := range errs {
		if err != nil {
			filtered = append(filtered, err)
		}
	}
	if len(filtered) == 0 {
		return nil
	}
	return &CSVImportErrorReport{Errors: filtered}
}
