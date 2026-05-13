package reports_test

import (
	"archive/zip"
	"bytes"
	"context"
	"errors"
	"io"
	"reflect"
	"sort"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/reports"
)

func TestWriteXLSXUsesInlineStringsByDefault(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{
		{Key: "name", Header: "Name"},
		{Key: "email", Header: "Email"},
		{Key: "visits", Header: "Visits"},
		{Key: "active", Header: "Active"},
	}
	rows := []reports.Row{
		{"visits": 3, "name": "Ada & <Grace>", "email": "ada@example.test", "active": true},
		{"email": "linus@example.test", "name": "Linus", "active": false},
	}

	var buf bytes.Buffer
	if err := reports.WriteXLSX(context.Background(), &buf, columns, rows); err != nil {
		t.Fatalf("WriteXLSX() error = %v", err)
	}

	entries := readXLSXEntries(t, buf.Bytes())
	assertXLSXEntryNames(t, entries, []string{
		"[Content_Types].xml",
		"_rels/.rels",
		"xl/_rels/workbook.xml.rels",
		"xl/workbook.xml",
		"xl/worksheets/sheet1.xml",
	})

	const wantSheet = `<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Email</t></is></c><c r="C1" t="inlineStr"><is><t>Visits</t></is></c><c r="D1" t="inlineStr"><is><t>Active</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Ada &amp; &lt;Grace&gt;</t></is></c><c r="B2" t="inlineStr"><is><t>ada@example.test</t></is></c><c r="C2" t="n"><v>3</v></c><c r="D2" t="b"><v>1</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Linus</t></is></c><c r="B3" t="inlineStr"><is><t>linus@example.test</t></is></c><c r="C3"/><c r="D3" t="b"><v>0</v></c></row></sheetData></worksheet>`
	if got := entries["xl/worksheets/sheet1.xml"]; got != wantSheet {
		t.Fatalf("sheet xml = %q, want %q", got, wantSheet)
	}
	if _, ok := entries["xl/sharedStrings.xml"]; ok {
		t.Fatalf("shared strings entry present for inline-string workbook")
	}
}

func TestWriteXLSXCanUseSharedStrings(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "name", Header: "Name"}, {Key: "role", Header: "Role"}}
	rows := []reports.Row{
		{"name": "Ada", "role": "engineer"},
		{"name": "Ada", "role": "engineer"},
	}

	var buf bytes.Buffer
	if err := reports.WriteXLSX(context.Background(), &buf, columns, rows, reports.WithXLSXSharedStrings(true)); err != nil {
		t.Fatalf("WriteXLSX() error = %v", err)
	}

	entries := readXLSXEntries(t, buf.Bytes())
	const wantSheet = `<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2" t="s"><v>3</v></c></row><row r="3"><c r="A3" t="s"><v>2</v></c><c r="B3" t="s"><v>3</v></c></row></sheetData></worksheet>`
	if got := entries["xl/worksheets/sheet1.xml"]; got != wantSheet {
		t.Fatalf("sheet xml = %q, want %q", got, wantSheet)
	}

	const wantShared = `<?xml version="1.0" encoding="UTF-8"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="6" uniqueCount="4"><si><t>Name</t></si><si><t>Role</t></si><si><t>Ada</t></si><si><t>engineer</t></si></sst>`
	if got := entries["xl/sharedStrings.xml"]; got != wantShared {
		t.Fatalf("shared strings xml = %q, want %q", got, wantShared)
	}
	if !strings.Contains(entries["[Content_Types].xml"], `/xl/sharedStrings.xml`) {
		t.Fatalf("content types missing shared strings override: %q", entries["[Content_Types].xml"])
	}
}

