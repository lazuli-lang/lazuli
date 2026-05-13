package reports_test

import (
	"errors"
	"fmt"
	"math"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/reports"
)

func TestPDFPlanHelpersValidateCompletePlan(t *testing.T) {
	t.Parallel()

	page := reports.PDFPage{
		Size:    reports.PDFPageSizeLetter(),
		Margins: reports.PDFMarginsUniform(36),
		Sections: []reports.PDFSection{{
			Key:   "summary",
			Title: "Summary",
			Blocks: []reports.PDFBlock{
				reports.PDFTable(
					"customers",
					[]reports.Column{{Key: "name", Header: "Name"}, {Key: "total", Header: "Total"}},
					[]reports.Row{{"total": 2, "name": "Ada"}},
				),
				reports.PDFImage("chart", "charts/revenue.png", 288, 144, "Monthly revenue"),
			},
		}},
	}
	plan := reports.PDFPlan{
		Title: "Operations",
		Pages: []reports.PDFPage{page},
	}

	if err := plan.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if got, want := page.ContentWidth(), reports.PDFPoints(540); got != want {
		t.Fatalf("ContentWidth() = %v, want %v", got, want)
	}
	if got, want := page.ContentHeight(), reports.PDFPoints(720); got != want {
		t.Fatalf("ContentHeight() = %v, want %v", got, want)
	}
	if got := plan.Pages[0].Sections[0].Blocks[0].Kind; got != reports.PDFBlockKindTable {
		t.Fatalf("table block kind = %q, want %q", got, reports.PDFBlockKindTable)
	}
	if got := plan.Pages[0].Sections[0].Blocks[1].Kind; got != reports.PDFBlockKindImage {
		t.Fatalf("image block kind = %q, want %q", got, reports.PDFBlockKindImage)
	}
}

func TestPDFPageSizeOrientationAndMargins(t *testing.T) {
	t.Parallel()

	a4 := reports.PDFPageSizeA4()
	landscape := a4.Landscape()
	if landscape.Width != a4.Height || landscape.Height != a4.Width {
		t.Fatalf("Landscape() = %#v, want swapped A4 dimensions", landscape)
	}
	if portrait := landscape.Portrait(); portrait.Width != a4.Width || portrait.Height != a4.Height {
		t.Fatalf("Portrait() = %#v, want original A4 dimensions", portrait)
	}

	margins := reports.PDFMarginsSymmetric(24, 36)
	if got, want := margins.Vertical(), reports.PDFPoints(48); got != want {
		t.Fatalf("Vertical() = %v, want %v", got, want)
	}
	if got, want := margins.Horizontal(), reports.PDFPoints(72); got != want {
		t.Fatalf("Horizontal() = %v, want %v", got, want)
	}
}

func TestValidatePDFPlanRejectsMissingPages(t *testing.T) {
	t.Parallel()

	err := reports.ValidatePDFPlan(reports.PDFPlan{})
	if !errors.Is(err, reports.ErrInvalidPDFPlan) {
		t.Fatalf("ValidatePDFPlan() error = %v, want ErrInvalidPDFPlan", err)
	}
	if !errors.Is(err, reports.ErrNoPDFPages) {
		t.Fatalf("ValidatePDFPlan() error = %v, want ErrNoPDFPages", err)
	}

	var report *reports.PDFPlanErrorReport
	if !errors.As(err, &report) {
		t.Fatalf("ValidatePDFPlan() error = %T, want *PDFPlanErrorReport", err)
	}
	if got := len(report.Errors); got != 1 {
		t.Fatalf("error count = %d, want 1", got)
	}
}

func TestValidatePDFPlanReportsDeterministicErrors(t *testing.T) {
	t.Parallel()

	plan := reports.PDFPlan{
		Pages: []reports.PDFPage{
			{
				Size:    reports.PDFPageSize{Width: 100, Height: 100},
				Margins: reports.PDFMargins{Right: 60, Left: 60},
				Sections: []reports.PDFSection{
					{Key: "summary"},
					{
						Key: "summary",
						Blocks: []reports.PDFBlock{
							reports.PDFTable("table", []reports.Column{{Key: " id "}}, nil),
						},
					},
				},
			},
			{
				Size:    reports.PDFPageSizeLetter(),
				Margins: reports.PDFMarginsUniform(36),
				Sections: []reports.PDFSection{{
					Key: "media",
					Blocks: []reports.PDFBlock{
						reports.PDFImage("chart", " charts/revenue.png ", 0, reports.PDFPoints(math.Inf(1)), ""),
						{
							Key:   "chart",
							Kind:  reports.PDFBlockKindImage,
							Table: &reports.PDFTableBlock{Columns: []reports.Column{{Key: "id"}}},
						},
						{Key: "unknown", Kind: reports.PDFBlockKind("chart")},
					},
				}},
			},
		},
	}

	err := reports.ValidatePDFPlan(plan)
	for _, wantErr := range []error{
		reports.ErrInvalidPDFPlan,
		reports.ErrInvalidPDFMargins,
		reports.ErrNoPDFBlocks,
		reports.ErrDuplicatePDFKey,
		reports.ErrInvalidColumn,
		reports.ErrInvalidPDFImage,
		reports.ErrInvalidPDFBlock,
	} {
		if !errors.Is(err, wantErr) {
			t.Fatalf("ValidatePDFPlan() error = %v, want %v", err, wantErr)
		}
	}

	var report *reports.PDFPlanErrorReport
	if !errors.As(err, &report) {
		t.Fatalf("ValidatePDFPlan() error = %T, want *PDFPlanErrorReport", err)
	}

	got := pdfPlanErrorLocations(report.Errors)
	want := []string{
		"1/0/0/margins",
		"1/1/0/blocks",
		"1/2/0/key",
		"1/2/1/table.columns",
		"2/1/1/image.source",
		"2/1/1/image.width",
		"2/1/1/image.height",
		"2/1/2/key",
		"2/1/2/image",
		"2/1/2/table",
		"2/1/3/kind",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("error locations = %#v, want %#v", got, want)
	}
}

func pdfPlanErrorLocations(errs []*reports.PDFPlanError) []string {
	locations := make([]string, 0, len(errs))
	for _, err := range errs {
		locations = append(locations, fmt.Sprintf("%d/%d/%d/%s", err.Page, err.Section, err.Block, err.Field))
	}
	return locations
}
