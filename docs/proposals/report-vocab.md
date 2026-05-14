# Tabular Report Vocab (CSV + XLSX) — v0.1

**Status**: draft (v0.1, ungraded). Cell #3 of the corbanx-class-readiness
meta-roadmap. Pairs with runtime cells `cdx-report-csv` / `cdx-report-xlsx`
(`runtime/go/lazuli/report/{csv,xlsx}.go`) shipping in parallel.

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-14.

**Inputs**:

- `docs/proposals/corbanx-class-readiness.md` (gap #3 + #4).
- `docs/proposals/bucket-storage-cycle.md` (typed `@cap.File` lowering — the
  storage primitive this proposal composes onto).
- `docs/design-principles.md` Rule Zero.
- `docs/invariants.md` (closed grammar, capability catalog, namespace catalog,
  events/locator spaces).
- `examples/full-capsule/full-capsule.lzi:370-376` — `api customer_export` with
  `output @cap.File(...)` + opaque `handler "./api/export_customers.go"`. This
  is the closest expression of "an export endpoint" today; the export bytes
  are *invisible to the language*.
- `c:/Users/lucas/dev-trabalho/corbanx/apps/api/src/features/multi-bank/multi-bank-report.service.ts`
  — real-world anchor: dynamic per-bank columns, date-range filter,
  `format: csv|xlsx`, upload to storage, return signed URL. 277 LOC of hand-rolled
  ExcelJS + CSV string building + storage glue.

## Contexto

Two distinct shapes show up under the banner "tabular export":

1. **One-off downloads** — a list query that should also be downloadable
   "as CSV". Same projection as the on-screen list, same filter, same policy.
   Today authors duplicate the query into an `api` with an opaque handler.
   That handler re-runs the query, re-checks policy by hand, writes CSV
   bytes, uploads, signs. Nothing in the language sees the columns; nothing
   in doctor checks the policy parity; nothing in inspect lists the export.
2. **Genuine reports** — multi-format artifacts with their own header,
   own column definitions (often a *superset* or *subset* of the source
   record), a date-range or scope filter that is **not** the list filter,
   and downstream needs (scheduling, recurrence, retention). The corbanx
   `multi-bank-report` is the canonical example: 1 source query → 2
   formats (`.csv` + `.xlsx`) → uploaded to storage → signed URL → emailed
   or returned inline → columns vary per *run* depending on which banks
   the user has credentials for.

Both shapes share a runtime mechanism: stream rows through a writer, push
bytes to `object_storage`, return a signed URL. They diverge in *intent*:
the first reuses the query's contract; the second declares a new contract.

Folding both into one keyword loses information. Folding neither produces
a `kind report` that nobody uses for the easy case.

This proposal **recommends a split**: extend `query.list` with an `export`
slot for the one-off case (Surface A), and introduce a new `kind report`
for the report-as-contract case (Surface B). The two coexist; doctor
disambiguates; codegen and runtime share the writer machinery.

## Surface candidates

### Surface A — `query.list ... export csv,xlsx`

    query.list list
      params
        status: CustomerStatus optional
        tier: CustomerTier optional
      filters
        status when params.status
        tier when params.tier
      paginate 50
      export
        formats csv, xlsx
        storage object_storage.files
        signed_ttl 1h
        filename "customers_{ctx.now:yyyymmdd}.{format}"

Generates `GET /api/customers.csv` and `GET /api/customers.xlsx` alongside
the JSON `GET /api/customers`. The export inherits:

- `params` (same filters as the list).
- `policy` (the query's effective policy from the feature `policies` table).
- `scope` (tenant + soft-delete).
- The list projection (the columns are the resource record fields the
  query naturally returns — same shape the JSON endpoint emits).

What it cannot express:

- A separate set of columns from the on-screen list.
- A header row in pt-BR while keys are in English.
- A column that's *derived* per row at export time but not part of the
  resource (`row.banks[bank].margem` style).
- Scheduling (run nightly, deliver to S3).
- A range filter that's only meaningful at export time (not on the list
  page).

**Pros**: terse — three new lines on an existing block; zero new IR kinds,
zero new namespaces; most "download as CSV" buttons in real apps map 1:1.

**Cons**: polysemy creep on `query`; "just one more thing" is how queries
become unreadable; cannot express the corbanx multi-bank report (dynamic
columns).

### Surface B — `report <name>` kind

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
          emit_to audit_log

What it adds:

- **Explicit column list**: declared at compile time, doctor cross-checks
  every `from row.<field>` against the source query's projection. Labels,
  formats, and per-cell transforms are first-class.
- **Source decoupling**: `source customer.query.list` reuses a query but
  the report is its own contract — it may carry stricter policy, its own
  rate limit, its own audit.
- **Multi-format from one source**: `formats csv, xlsx` produces both
  endpoints under one declaration.
- **Surface for the dynamic case (deferred — see §Out of scope below)**:
  one column declaration can stand for a runtime-discovered set of
  columns via an `expand` modifier we *do not* ship in v0.1.

**Pros**: clean separation of concerns; cold-readable; reasonable home
for scheduling/recurrence if/when pressure arrives.

**Cons**: another `kind` to learn; for "download this list" the
boilerplate ratio is bad.

### Recommended split — both coexist

| Pressure | Surface | Why |
|---|---|---|
| "Download the current list as CSV" — same columns, same filter, same policy. | **A** (`query.list ... export`) | One block, zero ceremony. The list *is* the export. |
| "Monthly audit report with custom columns / labels / formats" — separate contract, separate policy, often multiple formats. | **B** (`report <name>`) | A report is its own product surface, not a view of a list. |
| "Per-bank column expansion at runtime" (corbanx multi-bank). | **B** (`report ... columns expand`) | **Deferred** out of v0.1. v0.1 says: declare static columns; for runtime-dynamic columns, fall through to `handler "./..."` with a documented escape hatch. |

Doctor refuses overlap: a `query.list` carrying `export` and a `report`
whose `source` points at the same `query.list` is a
`REPORT-EXPORT-DUPLICATE-001` warning — pick one.

## Boundary discipline

What stays in Lazuli core: grammar (the two surfaces above), IR shape,
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

### Surface A — extension to `query.list`

```ebnf
query_list_child   = ... | export_block ;

export_block       = "export" NEWLINE INDENT
                       export_formats
                       [ export_storage ]
                       [ export_signed_ttl ]
                       [ export_visibility ]
                       [ export_filename ]
                     DEDENT ;

export_formats     = "formats" format_list ;
format_list        = format_id ( "," format_id )* ;
format_id          = "csv" | "xlsx" ;

export_storage     = "storage" capability_ref ;
export_signed_ttl  = "signed_ttl" duration_literal ;
export_visibility  = "visibility" file_visibility ;
export_filename    = "filename" string_literal ;
```

Filename interpolation is closed: `{format}`, `{ctx.now:<strftime>}`
(only `yyyy`, `mm`, `dd`, `HH`, `MM`, `ss` tokens), `{ctx.user.id}`,
`{ctx.tenant.id}`. Any other token is `REPORT-FILENAME-TOKEN-UNKNOWN-001`.

### Surface B — `report <name>` kind

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
                   | fn_call
                   | string_literal ;
```

### Slot inventory (Surface B)

| Slot | Required | Type | Closed catalog | Notes |
|---|---|---|---|---|
| `source <ref>` | yes | qualified query ref | must resolve to `query.list` or `query.sql` in same package | drives row cursor |
| `columns <list>` | yes | indent block of `<name> from <expr>` | at least one column | empty block is `REPORT-COLUMNS-EMPTY-001` |
| `formats <ids>` | yes | comma-list of format ids | **closed**: `csv`, `xlsx` | future via catalog |
| `storage <ref>` | no (defaults to feature's first `object_storage` cap) | capability ref | must resolve in `registry.lzi` | reuses `APP-CAP-001` |
| `signed_ttl <duration>` | optional unless `visibility:signed` | duration literal `s|m|h|d` | mirrors `@cap.File(signed_ttl)` | doctor refuses pairing with public/private |
| `visibility <mode>` | no, defaults to `signed` | identifier | **closed**: `public`, `private`, `signed` | reports are downloadable artifacts |
| `filename <pattern>` | no | template string | closed token catalog | default: `<feature>.<report_name>_<ts>.<format>` |
| `policy @policy.<name>` | yes | policy ref | must resolve in feature `policies` | reports always declare policy explicitly |
| `rate_limit <expr>` | required when policy includes `@scope.public` | rate-limit literal | mirrors command rate_limit | |
| `audit <fields>` | optional | audit fields | same closed catalog as command audit | reports are first-class audit subjects |

### Worked example

Today, `examples/full-capsule/full-capsule.lzi:370-376` declares a
file-output api with an opaque handler. Stage 3 replaces the handler.

Surface A (list export):

    query.list list
      ...
      export
        formats csv, xlsx
        storage object_storage.files
        visibility signed
        signed_ttl 1h
        filename "customers_{ctx.now:yyyymmdd}.{format}"

Surface B (separate audit report):

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
        emit_to audit_log

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
    Constant(String),
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

For Surface A, the `query.list` IR gets an optional `export` field
carrying a reduced `Report`-shaped struct (no `columns` — derived from
the list projection):

```rust
pub struct QueryExport {
    pub formats: Vec<ReportFormat>,
    pub storage: Option<QualifiedName>,
    pub visibility: FileVisibility,
    pub signed_ttl: Option<Duration>,
    pub filename: Option<ReportFilenamePattern>,
    pub origin: Origin,
}
```

### Inspect JSON shape

New `--expand=reports` flag in `ExpandSet`. Projection emits `reports[]`
+ `query_exports[]` under each feature.

### Cross-refs registered by analyzer

| Edge | Source | Target | Resolution |
|---|---|---|---|
| `report.source` | `report <name>` | `query.list` or `query.sql` in same package | feature-local + cross-feature refs |
| `report.column.source.row_field` | each `from row.<field>` | field on `report.source` projection | analyzer-time `REPORT-COLUMN-MISMATCH-001` |
| `report.column.source.fn` | each `from @fn.<name>(...)` | `@fn.*` declaration | reuses `fn_unresolved` analyzer |
| `report.storage` | `storage <ref>` | capability in `registry.lzi` | reuses `APP-CAP-001` |
| `report.visibility = signed` | `visibility signed` | requires `signed_ttl` | `REPORT-SIGNED-TTL-MISSING-001` |
| `query_export` | `query.list X ... export` | requires `object_storage` capability | `REPORT-SIGNED-NO-STORAGE-001` |
| `report.policy` | `policy @policy.<name>` | feature `policies` | reuses command policy resolution |

## Codegen (Stage 5)

One generated file per feature carrying any report or query export.

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

For Surface A, the generated wiring lives in `dist/go/customer/list.gen.go`
alongside the existing list query and reuses the same `report.Run`
entrypoint — `CustomerList` is the existing generated list-query function.

## Runtime (Stage 6)

Three new files under `runtime/go/lazuli/report/` (writers shipped via
Codex cells in parallel; pipeline wired by orchestrator).

### `runtime/go/lazuli/report/contract.go`

Typed `Contract`, `Column`, `Format`, `RowField`, `FnCall`, `Constant`,
`Pattern`. Pure types consumed by generated code and the writers.

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
`signed_ttl_missing.lzi`, `signed_ttl_forbidden.lzi`,
`export_duplicate.lzi`, `columns_empty.lzi`.

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
  declarations.
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
| `REPORT-EXPORT-DUPLICATE-001` | warning | `query.list <q>` declares `export {...}` and `report <r>` references `<q>` as source; pick one. | analyzer-time overlap | `export_duplicate.lzi` |
| `REPORT-COLUMNS-EMPTY-001` | error | report `<name>` declares no columns. | parsed `columns` block has 0 entries | `columns_empty.lzi` |

All codes register under `is_security_enforcement_code`.

### LSP hovers

Add to `KEYWORD_HOVER`: `report`, `formats`, `columns`, `source` (under
report), `export` (under query.list), `filename`.

Closed-catalog completions: `formats <|` → `csv`, `xlsx`; `visibility <|`
→ `public`, `private`, `signed`; `signed_ttl <int>` → `s`, `m`, `h`,
`d`; filename token completions.

### Namespaces

No new namespace. `report` is a new top-level kind alongside `command`,
`query`, `job`, `webhook`, `agent`, `notification`, `api`.

### Highlighting

Add `report` to keyword scope; report-child keywords (`columns`, `from`,
`label`, `source`, `export`, `filename`, `signed_ttl`) use existing
argument scope.

## Critério de "ciclo fechado"

- [ ] Fixture exercises both Surface A and Surface B in
  `examples/full-capsule/full-capsule.lzi`, replacing the opaque `api
  customer_export` handler.
- [ ] `lazuli check examples/full-capsule` accepts the syntax with no
  diagnostics.
- [ ] `lazuli inspect --format=json --expand=reports examples/full-capsule`
  emits the projection.
- [ ] `lazuli doctor` emits the 8 named diagnostics on matching fixtures.
- [ ] `lazuli generate` produces `dist/go/customer/reports.gen.go` and
  updated `list.gen.go`.
- [ ] `runtime/go/lazuli/report/report_test.go` round-trip passes for
  CSV (1k rows) and XLSX (1k rows via excelize streaming).
- [ ] LSP hovers + completion cover 6 new keywords + 4 closed catalogs.
- [ ] `runtime/go/lazuli/report/` total LOC < 250 across
  `contract.go`, `csv.go`, `xlsx.go`, `pipeline.go`.

## Risks and open questions

### Biggest risk

**Surface A + Surface B coexistence creates two ways to express
"download this list as CSV"**, and Lazuli's Rule Zero says one way to
say each thing. Mitigation: `REPORT-EXPORT-DUPLICATE-001` doctor warning
when the same query is both `export`-extended and the source of a
report. If the lint is not shippable on day one, recommend shipping
only Surface B and treating `query.list ... export` as a v0.2 addition.

### Biggest open question

**Dynamic columns** — the corbanx multi-bank case where the column set
is determined at runtime by which banks the requesting user has
credentials for. v0.1 deliberately does **not** support this; it falls
through to a handler. Two candidate v0.2 shapes:

    columns
      cpf       from row.cpf
      banks     expand from @fn.discover_banks(ctx.user) as bank
        margem  from row.bank_data[bank].margem    label "Margem {bank}"
        status  from row.bank_data[bank].status    label "Status {bank}"

vs:

    columns
      cpf  from row.cpf
      + dynamic from @fn.bank_columns(ctx.user)   ! returns []report.Column

The first is more declarative but extends the column-source grammar;
the second leans on `@fn.*` returning a slice and is a smaller language
change. Decision deferred until a second site beyond corbanx demands it.

### Smaller open questions

- `format "yyyy-mm-dd"` / `format "currency:BRL"` — closed catalog vs free
  string? v0.1 is **closed**: `yyyy-mm-dd`, `yyyy-mm-dd HH:MM:SS`,
  `currency:BRL|USD|EUR`, `percent`, `integer`, `boolean:yes|no`. New
  entries require pilot evidence.
- Auto-mount HTTP endpoints (`GET /api/reports/<name>.csv`) vs explicit
  `api`-style block? v0.1: **auto-mount**.
- `audit` block required by default? v0.1: optional; strict profile may
  upgrade to "required when `@scope.*` other than `@role.admin`" once
  pilot evidence shows the bar.

## Próximo passo

1. Grade this proposal v0.1 with `lazuli-language-architect`. Target ≥
   8.5; gate at ≥ 8.5 with no individual criterion < 7.
2. Apply blockers; ship v0.2 if necessary.
3. After PASS:
   - Codex cells `cdx-report-csv` and `cdx-report-xlsx` shipped the
     writers (commit `5e2138e`).
   - Orchestrator wires `runtime/go/lazuli/report/pipeline.go`.
   - Implementation cells for `crates/lazuli_ir` additive types,
     `crates/lazuli_analyzer` lowering, `crates/lazuli_lsp` diagnostics,
     `crates/lazuli_cli/src/doctor.rs` cross-feature checks,
     `crates/lazuli_codegen_go` emission.
   - Fixture extension: rewrite `examples/full-capsule/full-capsule.lzi:370-376`
     from `api customer_export` to either Surface A or Surface B.

## Rows sugeridas para `docs/next-checklist.md`

```
| 32 | Report vocab — `query.list ... export` + `report` kind | planned | Add `Report`, `ReportColumn`, `ReportSource`, `ReportFormat`, `QueryExport` to IR. Extend grammar with `report <name>` kind and `query.list.export` child. New `--expand=reports` projection. |
| 33 | Report vocab — 8 doctor diagnostics + LSP coverage | planned | 8 codes + LSP hovers for 6 keywords + closed-catalog completions. Depends on row 32. |
| 34 | Report runtime — pipeline.go + CSV/XLSX writers | shipped (writers) | Writers shipped at commit 5e2138e. Pipeline + contract.go pending. Wire-thin: total < 250 LOC. Depends on row 32 + bucket-storage cycle. |
```
