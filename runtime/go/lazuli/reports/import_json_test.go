package reports_test

import (
	"context"
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/reports"
)

func TestReadImportJSONMapsArrayRowsAndIgnoresUnknownColumns(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{
		{Key: "name", Required: true},
		{Key: "email"},
		{Key: "score"},
	}

	rows, err := reports.ReadImportJSON(
		context.Background(),
		strings.NewReader(`[
			{"name":"Ada","email":"ada@example.test","extra":"ignored","score":7},
			{"score":8,"name":"Linus","email":"linus@example.test"}
		]`),
		columns,
		reports.WithJSONImportUnknownColumns(reports.IgnoreUnknownJSONImportColumns),
	)
	if err != nil {
		t.Fatalf("ReadImportJSON() error = %v", err)
	}

	want := []reports.Row{
		{"name": "Ada", "email": "ada@example.test", "score": json.Number("7")},
		{"name": "Linus", "email": "linus@example.test", "score": json.Number("8")},
	}
	if !reflect.DeepEqual(rows, want) {
		t.Fatalf("rows = %#v, want %#v", rows, want)
	}
}

func TestReadImportJSONMapsSingleObjectRow(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{{Key: "id", Required: true}, {Key: "name"}}

	rows, err := reports.ReadImportJSON(
		context.Background(),
		strings.NewReader(`{"id":1,"name":"Ada"}`),
		columns,
	)
	if err != nil {
		t.Fatalf("ReadImportJSON() error = %v", err)
	}

	want := []reports.Row{{"id": json.Number("1"), "name": "Ada"}}
	if !reflect.DeepEqual(rows, want) {
		t.Fatalf("rows = %#v, want %#v", rows, want)
	}
}

func TestReadImportJSONReportsRequiredUnknownAndDuplicateKeys(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{
		{Key: "name", Required: true},
		{Key: "email", Required: true},
	}

	rows, err := reports.ReadImportJSON(
		context.Background(),
		strings.NewReader(`[{"name":"Ada","extra":true,"name":"Lovelace"}]`),
		columns,
	)
	if !errors.Is(err, reports.ErrUnknownJSONImportColumn) {
		t.Fatalf("ReadImportJSON() error = %v, want ErrUnknownJSONImportColumn", err)
	}
	if !errors.Is(err, reports.ErrDuplicateJSONImportKey) {
		t.Fatalf("ReadImportJSON() error = %v, want ErrDuplicateJSONImportKey", err)
	}
	if !errors.Is(err, reports.ErrMissingRequiredJSONImportColumn) {
		t.Fatalf("ReadImportJSON() error = %v, want ErrMissingRequiredJSONImportColumn", err)
	}
	if len(rows) != 0 {
		t.Fatalf("rows = %d, want 0", len(rows))
	}

	var report *reports.JSONImportErrorReport
	if !errors.As(err, &report) {
		t.Fatalf("ReadImportJSON() error type = %T, want JSONImportErrorReport", err)
	}
	if len(report.Errors) != 3 {
		t.Fatalf("report errors = %d, want 3: %v", len(report.Errors), report.Errors)
	}
	if got := report.Errors[0]; got.Row != 1 || got.Key != "extra" {
		t.Fatalf("unknown error = %+v, want row 1 key extra", got)
	}
	if got := report.Errors[1]; got.Row != 1 || got.Key != "name" {
		t.Fatalf("duplicate error = %+v, want row 1 key name", got)
	}
	if got := report.Errors[2]; got.Row != 1 || got.Key != "email" {
		t.Fatalf("missing error = %+v, want row 1 key email", got)
	}
	if report.RollbackPlan.AcceptedRows != 0 || !reflect.DeepEqual(report.RollbackPlan.FailedRowNumbers, []int{1}) {
		t.Fatalf("rollback plan = %+v, want failed row 1 and no accepted rows", report.RollbackPlan)
	}
}

