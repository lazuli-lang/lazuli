# Bucket Cycle: AI Debug Loop (L0→L2)

**Status**: design proposal. Stages 3–9 of the
`bucket=ai-debug-loop` pipeline. Implementation deferred to a
separate run with `mode=implement`.

**Audience**: language team (Lazuli core), Lazuli Go runtime team,
codegen-go owners.

**Date**: 2026-05-13.

**Pilot bucket**: cross-cutting hardening cycle. This is **not** a
feature bucket — it protects every other bucket's debuggability and
authoring economy under the AI-100x cost assumption. Lands once;
every subsequent bucket inherits the discipline.

**Companion**: `docs/release-policy.md` (2026-05-13) defines the
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
- The IR is semantic. Errors can carry capsule/feature/kind/op
  context structurally, not as concatenated strings.

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
review on 2026-05-13) motivate cells in this bucket:

4. **Lib bug vs user error disambiguation**. Rails has a public
   long-tail that absorbs "is this me or is this framework"
   triage. Lazuli does not. Without a `Surface` discriminator on
   the error envelope, every failure costs the IA tokens to decide
   whether to debug the user's `.lzi`, file an issue against
   Lazuli core, or escalate to the adapter author.
5. **DSL/IR version churn invalidates IA learned context**. Each
   release that changes language surface burns IA cache.
   `lazuli_version` pin + automated migration recipes prevent the
   silent half of that breakage.

The bucket lands **ten cells (D1–D10)** that together yield a debug
loop in which:

- **The IA reads `.lzi` source + structured error envelope + IR
  snippet — not generated Go.**
- **Perf attribution is automatic** via pprof labels keyed by
  `.lzi` op; `lazuli profile` reports flame-graph entries already
  mapped to ops, with template patterns annotated separately so
  the IA can tell user-introduced cost from codegen-introduced
  cost.
- **Every breaking release ships a migration recipe** with input +
  expected-output fixtures that run in CI; no recipe means no
  merge.

The closed-cycle criterion is the §0 8-item checklist (fixture +
check + inspect + doctor lint + generate Go + Lazuli Go runs +
eval/test + LSP hover) applied per cell rather than per
construct, since several cells span more than one IR kind. Each
cell carries its own smoke in §Cycle.

## Baseline (Stages 1-2 inventory)

| Surface | Today | Anchor | L-level |
|---|---|---|---|
| `SpanRef` in IR | `{ start: usize, end: usize }` only — no file path | `crates/lazuli_ir/src/lib.rs` SpanRef definition | **L0 (offsets), missing file id** |
| `SpanRef` use sites | ~30 IR kinds carry `Option<SpanRef>` | scattered across `lazuli_ir/src/lib.rs` | **wired but unresolvable to file:line** |
| Source path threading in codegen | only `app.name` as `source_label` in printer banner | `crates/lazuli_codegen_go/src/emitter/printer.rs` | **L0 (label only)** |
| `//line` directives in any codegen | none — `grep //line crates/` returns zero | confirmed | **missing** |
| `lazuli.Error` envelope | `{ Status, Code, Message, Data any }` + canonical codes | `runtime/go/lazuli/error.go` | **L1 (flat, untyped `Data`)** |
| Bucket sentinel errors | `errors.New("auth: foo")` × ~40 sites, untyped | `runtime/go/lazuli/auth/*.go`, `…/jobs/*.go`, `…/storage/*.go`, `…/notifications/*.go`, `…/migrations/*.go`, `…/webhooks/*.go` | **L1 sentinels, no envelope** |
| `errors.Unwrap` chains | none on `*lazuli.Error` today | `runtime/go/lazuli/error.go` | **missing** |
| `WithSource` context helper | none | n/a | **missing** |
| `observability/panic.go` recovery | `RecoverHTTP` + `RecoverScope`; TODOs for typed-error introspection; 500 envelope is literal `{"error":{"code":"internal_panic"}}` | `runtime/go/lazuli/observability/panic.go` | **L1 skeleton, missing typed wire** |
| pprof labels per op | none | n/a | **missing** |
| `lazuli profile` verb | does not exist | n/a | **missing** |
| `lazuli debug` verb | does not exist | n/a | **missing** |
| `lazuli examples --bundle` verb | does not exist | n/a | **missing** |
| `lazuli upgrade` verb | does not exist | n/a | **missing** |
| `lazuli_version` pin in `app.lzi` | not declared, not parsed | n/a | **missing** |
| `LZIR_SCHEMA` constant | exists in `crates/lazuli_ir/src/lib.rs` | `lazuli_ir::LZIR_SCHEMA` | **partial — exists but not surfaced to authors** |
| `app.observability` block | does not exist; sibling blocks `app.logging` and `app.tracing` exist | `crates/lazuli_ir/src/lib.rs` AppLogging/AppTracing | **missing** |
| Codegen banner | `// Code generated by lazuli; DO NOT EDIT.\n// source: <path>\n\npackage <name>\n` | `crates/lazuli_codegen_go/src/emitter/printer.rs` | **L1 — extensible** |
| Pattern annotation on emitted functions | none | n/a | **missing** |
| Migration recipes infra | does not exist | n/a | **missing** |
| Release policy doc | drafted 2026-05-13 | `docs/release-policy.md` | **L1 — landed pre-cycle** |

