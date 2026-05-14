// Package report writes tabular exports (CSV, XLSX) from streamed rows.
// Pair with future `query.export csv,xlsx` surface.
package report

import (
	"encoding/csv"
	"io"
)

// CSVWriter writes header + rows in RFC 4180 form (UTF-8, CRLF).
type CSVWriter struct {
	w *csv.Writer
}

// NewCSVWriter wraps w and writes the header row immediately.
func NewCSVWriter(w io.Writer, headers []string) (*CSVWriter, error) {
	cw := csv.NewWriter(w)
	cw.UseCRLF = true
	if err := cw.Write(headers); err != nil {
		return nil, err
	}
	return &CSVWriter{w: cw}, nil
}

// WriteRow appends one row. Field count is not validated against headers.
func (c *CSVWriter) WriteRow(row []string) error {
	return c.w.Write(row)
}

// Close flushes and returns any deferred error.
func (c *CSVWriter) Close() error {
	c.w.Flush()
	return c.w.Error()
}
