# Bucket Cycle: AI Debug Loop (L0→L2)

**Status**: design proposal. Stages 3–9 of the
`bucket=ai-debug-loop` pipeline. Implementation deferred to a
separate run with `mode=implement`.

**Audience**: language team (Lazuli core), Lazuli Go runtime team,
codegen-go owners.

**Date**: 2026-05-11.

**Pilot bucket**: cross-cutting hardening cycle. This is **not** a
feature bucket — it protects every other bucket's debuggability and
authoring economy under the AI-100x cost assumption. Lands once;
every subsequent bucket inherits the discipline.

**Companion**: `docs/release-policy.md` (2026-05-11) defines the
public stability contract this proposal's D10 cell relies on.

## Contexto

Under the assumption that AI inference is 100× more expensive than
it is today, the dominant cost of any DSL is no longer "how many
tokens does the IA write to author code" — it is **"how many tokens
does the IA burn to debug code in production"**. The naive critique
of compiled DSLs is that the loop is expensive because the IA must
read generated host-language source. We reject that critique for
Lazuli specifically:

- Lazuli's codegen is FINO. A generated `<feature>/resource.gen.go`
  is ~50 LOC; the bulk of any stack trace lives in the
  hand-written `runtime/go/lazuli/*` library, which is stable and
  reads like Rails internals.
- The IR is semantic. Errors can carry feature/kind/op context
  structurally, not as concatenated strings. (The "capsule" axis is
  excluded from v0 — see §7.1 + Anti-scope.)

But the critique has teeth in three concrete places, and this
bucket closes them:

1. **Source location**: today a panic in a generated handler shows
   `crm.gen.go:380`, not `crm.lzi:42`. The IA cannot map back
   without reading the generated file. Token cost: high.
2. **Error structure**: today `errors.New("auth: password
   mismatch")` is a string. The IA must regex-parse to extract
   field/path/tenant context. With a typed hierarchy
   (`*lazuli.FieldError` with exported `Field`, `Reason`, `Path`),
   the IA does `errors.As(err, &fe)` and reads structured context.
   At 100× cost, prompt size becomes load-bearing.
3. **Perf attribution**: errors are discrete and rare; performance
   is continuous and diffuse. In mature systems "which `.lzi` op
   is slow" is the most-asked question. pprof flamegraphs name Go
   functions, not `.lzi` ops. Without explicit instrumentation the
   IA reads generated code to map back. Token cost: highest of the
   three.

Beyond those three, two adjacent risks (raised during architect
review on 2026-05-11) motivate cells in this bucket:

4. **Lib bug vs user error disambiguation**. Rails has a public
   long-tail that absorbs "is this me or is this framework"
   triage. Lazuli does not. Without an `Origin` discriminator on
   the error envelope, every failure costs the IA tokens to decide
   whether to debug the user's `.lzi`, file an issue against
   Lazuli core, or escalate to the adapter author.
5. **DSL/IR version churn invalidates IA learned context**. Each
   release that changes language surface burns IA cache.
   `lazuli_version` pin + automated migration recipes prevent the
   silent half of that breakage.

The bucket lands ten cells (D1–D10) that together yield a debug
loop in which:

- **The IA reads `.lzi` source + structured error envelope + IR
  snippet — not generated Go.**
- **Perf attribution is automatic via pprof labels keyed by `.lzi`
  op; `lazuli profile` reports flame-graph entries already mapped
  to ops, with template patterns annotated separately so the IA can
  tell user-introduced cost from codegen-introduced cost.**
- **Every breaking release ships a migration recipe with input +
  expected-output fixtures that run in CI; no recipe means no
  merge.**

The closed-cycle criterion is the §0 8-item checklist (fixture +
check + inspect + doctor lint + generate Go + Lazuli Go runs +
eval/test + LSP hover) applied per cell rather than per construct,
since several cells span more than one IR kind. Each cell carries
its own smoke in §Cycle.

## Baseline (Stages 1-2 inventory)

| Surface | Today | Anchor | L-level |
|---|---|---|---|
| `SpanRef` in IR | `{ start: usize, end: usize }` only — no file path | `crates/lazuli_ir/src/lib.rs:44-50` | **L0 (offsets), missing file id** |
| `SpanRef` use sites | ~30 IR kinds carry `Option<SpanRef>` | `crates/lazuli_ir/src/lib.rs:88, 171, 295, 312, 362, 391, …` | **wired but unresolvable** |
| Source path threading in codegen | only `app.name` as `source_label` | `crates/lazuli_codegen_go/src/emitter/module.rs:108-113` | **L0 (label only)** |
| `//line` directives in any codegen | none — `rg //line crates/` returns zero | confirmed | **missing** |
| `lazuli.Error` envelope | `{ Status, Code, Message, Data any }` + 9 canonical codes | `runtime/go/lazuli/error.go:10-34` | **L1 (flat, untyped Data)** |
| Bucket sentinel errors | `errors.New("auth: foo")` × ~40 sites, untyped | `runtime/go/lazuli/auth/*.go`, `…/jobs/*.go`, `…/storage/*.go`, `…/notifications/*.go`, `…/migrations/*.go`, `…/webhooks/*.go` | **L1 sentinels, no envelope** |
| `errors.Unwrap` chains | none on `*lazuli.Error` | `runtime/go/lazuli/error.go` | **missing** |
| `WithSource` context helper | none | n/a | **missing** |
| `observability/panic.go` recovery | `RecoverHTTP` + `RecoverScope`; TODOs for typed-error introspection; 500 envelope is literal `{"error":{"code":"internal_panic"}}` | `runtime/go/lazuli/observability/panic.go:41-77` | **L1 skeleton, missing wire** |
| pprof labels per op | none | n/a | **missing** |
| `lazuli profile` verb | does not exist | n/a | **missing** |
| `lazuli debug` verb | does not exist | n/a | **missing** |
| `lazuli examples --bundle` verb | does not exist | n/a | **missing** |
| `lazuli upgrade` verb | does not exist | n/a | **missing** |
| `lazuli_version` pin in `app.lzi` | not declared, not parsed | n/a | **missing** |
| `LZIR_SCHEMA` constant | `0.11.0` | `crates/lazuli_ir/src/lib.rs:42` | **partial — exists but not surfaced to authors** |
| `app.observability` block | does not exist; sibling blocks `app.logging` and `app.tracing` exist | `crates/lazuli_ir/src/lib.rs:AppLogging/AppTracing` | **missing** |
| Codegen banner | `// Code generated by lazuli; DO NOT EDIT.\n// source: <path>\n\npackage <name>\n` | `crates/lazuli_codegen_go/src/emitter/printer.rs:63-69` | **L1 — extensible** |
| Pattern annotation on emitted functions | none | n/a | **missing** |
| Migration recipes infra | does not exist | n/a | **missing** |
| Release policy doc | drafted 2026-05-11 | `docs/release-policy.md` | **L1 — landed pre-cycle** |