**Cross-cutting fact**: every gap above is **additive**. No existing
surface needs to be removed or have its semantics changed. This
bucket lands as widening on three sides (IR, runtime, CLI) plus
two new authoring surfaces (`lazuli_version` pin, `app.observability`
block).

## Linguagem proposta (Stage 3)

Authoring surface in this bucket is **deliberately small**. Most
of the work is below the surface (IR, codegen, runtime, CLI). The
two new authoring surfaces are:

### 3.1 `lazuli_version` pin on `app.lzi`

```lzi
app AcmeCRM
  lazuli_version "0.12"
  urls
    ...
```

Required from 1.0 onward; warning-only in 0.x. Loader compares the
pinned value with the CLI's `LZIR_SCHEMA`. Mismatch is doctor
diagnostic `LAZULI-VERSION-001` whose message names the migration
recipe path the user should run (`lazuli upgrade --from 0.11 --to
0.12`).

Rationale: a single declarative pin keeps the version-to-IA
contract explicit without polluting every feature file. See
`docs/release-policy.md §Doctor enforcement`.

### 3.2 `app.observability` block

```lzi
app AcmeCRM
  observability
    error_source dev,staging   # closed-catalog: any of "dev", "staging", "prod"
    panic_recover true         # default true; opt-out for debug builds
```

Closed-catalog list. `error_source` controls whether the typed
`*lazuli.Error.Source` field (e.g. `"crm.lzi:42:8"`) is included in
HTTP response envelopes per environment. Default `dev,staging` —
production strips. Doctor diagnostic `OBSERVABILITY-SOURCE-001`
flags unknown environment tokens.

Rationale: leaking `.lzi:line` in a prod 500 envelope is structural
information leak, low severity but non-zero. Authors opt in
explicitly.

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
    pub line_offsets: Vec<u32>,   // byte offset of each line start
}
```

`SourceMap` is **not embedded in `Module`**. It is passed
**alongside** the IR module into codegen as a separate argument
(`emit_module(module, source_map, options)`), and serialised
opt-in via a sidecar JSON (`<module>.sourcemap.json`) when the CLI
is invoked with `--with-source`. Rationale: SpanRef is referenced
in ~30 IR kinds and widening it cascades into every snapshot
test. The companion shape keeps the IR JSON ABI stable.

`SpanRef` itself stays at `{ start, end }`. The resolver helper:

```rust
// crates/lazuli_analyzer/src/source_map.rs (new)
impl SourceMap {
    pub fn resolve(&self, file: FileId, span: SpanRef) -> ResolvedLoc { ... }
}

pub struct ResolvedLoc { pub file: String, pub line: u32, pub column: u32 }
```

closes the loop. Where SpanRef appears today without file
context, codegen-side threading carries the `FileId` separately
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
compares against `LZIR_SCHEMA` when present; emits a 0.x warning
when absent (error at 1.0). When absent in 0.x, the warning message
carries `expected_value = "<major>.<minor>"` derived from the current
`LZIR_SCHEMA` constant, so the warning is self-correcting (the
author/AI can write the line directly from the message). When
present and mismatched, the message carries the recipe path
`migrations/recipes/<pinned>-to-<current>/` for `lazuli upgrade`.

### 4.3 `AppManifest.observability: Option<AppObservability>`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppObservability {
    pub error_source: Vec<String>,     // subset of {"dev", "staging", "prod"}
    pub panic_recover: bool,           // default true
    pub span_ref: Option<SpanRef>,
}
```

