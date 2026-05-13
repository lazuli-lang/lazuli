package reports

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
)

var (
	// ErrMissingJSONImportDocument reports an empty JSON import.
	ErrMissingJSONImportDocument = errors.New("lazuli/reports: missing json import document")

	// ErrInvalidJSONImportDocument wraps JSON syntax and top-level shape failures.
	ErrInvalidJSONImportDocument = errors.New("lazuli/reports: invalid json import document")

	// ErrMissingRequiredJSONImportColumn reports a required import key absent from a JSON row.
	ErrMissingRequiredJSONImportColumn = errors.New("lazuli/reports: missing required json import column")

	// ErrUnknownJSONImportColumn reports an input JSON key not declared by the import schema.
	ErrUnknownJSONImportColumn = errors.New("lazuli/reports: unknown json import column")

	// ErrDuplicateJSONImportKey reports a repeated key in a JSON row object.
	ErrDuplicateJSONImportKey = errors.New("lazuli/reports: duplicate json import key")

	// ErrInvalidJSONImportRow wraps JSON row decode failures, row validator failures, and sink failures.
	ErrInvalidJSONImportRow = errors.New("lazuli/reports: invalid json import row")
)

// JSONImportUnknownColumnBehavior configures how row mapping handles undeclared
// JSON object keys.
type JSONImportUnknownColumnBehavior uint8

const (
	// RejectUnknownJSONImportColumns makes undeclared input keys validation errors.
	RejectUnknownJSONImportColumns JSONImportUnknownColumnBehavior = iota
	// IgnoreUnknownJSONImportColumns leaves undeclared input keys out of mapped rows.
	IgnoreUnknownJSONImportColumns
)

// JSONImportRowValidator validates one mapped JSON row.
//
// rowNumber is the 1-based JSON row number. For a top-level object, rowNumber is
// 1. For a top-level array, the first object element is row 1.
// Return *JSONImportError, or errors.Join of them, to attach key-specific
// details to the final report.
type JSONImportRowValidator func(ctx context.Context, rowNumber int, row Row) error

// JSONImportRowHandler receives one validated JSON import row.
type JSONImportRowHandler func(ctx context.Context, rowNumber int, row Row) error

// JSONImportOption configures JSON import validation helpers.
type JSONImportOption func(*jsonImportOptions)

type jsonImportOptions struct {
	unknownColumnBehavior JSONImportUnknownColumnBehavior
	validateRow           JSONImportRowValidator
}

// WithJSONImportUnknownColumns configures whether undeclared JSON object keys
// are rejected or ignored. The default is RejectUnknownJSONImportColumns.
func WithJSONImportUnknownColumns(behavior JSONImportUnknownColumnBehavior) JSONImportOption {
	return func(options *jsonImportOptions) {
		options.unknownColumnBehavior = behavior
	}
}

// WithJSONImportRowValidator runs validate for every mapped JSON row before it
// is returned or streamed to the row handler.
func WithJSONImportRowValidator(validate JSONImportRowValidator) JSONImportOption {
	return func(options *jsonImportOptions) {
		options.validateRow = validate
	}
}

// JSONImportRollbackPlan describes rows a caller may need to roll back after a
// failed streaming import. It only includes rows that passed validation and were
// accepted by the row handler before the failure report was returned.
type JSONImportRollbackPlan struct {
	AcceptedRows       int    `json:"accepted_rows"`
	AcceptedRowNumbers []int  `json:"accepted_row_numbers,omitempty"`
	FailedRowNumbers   []int  `json:"failed_row_numbers,omitempty"`
	Reason             string `json:"reason,omitempty"`
}

// JSONImportErrorReport reports one or more JSON import validation errors and
// rollback metadata for rows accepted before the failure.
type JSONImportErrorReport struct {
	Errors       []*JSONImportError     `json:"errors"`
	RollbackPlan JSONImportRollbackPlan `json:"rollback_plan"`
}

// Error returns a stable human-readable import report summary.
func (r *JSONImportErrorReport) Error() string {
	if r == nil || len(r.Errors) == 0 {
		return "<nil>"
	}
	if len(r.Errors) == 1 {
		return r.Errors[0].Error()
	}
	return fmt.Sprintf("lazuli/reports: json import validation failed (%d errors)", len(r.Errors))
}

