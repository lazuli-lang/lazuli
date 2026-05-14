package report

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"time"

	"github.com/jackc/pgx/v5"

	"lazuli.dev/runtime/lazuli/storage"
)

// SourceFn opens a streaming cursor over the report's source rows.
// Generated code wires this to the resolved `report.source` query
// (list / sql).
type SourceFn func(ctx context.Context) (pgx.Rows, error)

// Run executes the report end-to-end: opens the source cursor, pipes
// rows through the format writer, uploads bytes to the bound
// ObjectStore, returns a signed URL (or the storage key when the
// report is public/private). Wire-thin: composes csv.go / xlsx.go and
// the storage runtime — no homegrown bytes work.
func Run(
	ctx context.Context,
	c Contract,
	format Format,
	source SourceFn,
	store storage.ObjectStore,
) (string, error) {
	headers := headersFor(c)
	rows, err := source(ctx)
	if err != nil {
		return "", fmt.Errorf("report %s: open source: %w", c.Name, err)
	}
	defer rows.Close()

	var buf bytes.Buffer
	if err := writeFormat(&buf, format, headers, c.Columns, rows); err != nil {
		return "", fmt.Errorf("report %s: write: %w", c.Name, err)
	}

	fc := storage.FileContract{API: c.Name, Visibility: c.Visibility, SignedTTL: c.SignedTTL}
	key, err := storage.Upload(ctx, fc, store, &buf, storage.Metadata{
		Filename:    resolveFilename(c, format),
		ContentType: contentType(format),
		Size:        int64(buf.Len()),
	})
	if err != nil {
		return "", fmt.Errorf("report %s: upload: %w", c.Name, err)
	}
	if c.Visibility != storage.VisibilitySigned {
		return string(key), nil
	}
	return storage.IssueSignedURL(ctx, fc, store, key)
}

func headersFor(c Contract) []string {
	out := make([]string, len(c.Columns))
	for i, col := range c.Columns {
		if col.Label != "" {
			out[i] = col.Label
		} else {
			out[i] = col.Name
		}
	}
	return out
}

func writeFormat(buf io.Writer, format Format, headers []string, cols []Column, rows pgx.Rows) error {
	switch format {
	case CSV:
		w, err := NewCSVWriter(buf, headers)
		if err != nil {
			return err
		}
		if err := stream(cols, rows, w.WriteRow); err != nil {
			_ = w.Close()
			return err
		}
		return w.Close()
	case XLSX:
		w, err := NewXLSXWriter(headers, XLSXOptions{})
		if err != nil {
			return err
		}
		if err := stream(cols, rows, w.WriteRow); err != nil {
			_ = w.Close()
			return err
		}
		if _, err := w.WriteTo(buf); err != nil {
			_ = w.Close()
			return err
		}
		return w.Close()
	default:
		return fmt.Errorf("unknown format %d", format)
	}
}

// stream walks pgx rows and projects `cols` by name (with positional
// fallback). `@fn` columns expect the projection to already include
// the computed value under the column name — codegen wires this.
func stream(cols []Column, rows pgx.Rows, write func([]string) error) error {
	fd := rows.FieldDescriptions()
	idx := make(map[string]int, len(fd))
	for i, f := range fd {
		idx[string(f.Name)] = i
	}
	for rows.Next() {
		vals, err := rows.Values()
		if err != nil {
			return err
		}
		out := make([]string, len(cols))
		for i, col := range cols {
			key := col.Name
			if col.From.IsRowField() {
				key = col.From.RowFieldName()
			}
			if j, ok := idx[key]; ok && j < len(vals) {
				out[i] = stringify(vals[j])
			} else if i < len(vals) {
				out[i] = stringify(vals[i])
			}
		}
		if err := write(out); err != nil {
			return err
		}
	}
	return rows.Err()
}

func stringify(v any) string {
	switch v := v.(type) {
	case nil:
		return ""
	case string:
		return v
	case []byte:
		return string(v)
	case time.Time:
		return v.Format(time.RFC3339)
	default:
		return fmt.Sprintf("%v", v)
	}
}

func resolveFilename(c Contract, format Format) string {
	if c.Filename.IsZero() {
		return fmt.Sprintf("%s_%d.%s", c.Name, time.Now().Unix(), format.Token())
	}
	// Minimal `{format}` substitution; `{ctx.now:...}` /
	// `{ctx.user.id}` / `{ctx.tenant.id}` are caller-resolved.
	literal := c.Filename.Literal()
	out := make([]byte, 0, len(literal))
	for i := 0; i < len(literal); {
		if i+8 <= len(literal) && literal[i:i+8] == "{format}" {
			out = append(out, format.Token()...)
			i += 8
			continue
		}
		out = append(out, literal[i])
		i++
	}
	return string(out)
}

func contentType(format Format) string {
	switch format {
	case CSV:
		return "text/csv"
	case XLSX:
		return "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
	default:
		return "application/octet-stream"
	}
}
