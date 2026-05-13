package reports

import (
	"errors"
	"fmt"
	"math"
	"strings"
)

const (
	// PDFPointsPerInch is the PDF user-space point scale.
	PDFPointsPerInch PDFPoints = 72
)

var (
	// ErrInvalidPDFPlan is exposed by PDF plan validation reports.
	ErrInvalidPDFPlan = errors.New("lazuli/reports: invalid pdf plan")

	// ErrNoPDFPages is returned when a PDF plan has no pages.
	ErrNoPDFPages = errors.New("lazuli/reports: no pdf pages")

	// ErrNoPDFSections is returned when a PDF page has no sections.
	ErrNoPDFSections = errors.New("lazuli/reports: no pdf sections")

	// ErrNoPDFBlocks is returned when a PDF section has no blocks.
	ErrNoPDFBlocks = errors.New("lazuli/reports: no pdf blocks")

	// ErrInvalidPDFPageSize is wrapped when a PDF page size is unusable.
	ErrInvalidPDFPageSize = errors.New("lazuli/reports: invalid pdf page size")

	// ErrInvalidPDFMargins is wrapped when PDF margins are unusable.
	ErrInvalidPDFMargins = errors.New("lazuli/reports: invalid pdf margins")

	// ErrInvalidPDFKey is wrapped when a PDF section or block key is unusable.
	ErrInvalidPDFKey = errors.New("lazuli/reports: invalid pdf key")

	// ErrDuplicatePDFKey is wrapped when a PDF section or block key is repeated in its scope.
	ErrDuplicatePDFKey = errors.New("lazuli/reports: duplicate pdf key")

	// ErrInvalidPDFBlock is wrapped when a PDF block is structurally unusable.
	ErrInvalidPDFBlock = errors.New("lazuli/reports: invalid pdf block")

	// ErrInvalidPDFImage is wrapped when an image block is structurally unusable.
	ErrInvalidPDFImage = errors.New("lazuli/reports: invalid pdf image")
)

// PDFPoints is a measurement in PDF user-space points.
type PDFPoints float64

// PDFPageSize describes one PDF page size in points.
//
// Name is optional metadata for callers and renderers. Width and Height must
// be positive, finite values before a page can be rendered.
type PDFPageSize struct {
	Name   string    `json:"name,omitempty"`
	Width  PDFPoints `json:"width"`
	Height PDFPoints `json:"height"`
}

// PDFPageSizeA4 returns an A4 portrait page size.
func PDFPageSizeA4() PDFPageSize {
	return PDFPageSize{Name: "A4", Width: 595.28, Height: 841.89}
}

// PDFPageSizeLetter returns a US Letter portrait page size.
func PDFPageSizeLetter() PDFPageSize {
	return PDFPageSize{Name: "Letter", Width: 612, Height: 792}
}

// Landscape returns size with the longer edge as Width.
func (s PDFPageSize) Landscape() PDFPageSize {
	if s.Width >= s.Height {
		return s
	}
	s.Width, s.Height = s.Height, s.Width
	return s
}

// Portrait returns size with the shorter edge as Width.
func (s PDFPageSize) Portrait() PDFPageSize {
	if s.Width <= s.Height {
		return s
	}
	s.Width, s.Height = s.Height, s.Width
	return s
}

// PDFMargins describes page margins in points.
type PDFMargins struct {
	Top    PDFPoints `json:"top"`
	Right  PDFPoints `json:"right"`
	Bottom PDFPoints `json:"bottom"`
	Left   PDFPoints `json:"left"`
}

// PDFMarginsUniform returns equal margins on every page edge.
func PDFMarginsUniform(points PDFPoints) PDFMargins {
	return PDFMargins{Top: points, Right: points, Bottom: points, Left: points}
}

// PDFMarginsSymmetric returns vertical and horizontal margin pairs.
func PDFMarginsSymmetric(vertical, horizontal PDFPoints) PDFMargins {
	return PDFMargins{Top: vertical, Right: horizontal, Bottom: vertical, Left: horizontal}
}

// Horizontal returns the combined left and right margins.
func (m PDFMargins) Horizontal() PDFPoints {
	return m.Left + m.Right
}

// Vertical returns the combined top and bottom margins.
func (m PDFMargins) Vertical() PDFPoints {
	return m.Top + m.Bottom
}

// PDFPlan is a render-ready PDF report plan.
//
// The reports package only validates plan structure; it does not render PDFs.
type PDFPlan struct {
	Title string    `json:"title,omitempty"`
	Pages []PDFPage `json:"pages"`
}

// Validate checks the plan using deterministic slice traversal order.
func (p PDFPlan) Validate() error {
	return ValidatePDFPlan(p)
}

