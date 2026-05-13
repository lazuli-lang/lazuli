package reports

import (
	"archive/zip"
	"bytes"
	"context"
	"encoding/xml"
	"fmt"
	"io"
	"math"
	"reflect"
	"strconv"
	"time"
)

const (
	xlsxContentTypesPath      = "[Content_Types].xml"
	xlsxRootRelsPath          = "_rels/.rels"
	xlsxWorkbookPath          = "xl/workbook.xml"
	xlsxWorkbookRelsPath      = "xl/_rels/workbook.xml.rels"
	xlsxWorksheetPath         = "xl/worksheets/sheet1.xml"
	xlsxSharedStringsPath     = "xl/sharedStrings.xml"
	xlsxRelationshipNamespace = "http://schemas.openxmlformats.org/package/2006/relationships"
	xlsxOfficeRelationshipNS  = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
	xlsxSpreadsheetNamespace  = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
)

var xlsxZipModified = time.Date(1980, 1, 1, 0, 0, 0, 0, time.UTC)

// XLSXOption configures XLSX exports.
type XLSXOption func(*xlsxOptions)

type xlsxOptions struct {
	guardInjection bool
	sharedStrings  bool
}

// WithXLSXInjectionGuard prefixes dangerous string cells with a single quote.
//
// The guard is intentionally opt-in because it changes cell text. It applies to
// text-like values whose first non-space character could be interpreted as a
// spreadsheet formula trigger.
func WithXLSXInjectionGuard(enabled bool) XLSXOption {
	return func(options *xlsxOptions) {
		options.guardInjection = enabled
	}
}

// WithXLSXSharedStrings writes string cells through xl/sharedStrings.xml.
//
// Inline strings are used by default because they can be streamed without
// retaining all text values in memory.
func WithXLSXSharedStrings(enabled bool) XLSXOption {
	return func(options *xlsxOptions) {
		options.sharedStrings = enabled
	}
}

// WriteXLSX writes rows as a minimal XLSX workbook using columns for header
// labels and row ordering.
func WriteXLSX(ctx context.Context, w io.Writer, columns []Column, rows []Row, opts ...XLSXOption) error {
	return StreamXLSX(ctx, w, columns, sliceRowStream(rows), opts...)
}

// StreamXLSX streams rows as a minimal XLSX workbook using columns for header
// labels and row ordering.
func StreamXLSX(ctx context.Context, w io.Writer, columns []Column, stream RowStream, opts ...XLSXOption) error {
	if w == nil {
		return ErrNilWriter
	}
	if stream == nil {
		return ErrNilRowStream
	}
	if err := ValidateColumns(columns); err != nil {
		return err
	}
	options := applyXLSXOptions(opts)
	ctx = contextOrBackground(ctx)

	if err := ctx.Err(); err != nil {
		return err
	}

	var sheet bytes.Buffer
	shared := newXLSXSharedStrings()
	if err := writeXLSXSheet(ctx, &sheet, columns, stream, options, shared); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	zw := zip.NewWriter(w)
	if err := writeXLSXStaticParts(zw, options); err != nil {
		_ = zw.Close()
		return err
	}
	if err := writeXLSXZipEntry(zw, xlsxWorksheetPath, sheet.Bytes()); err != nil {
		_ = zw.Close()
		return err
	}
	if options.sharedStrings {
		if err := writeXLSXZipEntry(zw, xlsxSharedStringsPath, shared.XML()); err != nil {
			_ = zw.Close()
			return err
		}
	}
	if err := zw.Close(); err != nil {
		return fmt.Errorf("lazuli/reports: close xlsx: %w", err)
	}
	return nil
}

func applyXLSXOptions(opts []XLSXOption) xlsxOptions {
	options := xlsxOptions{}
	for _, opt := range opts {
		if opt != nil {
			opt(&options)
		}
	}
	return options
}

func writeXLSXStaticParts(zw *zip.Writer, options xlsxOptions) error {
	parts := []struct {
		name string
		data []byte
	}{
		{xlsxContentTypesPath, xlsxContentTypesXML(options.sharedStrings)},
		{xlsxRootRelsPath, []byte(`<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="` + xlsxRelationshipNamespace + `"><Relationship Id="rId1" Type="` + xlsxOfficeRelationshipNS + `/officeDocument" Target="xl/workbook.xml"/></Relationships>`)},
		{xlsxWorkbookPath, xlsxWorkbookXML()},
		{xlsxWorkbookRelsPath, xlsxWorkbookRelsXML(options.sharedStrings)},
	}
	for _, part := range parts {
		if err := writeXLSXZipEntry(zw, part.name, part.data); err != nil {
			return err
		}
	}
	return nil
}

func xlsxContentTypesXML(sharedStrings bool) []byte {
	var buf bytes.Buffer
	buf.WriteString(`<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">`)
	buf.WriteString(`<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>`)
	buf.WriteString(`<Default Extension="xml" ContentType="application/xml"/>`)
	buf.WriteString(`<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>`)
	buf.WriteString(`<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`)
	if sharedStrings {
		buf.WriteString(`<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>`)
	}
	buf.WriteString(`</Types>`)
	return buf.Bytes()
}

