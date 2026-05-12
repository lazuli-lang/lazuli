package reports

import (
	"context"
	"encoding/xml"
	"errors"
	"fmt"
	"io"
	"strings"
	"unicode"
	"unicode/utf8"
)

const (
	// DefaultXMLRootName is the default XML root element name.
	DefaultXMLRootName = "rows"

	// DefaultXMLRowName is the default XML element name for each report row.
	DefaultXMLRowName = "row"
)

var (
	// ErrInvalidXMLName is wrapped when an XML element name is unusable.
	ErrInvalidXMLName = errors.New("lazuli/reports: invalid xml name")
)

// XMLOption configures XML exports.
type XMLOption func(*xmlOptions)

type xmlOptions struct {
	rootName string
	rowName  string
}

// WithXMLRootName sets the root element name for XML exports.
func WithXMLRootName(name string) XMLOption {
	return func(options *xmlOptions) {
		options.rootName = name
	}
}

// WithXMLRowName sets the repeated row element name for XML exports.
func WithXMLRowName(name string) XMLOption {
	return func(options *xmlOptions) {
		options.rowName = name
	}
}

// WriteXML writes rows as XML using columns for element ordering.
func WriteXML(ctx context.Context, w io.Writer, columns []Column, rows []Row, opts ...XMLOption) error {
	return StreamXML(ctx, w, columns, sliceRowStream(rows), opts...)
}

// StreamXML streams rows as XML using columns for element ordering.
func StreamXML(ctx context.Context, w io.Writer, columns []Column, stream RowStream, opts ...XMLOption) error {
	if w == nil {
		return ErrNilWriter
	}
	if stream == nil {
		return ErrNilRowStream
	}
	if err := ValidateColumns(columns); err != nil {
		return err
	}
	options := applyXMLOptions(opts)
	if err := validateXMLConfig(options, columns); err != nil {
		return err
	}
	ctx = contextOrBackground(ctx)

	if err := ctx.Err(); err != nil {
		return err
	}

	encoder := xml.NewEncoder(w)
	root := xml.StartElement{Name: xml.Name{Local: options.rootName}}
	if err := encodeXMLToken(encoder, root, "root start"); err != nil {
		return err
	}
	if err := flushXML(encoder, "root"); err != nil {
		return err
	}

	err := stream(ctx, func(row Row) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		if err := encodeXMLRow(encoder, options.rowName, columns, row); err != nil {
			return err
		}
		if err := flushXML(encoder, "row"); err != nil {
			return err
		}
		return ctx.Err()
	})
	if err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := encodeXMLToken(encoder, root.End(), "root end"); err != nil {
		return err
	}
	if err := flushXML(encoder, "xml"); err != nil {
		return err
	}
	if _, err := io.WriteString(w, "\n"); err != nil {
		return fmt.Errorf("lazuli/reports: write xml newline: %w", err)
	}
	return nil
}

func applyXMLOptions(opts []XMLOption) xmlOptions {
	options := xmlOptions{
		rootName: DefaultXMLRootName,
		rowName:  DefaultXMLRowName,
	}
	for _, opt := range opts {
		if opt != nil {
			opt(&options)
		}
	}
	return options
}

func validateXMLConfig(options xmlOptions, columns []Column) error {
	if err := validateXMLElementName("root", options.rootName); err != nil {
		return err
	}
	if err := validateXMLElementName("row", options.rowName); err != nil {
		return err
	}
	for i, column := range columns {
		if err := validateXMLElementName(fmt.Sprintf("column %d key", i), column.Key); err != nil {
			return err
		}
	}
	return nil
}

func validateXMLElementName(label, name string) error {
	if name == "" || name != strings.TrimSpace(name) || !isSimpleXMLName(name) {
		return fmt.Errorf("%w: %s %q", ErrInvalidXMLName, label, name)
	}
	return nil
}

func isSimpleXMLName(name string) bool {
	if !utf8.ValidString(name) {
		return false
	}
	for i, r := range name {
		if i == 0 {
			if !isSimpleXMLNameStart(r) {
				return false
			}
			continue
		}
		if !isSimpleXMLNamePart(r) {
			return false
		}
	}
	return name != ""
}

func isSimpleXMLNameStart(r rune) bool {
	return r == '_' || unicode.IsLetter(r)
}

func isSimpleXMLNamePart(r rune) bool {
	return isSimpleXMLNameStart(r) || unicode.IsDigit(r) || r == '-' || r == '.'
}

func encodeXMLRow(encoder *xml.Encoder, rowName string, columns []Column, row Row) error {
	rowStart := xml.StartElement{Name: xml.Name{Local: rowName}}
	if err := encodeXMLToken(encoder, rowStart, "row start"); err != nil {
		return err
	}

	for _, column := range columns {
		if err := encodeXMLColumn(encoder, column, row[column.Key]); err != nil {
			return err
		}
	}

	if err := encodeXMLToken(encoder, rowStart.End(), "row end"); err != nil {
		return err
	}
	return nil
}

func encodeXMLColumn(encoder *xml.Encoder, column Column, value any) error {
	text, _, err := stringifyCSVValue(value)
	if err != nil {
		return fmt.Errorf("lazuli/reports: encode xml column %q: %w", column.Key, err)
	}

	start := xml.StartElement{Name: xml.Name{Local: column.Key}}
	if err := encodeXMLToken(encoder, start, "column start"); err != nil {
		return err
	}
	if text != "" {
		if err := encodeXMLToken(encoder, xml.CharData(text), "column text"); err != nil {
			return err
		}
	}
	if err := encodeXMLToken(encoder, start.End(), "column end"); err != nil {
		return err
	}
	return nil
}

func encodeXMLToken(encoder *xml.Encoder, token xml.Token, label string) error {
	if err := encoder.EncodeToken(token); err != nil {
		return fmt.Errorf("lazuli/reports: write xml %s: %w", label, err)
	}
	return nil
}

func flushXML(encoder *xml.Encoder, label string) error {
	if err := encoder.Flush(); err != nil {
		return fmt.Errorf("lazuli/reports: flush %s: %w", label, err)
	}
	return nil
}