// PDFPage describes one planned PDF page.
type PDFPage struct {
	Size     PDFPageSize  `json:"size"`
	Margins  PDFMargins   `json:"margins"`
	Sections []PDFSection `json:"sections"`
}

// ContentWidth returns the width left after horizontal margins.
func (p PDFPage) ContentWidth() PDFPoints {
	return p.Size.Width - p.Margins.Horizontal()
}

// ContentHeight returns the height left after vertical margins.
func (p PDFPage) ContentHeight() PDFPoints {
	return p.Size.Height - p.Margins.Vertical()
}

// PDFSection describes a logical section on a planned PDF page.
//
// Key is a stable, caller-defined identifier scoped to the page.
type PDFSection struct {
	Key    string     `json:"key"`
	Title  string     `json:"title,omitempty"`
	Blocks []PDFBlock `json:"blocks"`
}

// PDFBlockKind identifies a supported PDF report block type.
type PDFBlockKind string

const (
	// PDFBlockKindTable identifies a table block.
	PDFBlockKindTable PDFBlockKind = "table"

	// PDFBlockKindImage identifies an image block.
	PDFBlockKindImage PDFBlockKind = "image"
)

// PDFBlock describes one table or image block inside a PDF section.
//
// Key is a stable, caller-defined identifier scoped to the section. Exactly
// one payload must be set and must match Kind.
type PDFBlock struct {
	Key   string         `json:"key"`
	Kind  PDFBlockKind   `json:"kind"`
	Table *PDFTableBlock `json:"table,omitempty"`
	Image *PDFImageBlock `json:"image,omitempty"`
}

// PDFTable returns a table block using report columns for deterministic cell order.
func PDFTable(key string, columns []Column, rows []Row) PDFBlock {
	return PDFBlock{
		Key:  key,
		Kind: PDFBlockKindTable,
		Table: &PDFTableBlock{
			Columns: columns,
			Rows:    rows,
		},
	}
}

// PDFImage returns an image block with planned dimensions in points.
func PDFImage(key, source string, width, height PDFPoints, altText string) PDFBlock {
	return PDFBlock{
		Key:  key,
		Kind: PDFBlockKindImage,
		Image: &PDFImageBlock{
			Source:  source,
			AltText: altText,
			Width:   width,
			Height:  height,
		},
	}
}

// PDFTableBlock describes tabular report content.
//
// Columns determine cell ordering. Rows may be empty for a header-only table.
type PDFTableBlock struct {
	Caption string   `json:"caption,omitempty"`
	Columns []Column `json:"columns"`
	Rows    []Row    `json:"rows,omitempty"`
}

// PDFImageBlock describes an image placeholder for a future renderer.
//
// Source is an opaque caller-defined image reference; validation only checks
// that it is non-empty and trimmed.
type PDFImageBlock struct {
	Source  string    `json:"source"`
	AltText string    `json:"alt_text,omitempty"`
	Width   PDFPoints `json:"width"`
	Height  PDFPoints `json:"height"`
}

// PDFPlanErrorReport reports one or more deterministic PDF plan errors.
type PDFPlanErrorReport struct {
	Errors []*PDFPlanError
}

// Error returns a stable human-readable PDF plan report summary.
func (r *PDFPlanErrorReport) Error() string {
	if r == nil || len(r.Errors) == 0 {
		return "<nil>"
	}
	if len(r.Errors) == 1 {
		return r.Errors[0].Error()
	}
	return fmt.Sprintf("lazuli/reports: pdf plan validation failed (%d errors)", len(r.Errors))
}

// Unwrap exposes report entries for errors.Is and errors.As.
func (r *PDFPlanErrorReport) Unwrap() []error {
	if r == nil || len(r.Errors) == 0 {
		return nil
	}
	errs := make([]error, 0, len(r.Errors)+1)
	errs = append(errs, ErrInvalidPDFPlan)
	for _, err := range r.Errors {
		if err != nil {
			errs = append(errs, err)
		}
	}
	return errs
}

// PDFPlanError reports one validation error with optional plan coordinates.
//
// Page, Section, and Block are 1-based when set.
type PDFPlanError struct {
	Page    int
	Section int
	Block   int
	Field   string
	Err     error
}

