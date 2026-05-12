package reports_test

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/reports"
)

func TestMapCSVImportHeadersReportsRequiredAndUnknownColumns(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{
		{Key: "name", Header: "Name", Required: true},
		{Key: "email", Header: "Email", Required: true},
		{Key: "notes"},
	}

	_, err := reports.MapCSVImportHeaders([]string{"Name", "Extra"}, columns)
	if !errors.Is(err, reports.ErrUnknownCSVImportColumn) {
		t.Fatalf("MapCSVImportHeaders() error = %v, want ErrUnknownCSVImportColumn", err)
	}
	if !errors.Is(err, reports.ErrMissingRequiredCSVImportColumn) {
		t.Fatalf("MapCSVImportHeaders() error = %v, want ErrMissingRequiredCSVImportColumn", err)
	}

	var report *reports.CSVImportErrorReport
	if !errors.As(err, &report) {
		t.Fatalf("MapCSVImportHeaders() error type = %T, want CSVImportErrorReport", err)
	}
	if len(report.Errors) != 2 {
		t.Fatalf("report errors = %d, want 2: %v", len(report.Errors), report.Errors)
	}

	unknown := report.Errors[0]
	if unknown.Row != 1 || unknown.Column != 2 || unknown.Header != "Extra" {
		t.Fatalf("unknown error = %+v, want row 1 column 2 header Extra", unknown)
	}
	missing := report.Errors[1]
	if missing.Row != 1 || missing.Column != 0 || missing.Key != "email" || missing.Header != "Email" {
		t.Fatalf("missing error = %+v, want row 1 key email header Email", missing)
	}
}

func TestMapCSVImportHeadersIgnoresUnknownColumnsAndMapsRows(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{
		{Key: "name", Header: "Name", Required: true},
		{Key: "email", Header: "Email"},
	}

	mapping, err := reports.MapCSVImportHeaders(
		[]string{" Name ", "Extra", "Email"},
		columns,
		reports.WithCSVImportUnknownColumns(reports.IgnoreUnknownCSVImportColumns),
	)
	if err != nil {
		t.Fatalf("MapCSVImportHeaders() error = %v", err)
	}

	if got, want := mapping.IndexByKey, map[string]int{"name": 0, "email": 2}; !reflect.DeepEqual(got, want) {
		t.Fatalf("IndexByKey = %#v, want %#v", got, want)
	}
	if got, want := mapping.UnknownHeaders, []reports.CSVImportUnknownHeader{{Index: 1, Header: "Extra"}}; !reflect.DeepEqual(got, want) {
		t.Fatalf("UnknownHeaders = %#v, want %#v", got, want)
	}

	row := mapping.Row([]string{"Ada", "ignored", "ada@example.test"})
	want := reports.Row{"name": "Ada", "email": "ada@example.test"}
	if !reflect.DeepEqual(row, want) {
		t.Fatalf("mapped row = %#v, want %#v", row, want)
	}
}

func TestReadImportCSVRunsRowValidatorWithColumnReport(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{
		{Key: "name", Header: "Name", Required: true},
		{Key: "email", Header: "Email", Required: true},
	}
	validate := func(_ context.Context, rowNumber int, row reports.Row) error {
		if rowNumber != 3 {
			return nil
		}
		if row["email"] != "" {
			t.Fatalf("row[email] = %q, want empty string", row["email"])
		}
		return &reports.CSVImportError{
			Key: "email",
			Err: errors.New("email is required"),
		}
	}

	rows, err := reports.ReadImportCSV(
		context.Background(),
		strings.NewReader("Name,Email\nAda,ada@example.test\nLinus,\n"),
		columns,
		reports.WithCSVImportRowValidator(validate),
	)
	if !errors.Is(err, reports.ErrInvalidCSVImportRow) {
		t.Fatalf("ReadImportCSV() error = %v, want ErrInvalidCSVImportRow", err)
	}
	if len(rows) != 2 {
		t.Fatalf("rows = %d, want 2", len(rows))
	}

	var report *reports.CSVImportErrorReport
	if !errors.As(err, &report) {
		t.Fatalf("ReadImportCSV() error type = %T, want CSVImportErrorReport", err)
	}
	if len(report.Errors) != 1 {
		t.Fatalf("report errors = %d, want 1: %v", len(report.Errors), report.Errors)
	}
	got := report.Errors[0]
	if got.Row != 3 || got.Column != 2 || got.Key != "email" || got.Header != "Email" {
		t.Fatalf("validator error = %+v, want row 3 column 2 key email header Email", got)
	}
}

func TestReadImportCSVReportsCSVReadCoordinates(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{{Key: "name", Header: "Name", Required: true}}

	_, err := reports.ReadImportCSV(
		context.Background(),
		strings.NewReader("Name\n\"unterminated\n"),
		columns,
	)
	if !errors.Is(err, reports.ErrInvalidCSVImportRow) {
		t.Fatalf("ReadImportCSV() error = %v, want ErrInvalidCSVImportRow", err)
	}

	var report *reports.CSVImportErrorReport
	if !errors.As(err, &report) {
		t.Fatalf("ReadImportCSV() error type = %T, want CSVImportErrorReport", err)
	}
	if len(report.Errors) != 1 {
		t.Fatalf("report errors = %d, want 1: %v", len(report.Errors), report.Errors)
	}
	if report.Errors[0].Row == 0 {
		t.Fatalf("parse error row = 0, want CSV row coordinate: %+v", report.Errors[0])
	}
}

func TestValidateImportColumnsRejectsInvalidDefinitions(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		columns []reports.ImportColumn
		wantErr error
	}{
		{name: "empty", columns: nil, wantErr: reports.ErrNoColumns},
		{name: "blank key", columns: []reports.ImportColumn{{Key: " "}}, wantErr: reports.ErrInvalidColumn},
		{name: "padded key", columns: []reports.ImportColumn{{Key: " id "}}, wantErr: reports.ErrInvalidColumn},
		{name: "duplicate key", columns: []reports.ImportColumn{{Key: "id"}, {Key: "id"}}, wantErr: reports.ErrDuplicateColumn},
		{name: "duplicate header", columns: []reports.ImportColumn{{Key: "id", Header: "ID"}, {Key: "name", Header: "ID"}}, wantErr: reports.ErrDuplicateColumn},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := reports.ValidateImportColumns(tc.columns)
			if !errors.Is(err, tc.wantErr) {
				t.Fatalf("ValidateImportColumns() error = %v, want %v", err, tc.wantErr)
			}
		})
	}
}