Sibling of existing `AppLogging` and `AppTracing` on `AppManifest`.

## Doctor + LSP (Stage 5)

Seven IR-driven diagnostics across this bucket:

| Code | Surface | Severity | Trigger |
|---|---|---|---|
| `LAZULI-VERSION-001` | `app.lzi:lazuli_version` | error (1.0), warning (0.x) | pin missing or mismatched with `LZIR_SCHEMA`; message names the recipe path |
| `LAZULI-VERSION-002` | `app.lzi:lazuli_version` | error | pinned to a version that has no migration path from current `LZIR_SCHEMA` |
| `OBSERVABILITY-SOURCE-001` | `app.observability.error_source` | error | token outside closed catalog `{"dev","staging","prod"}` |
| `OBSERVABILITY-PANIC-001` | `app.observability.panic_recover` | warning | set to `false` outside `dev` environment (loud opt-out for prod) |
| `CODEGEN-PATTERN-001` | codegen-go emitter (Rust-side lint) | build error | an emitted Go function lacks a `//lazuli:pattern <id> <version>` header. See §6.3 |
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
    ...
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
    Capsule: "crm",
    Feature: "customer",
    Kind:    "command",
    Op:      "create_customer",
    Source:  "features/customer.lzi:42:1",
})
```

`lazuli.WithSource` stores the `SourceTag` in a context key. Any
error constructor (D2's `lazuli.Wrap` / `lazuli.Errorf`) reads the
tag back and populates `*lazuli.Error.{Capsule,Feature,Kind,Op,Source}`
automatically. User-code in bucket adapters never has to thread
metadata manually — context is the carrier.

### 6.3 Pattern annotations + emitter lint

Every emitter function in `crates/lazuli_codegen_go` that produces
a Go function/method must emit a `//lazuli:pattern <id> <version>`
header before the function:

```go
//lazuli:pattern command_pgx_insert v1
//line features/customer.lzi:42:1
func HandleCreateCustomer(...) {
    ...
}
```

A Rust-side lint in `crates/lazuli_codegen_go::emitter` walks the
final output before write and errors `CODEGEN-PATTERN-001` if any
function-shaped block lacks a `pattern_id`. Implementation: a
linter pass on the printer output looking for `^func ` lines and
verifying the preceding non-blank lines contain `//lazuli:pattern`.

Rationale (architect 2026-05-13): retrofitting `pattern_id` after
200 templates exist is expensive; greenfield discipline is
trivial. The lint converts discipline into a build gate.

Pattern versioning is independent of `LZIR_SCHEMA`: a pattern's
v2 supersedes v1 when its emitted code shape changes materially
(different allocation pattern, different lock discipline,
different SQL strategy). `lazuli profile`'s attribution report
(§7.5) groups by `(pattern_id, version)`.

## Lazuli Go runtime (Stage 7)

### 7.1 Typed error hierarchy

Base envelope plus concrete child types. Closed enum for
`Surface`. **All types ship marked `EXPERIMENTAL` per
release-policy §Stability tiers**.

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
    Surface  Surface  // discriminator: who is responsible
    Status   int      // HTTP status; 0 = derived from Code
    Message  string   // human-readable
    Capsule  string   // "crm"
    Feature  string   // "customer"
    Kind     string   // "command" | "query" | "job" | ...
    Op       string   // "create_customer"
    Source   string   // "features/customer.lzi:42:8" (stripped per app.observability.error_source)
    Cause    error    // wrapped underlying error; participates in errors.Is/As chain
}

// Surface routes the IA debug response: UserDSL → read .lzi;
// LibInternal → file issue against lazuli core; CodegenBug →
// route to codegen-go owner; AdapterRuntime → external adapter.
type Surface uint8

const (
    SurfaceUserDSL Surface = iota
    SurfaceLibInternal
    SurfaceCodegenBug
    SurfaceAdapterRuntime
)

func (e *Error) Error() string { ... }
func (e *Error) Unwrap() error { return e.Base.Cause }
```

Child types each embed `ErrorBase` as a **named** field (not Go
embed). See ADR-1 for rationale.

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

func (e *FieldError) Error() string { ... }
func (e *FieldError) Unwrap() error { return e.Base.Cause }
func (e *FieldError) ErrorBase() ErrorBase { return e.Base }
```