**Cross-cutting fact**: every gap above is **additive**. No existing
surface needs to be removed or have its semantics changed. This
bucket lands as widening on three sides (IR, runtime, CLI) plus
two new authoring surfaces (`lazuli_version` pin,
`app.observability` block).

## Linguagem proposta (Stage 3)

Authoring surface in this bucket is **deliberately small**. Most
of the work is below the surface (IR, codegen, runtime, CLI). The
two new authoring surfaces are:

### 3.1 `lazuli_version` pin on `app.lzi`

```lzi
app AcmeCRM
  lazuli_version "0.12"   # MINOR-only — patch-level pins rejected
  urls
    ...
```

Required from 1.0 onward; warning-only in 0.x. Pin is **MINOR
granularity** (`"0.12"` matches any `LZIR_SCHEMA == 0.12.x`).
PATCH mismatch is tolerated; MINOR or MAJOR mismatch fires
`LAZULI-VERSION-001` (full table in §5 + rationale in
`docs/release-policy.md §lazuli_version pin tolerance`). Three-
segment pins (`"0.12.0"`) are rejected by the loader as
`LAZULI-VERSION-003` — patch-level pins would freeze bug fixes
authors actually want.

Rationale: a single declarative pin keeps the version-to-IA
contract explicit without polluting every feature file. MINOR
granularity matches the natural break-vs-fix boundary; PATCH is
non-breaking by policy so binding to it serves no author goal.

### 3.2 `app.observability` block

```lzi
app AcmeCRM
  observability
    error_source dev,staging   # closed-catalog: any of "dev", "staging", "prod"
    panic_recover true         # default true; opt-out for debug builds
```

Closed-catalog list. `error_source` controls whether the typed
`*lazuli.Error.Base.Source` field (e.g. `"crm.lzi:42:8"`) is
included in HTTP response envelopes per environment. Default
`dev,staging` — production strips. Doctor diagnostic
`OBSERVABILITY-SOURCE-001` flags unknown environment tokens.

Rationale: leaking `.lzi:line` in a prod 500 envelope is structural
information leak, low severity but non-zero. Authors opt in
explicitly.

**Why a third sibling block alongside `app.logging` and
`app.tracing`** (architect grade pass-with-notes C, 2026-05-11):
`logging` declares the *output channel* (level, format, redaction);
`tracing` declares the *propagation contract* (sample rate,
exporter binding, propagated keys); `observability` declares
**cross-cutting response/recovery posture** that is neither a log
nor a span (`error_source`, `panic_recover`). The three blocks
follow the same "one block per axis" convention as the rest of
the language (`auth.password` vs `auth.sessions` vs `auth.mfa`).
The rule for "where does a new field go" is: if it shapes a *log
record*, `app.logging`; if it shapes a *span*, `app.tracing`; if
it shapes the *error envelope or recovery behaviour*,
`app.observability`. A Phase 2 follow-up cell may unify all three
under a single tree if pilot evidence shows the split confuses
authors; until then the boundary stays explicit. Phase 2
unification is **not** in this bucket — see Anti-scope.

### 3.3 No other new keywords

The remaining nine cells live below the surface. We do **not**
introduce:

- A `debug` block, `profile` keyword, or `version` keyword inside
  features. Per-feature versioning is not in scope; the app-level
  pin covers IA-cache invalidation.
- An authored `pattern_id` surface. Pattern annotations are
  codegen-emitter discipline, not author-visible.
- A `migration` or `recipe` keyword in `.lzi`. Recipes live as
  fixtures under `migrations/recipes/` (see release-policy §Migration
  recipes), not as authored language.

## IR (Stage 4)

### 4.1 `SourceMap` companion to `Module`

```rust
// crates/lazuli_ir/src/lib.rs (additive)

pub type FileId = u16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMap {
    pub files: Vec<SourceFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub id: FileId,
    pub path: String,             // canonical relative path, e.g. "features/customer.lzi"
    pub line_offsets: Vec<u32>,   // byte offset of each line start; line_offsets.len() = line count + 1
}
```

`SourceMap` is **not embedded in `Module`**. It is passed
**alongside** the IR module into codegen as a separate argument
(`emit_module(module, source_map, options)`), and serialised
opt-in via a sidecar JSON (`<module>.sourcemap.json`) when the CLI
is invoked with `--with-source`. Rationale: SpanRef is referenced
in ~30 IR kinds (`crates/lazuli_ir/src/lib.rs:88, 171, 295, …`) and
widening it cascades into every snapshot test. The companion shape
keeps the IR JSON ABI stable.

`SpanRef` itself stays at `{ start, end }`. The resolver helper

```rust
// crates/lazuli_analyzer/src/source_map.rs (new)
impl SourceMap {
    pub fn resolve(&self, file: FileId, span: SpanRef) -> ResolvedLoc { … }
}

pub struct ResolvedLoc { pub file: String, pub line: u32, pub column: u32 }
```

closes the loop. Where SpanRef appears today without file context,
codegen-side threading carries the `FileId` separately
(per-feature granularity is sufficient for v0 since each feature
is a single file).

**`LZIR_SCHEMA` bump**: this bucket adds `SourceMap` to the
sidecar emission contract but does not alter `Module`'s
shape. Sidecar is an additive output. We bump to `0.12.0` to
signal the companion exists; doctor surface remains unchanged.

