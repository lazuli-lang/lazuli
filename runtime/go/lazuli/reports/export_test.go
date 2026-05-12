package reports_test

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/reports"
)

func TestWriteCSVUsesColumnOrderAndHeaders(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{
		{Key: "name", Header: "Name"},
		{Key: "email", Header: "Email"},
		{Key: "visits", Header: "Visits"},
	}
	rows := []reports.Row{
		{"visits": 3, "name": "Ada", "email": "ada@example.test"},
		{"email": "linus@example.test", "name": "Linus"},
	}

	var buf strings.Builder
	if err := reports.WriteCSV(context.Background(), &buf, columns, rows); err != nil {
		t.Fatalf("WriteCSV() error = %v", err)
	}

	const want = "Name,Email,Visits\nAda,ada@example.test,3\nLinus,linus@example.test,\n"
	if got := buf.String(); got != want {
		t.Fatalf("csv = %q, want %q", got, want)
	}
}

func TestWriteCSVInjectionGuardIsOptIn(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "value"}}
	rows := []reports.Row{
		{"value": "=1+1"},
		{"value": "  @cmd"},
		{"value": "-10"},
		{"value": -10},
	}

	var unguarded strings.Builder
	if err := reports.WriteCSV(context.Background(), &unguarded, columns, rows); err != nil {
		t.Fatalf("WriteCSV() unguarded error = %v", err)
	}
	const wantUnguarded = "value\n=1+1\n\"  @cmd\"\n-10\n-10\n"
	if got := unguarded.String(); got != wantUnguarded {
		t.Fatalf("unguarded csv = %q, want %q", got, wantUnguarded)
	}

	var guarded strings.Builder
	if err := reports.WriteCSV(context.Background(), &guarded, columns, rows, reports.WithCSVInjectionGuard(true)); err != nil {
		t.Fatalf("WriteCSV() guarded error = %v", err)
	}
	const wantGuarded = "value\n'=1+1\n'  @cmd\n'-10\n-10\n"
	if got := guarded.String(); got != wantGuarded {
		t.Fatalf("guarded csv = %q, want %q", got, wantGuarded)
	}
}

func TestWriteJSONUsesColumnOrderAndNulls(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{
		{Key: "name", Header: "Name"},
		{Key: "id", Header: "ID"},
		{Key: "email", Header: "Email"},
	}
	rows := []reports.Row{
		{"id": 2, "email": "ada@example.test", "name": "Ada"},
		{"id": 3, "name": "Linus"},
	}

	var buf strings.Builder
	if err := reports.WriteJSON(context.Background(), &buf, columns, rows); err != nil {
		t.Fatalf("WriteJSON() error = %v", err)
	}

	const want = `[{"name":"Ada","id":2,"email":"ada@example.test"},{"name":"Linus","id":3,"email":null}]` + "\n"
	if got := buf.String(); got != want {
		t.Fatalf("json = %q, want %q", got, want)
	}
	if !json.Valid([]byte(buf.String())) {
		t.Fatalf("json is invalid: %q", buf.String())
	}
}

func TestStreamJSONUsesRowStream(t *testing.T) {
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

	var buf strings.Builder
	if err := reports.StreamJSON(context.Background(), &buf, columns, stream); err != nil {
		t.Fatalf("StreamJSON() error = %v", err)
	}

	const want = `[{"id":1,"name":"first"},{"id":2,"name":"second"}]` + "\n"
	if got := buf.String(); got != want {
		t.Fatalf("json = %q, want %q", got, want)
	}
}

func TestStreamCSVStopsOnContextCancellation(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "id"}}
	ctx, cancel := context.WithCancel(context.Background())
	calls := 0
	stream := func(ctx context.Context, yield func(reports.Row) error) error {
		calls++
		if err := yield(reports.Row{"id": 1}); err != nil {
			return err
		}
		cancel()
		calls++
		if err := yield(reports.Row{"id": 2}); err != nil {
			return err
		}
		return ctx.Err()
	}

	var buf strings.Builder
	err := reports.StreamCSV(ctx, &buf, columns, stream)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("StreamCSV() error = %v, want context.Canceled", err)
	}
	if calls != 2 {
		t.Fatalf("stream calls = %d, want 2", calls)
	}
	if got, want := buf.String(), "id\n1\n"; got != want {
		t.Fatalf("partial csv = %q, want %q", got, want)
	}
}

func TestValidateColumnsRejectsInvalidDefinitions(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		columns []reports.Column
		wantErr error
	}{
		{name: "empty", columns: nil, wantErr: reports.ErrNoColumns},
		{name: "blank key", columns: []reports.Column{{Key: " "}}, wantErr: reports.ErrInvalidColumn},
		{name: "padded key", columns: []reports.Column{{Key: " id "}}, wantErr: reports.ErrInvalidColumn},
		{name: "duplicate key", columns: []reports.Column{{Key: "id"}, {Key: "id"}}, wantErr: reports.ErrDuplicateColumn},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := reports.ValidateColumns(tc.columns)
			if !errors.Is(err, tc.wantErr) {
				t.Fatalf("ValidateColumns() error = %v, want %v", err, tc.wantErr)
			}
		})
	}
}

func TestStreamWritersRejectNilInputs(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "id"}}
	if err := reports.StreamCSV(context.Background(), nil, columns, func(context.Context, func(reports.Row) error) error { return nil }); !errors.Is(err, reports.ErrNilWriter) {
		t.Fatalf("StreamCSV() nil writer error = %v, want ErrNilWriter", err)
	}

	var buf strings.Builder
	if err := reports.StreamJSON(context.Background(), &buf, columns, nil); !errors.Is(err, reports.ErrNilRowStream) {
		t.Fatalf("StreamJSON() nil stream error = %v, want ErrNilRowStream", err)
	}
}