Siblings: `PolicyError` (`Rule`, `Subject`, `Resource`, `Tenant`),
`TenantError` (`Axis`, `Expected`, `Actual`), `AdapterError`
(`Adapter`, `Op`, `RetryBudgetConsumed`, `RetryBudgetMax`),
`LibBugError` (`Component`, `Invariant`, `IssueURL`).

### 7.2 Envelope discipline (architect-mandated)

Bucket sentinel errors stay **bare** at their declaration site
(`errors.New("auth: password mismatch")`). Wrapping into
`*lazuli.FieldError{..., Cause: auth.ErrPasswordMismatch}` happens
**exactly once**, at the bucket → runtime boundary, in the
**codegen-generated handler**. Bucket authors never construct
`*lazuli.FieldError` directly.

Rationale (architect 2026-05-13): double-wrapping
(`FieldError{Cause: FieldError{Cause: sentinel}}`) forces the IA
to unwrap N times to find the sentinel. Codegen owning the
boundary keeps N = 1 by construction. Bucket authors stay
ignorant of the hierarchy — they emit sentinels and return them.

Codegen template (`crates/lazuli_codegen_go::emitter::command`)
will recognise known sentinel categories via a small static
mapping (`auth.ErrPasswordMismatch → FieldError{Field:"password",
Reason: FieldReasonMismatch}`) and emit the wrap. Unknown
sentinels fall through to a `*lazuli.Error` envelope with
`Surface: SurfaceUserDSL` and `Code: "internal"`.

**Enforcement (added per architect grade 2026-05-13)**: convention
alone is brittle. New doctor diagnostic `CODEGEN-WRAP-001`
(severity=error) scans Go source under `runtime/go/lazuli/<bucket>/`
(any subdir of the runtime that is NOT `runtime/go/lazuli/` top-level
and NOT a generated `.gen.go`) and fails the build if any file
imports a typed-error constructor (`lazuli.FieldError{...}`,
`lazuli.PolicyError{...}`, etc. as struct literals or via
`lazuli.NewFieldError`-style constructors). Allowed: import the
**type** for `errors.As(...)` destructuring; forbidden: construct
the struct value. The lint is straightforward AST traversal
(~30 LOC) and converts the one-wrap rule from convention into a
build gate. Bucket authors get a clear error message:
"construct typed errors from sentinels; codegen wraps at the
boundary. See bucket-ai-debug-loop-cycle.md §7.2."

### 7.3 pprof labels

`lazuli.StartOp(ctx, tag) (ctx, end func())` opens a pprof label
scope keyed by `SourceTag`:

```go
ctx, end := lazuli.StartOp(ctx, lazuli.SourceTag{
    Capsule: "crm", Feature: "customer",
    Kind: "command", Op: "create_customer",
})
defer end()
```

Implementation wraps `pprof.Do(ctx, pprof.Labels("capsule",
"crm", "feature", "customer", "kind", "command", "op",
"create_customer"), fn)`. Codegen injects the call at the top of
every handler, reusing the same `SourceTag` D5 already constructs.

### 7.4 `observability/panic.go` integration

Today (`runtime/go/lazuli/observability/panic.go`) `RecoverHTTP`
and `RecoverScope` carry TODOs. After this cell:

- `errors.As(rec, &lz)` extracts `ErrorBase` from any panic that
  wrapped a `lazuli.Error`; the trace event payload
  (`command_run` / `job_run` / `webhook_run`) carries
  `{capsule, feature, kind, op, source, surface}`.
- 500 envelope reads from the typed error. Per
  `app.observability.error_source`, the `Source` field is included
  in dev/staging response bodies and stripped in prod.
- Surface `LibInternal` / `CodegenBug` panics emit an additional
  `lazuli_internal_panic` trace event with the `IssueURL` field
  populated for telemetry consumers.

### 7.5 `lazuli profile` reporting contract

The runtime emits pprof profiles with the labels from §7.3 plus
the comment `//lazuli:pattern <id> <version>` headers preserved
in the debug info (Go preserves them per `//line` adjacency). The
`lazuli profile <profile.pb.gz>` CLI verb (§8.3) reads both,
groups frames by `(capsule, feature, kind, op)`, and reports:

