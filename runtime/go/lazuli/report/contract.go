// Package report hosts the runtime side of the `report <name>` vocab.
// The language-level shape lowered by
// `crates/lazuli_analyzer/src/lib.rs:lower_report_decl` mirrors the
// types declared here so generated `*.gen.go` code references typed
// contract values rather than raw strings.
//
// Boundary discipline: this package never names a concrete provider
// (S3, GCS, MinIO, ...). Signed-URL signing flows through
// `runtime/go/lazuli/storage` and the bound `ObjectStore` adapter.
// Bytes-level CSV / XLSX writing lives in csv.go / xlsx.go (shipped at
// commit 5e2138e). pipeline.go composes them.
package report

import (
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

// Contract is the lowered `report <name>` shape. Codegen instantiates
// one Contract per declaration and passes it to Run().
type Contract struct {
	// Feature owns the report. Generated code sets this to the
	// authoring feature name (e.g. "customer").
	Feature string

	// Name is the report identifier from the `report <name>` header.
	Name string

	// Source is the qualified query reference (e.g.
	// "customer.query.list"). Adapters use this to resolve the
	// underlying SourceFn at boot.
	Source string

	// Columns is the declared column list, in author order. Each
	// column either projects a row field or invokes an @fn.
	Columns []Column

	// Formats is the list of writer formats the report exposes.
	// Generated routing mounts one HTTP endpoint per format.
	Formats []Format

	// Storage is the bound object-storage capability name (e.g.
	// "files"). When empty, the implicit-binding rule selects the
	// single declared capability; doctor's `REPORT-STORAGE-AMBIGUOUS-001`
	// catches the multi-cap case before runtime.
	Storage string

	// Visibility mirrors the storage file visibility contract. The
	// report runtime supports `Signed` (default) and `Public`;
	// `Private` reports still mount the endpoint but downloads
	// re-check the policy via the bound storage adapter.
	Visibility storage.FileVisibility

	// SignedTTL is the signed URL expiry duration. Non-zero only when
	// `Visibility == Signed`; the storage runtime enforces this.
	SignedTTL time.Duration

	// Filename is the lowered filename template. Empty means the
	// runtime synthesises a default (`<feature>.<name>_<ts>.<format>`).
	Filename Pattern

	// Policy is the verbatim policy atom or category ref. The
	// generated HTTP handler resolves it against the feature's
	// policies map.
	Policy string

	// RateLimit is the verbatim rate-limit literal. Empty when the
	// declaration omits the slot.
	RateLimit string
}

// Column is one entry in the report's column list. Either `RowField`
// or `Fn` is non-empty (mutually exclusive). `Label` / `Format` are
// optional adornments consumed by the writers.
type Column struct {
	Name     string
	From     ColumnSource
	Label    string
	Format   string
}

// ColumnSource is the closed two-variant column source mirror of the
// IR enum (`row.<field>` | `@fn.<name>(args)`). Generated code uses
// the constructors `RowField(name)` / `FnCall(name, args...)`.
type ColumnSource struct {
	rowField string
	fnName   string
	fnArgs   []string
}

// RowField constructs a `row.<field>` column source.
func RowField(field string) ColumnSource {
	return ColumnSource{rowField: field}
}

// FnCall constructs a `@fn.<name>(args)` column source.
func FnCall(name string, args ...string) ColumnSource {
	return ColumnSource{fnName: name, fnArgs: args}
}

// IsRowField reports whether this source projects a row field.
func (c ColumnSource) IsRowField() bool { return c.rowField != "" }

// RowFieldName returns the row field name (`""` if not a row field).
func (c ColumnSource) RowFieldName() string { return c.rowField }

// IsFnCall reports whether this source invokes an @fn.
func (c ColumnSource) IsFnCall() bool { return c.fnName != "" }

// FnName returns the @fn name (`""` if not an fn call).
func (c ColumnSource) FnName() string { return c.fnName }

// FnArgs returns the verbatim @fn argument list.
func (c ColumnSource) FnArgs() []string { return c.fnArgs }

// Format is the closed catalog of writer formats. Mirrors
// `lazuli_ir::ReportFormat`.
type Format int

const (
	// CSV writes the report via `report.CSVWriter`
	// (encoding/csv, RFC 4180, CRLF).
	CSV Format = iota
	// XLSX writes the report via `report.XLSXWriter`
	// (github.com/xuri/excelize/v2).
	XLSX
)

// Token returns the canonical lowercase identifier (`csv` / `xlsx`).
func (f Format) Token() string {
	switch f {
	case CSV:
		return "csv"
	case XLSX:
		return "xlsx"
	default:
		return "unknown"
	}
}

// FormatFromToken is the inverse of `Format.Token`. Returns `false`
// for tokens outside the closed catalog.
func FormatFromToken(token string) (Format, bool) {
	switch token {
	case "csv":
		return CSV, true
	case "xlsx":
		return XLSX, true
	default:
		return 0, false
	}
}

// Pattern is the lowered filename template. The literal is preserved
// verbatim for runtime template resolution; codegen also emits the
// parsed token list when it needs to type-check the placeholders.
type Pattern struct {
	literal string
}

// MustPattern returns a Pattern over `literal`. Generated code uses
// this; the parser already validated token shapes at lowering, so the
// runtime does not re-parse.
func MustPattern(literal string) Pattern {
	return Pattern{literal: literal}
}

// Literal returns the verbatim template string (e.g.
// `"monthly_audit_{ctx.now:yyyymm}.{format}"`).
func (p Pattern) Literal() string { return p.literal }

// IsZero reports whether the pattern is unset (default filename
// synthesis applies).
func (p Pattern) IsZero() bool { return p.literal == "" }