func xlsxWorkbookXML() []byte {
	return []byte(`<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="` + xlsxSpreadsheetNamespace + `" xmlns:r="` + xlsxOfficeRelationshipNS + `"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>`)
}

func xlsxWorkbookRelsXML(sharedStrings bool) []byte {
	var buf bytes.Buffer
	buf.WriteString(`<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="` + xlsxRelationshipNamespace + `"><Relationship Id="rId1" Type="` + xlsxOfficeRelationshipNS + `/worksheet" Target="worksheets/sheet1.xml"/>`)
	if sharedStrings {
		buf.WriteString(`<Relationship Id="rId2" Type="` + xlsxOfficeRelationshipNS + `/sharedStrings" Target="sharedStrings.xml"/>`)
	}
	buf.WriteString(`</Relationships>`)
	return buf.Bytes()
}

func writeXLSXSheet(ctx context.Context, w io.Writer, columns []Column, stream RowStream, options xlsxOptions, shared *xlsxSharedStrings) error {
	if _, err := io.WriteString(w, `<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="`+xlsxSpreadsheetNamespace+`"><sheetData>`); err != nil {
		return fmt.Errorf("lazuli/reports: write xlsx sheet: %w", err)
	}
	if err := writeXLSXRow(w, 1, xlsxHeaderCells(columns, options, shared)); err != nil {
		return err
	}

	rowIndex := 2
	err := stream(ctx, func(row Row) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		cells, err := xlsxRowCells(columns, row, options, shared)
		if err != nil {
			return err
		}
		if err := writeXLSXRow(w, rowIndex, cells); err != nil {
			return err
		}
		rowIndex++
		return ctx.Err()
	})
	if err != nil {
		return err
	}
	if _, err := io.WriteString(w, `</sheetData></worksheet>`); err != nil {
		return fmt.Errorf("lazuli/reports: write xlsx sheet: %w", err)
	}
	return nil
}

func xlsxHeaderCells(columns []Column, options xlsxOptions, shared *xlsxSharedStrings) []xlsxCell {
	cells := make([]xlsxCell, len(columns))
	for i, header := range csvHeaders(columns) {
		cells[i] = xlsxStringCell(header, true, options, shared)
	}
	return cells
}

func xlsxRowCells(columns []Column, row Row, options xlsxOptions, shared *xlsxSharedStrings) ([]xlsxCell, error) {
	cells := make([]xlsxCell, len(columns))
	for i, column := range columns {
		cell, err := xlsxValueCell(row[column.Key], options, shared)
		if err != nil {
			return nil, fmt.Errorf("lazuli/reports: encode xlsx column %q: %w", column.Key, err)
		}
		cells[i] = cell
	}
	return cells, nil
}

type xlsxCell struct {
	kind  string
	value string
}

func xlsxValueCell(value any, options xlsxOptions, shared *xlsxSharedStrings) (xlsxCell, error) {
	if value == nil {
		return xlsxCell{}, nil
	}
	if cell, ok := xlsxScalarCell(value); ok {
		return cell, nil
	}
	text, wasText, err := stringifyCSVValue(value)
	if err != nil {
		return xlsxCell{}, fmt.Errorf("lazuli/reports: encode xlsx cell: %w", err)
	}
	return xlsxStringCell(text, wasText, options, shared), nil
}

func xlsxScalarCell(value any) (xlsxCell, bool) {
	switch typed := value.(type) {
	case bool:
		if typed {
			return xlsxCell{kind: "b", value: "1"}, true
		}
		return xlsxCell{kind: "b", value: "0"}, true
	case int:
		return xlsxCell{kind: "n", value: strconv.FormatInt(int64(typed), 10)}, true
	case int8:
		return xlsxCell{kind: "n", value: strconv.FormatInt(int64(typed), 10)}, true
	case int16:
		return xlsxCell{kind: "n", value: strconv.FormatInt(int64(typed), 10)}, true
	case int32:
		return xlsxCell{kind: "n", value: strconv.FormatInt(int64(typed), 10)}, true
	case int64:
		return xlsxCell{kind: "n", value: strconv.FormatInt(typed, 10)}, true
	case uint:
		return xlsxCell{kind: "n", value: strconv.FormatUint(uint64(typed), 10)}, true
	case uint8:
		return xlsxCell{kind: "n", value: strconv.FormatUint(uint64(typed), 10)}, true
	case uint16:
		return xlsxCell{kind: "n", value: strconv.FormatUint(uint64(typed), 10)}, true
	case uint32:
		return xlsxCell{kind: "n", value: strconv.FormatUint(uint64(typed), 10)}, true
	case uint64:
		return xlsxCell{kind: "n", value: strconv.FormatUint(typed, 10)}, true
	case float32:
		return xlsxFloatCell(float64(typed))
	case float64:
		return xlsxFloatCell(typed)
	default:
		return xlsxReflectNumberCell(value)
	}
}