```
Top 10 ops by CPU:
  crm.customer.command.create_customer    12.4%  (pattern: command_pgx_insert v1)
  crm.invoice.query.list                   8.1%  (pattern: query_pgx_list v2)
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

### 8.1 `lazuli debug --error <json> --capsule <name>`

Reads a structured error envelope from stdin or `--error`, walks
the IR for the named capsule, and emits a minimal context bundle:

- The originating `.lzi` block (resolved via `Source`).
- The IR snippet for the failing op (one feature, one kind, one op).
- The error envelope itself, pretty-printed with `Surface` →
  recommended debug surface (read .lzi / file issue / contact
  adapter author).

Target ceiling: **< 4 000 tokens per debug bundle** for the
canonical fixture. CI smoke compares against a frozen fixture.

### 8.2 `lazuli examples --bundle [--out <path>]`

Emits a deterministic JSONL of curated `.lzi` examples for IA
file-load / RAG consumption:

```jsonl
{ "name": "command_with_safety", "intent": "create command guarded by a safety validator", "lzi_source": "...", "ir_snippet": "...", "common_errors": ["safety_unbound", "validator_pii_class_mismatch"] }
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
   Speculative examples live in `docs/recipes/` outside the
   bundle.

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

### ADR-1: Named field `Base ErrorBase`, not Go embed

**Decision**: child error types carry `ErrorBase` as a named
field, not an embedded type.

**Considered**:

- **Option A (embed)**: `type FieldError struct { Error; Field string }`. Pros: ergonomic (`fe.Capsule` reads through). Cons: overriding `Error() string` on the child silently breaks the promotion; `errors.As(err, &Error{})` against a child's pointer is subtly incorrect because of method-set rules.
- **Option B (named)**: `type FieldError struct { Base ErrorBase; Field string }` + explicit `Error()` per child. Pros: no method promotion fragility; refactor-safe. Cons: `fe.Base.Capsule` is wordier.

**Decision**: Option B. Refactor safety trumps a few extra
characters; the IA debug loop is the primary consumer and pays
the lookup cost once, not per access.

Each child also exposes `ErrorBase()` accessor for ergonomic
context-walking without going through `.Base`.

### ADR-2: `Surface` is closed 4-variant enum, sized `uint8`

**Decision**: `SurfaceUserDSL | SurfaceLibInternal |
SurfaceCodegenBug | SurfaceAdapterRuntime`. No fifth variant in v0.

**Rationale**: four variants cleanly partition who is responsible
for the failure. A fifth (`SurfacePlatform`? `SurfaceConfig`?)
risks bleeding the discriminator's purpose. If pilot evidence
demands a fifth surface, it ships as MINOR (additive enum
variant) with experimental marker on the new constant — see
release-policy §Stability tiers.

### ADR-3: Sidecar `SourceMap`, not embedded in `Module`

**Decision**: `SourceMap` ships as a separate IR companion
serialised to `<module>.sourcemap.json` only when `--with-source`
is passed.

**Considered**:

- **Option A (embed in `Module`)**: every IR JSON carries the
  source map. Pros: single artifact. Cons: doubles IR JSON size,
  cascades into every snapshot test, every `lazuli inspect` call
  pays the cost even when not debugging.
- **Option B (sidecar)**: `<module>.json` + `<module>.sourcemap.json`.
  Pros: opt-in cost, IR JSON ABI stable, snapshot tests unaffected.
  Cons: two files, must be kept in sync (we already do this for
  `--with-source` via the same emit pass).

**Decision**: Option B. Sidecar matches existing CLI verb
discipline (`--with-source` is opt-in; default workflow doesn't
pay the cost).

### ADR-4: Pattern annotations are codegen-discipline, not authored

**Decision**: `pattern_id` is set by emitter functions, not
declared in `.lzi`. Authors never see or write pattern_id.

**Rationale**: pattern_id tracks **codegen template identity**,
not user intent. An author writes `command create_customer`; the
codegen decides whether to emit it as `command_pgx_insert v1`
(simple insert) or `command_pgx_insert_with_audit v1`
(insert + audit row). Author surface stays declarative; pattern
choice is the emitter's responsibility.