// Error returns a stable human-readable plan validation error.
func (e *PDFPlanError) Error() string {
	if e == nil {
		return "<nil>"
	}
	err := e.Err
	if err == nil {
		err = ErrInvalidPDFPlan
	}

	var parts []string
	if e.Page > 0 {
		parts = append(parts, fmt.Sprintf("page %d", e.Page))
	}
	if e.Section > 0 {
		parts = append(parts, fmt.Sprintf("section %d", e.Section))
	}
	if e.Block > 0 {
		parts = append(parts, fmt.Sprintf("block %d", e.Block))
	}
	if e.Field != "" {
		parts = append(parts, fmt.Sprintf("field %q", e.Field))
	}
	if len(parts) == 0 {
		return fmt.Sprintf("lazuli/reports: pdf plan: %v", err)
	}
	return fmt.Sprintf("lazuli/reports: pdf plan %s: %v", strings.Join(parts, " "), err)
}

// Unwrap exposes the classified cause for errors.Is and errors.As.
func (e *PDFPlanError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

// ValidatePDFPlan checks plan structure without rendering or touching sources.
//
// Validation walks pages, sections, and blocks in slice order so returned
// PDFPlanError entries are deterministic.
func ValidatePDFPlan(plan PDFPlan) error {
	var errs []*PDFPlanError
	if len(plan.Pages) == 0 {
		errs = append(errs, pdfPlanError(0, 0, 0, "pages", ErrNoPDFPages))
		return pdfPlanErrorReport(errs)
	}

	for pageIndex, page := range plan.Pages {
		pageNumber := pageIndex + 1
		errs = append(errs, validatePDFPage(pageNumber, page)...)
	}
	return pdfPlanErrorReport(errs)
}

func validatePDFPage(pageNumber int, page PDFPage) []*PDFPlanError {
	var errs []*PDFPlanError
	sizeValid := true
	if !isPositivePDFPoints(page.Size.Width) {
		sizeValid = false
		errs = append(errs, pdfPlanError(pageNumber, 0, 0, "size.width", ErrInvalidPDFPageSize))
	}
	if !isPositivePDFPoints(page.Size.Height) {
		sizeValid = false
		errs = append(errs, pdfPlanError(pageNumber, 0, 0, "size.height", ErrInvalidPDFPageSize))
	}

	marginsValid := true
	for _, margin := range []struct {
		field string
		value PDFPoints
	}{
		{field: "margins.top", value: page.Margins.Top},
		{field: "margins.right", value: page.Margins.Right},
		{field: "margins.bottom", value: page.Margins.Bottom},
		{field: "margins.left", value: page.Margins.Left},
	} {
		if !isNonNegativePDFPoints(margin.value) {
			marginsValid = false
			errs = append(errs, pdfPlanError(pageNumber, 0, 0, margin.field, ErrInvalidPDFMargins))
		}
	}
	if sizeValid && marginsValid {
		if page.Margins.Horizontal() >= page.Size.Width {
			errs = append(errs, pdfPlanError(
				pageNumber,
				0,
				0,
				"margins",
				fmt.Errorf("%w: horizontal margins leave no content width", ErrInvalidPDFMargins),
			))
		}
		if page.Margins.Vertical() >= page.Size.Height {
			errs = append(errs, pdfPlanError(
				pageNumber,
				0,
				0,
				"margins",
				fmt.Errorf("%w: vertical margins leave no content height", ErrInvalidPDFMargins),
			))
		}
	}

	if len(page.Sections) == 0 {
		errs = append(errs, pdfPlanError(pageNumber, 0, 0, "sections", ErrNoPDFSections))
		return errs
	}

	seenSections := make(map[string]int, len(page.Sections))
	for sectionIndex, section := range page.Sections {
		sectionNumber := sectionIndex + 1
		if err := validatePDFKey("section key", section.Key); err != nil {
			errs = append(errs, pdfPlanError(pageNumber, sectionNumber, 0, "key", err))
		} else if first, ok := seenSections[section.Key]; ok {
			errs = append(errs, pdfPlanError(
				pageNumber,
				sectionNumber,
				0,
				"key",
				fmt.Errorf("%w: section key %q also appears at section %d", ErrDuplicatePDFKey, section.Key, first),
			))
		} else {
			seenSections[section.Key] = sectionNumber
		}

		errs = append(errs, validatePDFSection(pageNumber, sectionNumber, section)...)
	}
	return errs
}

func validatePDFSection(pageNumber, sectionNumber int, section PDFSection) []*PDFPlanError {
	var errs []*PDFPlanError
	if len(section.Blocks) == 0 {
		errs = append(errs, pdfPlanError(pageNumber, sectionNumber, 0, "blocks", ErrNoPDFBlocks))
		return errs
	}

	seenBlocks := make(map[string]int, len(section.Blocks))
	for blockIndex, block := range section.Blocks {
		blockNumber := blockIndex + 1
		if err := validatePDFKey("block key", block.Key); err != nil {
			errs = append(errs, pdfPlanError(pageNumber, sectionNumber, blockNumber, "key", err))
		} else if first, ok := seenBlocks[block.Key]; ok {
			errs = append(errs, pdfPlanError(
				pageNumber,
				sectionNumber,
				blockNumber,
				"key",
				fmt.Errorf("%w: block key %q also appears at block %d", ErrDuplicatePDFKey, block.Key, first),
			))
		} else {
			seenBlocks[block.Key] = blockNumber
		}

		errs = append(errs, validatePDFBlock(pageNumber, sectionNumber, blockNumber, block)...)
	}
	return errs
}

func validatePDFBlock(pageNumber, sectionNumber, blockNumber int, block PDFBlock) []*PDFPlanError {
	switch block.Kind {
	case PDFBlockKindTable:
		return validatePDFTableBlock(pageNumber, sectionNumber, blockNumber, block)
	case PDFBlockKindImage:
		return validatePDFImageBlock(pageNumber, sectionNumber, blockNumber, block)
	default:
		return []*PDFPlanError{pdfPlanError(
			pageNumber,
			sectionNumber,
			blockNumber,
			"kind",
			fmt.Errorf("%w: unsupported kind %q", ErrInvalidPDFBlock, block.Kind),
		)}
	}
}

func validatePDFTableBlock(pageNumber, sectionNumber, blockNumber int, block PDFBlock) []*PDFPlanError {
	var errs []*PDFPlanError
	if block.Table == nil {
		errs = append(errs, pdfPlanError(pageNumber, sectionNumber, blockNumber, "table", ErrInvalidPDFBlock))
	} else if err := ValidateColumns(block.Table.Columns); err != nil {
		errs = append(errs, pdfPlanError(pageNumber, sectionNumber, blockNumber, "table.columns", err))
	}
	if block.Image != nil {
		errs = append(errs, pdfPlanError(
			pageNumber,
			sectionNumber,
			blockNumber,
			"image",
			fmt.Errorf("%w: table block cannot include image payload", ErrInvalidPDFBlock),
		))
	}
	return errs
}

func validatePDFImageBlock(pageNumber, sectionNumber, blockNumber int, block PDFBlock) []*PDFPlanError {
	var errs []*PDFPlanError
	if block.Image == nil {
		errs = append(errs, pdfPlanError(pageNumber, sectionNumber, blockNumber, "image", ErrInvalidPDFBlock))
	} else {
		if source := strings.TrimSpace(block.Image.Source); source == "" || source != block.Image.Source {
			errs = append(errs, pdfPlanError(pageNumber, sectionNumber, blockNumber, "image.source", ErrInvalidPDFImage))
		}
		if !isPositivePDFPoints(block.Image.Width) {
			errs = append(errs, pdfPlanError(pageNumber, sectionNumber, blockNumber, "image.width", ErrInvalidPDFImage))
		}
		if !isPositivePDFPoints(block.Image.Height) {
			errs = append(errs, pdfPlanError(pageNumber, sectionNumber, blockNumber, "image.height", ErrInvalidPDFImage))
		}
	}
	if block.Table != nil {
		errs = append(errs, pdfPlanError(
			pageNumber,
			sectionNumber,
			blockNumber,
			"table",
			fmt.Errorf("%w: image block cannot include table payload", ErrInvalidPDFBlock),
		))
	}
	return errs
}

func validatePDFKey(label, key string) error {
	trimmed := strings.TrimSpace(key)
	if trimmed == "" || trimmed != key {
		return fmt.Errorf("%w: %s must be non-empty and trimmed", ErrInvalidPDFKey, label)
	}
	return nil
}

func isPositivePDFPoints(points PDFPoints) bool {
	return isFinitePDFPoints(points) && points > 0
}

func isNonNegativePDFPoints(points PDFPoints) bool {
	return isFinitePDFPoints(points) && points >= 0
}

func isFinitePDFPoints(points PDFPoints) bool {
	value := float64(points)
	return !math.IsNaN(value) && !math.IsInf(value, 0)
}

func pdfPlanError(page, section, block int, field string, err error) *PDFPlanError {
	return &PDFPlanError{
		Page:    page,
		Section: section,
		Block:   block,
		Field:   field,
		Err:     err,
	}
}

func pdfPlanErrorReport(errs []*PDFPlanError) error {
	filtered := make([]*PDFPlanError, 0, len(errs))
	for _, err := range errs {
		if err != nil {
			filtered = append(filtered, err)
		}
	}
	if len(filtered) == 0 {
		return nil
	}
	return &PDFPlanErrorReport{Errors: filtered}
}
