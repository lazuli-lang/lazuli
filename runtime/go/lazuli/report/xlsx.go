package report

import (
	"io"

	"github.com/xuri/excelize/v2"
)

type XLSXOptions struct {
	SheetName    string
	ColumnWidths []float64 // optional, len must equal len(headers) if set
}

type XLSXWriter struct {
	f         *excelize.File
	sheetName string
	rowIdx    int // next row to write; row 1 holds headers
}

func NewXLSXWriter(headers []string, opts XLSXOptions) (*XLSXWriter, error) {
	sheet := opts.SheetName
	if sheet == "" {
		sheet = "Sheet1"
	}
	if len(opts.ColumnWidths) > 0 && len(opts.ColumnWidths) != len(headers) {
		return nil, excelize.ErrColumnNumber
	}

	f := excelize.NewFile()
	if sheet != "Sheet1" {
		if _, err := f.NewSheet(sheet); err != nil {
			return nil, err
		}
		if err := f.DeleteSheet("Sheet1"); err != nil {
			return nil, err
		}
	}

	cells := make([]any, len(headers))
	for i, h := range headers {
		cells[i] = h
	}
	if err := f.SetSheetRow(sheet, "A1", &cells); err != nil {
		return nil, err
	}

	for i, w := range opts.ColumnWidths {
		col, err := excelize.ColumnNumberToName(i + 1)
		if err != nil {
			return nil, err
		}
		if err := f.SetColWidth(sheet, col, col, w); err != nil {
			return nil, err
		}
	}

	return &XLSXWriter{f: f, sheetName: sheet, rowIdx: 2}, nil
}

func (x *XLSXWriter) WriteRow(row []string) error {
	cells := make([]any, len(row))
	for i, v := range row {
		cells[i] = v
	}
	cellRef, err := excelize.CoordinatesToCellName(1, x.rowIdx)
	if err != nil {
		return err
	}
	if err := x.f.SetSheetRow(x.sheetName, cellRef, &cells); err != nil {
		return err
	}
	x.rowIdx++
	return nil
}

func (x *XLSXWriter) WriteTo(w io.Writer) (int64, error) {
	return x.f.WriteTo(w)
}

func (x *XLSXWriter) Close() error {
	return x.f.Close()
}