This also keeps pattern semver internal to codegen-go — a v2
template can ship as a PATCH release without authoring impact, as
long as runtime behavior is unchanged. Perf-only template
improvements ship freely.

### ADR-5: LTS dual-channel deferred

See `docs/release-policy.md §Why this exists`. Decision
restated here for visibility: LTS is the 20% caudal of the
churn-burn-IA-cache problem; pin + auto-migrator solves the 80%.
Revisit after second production pilot.

## Cycle (Stage 8 — execution plan)

| # | Cell | Owner | Smoke | Depends on |
|---|---|---|---|---|
| **D1** | `SourceMap` companion in IR + resolver helper | [lang] | `inspect --with-source` resolves span → `crm.lzi:42:8`. `LZIR_SCHEMA` bumps to 0.12.0 with migration recipe. | — |
| **D2** | Typed error hierarchy (Error base + FieldError/PolicyError/TenantError/AdapterError/LibBugError + Surface discriminator) | [ambos] | `errors.As(err, &fe)` recovers Field/Reason/Path without regex. All ship `EXPERIMENTAL`. | — |
| **D3** | Bucket sentinels envelopped at runtime boundary in codegen | [runtime + codegen] | `errors.Is(err, auth.ErrPasswordMismatch)` ok; `errors.As(&fe)` recovers structured fields | D2 |
| **D4** | Codegen emits `//line <file>:<line>` per handler | [lang] | Panic shows `crm.lzi:42`, not `crm.gen.go:380`. Verified via Go `delve` integration. | D1 |
| **D5** | Codegen injects `ctx = lazuli.WithSource(...)` in handlers | [lang] | Error returned by generated handler carries `Source` automatically | D2, D4 |
| **D6** | `observability/panic.go` integrates typed envelope + `app.observability` block | [runtime] | Trace event carries `{capsule, feature, kind, op, source, surface}`. Doctor `OBSERVABILITY-SOURCE-001` enforced. | D2 |
| **D7** | Docs AI loop + `lazuli debug` verb + `lazuli examples --bundle` verb + inclusion policy | [lang] | `lazuli debug` returns < 4k tokens; `lazuli examples` produces JSONL deterministic | D1, D2 |
| **D8** | pprof labels + `lazuli profile` verb + codegen-pattern annotation + emitter lint (promoted from pilot-gated) | [ambos] | `lazuli profile <pb.gz>` lists top-N ops by cpu/alloc; pattern_id distinguishes user vs template | D4 |
| **D9** | LSP jump-from-Go-stack | [lang], pilot-gated | Click on panic from IDE opens `.lzi` | D1, D4 |
| **D10** | `lazuli_version` pin + `lazuli upgrade` migrator + release policy doc + recipe CI gate | [lang] | `lazuli upgrade --from 0.11 --to 0.12` rewrites canonical fixture without loss; doctor `LAZULI-VERSION-001` blocks version mismatch | release-policy.md (landed) |

**Sequencing rules:**

- **D1, D2, release-policy doc** ship in parallel as foundation
  (no dependencies among them).
- **D3, D4, D6** run after D2/D1 close. D3 needs D2's hierarchy
  definitions; D4 needs D1's SourceMap.
- **D5, D8** depend on D4 (line directives) and D2 (SourceTag
  shape).
- **D7** depends on D1/D2 (structured error + IR resolver). Can
  run parallel with D5/D8 once those land.
- **D10** depends only on release-policy.md (the recipe CI gate
  needs the policy documented). Migrator implementation can run
  parallel with D5+.
- **D9** is pilot-gated — gate on real IDE consumer demand.

**Wave plan (Codex parallelism):**

- Wave 1: D1 + D2 (foundational, no inter-dependency).
- Wave 2 (after D1+D2 cherry-pick): D3 + D4 + D6 + D7 + D10.
- Wave 3 (after D4): D5 + D8.
- Wave 4 (pilot-gated): D9.

## Onde fica nuance

### D8 pattern_id discipline depends on us

Every codegen template that involves a non-trivial choice (allocation,
lock, batch, SQL strategy) must carry a `pattern_id`. Skipping = the
pattern becomes invisible to `lazuli profile`. The §6.3 lint converts
this into a build gate, but the human-side discipline of **choosing a
pattern_id** that's stable and meaningful across versions is still
team responsibility. A `pattern_id = "thing"` that gets renamed for
cosmetic reasons every release is worse than no annotation.

