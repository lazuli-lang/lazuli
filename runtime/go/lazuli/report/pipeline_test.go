package report

import (
	"context"
	"io"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli/storage"
)

// fakeRows is a minimal pgx.Rows test double.
type fakeRows struct {
	fields []pgconn.FieldDescription
	rows   [][]any
	pos    int
}

func (f *fakeRows) Close()                                       {}
func (f *fakeRows) Err() error                                   { return nil }
func (f *fakeRows) CommandTag() pgconn.CommandTag                { return pgconn.CommandTag{} }
func (f *fakeRows) FieldDescriptions() []pgconn.FieldDescription { return f.fields }
func (f *fakeRows) Next() bool {
	if f.pos >= len(f.rows) {
		return false
	}
	f.pos++
	return true
}
func (f *fakeRows) Values() ([]any, error) {
	return f.rows[f.pos-1], nil
}
func (f *fakeRows) Scan(dest ...any) error            { return nil }
func (f *fakeRows) RawValues() [][]byte               { return nil }
func (f *fakeRows) Conn() *pgx.Conn                   { return nil }

func newFakeRows(fields []string, rows [][]any) pgx.Rows {
	fd := make([]pgconn.FieldDescription, len(fields))
	for i, name := range fields {
		fd[i] = pgconn.FieldDescription{Name: name}
	}
	return &fakeRows{fields: fd, rows: rows}
}

func TestRunCSVRoundTrip(t *testing.T) {
	contract := Contract{
		Feature: "customer",
		Name:    "monthly_audit",
		Source:  "customer.query.list",
		Columns: []Column{
			{Name: "id", From: RowField("id")},
			{Name: "name", From: RowField("name"), Label: "Cliente"},
		},
		Formats:    []Format{CSV},
		Storage:    "files",
		Visibility: storage.VisibilitySigned,
		SignedTTL:  1 * time.Hour,
	}

	source := func(ctx context.Context) (pgx.Rows, error) {
		return newFakeRows(
			[]string{"id", "name"},
			[][]any{
				{"u-1", "Alice"},
				{"u-2", "Bob"},
			},
		), nil
	}

	store := storage.NewLocalStore("")
	url, err := Run(context.Background(), contract, CSV, source, store)
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	if url == "" {
		t.Fatalf("expected signed URL, got empty")
	}

	// Resolve the signed token to the stored Key and read bytes back.
	key, err := store.Resolve(url)
	if err != nil {
		t.Fatalf("resolve signed url: %v", err)
	}
	rc, err := store.Get(context.Background(), key)
	if err != nil {
		t.Fatalf("get bytes: %v", err)
	}
	defer rc.Close()
	body, err := io.ReadAll(rc)
	if err != nil {
		t.Fatalf("read bytes: %v", err)
	}

	// Header row uses labels (falls back to name when empty).
	text := string(body)
	if !strings.Contains(text, "id,Cliente") {
		t.Fatalf("expected CSV header `id,Cliente`, got: %q", text)
	}
	if !strings.Contains(text, "u-1,Alice") || !strings.Contains(text, "u-2,Bob") {
		t.Fatalf("expected rendered rows, got: %q", text)
	}
}

func TestFilenameSubstitutesFormat(t *testing.T) {
	contract := Contract{
		Name:     "monthly_audit",
		Filename: MustPattern("monthly_audit.{format}"),
	}
	got := resolveFilename(contract, XLSX)
	if got != "monthly_audit.xlsx" {
		t.Fatalf("expected `monthly_audit.xlsx`, got %q", got)
	}
}

func TestFormatTokenCatalog(t *testing.T) {
	if CSV.Token() != "csv" || XLSX.Token() != "xlsx" {
		t.Fatalf("token round-trip broken")
	}
	if _, ok := FormatFromToken("pdf"); ok {
		t.Fatalf("pdf must not be accepted (closed catalog)")
	}
}
