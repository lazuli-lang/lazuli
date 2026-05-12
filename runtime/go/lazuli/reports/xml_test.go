package reports_test

import (
	"context"
	"encoding/xml"
	"errors"
	"io"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/reports"
)

func TestWriteXMLUsesColumnOrderAndEscapesText(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{
		{Key: "name", Header: "Name"},
		{Key: "email", Header: "Email"},
		{Key: "visits", Header: "Visits"},
	}
	rows := []reports.Row{
		{"visits": 3, "name": "Ada & <Grace>", "email": "ada@example.test"},
		{"email": "linus@example.test", "name": "Linus"},
	}

	var buf strings.Builder
	if err := reports.WriteXML(context.Background(), &buf, columns, rows); err != nil {
		t.Fatalf("WriteXML() error = %v", err)
	}

	const want = `<rows><row><name>Ada &amp; &lt;Grace&gt;</name><email>ada@example.test</email><visits>3</visits></row><row><name>Linus</name><email>linus@example.test</email><visits></visits></row></rows>` + "\n"
	if got := buf.String(); got != want {
		t.Fatalf("xml = %q, want %q", got, want)
	}
	assertValidXML(t, buf.String())
}

func TestWriteXMLSupportsCustomRootAndRowNames(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "id"}}
	rows := []reports.Row{{"id": 7}}

	var buf strings.Builder
	err := reports.WriteXML(
		context.Background(),
		&buf,
		columns,
		rows,
		reports.WithXMLRootName("customers"),
		reports.WithXMLRowName("customer"),
	)
	if err != nil {
		t.Fatalf("WriteXML() error = %v", err)
	}

	const want = `<customers><customer><id>7</id></customer></customers>` + "\n"
	if got := buf.String(); got != want {
		t.Fatalf("xml = %q, want %q", got, want)
	}
}

func TestStreamXMLUsesRowStream(t *testing.T) {
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
	if err := reports.StreamXML(context.Background(), &buf, columns, stream); err != nil {
		t.Fatalf("StreamXML() error = %v", err)
	}

	const want = `<rows><row><id>1</id><name>first</name></row><row><id>2</id><name>second</name></row></rows>` + "\n"
	if got := buf.String(); got != want {
		t.Fatalf("xml = %q, want %q", got, want)
	}
}

func TestWriteXMLRejectsInvalidElementNames(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		columns []reports.Column
		opts    []reports.XMLOption
	}{
		{
			name:    "root starts with digit",
			columns: []reports.Column{{Key: "id"}},
			opts:    []reports.XMLOption{reports.WithXMLRootName("1rows")},
		},
		{
			name:    "row contains space",
			columns: []reports.Column{{Key: "id"}},
			opts:    []reports.XMLOption{reports.WithXMLRowName("line item")},
		},
		{
			name:    "column contains space",
			columns: []reports.Column{{Key: "line item"}},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			var buf strings.Builder
			err := reports.WriteXML(context.Background(), &buf, tc.columns, []reports.Row{{"id": 1}}, tc.opts...)
			if !errors.Is(err, reports.ErrInvalidXMLName) {
				t.Fatalf("WriteXML() error = %v, want ErrInvalidXMLName", err)
			}
			if got := buf.String(); got != "" {
				t.Fatalf("WriteXML() wrote %q before validation failed", got)
			}
		})
	}
}

func TestStreamXMLStopsOnContextCancellation(t *testing.T) {
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
	err := reports.StreamXML(ctx, &buf, columns, stream)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("StreamXML() error = %v, want context.Canceled", err)
	}
	if calls != 2 {
		t.Fatalf("stream calls = %d, want 2", calls)
	}
	if got, want := buf.String(), `<rows><row><id>1</id></row>`; got != want {
		t.Fatalf("partial xml = %q, want %q", got, want)
	}
}

func TestStreamXMLRejectsNilInputs(t *testing.T) {
	t.Parallel()

	columns := []reports.Column{{Key: "id"}}
	if err := reports.StreamXML(context.Background(), nil, columns, func(context.Context, func(reports.Row) error) error { return nil }); !errors.Is(err, reports.ErrNilWriter) {
		t.Fatalf("StreamXML() nil writer error = %v, want ErrNilWriter", err)
	}

	var buf strings.Builder
	if err := reports.StreamXML(context.Background(), &buf, columns, nil); !errors.Is(err, reports.ErrNilRowStream) {
		t.Fatalf("StreamXML() nil stream error = %v, want ErrNilRowStream", err)
	}
}

func assertValidXML(t *testing.T, data string) {
	t.Helper()

	decoder := xml.NewDecoder(strings.NewReader(data))
	for {
		_, err := decoder.Token()
		if errors.Is(err, io.EOF) {
			return
		}
		if err != nil {
			t.Fatalf("xml decode error = %v; xml = %q", err, data)
		}
	}
}