Mitigation: pattern_ids are reviewed in the same PR that introduces
them. Renaming an existing pattern_id requires a release-policy
recipe (it's a `pattern_v1 → pattern_v2` migration even if behavior
is identical, so perf metrics keep continuity).

### D10 release policy is a political commitment

The hardest part of `lazuli upgrade` is not the technology — it's the
human discipline of **not making breaking changes covertly**. The CI
gate (`MIGRATION-RECIPE-001`) helps but it cannot detect "we shipped
a MINOR that changed semantics of an existing keyword". That requires
us to actually read each diff and ask "did this change observable
behaviour for existing `.lzi` files?".

Mitigation: pre-1.0 we additionally run the **lazuli-language-architect**
agent against every release diff with the question "does this change
the meaning of any keyword for existing pinned versions?". Architect
review catches subtle changes; CI catches absence of recipes.

### Migration recipes for typed error hierarchy

When we add `RateLimitError` in 0.13, or perceive that `FieldError.Reason`
needs a new variant, this is a breaking change to **Go-side consumer
code**. `lazuli upgrade` covers `.lzi` transformation but doesn't cover
user-written Go code doing `errors.As(err, &lazuli.FieldError{})`.

Decision (this proposal, ADR-implicit): the typed hierarchy is
**experimental** until first pilot consumer ships it in production
Go code. From that point, every addition to `FieldReason` enum or
new `*Error` child type ships as MINOR with a Go-side migration recipe
(under `migrations/recipes/<from>-to-<to>/go/`) that rewrites consumer
code via `gofmt -r` patterns.

Before that pilot consumer exists, additions ship freely as MINOR with
an `// EXPERIMENTAL` docstring update.

### Lib bug vs user error: imperfect by construction

`Surface` discriminator routes the IA correctly in 95% of cases. The
remaining 5% are panics in third-party deps (pgx, river, slog) that
Lazuli surfaces as `SurfaceLibInternal` — they're not actually Lazuli
bugs, but we can't tell the IA that without per-dep classification.

Mitigation: `LibBugError.Component` field carries the offending
Go package path. The IA's debug bundle includes a "third-party
component" indicator when `Component` is outside `lazuli.dev/runtime/*`,
routing to upstream-issue mode rather than file-issue-against-Lazuli
mode.

## Stage 9 — closed-cycle smoke

The bucket is "closed" when:

1. **Fixture**: `examples/marketplace-mini/` has `lazuli_version "0.12"`
   pin + an `observability` block. Generated code uses `//line` +
   `//lazuli:pattern` + `WithSource`. `examples/full-capsule/` stays
   exempt as canonical codegen fixture (no manifest required).
2. **Check + inspect**: `lazuli check examples/marketplace-mini` passes;
   `lazuli inspect --with-source examples/marketplace-mini` emits IR
   JSON + sidecar source map.
3. **Doctor**: 7 new diagnostics from §5 trigger correctly on
   negative fixtures.
4. **Codegen Go**: `lazuli generate go examples/marketplace-mini` emits
   `//line` + `//lazuli:pattern` + `WithSource` in every handler.
   Emitter lint refuses to emit if `pattern_id` is missing.
5. **Lazuli Go runs**: `go build` against generated output succeeds;
   `go run` against a smoke binary panics with `panic: <crm.lzi:42:8>`
   in the trace.
6. **Eval/test**: `lazuli debug` smoke fixture returns < 4k tokens
   for a known error. `lazuli profile` against a captured pprof
   produces expected top-N output.
7. **LSP**: hovers for `lazuli_version`, `observability`,
   `error_source`, `panic_recover` exist; closed-catalog completion
   on `error_source` works.
8. **Migration round-trip**: `migrations/recipes/0.11-to-0.12/` ships
   at least one recipe (e.g., `add-lazuli-version-pin/`) whose
   `input.lzi → output.lzi` smoke passes via `lazuli upgrade`.

## Risks