func TestStreamImportJSONReportsValidatorFailureWithRollbackPlan(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{{Key: "name", Required: true}, {Key: "email", Required: true}}
	var streamed []reports.Row
	validate := func(_ context.Context, rowNumber int, row reports.Row) error {
		if rowNumber != 2 {
			return nil
		}
		if row["email"] != "" {
			t.Fatalf("row[email] = %q, want empty string", row["email"])
		}
		return &reports.JSONImportError{
			Key: "email",
			Err: errors.New("email is required"),
		}
	}

	err := reports.StreamImportJSON(
		context.Background(),
		strings.NewReader(`[{"name":"Ada","email":"ada@example.test"},{"name":"Linus","email":""},{"name":"Grace","email":"grace@example.test"}]`),
		columns,
		func(_ context.Context, _ int, row reports.Row) error {
			streamed = append(streamed, row)
			return nil
		},
		reports.WithJSONImportRowValidator(validate),
	)
	if !errors.Is(err, reports.ErrInvalidJSONImportRow) {
		t.Fatalf("StreamImportJSON() error = %v, want ErrInvalidJSONImportRow", err)
	}
	if len(streamed) != 2 {
		t.Fatalf("streamed rows = %d, want 2", len(streamed))
	}

	var report *reports.JSONImportErrorReport
	if !errors.As(err, &report) {
		t.Fatalf("StreamImportJSON() error type = %T, want JSONImportErrorReport", err)
	}
	if len(report.Errors) != 1 {
		t.Fatalf("report errors = %d, want 1: %v", len(report.Errors), report.Errors)
	}
	if got := report.Errors[0]; got.Row != 2 || got.Key != "email" {
		t.Fatalf("validator error = %+v, want row 2 key email", got)
	}
	wantPlan := reports.JSONImportRollbackPlan{
		AcceptedRows:       2,
		AcceptedRowNumbers: []int{1, 3},
		FailedRowNumbers:   []int{2},
		Reason:             "lazuli/reports: invalid json import row\nemail is required",
	}
	if !reflect.DeepEqual(report.RollbackPlan, wantPlan) {
		t.Fatalf("rollback plan = %+v, want %+v", report.RollbackPlan, wantPlan)
	}
}

func TestStreamImportJSONReportsSinkFailureWithRollbackPlan(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{{Key: "name", Required: true}}
	errSink := errors.New("insert failed")

	err := reports.StreamImportJSON(
		context.Background(),
		strings.NewReader(`[{"name":"Ada"},{"name":"Linus"}]`),
		columns,
		func(_ context.Context, rowNumber int, _ reports.Row) error {
			if rowNumber == 2 {
				return errSink
			}
			return nil
		},
	)
	if !errors.Is(err, reports.ErrInvalidJSONImportRow) {
		t.Fatalf("StreamImportJSON() error = %v, want ErrInvalidJSONImportRow", err)
	}
	if !errors.Is(err, errSink) {
		t.Fatalf("StreamImportJSON() error = %v, want sink error", err)
	}

	var report *reports.JSONImportErrorReport
	if !errors.As(err, &report) {
		t.Fatalf("StreamImportJSON() error type = %T, want JSONImportErrorReport", err)
	}
	wantPlan := reports.JSONImportRollbackPlan{
		AcceptedRows:       1,
		AcceptedRowNumbers: []int{1},
		FailedRowNumbers:   []int{2},
		Reason:             "lazuli/reports: invalid json import row\ninsert failed",
	}
	if !reflect.DeepEqual(report.RollbackPlan, wantPlan) {
		t.Fatalf("rollback plan = %+v, want %+v", report.RollbackPlan, wantPlan)
	}
}

func TestReadImportJSONReportsDocumentErrors(t *testing.T) {
	t.Parallel()

	columns := []reports.ImportColumn{{Key: "name", Required: true}}

	_, err := reports.ReadImportJSON(context.Background(), strings.NewReader(""), columns)
	if !errors.Is(err, reports.ErrMissingJSONImportDocument) {
		t.Fatalf("ReadImportJSON(empty) error = %v, want ErrMissingJSONImportDocument", err)
	}

	_, err = reports.ReadImportJSON(context.Background(), strings.NewReader(`[{"name":"Ada"}`), columns)
	if !errors.Is(err, reports.ErrInvalidJSONImportDocument) {
		t.Fatalf("ReadImportJSON(invalid) error = %v, want ErrInvalidJSONImportDocument", err)
	}
}