func xlsxReflectNumberCell(value any) (xlsxCell, bool) {
	v := reflect.ValueOf(value)
	if !v.IsValid() {
		return xlsxCell{}, false
	}
	switch v.Kind() {
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return xlsxCell{kind: "n", value: strconv.FormatInt(v.Int(), 10)}, true
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		return xlsxCell{kind: "n", value: strconv.FormatUint(v.Uint(), 10)}, true
	case reflect.Float32, reflect.Float64:
		return xlsxFloatCell(v.Float())
	default:
		return xlsxCell{}, false
	}
}

func xlsxFloatCell(value float64) (xlsxCell, bool) {
	if math.IsInf(value, 0) || math.IsNaN(value) {
		return xlsxCell{}, false
	}
	return xlsxCell{kind: "n", value: strconv.FormatFloat(value, 'g', -1, 64)}, true
}

func xlsxStringCell(text string, wasText bool, options xlsxOptions, shared *xlsxSharedStrings) xlsxCell {
	if options.guardInjection && wasText {
		text = guardCSVInjection(text)
	}
	if options.sharedStrings {
		return xlsxCell{kind: "s", value: strconv.Itoa(shared.Index(text))}
	}
	return xlsxCell{kind: "inlineStr", value: text}
}

func writeXLSXRow(w io.Writer, rowIndex int, cells []xlsxCell) error {
	if _, err := fmt.Fprintf(w, `<row r="%d">`, rowIndex); err != nil {
		return fmt.Errorf("lazuli/reports: write xlsx row: %w", err)
	}
	for i, cell := range cells {
		if err := writeXLSXCell(w, xlsxCellRef(i+1, rowIndex), cell); err != nil {
			return err
		}
	}
	if _, err := io.WriteString(w, `</row>`); err != nil {
		return fmt.Errorf("lazuli/reports: write xlsx row: %w", err)
	}
	return nil
}

func writeXLSXCell(w io.Writer, ref string, cell xlsxCell) error {
	switch cell.kind {
	case "":
		if _, err := fmt.Fprintf(w, `<c r="%s"/>`, ref); err != nil {
			return fmt.Errorf("lazuli/reports: write xlsx cell: %w", err)
		}
	case "inlineStr":
		if _, err := fmt.Fprintf(w, `<c r="%s" t="inlineStr"><is><t>`, ref); err != nil {
			return fmt.Errorf("lazuli/reports: write xlsx cell: %w", err)
		}
		xlsxEscape(w, cell.value)
		if _, err := io.WriteString(w, `</t></is></c>`); err != nil {
			return fmt.Errorf("lazuli/reports: write xlsx cell: %w", err)
		}
	default:
		if _, err := fmt.Fprintf(w, `<c r="%s" t="%s"><v>`, ref, cell.kind); err != nil {
			return fmt.Errorf("lazuli/reports: write xlsx cell: %w", err)
		}
		xlsxEscape(w, cell.value)
		if _, err := io.WriteString(w, `</v></c>`); err != nil {
			return fmt.Errorf("lazuli/reports: write xlsx cell: %w", err)
		}
	}
	return nil
}

func xlsxCellRef(column, row int) string {
	var letters []byte
	for column > 0 {
		column--
		letters = append(letters, byte('A'+column%26))
		column /= 26
	}
	for i, j := 0, len(letters)-1; i < j; i, j = i+1, j-1 {
		letters[i], letters[j] = letters[j], letters[i]
	}
	return string(letters) + strconv.Itoa(row)
}

type xlsxSharedStrings struct {
	index map[string]int
	items []string
	count int
}

func newXLSXSharedStrings() *xlsxSharedStrings {
	return &xlsxSharedStrings{index: make(map[string]int)}
}

func (shared *xlsxSharedStrings) Index(text string) int {
	shared.count++
	if index, ok := shared.index[text]; ok {
		return index
	}
	index := len(shared.items)
	shared.index[text] = index
	shared.items = append(shared.items, text)
	return index
}

func (shared *xlsxSharedStrings) XML() []byte {
	var buf bytes.Buffer
	buf.WriteString(`<?xml version="1.0" encoding="UTF-8"?><sst xmlns="` + xlsxSpreadsheetNamespace + `" count="`)
	buf.WriteString(strconv.Itoa(shared.count))
	buf.WriteString(`" uniqueCount="`)
	buf.WriteString(strconv.Itoa(len(shared.items)))
	buf.WriteString(`">`)
	for _, item := range shared.items {
		buf.WriteString(`<si><t>`)
		xlsxEscape(&buf, item)
		buf.WriteString(`</t></si>`)
	}
	buf.WriteString(`</sst>`)
	return buf.Bytes()
}

func writeXLSXZipEntry(zw *zip.Writer, name string, data []byte) error {
	header := &zip.FileHeader{
		Name:     name,
		Method:   zip.Deflate,
		Modified: xlsxZipModified,
	}
	w, err := zw.CreateHeader(header)
	if err != nil {
		return fmt.Errorf("lazuli/reports: create xlsx part %s: %w", name, err)
	}
	if _, err := w.Write(data); err != nil {
		return fmt.Errorf("lazuli/reports: write xlsx part %s: %w", name, err)
	}
	return nil
}

func xlsxEscape(w io.Writer, text string) {
	_ = xml.EscapeText(w, []byte(text))
}
