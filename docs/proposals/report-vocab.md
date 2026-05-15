# Tabular Report Vocab (CSV + XLSX) — v0.2

**Status**: draft (v0.2). v0.1 graded BLOCK at 7.85/10 with 4 blockers:
(B1) two coexisting surfaces violated Rule Zero with no pilot pressure for
Surface A; (B2) `ReportColumnSource::Constant(String)` widened the column
grammar without justification; (B3) the proposal claimed to replace the
production-grade 277-LOC handler while explicitly deferring dynamic columns; (B4)
`audit ... emit_to audit_log` invented a sub-verb already covered by the
canonical `audit` block. v0.2 commits to **Surface B only**, drops the
`Constant` column source, reframes pilot grounding around the
static-columns gap, and removes the polysemous `emit_to`.

Cell #3 of the production-readiness meta-roadmap. Pairs with runtime
cells `cdx-report-csv` / `cdx-report-xlsx`
(`runtime/go/lazuli/report/{csv,xlsx}.go`) shipping in parallel.

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-14.

**Inputs**:

- `docs/proposals/production-readiness.md` (gap #3 + #4).
- `docs/proposals/bucket-storage-cycle.md` (typed `@cap.File` lowering — the
  storage primitive this proposal composes onto).
- `docs/design-principles.md` Rule Zero.
- `docs/invariants.md` (closed grammar, capability catalog, namespace catalog,
  events/locator spaces; **§93-97 canonical `audit` block syntax**).
- `examples/full-capsule/full-capsule.lzi:370-376` — `api customer_export` with
  `output @cap.File(...)` + opaque `handler "./api/export_customers.go"`. This
  is the closest expression of "an export endpoint" today; the export bytes
  are *invisible to the language*. **Primary v0.1 anchor.**
- `c:/Users/lucas/dev-trabalho/production-grade/apps/api/src/features/multi-bank/multi-bank-report.service.ts`
  — partial anchor: the *static-columns half* of this handler (header row,
  fixed columns, format dispatch, storage upload, signed URL) is in scope for
  v0.1. The *dynamic per-bank columns half* is out of scope — covered by a
  future v0.2 `expand` extension. See §Out of scope and §Risks.

## Contexto

Today, "give me this list as a file" is expressed as an `api` block with an
opaque `handler "./path.go"` whose body re-runs a query, hand-writes CSV
or XLSX bytes, uploads to storage, signs the URL, and returns the URL.
The full-capsule fixture demonstrates exactly this pattern at
`full-capsule.lzi:370-376`:

    api customer_export
      output @cap.File(...)
      handler "./api/export_customers.go"

The 50-200 LOC inside that handler is invisible to the language. Doctor
cannot check policy parity. Inspect cannot list the export among the
feature's surfaces. The columns are not declared anywhere a cold-reader
can find them; they live in `for col := range cols { writer.Write(...) }`
loops in `.go` files. LSP cannot complete `from row.<field>`. Renames
silently break exports.

Across pilots, the **dominant shape** is: a fixed column list, a fixed
filter (or none), one or two formats, upload, signed URL. That's the
static-columns case. v0.1 of this proposal **declares that shape as a
first-class artifact** — `report <name>` with explicit `columns from
row.<field> | @fn.<name>(...)`. After v0.1 lands, the `api +
opaque-handler` pattern is the *escape hatch*, not the default expression.

There is a second pressure — **dynamic columns**, where the column set is
discovered at runtime (multi-provider report: which banks does the requesting
user have credentials for? expand columns accordingly). v0.1 does **not**
cover that case; §Out of scope and §Risks describe the v0.2 path.

v0.1 of this proposal explicitly **does not extend `query.list`** with an
`export` slot. An earlier draft proposed a Surface A (`query.list ...
export csv, xlsx`) for the trivial "download the current list" case
alongside Surface B. v0.1 drops Surface A: it has no pilot pressure
(real-world anchor is a contract-export with a discrete name, audit, and
policy — not a list-export); it created a second way to say the same
thing; and doctor disambiguation machinery was needed only because the
language was offering two surfaces for one intent. **See §Surface A —
v0.2 candidate** for the gate that would unlock it later.

## Surface — `report <name>` kind

    feature customer
      report monthly_audit
        source customer.query.list
        columns
          id            from row.id
          name          from row.name
          tier          from row.tier             label "Plano"
          ltv           from @fn.lifetime_value(row.id) label "Valor de vida"
          created_at    from row.created_at       format "yyyy-mm-dd"
        formats csv, xlsx
        storage object_storage.files
        visibility signed
        signed_ttl 1h
        filename "customers_{ctx.now:yyyymmdd}.{format}"
        policy @policy.global_read
        rate_limit "10 per hour per user"
        audit actor, ctx.now, source.params

What `report` adds over `api + handler`:

- **Explicit column list**: declared at compile time, doctor cross-checks
  every `from row.<field>` against the source query's projection. Labels
  and formats are first-class.
- **Source decoupling**: `source customer.query.list` reuses a query but
  the report is its own contract — it may carry stricter policy, its own
  rate limit, its own audit.
- **Multi-format from one source**: `formats csv, xlsx` produces both
  endpoints under one declaration.
- **Audit block**: uses the canonical `audit` block from
  `docs/invariants.md:93-97`, same as `command` / `query` / `job` /
  `webhook`. No new sub-verb.

What `report` deliberately does NOT add:

- **Dynamic columns** — runtime-discovered column sets (production-grade
  multi-bank). v0.2 candidate; see §Out of scope.
- **Scheduling / recurrence** — use `job trigger schedule` for nightly
  runs.
- **Email delivery** — use `notification` + plugin adapter; report
  returns a signed URL, delivery is downstream.

### Surface A — v0.2 candidate (deferred)

An earlier draft considered `query.list <name> ... export csv, xlsx` as a
terse path for "download the current list with the list's columns,
filter, and policy". v0.1 **does not ship Surface A**.

**Gate to revisit**: 2+ independent pilots express download-this-list
pressure where (a) the export columns must equal the list projection,
(b) the export filter must equal the list filter, (c) the export policy
must equal the list policy. Until then, `report <name> source <q>` is the
single surface; the cost is ~6 extra lines per export, which is
acceptable given there's only one way to say it.

If the gate trips, Surface A re-enters as a follow-up proposal. It will
**not** coexist with Surface B without a doctor lint disambiguating
duplicates — and the design will revisit whether B can be re-derived
from A with column overrides instead of being a parallel surface.

## Boundary discipline

What stays in Lazuli core: grammar (`report <name>` kind only), IR shape,
inspect projection, doctor diagnostics, codegen. Closed catalog: `formats`
is `csv` or `xlsx`. JSON / Parquet / Avro / PDF are **not v0**.

What stays in `runtime/go/lazuli/report/` (Lazuli Go): `report.CSVWriter`
/ `report.XLSXWriter` — thin wrappers over `encoding/csv` (stdlib) and
`github.com/xuri/excelize/v2`. Per wire-thin, each writer is ~30-80 LOC
of import + adapt; no homegrown CSV/XLSX parser. `report.Pipeline`
streams `pgx.Rows` through the writer, pushes bytes to the bound
`storage.ObjectStore`, returns a signed URL. Adapters under
`runtime/go/lazuli/storage/` already handle signing.

What stays in adapters: per-vendor SaaS delivery (Sendgrid attachment,
SES batch, Slack file upload). The `report` contract returns a signed
URL; delivery is `notification` + plugin adapter responsibility.

**Explicitly not in scope**: PDF (different rendering paradigm);
server-side aggregation/pivoting (use `query.sql` upstream); dynamic
column expansion (v0.2 candidate); email delivery / cron scheduling
(reuse `notification` + `job trigger schedule`).

## Linguagem (Stage 3)

### `report <name>` grammar

```ebnf
report_decl        = "report" IDENT NEWLINE INDENT
                       report_source
                       report_columns
                       report_formats
                       [ report_storage ]
                       [ report_signed_ttl ]
                       [ report_visibility ]
                       [ report_filename ]
                       [ report_policy ]
                       [ report_rate_limit ]
                       [ report_audit ]
                     DEDENT ;

report_source      = "source" qualified_query_ref ;
report_columns     = "columns" NEWLINE INDENT
                       report_column+
                     DEDENT ;

report_column      = IDENT
                       [ "from" column_source ]
                       [ "label" string_literal ]
                       [ "format" string_literal ]
                     NEWLINE ;

column_source      = "row" "." IDENT
                   | fn_call ;

report_audit       = audit_block ;  (* canonical audit block,
                                       docs/invariants.md:93-97 *)
```

`column_source` is intentionally narrow: only `row.<field>` (project a
field from the source query's record) or `@fn.<name>(args)` (call a
user-defined or capability function). String literals as column values
are not in the grammar — there is no pilot evidence for a "constant
column" pattern; if one arises, it can be expressed via `@fn.const_xyz()`
without widening the surface.

Filename interpolation is closed: `{format}`, `{ctx.now:<strftime>}`
(only `yyyy`, `mm`, `dd`, `HH`, `MM`, `ss` tokens), `{ctx.user.id}`,
`{ctx.tenant.id}`. Any other token is `REPORT-FILENAME-TOKEN-UNKNOWN-001`.

### Slot inventory

| Slot | Required | Type | Closed catalog | Notes |
|---|---|---|---|---|
| `source <ref>` | yes | qualified query ref | must resolve to `query.list` or `query.sql` in same package | `query.lookup` rejected by `REPORT-SOURCE-KIND-001` |
| `columns <list>` | yes | indent block of `<name> from <expr>` | at least one column | empty block is `REPORT-COLUMNS-EMPTY-001` |
| `formats <ids>` | yes | comma-list of format ids | **closed**: `csv`, `xlsx` | future via catalog |
| `storage <ref>` | conditional | capability ref | must resolve in `registry.lzi` | **default rule**: if the package declares **exactly one** `object_storage` capability, omit and it binds to that. If **zero or two-plus** capabilities exist and `storage` is omitted, doctor emits `REPORT-STORAGE-AMBIGUOUS-001`. The previous "first declared" tiebreak is rejected — order-dependent. |
| `signed_ttl <duration>` | required when `visibility = signed` | duration literal `s|m|h|d` | mirrors `@cap.File(signed_ttl)` | doctor refuses pairing with `public` / `private` |
| `visibility <mode>` | no, defaults to `signed` | identifier | **closed**: `public`, `private`, `signed` | reports are downloadable artifacts |
| `filename <pattern>` | no | template string | closed token catalog | default: `<feature>.<report_name>_<ts>.<format>` |
| `policy @policy.<name>` | yes | policy ref | must resolve in feature `policies` | reports always declare policy explicitly |
| `rate_limit <expr>` | required when policy includes `@scope.public` | rate-limit literal | mirrors command rate_limit | enforced by `REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001` |
| `audit <fields>` | optional | canonical audit block | same closed catalog as command audit (`docs/invariants.md:93-97`) | `audit`, `audit <field>, <field>`, or `audit none`. **No `emit_to` sub-verb** — audit-log routing is fixed by the audit catalog. |

### Worked example

Today, `examples/full-capsule/full-capsule.lzi:370-376` declares a
file-output api with an opaque handler. v0.1 replaces the handler with a
declared report:

    report monthly_audit
      source customer.query.list
      columns
        id            from row.id
        name          from row.name
        tier          from row.tier              label "Plano"
        ltv           from @fn.lifetime_value(row.id) label "Valor de vida"
        created_at    from row.created_at        format "yyyy-mm-dd"
      formats csv, xlsx
      storage object_storage.files
      visibility signed
      signed_ttl 1h
      filename "monthly_audit_{ctx.now:yyyymm}.{format}"
      policy @policy.global_read
      rate_limit "10 per hour per user"
      audit actor, ctx.now, source.params

**Note on the real-world anchor**: this worked example covers the
static-columns case — a fixed column list, format dispatch, storage
upload, signed URL. The multi-provider report-report's
*dynamic per-bank columns* (each user's set of authorized banks expands
into per-bank columns at request time) is **not expressible in v0.1**.
That handler stays in the production-grade pilot as an `api + handler` until a
v0.2 `expand` modifier ships (see §Open questions). v0.1 covers the
static half of production-grade exports (header generation, format dispatch,
upload, signing) and the entirety of full-capsule's export.

## IR (Stage 4)

Additive IR types under `crates/lazuli_ir/src/lib.rs`:

```rust
pub struct Report {
    pub name: String,
    pub source: ReportSource,
    pub columns: Vec<ReportColumn>,
    pub formats: Vec<ReportFormat>,
    pub storage: Option<QualifiedName>,
    pub visibility: FileVisibility,
    pub signed_ttl: Option<Duration>,
    pub filename: Option<ReportFilenamePattern>,
    pub policy: PolicyRef,
    pub rate_limit: Option<RateLimit>,
    pub audit: Option<AuditBlock>,
    pub origin: Origin,
}

pub enum ReportSource {
    Query(QualifiedName),
}

pub struct ReportColumn {
    pub name: String,
    pub source: ReportColumnSource,
    pub label: Option<String>,
    pub format: Option<String>,
    pub origin: Origin,
}

pub enum ReportColumnSource {
    RowField(String),
    Fn(FnInvocation),
}

pub enum ReportFormat { Csv, Xlsx }

pub struct ReportFilenamePattern {
    pub literal: String,
    pub tokens: Vec<FilenameToken>,
}

pub enum FilenameToken {
    Format,
    CtxNowStrftime(String),
    CtxUserId,
    CtxTenantId,
}
```

`ReportColumnSource` has exactly two variants. The earlier
`Constant(String)` variant is removed: it had no grammar production (no
`column from "literal"` rule) and would have widened the IR without a
corresponding language surface. Constants, if ever needed, route through
`@fn.<name>()`.

There is no `QueryExport` IR type. Surface A is not in v0.1; its IR is a
v0.2 concern and will be designed at that time.

### Inspect JSON shape

New `--expand=reports` flag in `ExpandSet`. Projection emits `reports[]`
under each feature.

### Cross-refs registered by analyzer

| Edge | Source | Target | Resolution |
|---|---|---|---|
| `report.source` | `report <name>` | `query.list` or `query.sql` in same package | feature-local + cross-feature refs; `query.lookup` rejected by `REPORT-SOURCE-KIND-001` |
| `report.column.source.row_field` | each `from row.<field>` | field on `report.source` projection | analyzer-time `REPORT-COLUMN-MISMATCH-001` |
| `report.column.source.fn` | each `from @fn.<name>(...)` | `@fn.*` declaration | reuses `fn_unresolved` analyzer |
| `report.storage` | `storage <ref>` (or implicit single-cap binding) | capability in `registry.lzi` | reuses `APP-CAP-001`; `REPORT-STORAGE-AMBIGUOUS-001` on count ≠ 1 without explicit `storage` |
| `report.visibility = signed` | `visibility signed` | requires `signed_ttl` | `REPORT-SIGNED-TTL-MISSING-001` |
| `report.policy public + rate_limit missing` | `policy` includes `@scope.public`, no `rate_limit` | doctor cross-check | `REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001` |
| `report.policy` | `policy @policy.<name>` | feature `policies` | reuses command policy resolution |

## Codegen (Stage 5)

One generated file per feature carrying any report.

### `dist/go/customer/reports.gen.go`

```go
package customer

import (
    "github.com/lazuli-lang/runtime/go/lazuli"
    "github.com/lazuli-lang/runtime/go/lazuli/report"
    "github.com/lazuli-lang/runtime/go/lazuli/storage"
)

var MonthlyAuditReport = report.Contract{
    Feature:    "customer",
    Name:       "monthly_audit",
    Source:     "customer.query.list",
    Columns: []report.Column{
        {Name: "id",         From: report.RowField("id")},
        {Name: "name",       From: report.RowField("name")},
        {Name: "tier",       From: report.RowField("tier"), Label: "Plano"},
        {Name: "ltv",        From: report.FnCall("lifetime_value", "row.id"), Label: "Valor de vida"},
        {Name: "created_at", From: report.RowField("created_at"), Format: "yyyy-mm-dd"},
    },
    Formats:   []report.Format{report.CSV, report.XLSX},
    Storage:   "files",
    Visibility: storage.VisibilitySigned,
    SignedTTL: 1 * lazuli.Hour,
    Filename:  report.MustPattern("monthly_audit_{ctx.now:yyyymm}.{format}"),
    Policy:    "@policy.global_read",
    RateLimit: lazuli.MustRateLimit("10 per hour per user"),
}

func RunMonthlyAudit(ctx *lazuli.Ctx, format report.Format) (string, error) {
    return report.Run(ctx, MonthlyAuditReport, format, CustomerList)
}
```

**Wire-thin discipline**: ~30 LOC of import-and-declare. Bytes-level work
(pipe `pgx.Rows` into the writer, push to storage, return signed URL)
lives in `report.Run` inside the runtime library. **No** CSV/XLSX
byte-level code is emitted by codegen.

`report.Column.From` exposes exactly two constructors —
`report.RowField` and `report.FnCall` — mirroring the IR enum.

## Runtime (Stage 6)

Three new files under `runtime/go/lazuli/report/` (writers shipped via
Codex cells in parallel; pipeline wired by orchestrator).

### `runtime/go/lazuli/report/contract.go`

Typed `Contract`, `Column`, `Format`, `RowField`, `FnCall`, `Pattern`.
Pure types consumed by generated code and the writers. No `Constant`
column constructor.

### `runtime/go/lazuli/report/csv.go` (Codex cell `cdx-report-csv`)

Capability: `CSVWriter` thin wrapper over `encoding/csv` (stdlib).
External lib: stdlib only. Streaming: `CSVWriter.WriteRow(values []any)
error` never buffers the full result.

### `runtime/go/lazuli/report/xlsx.go` (Codex cell `cdx-report-xlsx`)

Capability: `XLSXWriter` thin wrapper over `github.com/xuri/excelize/v2`.
Wire-thin per the founding principle. Streaming: `excelize/v2` exposes
`StreamWriter` for >100k rows; `XLSXWriter` auto-selects streaming when
row count exceeds 10k (configurable).

### `runtime/go/lazuli/report/pipeline.go` (orchestrator-written)

Capability: `Run(ctx, contract, format, source SourceFn) (signedURL string, err error)`.
Per-dispatch lifecycle. Opens a `pgx.Rows` cursor via `SourceFn`, selects
writer (CSV or XLSX), iterates rows, applies `Column.From` resolution,
writes to a streaming buffer, uploads to `storage.ObjectStore`, calls
`storage.SignedURL`. Reads `contract.Storage`, `contract.Visibility`,
`contract.SignedTTL` — all wired through the existing storage runtime;
no parallel signing path. Typed errors map to HTTP codes.

### Streaming guarantee

Rows flow from `pgx.Rows` through the writer's `WriteRow` into the
storage upload stream without materialising the full result set in
memory. The codegen-emitted `SourceFn` returns a cursor, not a slice.
Runtime invariant — not enforced by doctor/LSP.

### What the runtime does NOT do (wire-thin checks)

- No CSV escaping (stdlib does that).
- No XLSX zip-packaging, cell formatting, column auto-width (excelize does).
- No signed-URL HMAC (storage runtime does).
- No vendor names (Sendgrid, S3, GCS) referenced.

## Evals / Testes (Stage 7)

### Doctor fixtures

`crates/lazuli_cli/tests/fixtures/report/` — one minimal fixture per
diagnostic: `column_mismatch.lzi`, `signed_no_storage.lzi`,
`format_unknown.lzi`, `filename_token_unknown.lzi`,
`signed_ttl_missing.lzi`, `signed_ttl_forbidden.lzi`, `columns_empty.lzi`,
`source_kind_lookup.lzi`, `storage_ambiguous.lzi`,
`policy_public_no_rate_limit.lzi`.

### Go integration test — round-trip via local adapter

`runtime/go/lazuli/report/report_test.go`:

1. Bind `object_storage files` to `@runtime/local` (filesystem).
2. Seed 1000 fake Customer rows into a pgx-mock cursor.
3. Call `report.Run` with a small contract (3 columns, CSV format).
4. Assert the returned signed URL is reachable.
5. Read bytes back; assert CSV header matches `contract.Columns` and row
   count == 1000.
6. Repeat with XLSX; open the workbook via excelize and assert first
   sheet has 1001 rows.

### LSP test — completion + hover on report children

`crates/lazuli_lsp/tests/report.rs`:

- Hover on `formats` shows closed catalog (`csv`, `xlsx`).
- Hover on `visibility` shows `public | private | signed`.
- Completion at `formats |` offers `csv`, `xlsx`.
- Completion at `source |` offers all `query.list` / `query.sql`
  declarations (no `query.lookup`).
- Completion at `from row.|` offers fields of the resolved source.

### Inspect contract test

`crates/lazuli_cli/tests/inspect_reports.rs`: runs `lazuli inspect
--format=json --expand=reports examples/full-capsule` and asserts the
projection.

## Doctor / LSP (Stage 8)

### Diagnostic table

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `REPORT-COLUMN-MISMATCH-001` | error | report `<name>` column `<column>` references `row.<field>` which is not in source `<source-ref>`'s projection. | analyzer-time | `column_mismatch.lzi` |
| `REPORT-SIGNED-NO-STORAGE-001` | error | report `<name>` declares `visibility:signed` but package has no `object_storage` capability. | any signed report without storage cap | `signed_no_storage.lzi` |
| `REPORT-SIGNED-TTL-MISSING-001` | error | report `<name>` has `visibility:signed` but no `signed_ttl`. | `visibility = Signed` + `signed_ttl = None` | `signed_ttl_missing.lzi` |
| `REPORT-SIGNED-TTL-FORBIDDEN-001` | error | report `<name>` has `signed_ttl` but `visibility:<public|private>`. | `visibility != Signed` + `signed_ttl = Some(_)` | `signed_ttl_forbidden.lzi` |
| `REPORT-FORMAT-UNKNOWN-001` | error | report `<name>` declares `formats <id>` outside `{csv, xlsx}`. | parsed format outside closed catalog | `format_unknown.lzi` |
| `REPORT-FILENAME-TOKEN-UNKNOWN-001` | error | report `<name>` filename pattern uses `{<token>}` which is not recognised. | unknown token | `filename_token_unknown.lzi` |
| `REPORT-COLUMNS-EMPTY-001` | error | report `<name>` declares no columns. | parsed `columns` block has 0 entries | `columns_empty.lzi` |
| `REPORT-SOURCE-KIND-001` | error | report `<name>` source `<ref>` resolves to `query.lookup`; only `query.list` and `query.sql` may be report sources. | analyzer-time kind check | `source_kind_lookup.lzi` |
| `REPORT-STORAGE-AMBIGUOUS-001` | error | report `<name>` omits `storage` and package declares N ≠ 1 `object_storage` capabilities; declare `storage <ref>` explicitly. | analyzer-time cap count | `storage_ambiguous.lzi` |
| `REPORT-POLICY-PUBLIC-NO-RATE-LIMIT-001` | error | report `<name>` policy includes `@scope.public` but no `rate_limit` declared. | analyzer-time policy scan | `policy_public_no_rate_limit.lzi` |

All codes register under `is_security_enforcement_code` where they
involve auth/quota; the rest under generic doctor catalog.

### LSP hovers

Add to `KEYWORD_HOVER`: `report`, `formats`, `columns`, `source` (under
report), `filename`. The `audit` keyword reuses the existing command
audit hover.

Closed-catalog completions: `formats <|` → `csv`, `xlsx`; `visibility <|`
→ `public`, `private`, `signed`; `signed_ttl <int>` → `s`, `m`, `h`,
`d`; filename token completions.

### Namespaces

No new namespace. `report` is a new top-level kind alongside `command`,
`query`, `job`, `webhook`, `agent`, `notification`, `api`.

### Highlighting

Add `report` to keyword scope; report-child keywords (`columns`, `from`,
`label`, `source`, `filename`, `signed_ttl`) use existing argument scope.

## Critério de "ciclo fechado"

- [ ] Fixture exercises `report` in
  `examples/full-capsule/full-capsule.lzi`, replacing the opaque `api
  customer_export` handler.
- [ ] `lazuli check examples/full-capsule` accepts the syntax with no
  diagnostics.
- [ ] `lazuli inspect --format=json --expand=reports examples/full-capsule`
  emits the projection.
- [ ] `lazuli doctor` emits the 10 named diagnostics on matching fixtures.
- [ ] `lazuli generate` produces `dist/go/customer/reports.gen.go`.
- [ ] `runtime/go/lazuli/report/report_test.go` round-trip passes for
  CSV (1k rows) and XLSX (1k rows via excelize streaming).
- [ ] LSP hovers + completion cover 5 new keywords + 4 closed catalogs.
- [ ] `runtime/go/lazuli/report/` total LOC < 250 across
  `contract.go`, `csv.go`, `xlsx.go`, `pipeline.go`.

## Risks and open questions

### Biggest risk

**Dynamic columns are the real-world anchor's defining trait, and v0.1
explicitly does not cover them.** v0.1 covers the static-columns half —
header rows, fixed column lists, format dispatch, upload, signing —
which is the dominant shape across pilots and the entirety of the
full-capsule fixture. The dynamic-columns half (per-bank column
expansion at request time) remains an `api + handler` until v0.2 ships
an `expand` modifier. Mitigation: §Open questions sketches two candidate
v0.2 shapes; pilot pressure beyond production-grade will resolve which one. v0.1
is **not** marketed as "replaces the production-grade 277-LOC handler" — it
establishes the surface and replaces the static-columns scaffold inside
that handler (50-100 LOC of header generation, format dispatch, upload,
signing — the part least worth hand-rolling). The dynamic-columns
business logic is what makes the production-grade handler 277 LOC; that piece
survives v0.1 untouched. v0.2 dynamic-columns will fully replace the
production-grade-shape handler.

### Biggest open question — dynamic columns (v0.2)

The multi-provider report case: the column set is determined at runtime by
which banks the requesting user has credentials for. Two candidate v0.2
shapes:

    columns
      cpf       from row.cpf
      banks     expand from @fn.discover_banks(ctx.user) as bank
        margem  from row.bank_data[bank].margem    label "Margem {bank}"
        status  from row.bank_data[bank].status    label "Status {bank}"

vs:

    columns
      cpf  from row.cpf
      + dynamic from @fn.bank_columns(ctx.user)   ! returns []report.Column

The first is more declarative but extends the column-source grammar with
nested `expand` blocks; the second leans on `@fn.*` returning a slice
and is a smaller language change. Decision deferred until a second site
beyond production-grade demands it.

### Smaller open questions

- **Surface A revisit** — terse `query.list ... export csv, xlsx`
  shorthand. Gated on 2+ independent pilots expressing
  download-this-list pressure (see §Surface A). If shipped, it must
  re-derive the report contract from the list, not duplicate the
  surface.
- **`audit` semantics on `report`** — v0.1 reuses the canonical `audit`
  block from `docs/invariants.md:93-97`. No new sub-verb is introduced
  in v0.1; audit-log routing is fixed by the existing command audit
  catalog. If reports surface a need for a distinct audit channel,
  that's a framework-wide change to the audit catalog, not a
  `report`-local one.
- **Format catalog** — `format "yyyy-mm-dd"` / `format "currency:BRL"`.
  v0.1 is **closed**: `yyyy-mm-dd`, `yyyy-mm-dd HH:MM:SS`,
  `currency:BRL|USD|EUR`, `percent`, `integer`, `boolean:yes|no`. New
  entries require pilot evidence.
- **Auto-mount HTTP endpoints** (`GET /api/reports/<name>.csv`) vs
  explicit `api`-style block? v0.1: **auto-mount**.
- **`audit` block required by default?** v0.1: optional; strict profile
  may upgrade to "required when `@scope.*` other than `@role.admin`"
  once pilot evidence shows the bar.

## Próximo passo

1. Grade this proposal v0.2 with `lazuli-language-architect`. Target ≥
   8.5; gate at ≥ 8.5 with no individual criterion < 7.
2. Apply remaining blockers if any; ship v0.3 if necessary.
3. After PASS:
   - Codex cells `cdx-report-csv` and `cdx-report-xlsx` shipped the
     writers (commit `5e2138e`).
   - Orchestrator wires `runtime/go/lazuli/report/pipeline.go`.
   - Implementation cells for `crates/lazuli_ir` additive types,
     `crates/lazuli_analyzer` lowering, `crates/lazuli_lsp` diagnostics,
     `crates/lazuli_cli/src/doctor.rs` cross-feature checks,
     `crates/lazuli_codegen_go` emission.
   - Fixture extension: rewrite `examples/full-capsule/full-capsule.lzi:370-376`
     from `api customer_export` to `report monthly_audit`.

## Rows sugeridas para `docs/next-checklist.md`

```
| 32 | Report vocab — `report <name>` kind | planned | Add `Report`, `ReportColumn`, `ReportSource`, `ReportFormat` to IR. Extend grammar with `report <name>` kind. New `--expand=reports` projection. |
| 33 | Report vocab — 10 doctor diagnostics + LSP coverage | planned | 10 codes + LSP hovers for 5 keywords + closed-catalog completions. Depends on row 32. |
| 34 | Report runtime — pipeline.go + CSV/XLSX writers | shipped (writers) | Writers shipped at commit 5e2138e. Pipeline + contract.go pending. Wire-thin: total < 250 LOC. Depends on row 32 + bucket-storage cycle. |
```