// Unwrap exposes report entries for errors.Is and errors.As.
func (r *JSONImportErrorReport) Unwrap() []error {
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

// JSONImportError reports one JSON import error with optional row, key, and byte
// offset coordinates. Row is a 1-based JSON row number when set.
type JSONImportError struct {
	Row    int
	Key    string
	Offset int64
	Err    error
}

// Error returns a stable human-readable import error.
func (e *JSONImportError) Error() string {
	if e == nil {
		return "<nil>"
	}

	var parts []string
	if e.Row > 0 {
		parts = append(parts, fmt.Sprintf("row %d", e.Row))
	}
	if e.Key != "" {
		parts = append(parts, fmt.Sprintf("key %q", e.Key))
	}
	if e.Offset > 0 {
		parts = append(parts, fmt.Sprintf("offset %d", e.Offset))
	}
	if len(parts) == 0 {
		return fmt.Sprintf("lazuli/reports: json import: %v", e.Err)
	}
	return fmt.Sprintf("lazuli/reports: json import %s: %v", strings.Join(parts, " "), e.Err)
}

// Unwrap exposes the classified cause for errors.Is and errors.As.
func (e *JSONImportError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

// ReadImportJSON reads, maps, and validates JSON rows using import columns.
//
// The input may be a top-level object for one row or a top-level array of
// objects for many rows.
func ReadImportJSON(ctx context.Context, r io.Reader, columns []ImportColumn, opts ...JSONImportOption) ([]Row, error) {
	var rows []Row
	err := StreamImportJSON(ctx, r, columns, func(_ context.Context, _ int, row Row) error {
		rows = append(rows, row)
		return nil
	}, opts...)
	return rows, err
}

// StreamImportJSON streams mapped and validated JSON rows to handle.
//
// The input may be a top-level object for one row or a top-level array of
// objects for many rows. Rows with validation errors are reported and skipped;
// rows accepted before a later failure are included in the rollback plan.
func StreamImportJSON(ctx context.Context, r io.Reader, columns []ImportColumn, handle JSONImportRowHandler, opts ...JSONImportOption) error {
	if r == nil {
		return ErrNilReader
	}
	if handle == nil {
		return ErrNilRowStream
	}
	if err := ValidateImportColumns(columns); err != nil {
		return err
	}
	ctx = contextOrBackground(ctx)
	options := applyJSONImportOptions(opts)
	columnByKey := jsonImportColumnByKey(columns)

	decoder := json.NewDecoder(r)
	decoder.UseNumber()
	if err := ctx.Err(); err != nil {
		return err
	}

	token, err := decoder.Token()
	if errors.Is(err, io.EOF) {
		return jsonImportErrorReport([]*JSONImportError{{
			Err: ErrMissingJSONImportDocument,
		}}, nil, nil)
	}
	if err != nil {
		return jsonImportErrorReport([]*JSONImportError{jsonImportDecodeError(err, 0)}, nil, nil)
	}

	var acceptedRows []int
	var failedRows []int
	var errs []*JSONImportError
	rowNumber := 0
	accept := func(rowNumber int, row Row) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		if err := handle(ctx, rowNumber, row); err != nil {
			failedRows = append(failedRows, rowNumber)
			errs = append(errs, &JSONImportError{
				Row: rowNumber,
				Err: ensureJSONImportRowError(err),
			})
			return jsonImportErrorReport(errs, acceptedRows, failedRows)
		}
		acceptedRows = append(acceptedRows, rowNumber)
		return nil
	}

	switch delim := token.(type) {
	case json.Delim:
		switch delim {
		case '[':
			for decoder.More() {
				rowNumber++
				row, rowErrs, ok := readJSONImportRowObject(decoder, rowNumber, columns, columnByKey, options)
				if !ok {
					failedRows = append(failedRows, rowNumber)
					errs = append(errs, rowErrs...)
					return jsonImportErrorReport(errs, acceptedRows, failedRows)
				}
				if len(rowErrs) > 0 {
					failedRows = append(failedRows, rowNumber)
					errs = append(errs, rowErrs...)
					continue
				}
				if rowErrs := validateJSONImportRow(ctx, rowNumber, row, options); len(rowErrs) > 0 {
					failedRows = append(failedRows, rowNumber)
					errs = append(errs, rowErrs...)
					continue
				}
				if err := accept(rowNumber, row); err != nil {
					return err
				}
			}
			if _, err := decoder.Token(); err != nil {
				errs = append(errs, jsonImportDecodeError(err, rowNumber))
				return jsonImportErrorReport(errs, acceptedRows, failedRows)
			}
		case '{':
			rowNumber = 1
			row, rowErrs, ok := readJSONImportRowObjectAfterOpen(decoder, rowNumber, columns, columnByKey, options)
			if !ok {
				failedRows = append(failedRows, rowNumber)
				errs = append(errs, rowErrs...)
				return jsonImportErrorReport(errs, acceptedRows, failedRows)
			}
			if len(rowErrs) > 0 {
				failedRows = append(failedRows, rowNumber)
				errs = append(errs, rowErrs...)
			} else {
				if rowErrs := validateJSONImportRow(ctx, rowNumber, row, options); len(rowErrs) > 0 {
					failedRows = append(failedRows, rowNumber)
					errs = append(errs, rowErrs...)
				} else if err := accept(rowNumber, row); err != nil {
					return err
				}
			}
		default:
			errs = append(errs, &JSONImportError{
				Offset: decoder.InputOffset(),
				Err:    ErrInvalidJSONImportDocument,
			})
			return jsonImportErrorReport(errs, acceptedRows, failedRows)
		}
	default:
		errs = append(errs, &JSONImportError{
			Offset: decoder.InputOffset(),
			Err:    ErrInvalidJSONImportDocument,
		})
		return jsonImportErrorReport(errs, acceptedRows, failedRows)
	}

	if err := ctx.Err(); err != nil {
		return err
	}
	if decoder.More() {
		errs = append(errs, &JSONImportError{
			Offset: decoder.InputOffset(),
			Err:    ErrInvalidJSONImportDocument,
		})
		return jsonImportErrorReport(errs, acceptedRows, failedRows)
	}
	if token, err := decoder.Token(); err == nil {
		errs = append(errs, &JSONImportError{
			Offset: decoder.InputOffset(),
			Err:    fmt.Errorf("%w: unexpected trailing token %v", ErrInvalidJSONImportDocument, token),
		})
		return jsonImportErrorReport(errs, acceptedRows, failedRows)
	} else if !errors.Is(err, io.EOF) {
		errs = append(errs, jsonImportDecodeError(err, rowNumber))
		return jsonImportErrorReport(errs, acceptedRows, failedRows)
	}

	return jsonImportErrorReport(errs, acceptedRows, failedRows)
}

func applyJSONImportOptions(opts []JSONImportOption) jsonImportOptions {
	options := jsonImportOptions{
		unknownColumnBehavior: RejectUnknownJSONImportColumns,
	}
	for _, opt := range opts {
		if opt != nil {
			opt(&options)
		}
	}
	return options
}

func jsonImportColumnByKey(columns []ImportColumn) map[string]ImportColumn {
	byKey := make(map[string]ImportColumn, len(columns))
	for _, column := range columns {
		byKey[column.Key] = column
	}
	return byKey
}

func readJSONImportRowObject(
	decoder *json.Decoder,
	rowNumber int,
	columns []ImportColumn,
	columnByKey map[string]ImportColumn,
	options jsonImportOptions,
) (Row, []*JSONImportError, bool) {
	token, err := decoder.Token()
	if err != nil {
		return nil, []*JSONImportError{jsonImportDecodeError(err, rowNumber)}, false
	}
	delim, ok := token.(json.Delim)
	if !ok || delim != '{' {
		return nil, []*JSONImportError{{
			Row:    rowNumber,
			Offset: decoder.InputOffset(),
			Err:    ErrInvalidJSONImportRow,
		}}, false
	}
	return readJSONImportRowObjectAfterOpen(decoder, rowNumber, columns, columnByKey, options)
}

func readJSONImportRowObjectAfterOpen(
	decoder *json.Decoder,
	rowNumber int,
	columns []ImportColumn,
	columnByKey map[string]ImportColumn,
	options jsonImportOptions,
) (Row, []*JSONImportError, bool) {
	row := make(Row, len(columns))
	seen := make(map[string]struct{}, len(columns))
	var errs []*JSONImportError

	for decoder.More() {
		keyToken, err := decoder.Token()
		if err != nil {
			return nil, []*JSONImportError{jsonImportDecodeError(err, rowNumber)}, false
		}
		key, ok := keyToken.(string)
		if !ok {
			return nil, []*JSONImportError{{
				Row:    rowNumber,
				Offset: decoder.InputOffset(),
				Err:    ErrInvalidJSONImportRow,
			}}, false
		}

		var value any
		if err := decoder.Decode(&value); err != nil {
			return nil, []*JSONImportError{jsonImportDecodeError(err, rowNumber)}, false
		}

		if _, ok := seen[key]; ok {
			errs = append(errs, &JSONImportError{
				Row:    rowNumber,
				Key:    key,
				Offset: decoder.InputOffset(),
				Err:    ErrDuplicateJSONImportKey,
			})
			continue
		}
		seen[key] = struct{}{}

		column, ok := columnByKey[key]
		if !ok {
			if options.unknownColumnBehavior == RejectUnknownJSONImportColumns {
				errs = append(errs, &JSONImportError{
					Row:    rowNumber,
					Key:    key,
					Offset: decoder.InputOffset(),
					Err:    ErrUnknownJSONImportColumn,
				})
			}
			continue
		}
		row[column.Key] = value
	}

	if _, err := decoder.Token(); err != nil {
		return nil, []*JSONImportError{jsonImportDecodeError(err, rowNumber)}, false
	}

	for _, column := range columns {
		if !column.Required {
			continue
		}
		if _, ok := row[column.Key]; ok {
			continue
		}
		errs = append(errs, &JSONImportError{
			Row: rowNumber,
			Key: column.Key,
			Err: ErrMissingRequiredJSONImportColumn,
		})
	}

	return row, errs, true
}

func validateJSONImportRow(
	ctx context.Context,
	rowNumber int,
	row Row,
	options jsonImportOptions,
) []*JSONImportError {
	if options.validateRow != nil {
		if err := options.validateRow(ctx, rowNumber, row); err != nil {
			return jsonImportRowValidationErrors(rowNumber, err)
		}
	}
	return nil
}

func jsonImportDecodeError(err error, rowNumber int) *JSONImportError {
	importErr := &JSONImportError{
		Row: rowNumber,
		Err: errors.Join(ErrInvalidJSONImportDocument, err),
	}
	var syntaxErr *json.SyntaxError
	if errors.As(err, &syntaxErr) && syntaxErr.Offset > 0 {
		importErr.Offset = syntaxErr.Offset
	}
	var typeErr *json.UnmarshalTypeError
	if errors.As(err, &typeErr) && typeErr.Offset > 0 {
		importErr.Offset = typeErr.Offset
	}
	return importErr
}

func jsonImportRowValidationErrors(rowNumber int, err error) []*JSONImportError {
	if err == nil {
		return nil
	}
	if joined, ok := err.(interface{ Unwrap() []error }); ok {
		children := joined.Unwrap()
		errs := make([]*JSONImportError, 0, len(children))
		for _, child := range children {
			errs = append(errs, jsonImportRowValidationErrors(rowNumber, child)...)
		}
		return errs
	}

	var importErr *JSONImportError
	if errors.As(err, &importErr) {
		out := *importErr
		if out.Row == 0 {
			out.Row = rowNumber
		}
		out.Err = ensureJSONImportRowError(out.Err)
		return []*JSONImportError{&out}
	}

	return []*JSONImportError{{
		Row: rowNumber,
		Err: ensureJSONImportRowError(err),
	}}
}

func ensureJSONImportRowError(err error) error {
	if err == nil {
		return ErrInvalidJSONImportRow
	}
	if errors.Is(err, ErrInvalidJSONImportRow) {
		return err
	}
	return errors.Join(ErrInvalidJSONImportRow, err)
}

func jsonImportErrorReport(errs []*JSONImportError, acceptedRows, failedRows []int) error {
	filtered := make([]*JSONImportError, 0, len(errs))
	for _, err := range errs {
		if err != nil {
			filtered = append(filtered, err)
		}
	}
	if len(filtered) == 0 {
		return nil
	}
	plan := JSONImportRollbackPlan{
		AcceptedRows:       len(acceptedRows),
		AcceptedRowNumbers: append([]int(nil), acceptedRows...),
		FailedRowNumbers:   append([]int(nil), failedRows...),
		Reason:             filtered[0].Err.Error(),
	}
	return &JSONImportErrorReport{
		Errors:       filtered,
		RollbackPlan: plan,
	}
}