| Risk | P | Impact | Mitigation |
|---|---|---|---|
| `//line` directives interact unexpectedly with delve / go pprof | 15% | Stack traces show wrong file:line | v0 uses per-function granularity (Go-standard, well-tested); per-statement deferred to pilot |
| Typed error hierarchy shape needs revision before 1.0 | 60% | Migration debt for Go consumers | Ship EXPERIMENTAL; revise freely until first pilot consumer ships it |
| Pattern lint blocks merge of legitimate emitter additions | 25% | Velocity friction | Lint allows `//lazuli:pattern <id> draft` for unmerged work; CI rejects `draft` only on release branch |
| `SourceMap` sidecar drifts out of sync with IR | 20% | Source resolution returns wrong line | Single emit pass writes both atomically; smoke test verifies pair |
| `lazuli upgrade` recipes accumulate without test discipline | 40% | Migrator silently rots | `MIGRATION-RECIPE-002` CI gate runs `input → output` smoke per recipe; lint refuses recipe without fixtures |

**Overall apetite**: ✅ Aggressive. The cells are sequenced for
parallel execution; the risks are well-understood; the IR-side and
runtime-side cells can ship behind feature flags (`smoke_debug_loop`)
so we never block existing fixtures.

## Out of scope (rejected for v0)

- **Distributed trace propagation** (W3C traceparent → `SourceTag`).
  Defer to observability bucket follow-up. The §7.4 trace event
  carries SourceTag locally; cross-service propagation requires
  separate proposal.
- **Source map per-statement** (vs per-function). Pilot-gated. v0
  is per-function; promote only if real stack ambiguity appears.
- **LTS dual-channel**. Per release-policy §Why this exists.
- **Public error catalog browser** (web UI for `errors.As` discovery).
  Tooling sugar; deferred.
- **AI-mediated diff review** for breaking changes detection. Manual
  architect review covers v0; automate later.

## Decision gate

Approve to proceed with cells D1–D10 implementation. Suggested
order (Codex parallelism):

1. **Wave 1**: D1 + D2 (foundational, 2 cells parallel).
2. **Wave 2** (after Wave 1 cherry-pick): D3 + D4 + D6 + D7 + D10
   (5 cells parallel).
3. **Wave 3** (after D4): D5 + D8 (2 cells parallel).
4. **Wave 4** (pilot-gated): D9.

Estimated total: **9-10 cells, 1-2 days at 5 parallel Codex agents**
assuming this proposal grades ≥ 9.0 with no major redesigns.

After D8 lands, `lazuli profile` against a real workload validates
the perf attribution loop. After D10 lands, the upgrade discipline
is enforceable. Lazurite is then ready for **first downstream product
port** with the debug economy in place.

## Tightening from architect grade (2026-05-13)

Applied inline before commit:
- §7.2 added `CODEGEN-WRAP-001` lint (typed-error constructor import
  forbidden in bucket subdirs) — gap 1.
- `docs/release-policy.md` added §Release PR review (process gate)
  for architect-agent verdict on every release PR — gap 2.
- `docs/release-policy.md` added §Stability surfacing in `inspect`
  (`"stability"` field in JSON output) — gap 3.
- §4.2 `lazuli_version` 0.x default carries `expected_value` in the
  warning message for self-correction — gap 5.

Deferred to D11 housekeeping cell (post-Wave-3):
- **`draft` pattern_id staleness**: a pattern_id can be marked `draft`
  in feature branches but must not appear on `main` for >7 days.
  Implementation: CI cron job lists `main` commits introducing
  `//lazuli:pattern <id> draft`; if older than 7 days, fail next
  build. Bound the escape hatch.
- **Examples bundle decay**: an example whose justifying pilot(s) no
  longer exist rotates to `docs/recipes/` at the next MINOR review.
  Process rule, not code — added to D7's inclusion policy doc.
- **Wave 2 internal ordering**: D3 (codegen wraps sentinels) lands
  before D6 (panic.go typed envelope integration) within Wave 2.
  Both touch the error-envelope contract; D2 defines shape, D3
  consumes downstream, D6 reads typed payloads — keep D3 in front
  to avoid D6 rebasing.

## Revision history

- 2026-05-13: initial proposal. Triggered by user observation that
  debuggability across framework layers had been deferred too long;
  AI-100x cost assumption changes the economics fundamentally.
- 2026-05-13 post-grade: applied 4 tightening gaps from architect
  9.16/10 review (CODEGEN-WRAP-001 lint, release PR agent review
  process gate, stability surfacing in inspect, self-correcting
  version warning). Deferred 3 housekeeping items (draft staleness,
  examples decay, wave 2 internal ordering) to D11 cell.