func TestWriteXLSXInjectionGuardIsOptIn(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "value"}}
	rows := []reports.Row{
		{"value": "=1+1"},
		{"value": "  @cmd"},
		{"value": "-10"},
		{"value": -10},
	}

	var unguarded bytes.Buffer
	if err := reports.WriteXLSX(context.Background(), &unguarded, columns, rows); err != nil {
		t.Fatalf("WriteXLSX() unguarded error = %v", err)
	}
	unguardedSheet := readXLSXEntries(t, unguarded.Bytes())["xl/worksheets/sheet1.xml"]
	if !strings.Contains(unguardedSheet, `<t>=1+1</t>`) || !strings.Contains(unguardedSheet, `<t>-10</t>`) || !strings.Contains(unguardedSheet, `<v>-10</v>`) {
		t.Fatalf("unguarded sheet did not preserve text/numeric values: %q", unguardedSheet)
	}

	var guarded bytes.Buffer
	if err := reports.WriteXLSX(context.Background(), &guarded, columns, rows, reports.WithXLSXInjectionGuard(true)); err != nil {
		t.Fatalf("WriteXLSX() guarded error = %v", err)
	}
	guardedSheet := readXLSXEntries(t, guarded.Bytes())["xl/worksheets/sheet1.xml"]
	if !strings.Contains(guardedSheet, `<t>&#39;=1+1</t>`) || !strings.Contains(guardedSheet, `<t>&#39;  @cmd</t>`) || !strings.Contains(guardedSheet, `<t>&#39;-10</t>`) || !strings.Contains(guardedSheet, `<v>-10</v>`) {
		t.Fatalf("guarded sheet did not guard only text values: %q", guardedSheet)
	}
}

func TestStreamXLSXUsesRowStream(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "id"}, {Key: "name"}}
	stream := func(ctx context.Context, yield func(reports.Row) error) error {
		if err := yield(reports.Row{"name": "first", "id": 1}); err != nil {
			return err
		}
		if err := yield(reports.Row{"name": "second", "id": 2}); err != nil {
			return err
		}
		return ctx.Err()
	}

	var buf bytes.Buffer
	if err := reports.StreamXLSX(context.Background(), &buf, columns, stream); err != nil {
		t.Fatalf("StreamXLSX() error = %v", err)
	}

	sheet := readXLSXEntries(t, buf.Bytes())["xl/worksheets/sheet1.xml"]
	if !strings.Contains(sheet, `<c r="A2" t="n"><v>1</v></c><c r="B2" t="inlineStr"><is><t>first</t></is></c>`) {
		t.Fatalf("sheet missing first streamed row: %q", sheet)
	}
	if !strings.Contains(sheet, `<c r="A3" t="n"><v>2</v></c><c r="B3" t="inlineStr"><is><t>second</t></is></c>`) {
		t.Fatalf("sheet missing second streamed row: %q", sheet)
	}
}

func TestStreamXLSXStopsOnContextCancellation(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "id"}}
	ctx, cancel := context.WithCancel(context.Background())
	stream := func(ctx context.Context, yield func(reports.Row) error) error {
		if err := yield(reports.Row{"id": 1}); err != nil {
			return err
		}
		cancel()
		if err := yield(reports.Row{"id": 2}); err != nil {
			return err
		}
		return ctx.Err()
	}

	var buf bytes.Buffer
	err := reports.StreamXLSX(ctx, &buf, columns, stream)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("StreamXLSX() error = %v, want context.Canceled", err)
	}
	if got := buf.Len(); got != 0 {
		t.Fatalf("StreamXLSX() wrote %d bytes before cancellation", got)
	}
}

func TestStreamXLSXRejectsNilInputs(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "id"}}
	if err := reports.StreamXLSX(context.Background(), nil, columns, func(context.Context, func(reports.Row) error) error { return nil }); !errors.Is(err, reports.ErrNilWriter) {
		t.Fatalf("StreamXLSX() nil writer error = %v, want ErrNilWriter", err)
	}

	var buf bytes.Buffer
	if err := reports.StreamXLSX(context.Background(), &buf, columns, nil); !errors.Is(err, reports.ErrNilRowStream) {
		t.Fatalf("StreamXLSX() nil stream error = %v, want ErrNilRowStream", err)
	}
}

func readXLSXEntries(t *testing.T, data []byte) map[string]string {
	t.Helper()

	reader, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		t.Fatalf("open xlsx zip: %v", err)
	}
	entries := make(map[string]string, len(reader.File))
	for _, file := range reader.File {
		rc, err := file.Open()
		if err != nil {
			t.Fatalf("open xlsx entry %s: %v", file.Name, err)
		}
		body, err := io.ReadAll(rc)
		if closeErr := rc.Close(); closeErr != nil && err == nil {
			err = closeErr
		}
		if err != nil {
			t.Fatalf("read xlsx entry %s: %v", file.Name, err)
		}
		entries[file.Name] = string(body)
	}
	return entries
}

func assertXLSXEntryNames(t *testing.T, entries map[string]string, want []string) {
	t.Helper()

	got := make([]string, 0, len(entries))
	for name := range entries {
		got = append(got, name)
	}
	sort.Strings(got)
	sort.Strings(want)
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("xlsx entries = %v, want %v", got, want)
	}
}