### 4.2 `AppManifest.lazuli_version: Option<String>`

```rust
// crates/lazuli_ir/src/lib.rs (additive on AppManifest)

#[serde(default, skip_serializing_if = "Option::is_none")]
pub lazuli_version: Option<String>,
```

Parsed at `parse_app_manifest`. Doctor `LAZULI-VERSION-001`
compares against `LZIR_SCHEMA` when present; emits a 0.x
warning when absent (error at 1.0).

### 4.3 `AppManifest.observability: Option<AppObservability>`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppObservability {
    pub error_source: Vec<String>,     // subset of {"dev", "staging", "prod"}
    pub panic_recover: bool,           // default true
    pub span_ref: Option<SpanRef>,
}
```

Sibling of existing `AppLogging` and `AppTracing` on
`AppManifest`.

## Doctor + LSP (Stage 5)

Seven IR-driven diagnostics across this bucket:

| Code | Surface | Severity | Trigger |
|---|---|---|---|
| `LAZULI-VERSION-001` | `app.lzi:lazuli_version` | error (1.0), warning (0.x) | pin missing or MINOR-mismatched with `LZIR_SCHEMA` (PATCH mismatch is tolerated per release-policy §`lazuli_version` pin tolerance); message names the recipe path |
| `LAZULI-VERSION-002` | `app.lzi:lazuli_version` | error | pinned to a version that has no migration path from current `LZIR_SCHEMA` |
| `LAZULI-VERSION-003` | `app.lzi:lazuli_version` | error | pin uses three-segment form (`"0.12.0"`) — patch-level pins are rejected per release-policy §`lazuli_version` pin tolerance. Authors must write MINOR-only (`"0.12"`) |
| `OBSERVABILITY-SOURCE-001` | `app.observability.error_source` | error | token outside closed catalog `{"dev","staging","prod"}` |
| `OBSERVABILITY-PANIC-001` | `app.observability.panic_recover` | warning | set to `false` outside `dev` environment (loud opt-out for prod) |
| `CODEGEN-PATTERN-001` | codegen-go emitter (Rust-side lint) | build error | an emitted Go function lacks a `//lazuli:pattern <id> <version>` header (defence-in-depth; primary gate is type-system per ADR-7). See §6.3 |
| `CODEGEN-SENTINEL-001` | codegen-go emitter (Rust-side lint) | build error | an emitted handler returns a Go sentinel that is not enumerated in `crates/lazuli_codegen_go::sentinels`. See §7.2 |
| `MIGRATION-RECIPE-001` | CI gate, not authoring | build error | `LZIR_SCHEMA` bumped without at least one recipe directory added under `migrations/recipes/<from>-to-<to>/` |
| `MIGRATION-RECIPE-002` | CI gate | build error | a recipe's `input.lzi → output.lzi` round-trip fails the `lazuli upgrade` smoke |

LSP coverage: hovers for `lazuli_version`, `observability`,
`error_source`, `panic_recover`. Closed-catalog completion on
`error_source` token set.

## Codegen (Stage 6)

### 6.1 `//line` directives

Every emitted Go function/method body opens with a `//line`
directive pointing back to the `.lzi` location its IR carrier
came from:

```go
// dist/go/customer/command.gen.go

//line features/customer.lzi:42:1
func HandleCreateCustomer(ctx context.Context, in CreateCustomerInput) (CreateCustomerOutput, error) {
    …
}
```

`//line` is Go's native source-map directive — `go tool compile`,
`go vet`, `pprof`, and `delve` all honour it. Panics report
`features/customer.lzi:42:8`, not `customer/command.gen.go:380`.

Granularity for v0: per-function (one `//line` at the top of each
emitted handler / method). Per-statement granularity is pilot-gated
— promote only if a real stack trace appears ambiguous in practice.

Depends on `SourceMap` plumbing (D1) so codegen can resolve
`SpanRef → file:line`.

### 6.2 `WithSource` context injection

Every generated handler opens with:

```go
ctx = lazuli.WithSource(ctx, lazuli.SourceTag{
    Feature: "customer",
    Kind:    "command",
    Op:      "create_customer",
    Source:  "features/customer.lzi:42:1",
})
```

