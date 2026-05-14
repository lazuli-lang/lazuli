package report_test

import (
	"bytes"
	"encoding/csv"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/report"

	"github.com/xuri/excelize/v2"
)

func TestCSVWriter_RoundTrip(t *testing.T) {
	var buf bytes.Buffer
	headers, rows := fixtureRows()

	w, err := report.NewCSVWriter(&buf, headers)
	requireNoError(t, err)
	for _, row := range rows {
		requireNoError(t, w.WriteRow(row))
	}
	requireNoError(t, w.Close())

	got, err := csv.NewReader(bytes.NewReader(buf.Bytes())).ReadAll()
	requireNoError(t, err)
	requireEqual(t, got, append([][]string{headers}, rows...))
}

func TestCSVWriter_EmptyRows(t *testing.T) {
	var buf bytes.Buffer

	w, err := report.NewCSVWriter(&buf, []string{"id", "name"})
	requireNoError(t, err)
	requireNoError(t, w.Close())

	got, err := csv.NewReader(bytes.NewReader(buf.Bytes())).ReadAll()
	requireNoError(t, err)
	if len(got) != 1 {
		t.Fatalf("record count = %d, want 1", len(got))
	}
}

func TestXLSXWriter_RoundTrip(t *testing.T) {
	headers, rows := fixtureRows()
	buf := writeXLSX(t, headers, rows, report.XLSXOptions{})
	f, err := excelize.OpenReader(bytes.NewReader(buf.Bytes()))
	requireNoError(t, err)
	defer func() { requireNoError(t, f.Close()) }()

	got, err := f.GetRows("Sheet1")
	requireNoError(t, err)
	requireEqual(t, got, append([][]string{headers}, rows...))
}

func TestXLSXWriter_CustomSheet(t *testing.T) {
	buf := writeXLSX(t, []string{"id"}, nil, report.XLSXOptions{SheetName: "Report"})
	f, err := excelize.OpenReader(bytes.NewReader(buf.Bytes()))
	requireNoError(t, err)
	defer func() { requireNoError(t, f.Close()) }()

	requireEqual(t, f.GetSheetList(), []string{"Report"})
}

func TestXLSXWriter_ColumnWidths(t *testing.T) {
	buf := writeXLSX(t, []string{"id", "name"}, nil, report.XLSXOptions{
		ColumnWidths: []float64{12, 24},
	})
	f, err := excelize.OpenReader(bytes.NewReader(buf.Bytes()))
	requireNoError(t, err)
	defer func() { requireNoError(t, f.Close()) }()
}

func writeXLSX(t *testing.T, headers []string, rows [][]string, opts report.XLSXOptions) *bytes.Buffer {
	t.Helper()

	var buf bytes.Buffer
	w, err := report.NewXLSXWriter(headers, opts)
	requireNoError(t, err)
	defer func() { requireNoError(t, w.Close()) }()
	for _, row := range rows {
		requireNoError(t, w.WriteRow(row))
	}
	_, err = w.WriteTo(&buf)
	requireNoError(t, err)
	return &buf
}

func fixtureRows() ([]string, [][]string) {
	return []string{"id", "name", "amount"}, [][]string{
		{"1", "Ada", "10.50"},
		{"2", "Grace", "20.00"},
		{"3", "Linus", "30.25"},
	}
}

func requireNoError(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func requireEqual(t *testing.T, got, want any) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, want %#v", got, want)
	}
}