`lazuli.WithSource` stores the `SourceTag` in a context key. Any
error constructor (D2's `lazuli.Wrap` / `lazuli.Errorf`) reads the
tag back and populates `*lazuli.Error.Base.{Feature,Kind,Op,Source}`
automatically. User-code in bucket adapters never has to thread
metadata manually — context is the carrier.

No `Capsule` field per §7.1 rationale (capsule project has not
landed; placeholder would bake unfinished vocabulary into the
runtime ABI).

### 6.3 Pattern annotations — closed catalog at the emitter API

Pattern IDs are not author-visible (per §3.3 anti-surface). They
are a **codegen-internal closed catalog** enforced at the
emitter-API boundary, *not* at output-lint time.

```rust
// crates/lazuli_codegen_go/src/patterns.rs (new)

/// Closed catalog of emitted code patterns. Each variant names a
/// shape the codegen produces; v2 supersedes v1 when the emitted
/// shape changes materially (allocation, locking, SQL strategy).
/// Adding a new variant requires updating the `lazuli profile`
/// attribution catalog in the same commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pattern {
    CommandPgxInsert(PatternVersion),
    CommandPgxUpdate(PatternVersion),
    QueryPgxList(PatternVersion),
    QueryPgxLookup(PatternVersion),
    ResourceListPgxScan(PatternVersion),
    JobRiverWorker(PatternVersion),
    WebhookHmacReceiver(PatternVersion),
    // … exhaustive
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternVersion { V1, V2, V3 }
```

The emitter API takes `Pattern` as a **required parameter** for
every function emission:

```rust
// crates/lazuli_codegen_go/src/emitter/printer.rs (widened)

impl GoPrinter {
    /// Emit a Go function. `pattern` is required: the emitter API
    /// rejects emission without it at the Rust type level, before
    /// any output is produced. This is strictly stronger than a
    /// post-output regex lint.
    pub fn func(
        &mut self,
        pattern: crate::patterns::Pattern,
        source_loc: Option<&str>,    // for //line directive
        signature: &str,
        body: impl FnOnce(&mut Self),
    ) { … }
}
```

The header in the produced Go file:

```go
//lazuli:pattern command_pgx_insert v1
//line features/customer.lzi:42:1
func HandleCreateCustomer(...) {
    ...
}
```

`CODEGEN-PATTERN-001` is the **defence-in-depth** output-lint
that runs after `printer.finish()` and asserts every `^func ` is
preceded by `//lazuli:pattern`. It catches accidental writes that
bypass `GoPrinter::func` (e.g. raw `printer.line(...)` calls that
happen to emit a func header). Both gates (API-required `Pattern`
parameter + output regex assertion) ship in D8.

Rationale (architect 2026-05-11 + amendment): the output regex
alone is fragile (method receivers, anonymous funcs, future
generic instantiation, multi-line signatures). Type-system
enforcement at the API entry is the source of truth; the regex is
the safety net. This converts "emitter author discipline" into "code
does not compile without a `Pattern`".

Pattern versioning is independent of `LZIR_SCHEMA`. `lazuli
profile`'s attribution report (§7.5) groups by
`(pattern_id, version)`. Adding a new variant or version to the
`Pattern` enum is an internal-crate change; it does not affect the
public language surface and does not require a migration recipe.

## Lazuli Go runtime (Stage 7)

### 7.1 Typed error hierarchy

Base envelope plus concrete child types. Closed enum for
`Origin` (renamed from earlier draft `Surface` — collided with
existing `.lzx` vocabulary, see ADR-2). **All types ship marked
`EXPERIMENTAL` per release-policy §Stability tiers**.

```go
// runtime/go/lazuli/error.go (widened)

package lazuli

// EXPERIMENTAL: error hierarchy may grow additive variants before
// 1.0. Stable promotion gated on first pilot consumer.
type Error struct {
    Base ErrorBase
}

type ErrorBase struct {
    Code     string   // canonical code, e.g. "field_invalid"
    Origin   Origin   // discriminator: who is responsible
    Status   int      // HTTP status; 0 = derived from Code
    Message  string   // human-readable
    Feature  string   // "customer"
    Kind     string   // "command" | "query" | "job" | ...
    Op       string   // "create_customer"
    Source   string   // "features/customer.lzi:42:8" (stripped per app.observability.error_source)
    Cause    error    // wrapped underlying error; participates in errors.Is/As chain
}

// Origin routes the IA debug response: UserDSL → read .lzi;
// LibInternal → file issue against lazuli core; CodegenBug →
// route to codegen-go owner; AdapterRuntime → external adapter.
type Origin uint8

const (
    OriginUserDSL Origin = iota
    OriginLibInternal
    OriginCodegenBug
    OriginAdapterRuntime
)

func (e *Error) Error() string { … }
func (e *Error) Unwrap() error { return e.Base.Cause }
```

Note: no `Capsule` field. The "capsule" concept (see
`MEMORY/project_component_capsules.md`) has not landed formally;
adding the field as a placeholder defaulting to `app.name` would
bake unfinished vocabulary into the runtime ABI (ADR-3 marks the
hierarchy EXPERIMENTAL, but additive growth is the only safe path
once `errors.As` consumers exist). `Feature` is enough provenance
for v0; `Capsule` lands as an additive MINOR when the capsule
contract is real.

Child types each embed `ErrorBase` as a **named** field (not Go
embed). See ADR-1 for rationale. Reads go through `.Base.<field>`
— one canonical path, no convenience accessor (ADR-1 amended).

```go
// runtime/go/lazuli/error_field.go

type FieldError struct {
    Base      ErrorBase
    Field     string
    Path      string         // "input.identity.email"
    Reason    FieldReason    // closed catalog
    InputType string         // "string" | "@semantic.Email" | ...
}

type FieldReason uint8
const (
    FieldReasonRequired FieldReason = iota
    FieldReasonInvalidFormat
    FieldReasonOutOfRange
    FieldReasonMismatch
    FieldReasonUnknownEnum
)

func (e *FieldError) Error() string { … }
func (e *FieldError) Unwrap() error { return e.Base.Cause }
```

Siblings: `PolicyError` (`Rule`, `Subject`, `Resource`, `Tenant`),
`TenantError` (`Axis`, `Expected`, `Actual`), `AdapterError`
(`Adapter`, `Op`, `RetryBudgetConsumed`, `RetryBudgetMax`),
`LibBugError` (`Component`, `Invariant`, `IssueURL`).

**Origin partial redundancy with type**: `FieldError` /
`PolicyError` / `TenantError` always carry `Origin == OriginUserDSL`;
`LibBugError` always `OriginLibInternal`; `AdapterError` always
`OriginAdapterRuntime`. The discriminator earns its keep only for
**fallback** `*lazuli.Error` envelopes (bare `Error` type, no
child) where the wrap site could not classify the sentinel. CI
invariant test: any `*FieldError|*PolicyError|*TenantError|*LibBugError|*AdapterError`
must have `Base.Origin` matching the child's implied origin; a
disagreement fails the test.

### 7.2 Envelope discipline (architect-mandated)

Bucket sentinel errors stay **bare** at their declaration site
(`errors.New("auth: password mismatch")`). Wrapping into
`*lazuli.FieldError{…, Cause: auth.ErrPasswordMismatch}` happens
**exactly once**, at the bucket → runtime boundary, in the
**codegen-generated handler**. Bucket authors never construct
`*lazuli.FieldError` directly.

Rationale (architect 2026-05-11): double-wrapping
(`FieldError{Cause: FieldError{Cause: sentinel}}`) forces the IA
to unwrap N times to find the sentinel. Codegen owning the
boundary keeps N = 1 by construction. Bucket authors stay
ignorant of the hierarchy — they emit sentinels and return them.

Codegen template (`crates/lazuli_codegen_go::emitter::command`)
will recognise known sentinel categories via a closed catalog in
`crates/lazuli_codegen_go::sentinels` (e.g.
`auth.ErrPasswordMismatch → FieldError{Field:"password",
Reason: FieldReasonMismatch}`) and emit the wrap.

**Unknown sentinels** fall through to a `*lazuli.Error` envelope
with `Origin: OriginLibInternal` and `Code: "uncatalogued_sentinel"`
— **not** `OriginUserDSL`. Rationale: an unclassified sentinel
reaching a wrap boundary is, by definition, a gap in the
codegen-go sentinel catalog; routing it as `UserDSL` would poison
the IA's first read by sending it to debug `.lzi` for a problem
that lives in the codegen template.

Belt-and-braces: `CODEGEN-SENTINEL-001` is an emitter-time error
(not runtime) when an emitted handler returns a sentinel that is
not enumerated in `crates/lazuli_codegen_go::sentinels`. The
catalog is exhaustive by construction — adding a new sentinel to
the Lazuli Go lib without registering it in the codegen catalog
fails CI. The `OriginLibInternal` fallback above is a defensive
net for the cross-build window (e.g. dev runs against a newer Go
lib than the codegen knows); it should never fire in CI-clean
builds.

### 7.3 pprof labels

`lazuli.StartOp(ctx, tag) (ctx, end func())` opens a pprof label
scope keyed by `SourceTag`:

```go
ctx, end := lazuli.StartOp(ctx, lazuli.SourceTag{ Feature: "customer", Kind: "command", Op: "create_customer" })
defer end()
```

Implementation wraps `pprof.Do(ctx, pprof.Labels("feature",
"customer", "kind", "command", "op", "create_customer"), fn)`.
Codegen injects the call at the top of every handler, reusing the
same `SourceTag` D5 already constructs.

### 7.4 `observability/panic.go` integration

Today (`runtime/go/lazuli/observability/panic.go:41-77`)
`RecoverHTTP` and `RecoverScope` carry TODOs. After this cell:

- `errors.As(rec, &lz)` extracts `ErrorBase` from any panic that
  wrapped a `lazuli.Error`; the trace event payload
  (`command_run` / `job_run` / `webhook_run`) carries
  `{feature, kind, op, source, origin}`.
- 500 envelope reads from the typed error. Per
  `app.observability.error_source`, the `Source` field is included
  in dev/staging response bodies and stripped in prod.
- Origin `LibInternal` / `CodegenBug` panics emit an additional
  `lazuli_internal_panic` trace event with the `IssueURL` field
  populated for telemetry consumers.

### 7.5 `lazuli profile` reporting contract

The runtime emits pprof profiles with the labels from §7.3 plus
the comment `//lazuli:pattern <id> <version>` headers preserved in
the debug info (Go preserves them per `//line` adjacency). The
`lazuli profile <profile.pb.gz>` CLI verb (§8.3) reads both,
groups frames by `(feature, kind, op)`, and reports:

```
Top 10 ops by CPU:
  customer.command.create_customer    12.4%  (pattern: command_pgx_insert v1)
  invoice.query.list                   8.1%  (pattern: query_pgx_list v2)
  ...

Top 5 codegen patterns by alloc:
  command_pgx_insert v1   34 MB across 6 ops
  resource_list_pgx_scan v1   12 MB across 4 ops
```

Allocation attribution to template vs user code is the
specifically valuable axis: if `command_pgx_insert v1` shows up
hot across multiple unrelated ops, that's evidence the template
needs revision, not the user's `.lzi`.

## CLI verbs (Stage 7 continued)

### 8.1 `lazuli debug --error <json> [--feature <name>]`

Reads a structured error envelope from stdin or `--error`, walks
the IR (filtered to `--feature` if specified, otherwise inferred
from `Base.Feature`), and emits a minimal context bundle:

- The originating `.lzi` block (resolved via `Base.Source`).
- The IR snippet for the failing op (one feature, one kind, one op).
- The error envelope itself, pretty-printed with `Base.Origin` →
  recommended debug route (read .lzi / file issue / contact
  codegen-go owner / contact adapter author).

**Token budget**: **≤ 4 000 tokens per debug bundle** for the
canonical fixture, measured by the **pinned tokenizer** in
`tools/tokenizer/` (Claude `cl100k`-compatible — exact version
pinned in `tools/tokenizer/version.txt`, currently `tiktoken
0.7.0` with `cl100k_base`). CI smoke compares against a frozen
fixture under `tests/golden/debug/` and fails the build if the
bundle exceeds budget against the pinned tokenizer. The tokenizer
pin is itself versioned and bumping it counts as a MINOR per
`docs/release-policy.md`.

### 8.2 `lazuli examples --bundle [--out <path>]`

Emits a deterministic JSONL of curated `.lzi` examples for IA
file-load / RAG consumption:

```jsonl
{ "name": "command_with_safety", "intent": "create command guarded by a safety validator", "lzi_source": "…", "ir_snippet": "…", "common_errors": ["safety_unbound", "validator_pii_class_mismatch"] }
```

**Inclusion policy** (architect-mandated, registered as
`migrations/recipes/examples-policy.md`):

1. Every example covers a pattern that has appeared in **at least
   two production pilots** OR illustrates an anti-pattern from a
   real bug report.
2. Every example has a CI test: `lazuli check`, `lazuli inspect`,
   and (when the example involves runtime) `lazuli generate go`
   + `go build` must pass against the example.
3. Examples not meeting (1) and (2) are rejected from the bundle.
   Speculative examples live in `docs/recipes/` outside the bundle.

Implementation includes a `lazuli examples --validate` mode that
runs the test suite against every entry; CI gate runs it on every
PR touching `examples/curated/`.

### 8.3 `lazuli profile <profile.pb.gz> [--top N] [--by cpu|alloc|block]`

See §7.5.

### 8.4 `lazuli upgrade --from X --to Y <path>`

Composes recipes from `migrations/recipes/<from>-to-<to>/` in
topological order, applies them to `<path>`, and runs each
recipe's `input.lzi → output.lzi` smoke after application to
confirm landing. See `docs/release-policy.md §Migration recipes`.

## Trade-offs (ADRs)

### ADR-1: Named field `Base ErrorBase`, not Go embed — one canonical path

**Decision**: child error types carry `ErrorBase` as a named
field, not an embedded type. Reads go through `.Base.<field>`.
**No `ErrorBase()` convenience accessor.**

**Considered**:

- **Option A (embed)**: `type FieldError struct { Error; Field string }`. Pros: ergonomic (`fe.Feature` reads through). Cons: overriding `Error() string` on the child silently breaks the promotion; `errors.As(err, &Error{})` against a child's pointer is subtly incorrect because of method-set rules.
- **Option B (named, with accessor)**: `type FieldError struct { Base ErrorBase; Field string }` + explicit `Error()` per child + `ErrorBase()` accessor. Pros: no method promotion fragility. Cons: two ways to read the same field (`fe.Base.Feature` vs `fe.ErrorBase().Feature`) — polysemy penalty by rubric criterion 5.
- **Option C (named, no accessor)**: as above but drop `ErrorBase()`. Pros: one canonical read path; type-system enforces refactor safety; no polysemy. Cons: `fe.Base.Feature` is ~33% wordier than `fe.Feature`; IA pays the verbosity tax on every read.

**Decision**: Option C. The IA debug loop reads `Base.Feature`
under RAG bundle context where token cost is amortised across
multiple errors per bundle, not per access. Determinism (one read
path) outweighs the 33% verbosity tax. ADR amended 2026-05-11
post-grade per architect feedback.

### ADR-2: `Origin` is closed 4-variant enum, sized `uint8` (renamed from `Surface`)

**Decision**: `OriginUserDSL | OriginLibInternal | OriginCodegenBug | OriginAdapterRuntime`. No fifth variant in v0.

**Naming**: the earlier draft used `Surface`. Architect review
2026-05-11 flagged a collision with the existing `.lzx`
vocabulary (`surface` already names a projection target — `web`,
`mobile`, …). For an LLM cold-reading a stack-trace context and a
`.lzx` fixture in the same conversation, the two meanings of
"surface" collapse. Renamed to `Origin` because: (a) reads
naturally as a who-is-responsible discriminator; (b) `lazuli
inspect` already uses `origin` as a derived-provenance label on
adapters, so the semantic carry is real not invented; (c)
`Surface` stays free to mean the language/authoring concept.

**Partial redundancy with type**: `FieldError`/`PolicyError`/
`TenantError` always imply `OriginUserDSL`; `LibBugError` always
`OriginLibInternal`; `AdapterError` always `OriginAdapterRuntime`.
The discriminator earns its keep only for **fallback** bare
`*lazuli.Error` envelopes (§7.2). CI invariant test enforces that
typed children carry the implied Origin — disagreement fails the
test. Treat `Origin` as load-bearing for fallback routing and as a
cross-check for typed children, not as a separate decision point.

**Rationale (4 variants)**: four variants cleanly partition who
is responsible for the failure. A fifth ("`OriginPlatform`"?
"`OriginConfig`"?) risks bleeding the discriminator's purpose. If
pilot evidence demands a fifth, ADR revision; do not extend
casually.

`uint8` rather than string: 1 byte payload, no allocation, no
typo risk, exhaustive `switch` analysis available in Go via
golangci-lint exhaustive checker.

### ADR-3: Error hierarchy ships EXPERIMENTAL pre-1.0

**Decision**: every type in the new hierarchy carries the
`// EXPERIMENTAL: subject to change before 1.0` docstring, per
`docs/release-policy.md §Stability tiers`.

**Why**: once IA-generated user-code does
`errors.As(err, &lazuli.FieldError{})`, every variant added
later must be additive only — no field renames, no enum
tightening, no migration of `Reason` from `uint8` to
something richer. We want one MINOR window to discover shape
mistakes before paying that bill.

**Promotion gate**: first production pilot consuming
`errors.As` against a child type promotes the entire hierarchy
to Stable.

### ADR-4: LTS dual-channel deferred

**Decision**: do not ship a `lazuli-stable` LTS channel
distinct from `lazuli` HEAD in v0.

**Why**: addressed in `docs/release-policy.md §Why this exists`.
Pinning + auto-migrator solves 80% of the IA-cache invalidation
pain. The remaining 20% (long-tail support windows) is not
worth dual-channel maintenance cost without pilot evidence.

**Revisit**: after second production pilot.

### ADR-5: `SourceMap` as companion, not IR widening

**Decision**: `SourceMap` is a sibling artifact passed alongside
`Module`, serialised to a sidecar JSON (`<module>.sourcemap.json`)
when requested. `SpanRef` stays `{ start, end }`.

**Considered**:

- **Option A (widen `SpanRef`)**: add `file: FileId` field.
  Touches ~30 IR kinds × every snapshot. High blast radius.
- **Option B (`Feature.source_file: Option<String>` per feature)**:
  per-feature granularity, no SpanRef change. Pros: minimal. Cons:
  insufficient if a feature spans multiple files (rare today, may
  be future-pressed by `include` semantics).
- **Option C (companion `SourceMap` indexed by `FileId`)**:
  decouples provenance from semantics. Pros: IR ABI stable; can
  be omitted from minimal builds. Cons: codegen must thread file
  context separately.

**Decision**: Option C. Cleanest separation; the codegen pipeline
already knows per-feature file paths from the loader.

### ADR-6: `//line` granularity per-function in v0

**Decision**: emit `//line` once per generated Go function. Not
per-statement.

**Why**: 80% of stack trace ambiguity is "which op?", solved by
function-level granularity. Per-statement granularity adds emitter
complexity and bloats `.gen.go` size for marginal benefit. Pilot
evidence of ambiguous traces would promote.

### ADR-7: Pattern enforcement is type-system first, output-lint second, never LSP

**Decision**: pattern IDs are enforced at three layers, in
ascending order of fragility:

1. **Source of truth — emitter API parameter** (§6.3). The
   `GoPrinter::func(pattern: Pattern, …)` signature makes
   emission without a pattern impossible at the Rust type level.
   New emitter authors cannot bypass this without modifying the
   printer.
2. **Defence-in-depth — output regex assertion**
   (`CODEGEN-PATTERN-001`). Walks final output after
   `printer.finish()` and asserts every `^func ` is preceded by
   `//lazuli:pattern`. Catches accidental raw `printer.line(...)`
   writes that happen to emit a func header.
3. **No LSP enforcement**. Pattern IDs are codegen-internal, not
   author-visible (per §3.3 anti-surface).

**Why type-system first**: amended 2026-05-11 post-grade. Initial
draft put output regex as the primary gate; the regex is fragile
(method receivers, anonymous funcs, future generic instantiation,
multi-line signatures). Type-system enforcement converts
discipline into "code does not compile without a `Pattern`" —
strictly stronger. The regex stays as defence-in-depth because
even a strong API can be bypassed by an emitter author who calls
`printer.line("func Foo() { … }")` directly.

## Anti-scope (what v0 explicitly does NOT do)

- **Per-statement `//line` directives**. (ADR-6.)
- **LTS dual-channel**. (ADR-4; release-policy §Why this exists.)
- **Public corpus for IA training**. (D7 is RAG-consumption only.
  Training requires scale we do not have.)
- **`Origin == OriginConfig` or `OriginPlatform` variants**. (ADR-2.)
- **`debug` block or per-feature observability declarations**. (§3.3.)
- **Recipes authored as `.lzi` syntax**. (§3.3; recipes are
  fixtures, not language.)
- **`Capsule` field in `ErrorBase` or `SourceTag`**. The capsule
  concept (`MEMORY/project_component_capsules.md`) has not landed
  formally; baking it into the runtime ABI as a placeholder
  defaulting to `app.name` would constrain the capsule project's
  shape. Reintroduce as additive MINOR when the capsule contract
  is real. (§7.1, §6.2.)
- **Pattern catalogs as authored language surface**. Authors
  describe intent (`command create_customer` and its children);
  codegen picks the pattern. There is no `command create_customer
  pattern command_pgx_insert` syntax. Pattern selection is
  codegen-internal forever. (§6.3; §3.3.)
- **Cross-host source-map abstraction**. `//line` is Go-specific.
  When a second runtime target lands (Rust, TypeScript, …), each
  target owns its own source-map mechanism (`#[track_caller]` in
  Rust, `//# sourceURL` in JS, etc.). This bucket does **not**
  invent a portable abstraction; the contract is "every target
  must provide *some* `.lzi`-to-line mapping" — implementation per
  target. (ADR-6.)
- **Automatic migration of Go user-code calling `errors.As` against
  the typed hierarchy**. Recipes cover `.lzi` upgrade; Go-side
  upgrades are documented in release notes per MINOR but not
  auto-rewritten in v0. Revisit if user-code starts breaking.
- **Replacing existing `lazuli.Error` use sites in
  `runtime/go/lazuli/error.go:24-34` constants**. The 9 canonical
  codes stay; the hierarchy adds child types around them.
- **OTel exporter** beyond pprof labels. Adapter territory.
- **Unifying `app.logging` + `app.tracing` + `app.observability`
  into a single tree**. The three-block split has a clear rule
  (§3.2); a single-tree refactor is a Phase 2 follow-up gated on
  pilot evidence of author confusion. This bucket lands the third
  block; it does not relitigate the existing two.

## Cycle (Stage 9) — Implementation cells

Owners: `[lang]` = this profile (language + codegen + CLI),
`[runtime]` = Lazuli Go runtime team, `[ambos]` = shape this
profile, implementation runtime team.

| # | Cell | Owner | Depends | Smoke |
|---|---|---|---|---|
| D1 | `SourceMap` companion + `--with-source` sidecar emission | [lang] | — | `lazuli inspect --with-source examples/full-capsule` produces `module.sourcemap.json`; resolver maps a known span to `features/customer.lzi:42:8` |
| D2 | Typed error hierarchy (`ErrorBase` + `FieldError`/`PolicyError`/`TenantError`/`AdapterError`/`LibBugError` + `Origin` enum) — EXPERIMENTAL marker | [ambos] | — | unit tests: `errors.As(err, &fe)` recovers exported fields; `errors.Is(err, sentinel)` walks the `Cause` chain; CI invariant test asserts typed children carry the implied `Origin` |
| D3 | Codegen emits envelope wrap at bucket→runtime boundary (single-wrap discipline) | [lang] | D2, D5 | round-trip test: bucket returns `auth.ErrPasswordMismatch`; generated handler wraps once into `*FieldError{Field:"password", Reason: FieldReasonMismatch, Cause: auth.ErrPasswordMismatch}`; `errors.Is(err, auth.ErrPasswordMismatch) == true` |
| D4 | Codegen emits `//line <file>:<line>` at each function head | [lang] | D1 | force panic in generated handler; `recover()` stack reports `features/customer.lzi:42`, not `customer/command.gen.go:380` |
| D5 | Codegen injects `lazuli.WithSource(ctx, SourceTag{…})` at handler head | [lang] | D1, D2 | error returned by generated handler carries `Source: "features/customer.lzi:42:8"` without user-code threading |
| D6 | `observability/panic.go` integrates typed envelope: `errors.As` extraction + 500 envelope reads `Source` per `app.observability.error_source` | [runtime] | D2, D5 | forced panic in `examples/full-capsule` produces a trace event with full provenance + 500 response includes `Source` in dev/staging and strips in prod |
| D7 | `lazuli debug` + `lazuli examples --bundle` verbs + inclusion-policy doc | [lang] | D2–D6 | `lazuli debug --error <golden.json>` returns ≤ 4 000 tokens against pinned tokenizer (`tools/tokenizer/version.txt`); `lazuli examples --bundle` produces a deterministic JSONL; `lazuli examples --validate` passes against every entry |
| D8 | pprof labels via `lazuli.StartOp`, `//lazuli:pattern` annotations + emitter lint, `lazuli profile` reporter | [ambos] | D5, D6 | `lazuli profile <pb.gz>` lists top N ops by cpu/alloc with `pattern_id` per row; `crates/lazuli_codegen_go` lint fails build when an emitted `func ` lacks `//lazuli:pattern` |
| D9 | LSP `textDocument/definition` jumps from a Go stack trace's `crm.lzi:42` to the `.lzi` source — pilot-gated | [lang] | D4 | clicking a panic frame in IDE opens the right `.lzi` line; gated on first pilot demand |
| D10 | `lazuli_version` pin parsing + `lazuli upgrade` migrator + `migrations/recipes/` infra + CI gates `MIGRATION-RECIPE-001/002` | [lang] | release-policy.md (landed) | `lazuli upgrade --from 0.11 --to 0.12 examples/full-capsule` rewrites the fixture without loss; `lazuli check` passes post-upgrade; CI gate blocks PR that bumps `LZIR_SCHEMA` without a recipe |

### Recommended execution order

```
release-policy.md (landed pre-cycle, gate for D10)
                                               │
D1 (SourceMap)  ────┐                          │
                    ├──→ D4 (//line) ─────┐    │
D2 (Error hier) ────┤                     │    │
                    ├──→ D5 (WithSource) ─┤    │
                    └──→ D3 (envelope) ───┼──→ D6 (panic wire) ──┐
                                          │                      │
                                          └──→ D8 (pprof) ───────┼──→ D7 (debug + examples)
                                                                 │
                                              D10 (versioning) ──┘  (independent path)

D9 (LSP jump) — pilot-gated, runs after D4
```

D1 + D2 + D10 are independent and can run parallel as Phase 1.
D3 + D4 + D5 fan out from those; D6 + D8 land after; D7 consolidates.

## Evals (Stage 8) — Acceptance gates

Each cell smoke is its acceptance gate. Cross-cutting acceptance:

1. **Token budget**: `lazuli debug` against the canonical fixture
   produces ≤ 4 000 tokens **measured against the pinned tokenizer
   in `tools/tokenizer/version.txt`** (currently `tiktoken 0.7.0`
   / `cl100k_base`). Frozen golden fixture in
   `tests/golden/debug/`. Tokenizer-pin bump is a MINOR per
   `docs/release-policy.md`.
2. **No `.gen.go` in IA bundles**: a CI smoke greps the `lazuli debug`
   and `lazuli examples` outputs and fails if any `dist/go/**`
   path appears. The contract is "IA never sees generated Go".
3. **Pattern coverage**: every `func ` in
   `crates/lazuli_codegen_go/tests/snapshots/` is preceded by
   `//lazuli:pattern`. Enforced by the emitter lint.
4. **Migration smoke**: every recipe under
   `migrations/recipes/<from>-to-<to>/` passes
   `lazuli upgrade input.lzi → output.lzi` exact-match.
5. **No regression in existing buckets**: `cargo test -p
   lazuli_cli` and `cargo test -p lazuli_codegen_go` stay green.

## Open questions

- **~~Pattern ID namespace registry~~**: **resolved** in §6.3 +
  ADR-7 amendment. `Pattern` is a closed Rust enum in
  `crates/lazuli_codegen_go::patterns`; emitter API takes it as a
  required parameter; output regex is defence-in-depth.
- **~~`SourceTag.Capsule` field semantic~~**: **resolved** by
  dropping the field entirely from `ErrorBase` and `SourceTag`
  in v0 (§7.1, §6.2, Anti-scope). Reintroduce as additive MINOR
  when the capsule contract lands formally.
- **`LibBugError.IssueURL`**: which repo? Recommend
  `https://github.com/lazuli-dev/lazuli/issues/new?template=lib-bug.md&error_code=<X>&origin=<Y>`
  — auto-fills the issue template. Gated on actually publishing
  the issue template.
- **D9 LSP jump pilot gate**: which pilot's evidence promotes?
  Replace earlier survey-based criterion (rubric flagged
  self-reported evidence as weak). Measurable criterion: D9
  promotes when **one** of (a) a pilot's debug-loop telemetry
  shows wall-clock `.lzi`-line resolution gap > 60 s across the
  pilot window OR (b) an IA debug session captured in
  `tests/golden/debug/` is observed making more than one
  round-trip to read generated Go for line resolution
  (auto-detected by the `lazuli debug` smoke's tool-call
  recorder). Both axes are mechanical; neither requires a
  survey.

## Revision history

- 2026-05-11 (initial): draft. Companion: `docs/release-policy.md`
  (drafted same day). Triggered by an architect conversation about
  AI-100x cost protection (see chat transcript referenced in
  commit message).
- 2026-05-11 (post-grade): four block-class fixes + five
  pass-with-notes addressed from `lazuli-language-architect`
  grade (verdict: pass-with-notes, 8.36/10). Changes:
  1. Renamed `Surface` discriminator → `Origin` throughout (`.lzx`
     vocabulary collision; ADR-2 amended).
  2. Unknown sentinel fallback routes to `OriginLibInternal` (was
     `OriginUserDSL`); added `CODEGEN-SENTINEL-001` to enumerate
     known sentinels exhaustively at codegen time.
  3. Dropped `Capsule` field from `ErrorBase` and `SourceTag`
     entirely (avoid baking placeholder into runtime ABI before
     capsule project lands); added to Anti-scope.
  4. `Pattern` is a closed Rust enum at emitter-API entry
     (§6.3 rewritten; ADR-7 amended). Output regex is
     defence-in-depth, not source of truth.
  5. Removed `ErrorBase()` convenience accessor — one canonical
     read path (`fe.Base.Feature`); ADR-1 amended.
  6. `app.observability` rationale added (where new field goes vs
     `app.logging` vs `app.tracing`); Phase 2 unification
     explicitly deferred in Anti-scope.
  7. Token budget assertion pins to `tools/tokenizer/version.txt`
     (currently `tiktoken 0.7.0 / cl100k_base`); §8.1 + Evals
     item 1 updated.
  8. `lazuli_version` pin granularity locked at MINOR;
     `LAZULI-VERSION-003` added for three-segment rejection;
     release-policy §`lazuli_version` pin tolerance added.
  9. D9 pilot-gate criterion replaced (survey → measurable
     telemetry: > 60 s `.lzi`-line resolution gap OR > 1 generated-Go
     round-trip in `lazuli debug` recorder).
  10. Anti-scope extended: pattern-as-language-surface,
      cross-host source-map abstraction, app.{logging,tracing,
      observability} unification.

  Open Questions OQ1 + OQ2 closed by the above.
