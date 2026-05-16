# Proposal — LSP + CLI surface cross-feature symbol origin

**Status:** L0 v0.2 DRAFT — 2026-05-16 (v0.1 graded PASS 8.92/10 via `lazuli-language-architect`; v0.2 applies 3 pre-dispatch polishes: mini-JSON examples for command/query/event/aggregate kinds, typed `SourceLocation::Builtin` variant replacing `<builtin>` string sentinel, `.lzx → .lzi` cross-target inspection example with `referenced_by` field)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Driver:** Hostpoint cross-feature audit 2026-05-16 surfaced a `Lazuli capitulating` smell — authors are tempted to write `# Gender imported from account` comments on field declarations because the IDE/CLI never tells them where `Gender` lives.
**Honors:** `docs/invariants.md` (closed grammar; no new vocabulary), `docs/design-principles.md` Rule Zero (the comment was the user reaching for vocabulary the language already has; surface it via tooling instead of growing syntax), memory `feedback_normative_not_narrative_2026-05-15` (procedence is metadata, not source content), `CLAUDE.md` founding principle (wire-thin: the analyzer already resolves; LSP/CLI are read-only consumers).
**Anchors:** `crates/lazuli_ir/src/lib.rs:875-882` (the existing `QualifiedName` struct), `crates/lazuli_analyzer/src/lib.rs:4144-4157` (`lower_qualified_name`), `crates/lazuli_lsp/src/lib.rs:137-174` (current hover handler), `crates/lazuli_cli/src/main.rs:119-127` + `:3686-3712` (current `inspect` command).
**Tracked source:** `docs/next-checklist.md` §"From cross-feature symbol resolution review (2026-05-16)".

---

## §1. Status & motivation

When `host.lzi` declares `field gender: Gender required` and `Gender` is defined in `account.lzi` (imported via `uses account`), the analyzer resolves the reference internally but neither the LSP nor the CLI surface the resolution to the author. A reader — human or LLM — sees `Gender` and has no anchor to the defining file. The shortest path to comprehension is grep, which is fine for one symbol and intolerable across a feature.

The 2026-05-16 external cruel review caught this as a `Lazuli capitulating` smell: authors were adding `# Gender imported from account` comments on field declarations to compensate. Those comments violate two existing memories:

- `feedback_normative_not_narrative_2026-05-15` — `.lzi` is a spec; comments-as-metadata pollute the prescriptive surface.
- `project_comments_are_vocabulary_smell_2026-05-15` — recurrent comment patterns signal a missing semantic slot. In this case the missing slot is **tooling**, not language vocabulary: the data already exists in the analyzer; the gap is that it doesn't reach the reader.

The fix is purely additive at the tooling boundary. Zero new grammar, zero new IR vocabulary, zero new doctor rules in v0.1. The analyzer already knows the answer; this proposal wires the answer through to LSP hover and a CLI subcommand.

### §1.1 Gap table

| Layer | Today | After this proposal |
|---|---|---|
| Analyzer | Resolves `Gender` to its declaring feature internally via the cross-feature index in `crates/lazuli_codegen_go/src/emitter/cross_feature.rs:60-100`; the result is consumed by codegen but never returned to LSP/CLI. `QualifiedName.feature` stays `None` for surface-authored references (`crates/lazuli_analyzer/src/lib.rs:1311-1314`). | Promotes the codegen-only `CrossFeatureIndex` to an analyzer-public artifact (`ir::SymbolOriginIndex`, §6.1). Per-reference resolution is added to the `Module` sidecar so any consumer reads the same answer. Surface IR (`QualifiedName`) **unchanged**. |
| LSP hover on `Gender` (cross-feature) | Returns rich keyword-catalog markdown only for closed-catalog kinds (`crates/lazuli_lsp/src/lib.rs:147-162`); for user-defined names like `Gender` it returns `None`. | Returns a Markdown block stating `Gender (enum)` plus an "Origin" section: `Defined in: features/account/account.lzi:42 (enum Gender)` and `Imported via: uses account (features/host/host.lzi:4)`. §4. |
| LSP hover on `Gender` (local) | Same as above — `None`. | Returns the same shape with `Imported via:` omitted (local definition). §4.3. |
| `lazuli inspect <symbol>` (CLI) | Does not accept a symbol argument. Today the command takes a path to a `.lzi` file or a project directory (`crates/lazuli_cli/src/main.rs:119-127` + `:3686-3712`). | Accepts a symbol argument **in addition** to the existing path-based mode (§5). Emits a JSON record (§5.2) for one symbol. Path-based mode is unchanged. |
| Doctor procedence comments | Not flagged. | Not flagged in v0.1 — tracked as a follow-up cut (§7) once the surfacing has shipped and pilots stop writing the comments organically. |

**Why now:** Hostpoint is the active pilot (memory `project_strategic_pivot_2026-05-15`). The Hostpoint capsule has the highest cross-feature reference density of any pilot to date (`host` references `account.Gender`, `account.Address`, `billing.Money`, `catalog.PropertyKind`). If we don't ship the surface before Hostpoint stabilizes, authors will either (a) inline-comment procedence everywhere or (b) collapse the cross-feature decomposition back into a monolith. Both outcomes destroy the cross-feature evidence the framework needs.

**Boundary discipline:** this proposal is **target-closing, not boundary-moving.** It moves data that already exists internally to a consumer surface. No new framework primitive. The `≥3-pilot evidence` rule from `feedback_scope_discipline_2026-05-14.md` does not apply.

---

## §2. Scope

### In scope

1. **Analyzer exposes resolution metadata.** A new `ir::SymbolOriginIndex` (sidecar to `Module`, **not** embedded — mirrors the `SourceMap` ADR-3 pattern at `crates/lazuli_ir/src/lib.rs:42-53`) populated during lowering. One entry per declared symbol with `(feature, name) -> origin`. §6.
2. **LSP hover enrichment** for cross-feature and local references to types, enums, scalars, semantic types, commands, queries, events, records, and aggregates. The hover text appends an **Origin block** below whatever the LSP already returns. §4.
3. **CLI `lazuli inspect <qualified-symbol>` subcommand** that emits a structured JSON record for one symbol. Reuses the existing `inspect` command name. §5.
4. **JSON output shape locked** in §5.2. Treat as a stable contract for downstream consumers (future audit-skill MVP, agent tooling, third-party readers).
5. **One smoke fixture** under `examples/marketplace-mini/` that asserts hover + CLI output for one cross-feature reference (`booking.Money` resolving to `billing`).

### Non-goals

1. **No go-to-definition jumping.** Already covered by existing LSP capabilities (`definition_provider`) for in-file references; cross-file jumping is a separate cell tracked outside this proposal.
2. **No rename-refactor support.** Out of scope; rename across features would require a write-side LSP capability the language server doesn't expose today.
3. **No workspace symbol search.** Existing `document_symbol_provider` covers per-file outline; cross-workspace search is a separate concern.
4. **No new `@-namespace` entries.** Authors cite cross-feature symbols the same way they always have (`Gender`, `account.Gender`); the analyzer resolves; the surface is unchanged.
5. **No new doctor rule in v0.1.** A `COMMENTS-AS-VOCABULARY-PROCEDENCE-001` rule that flags `# <Symbol> imported from <feature>` patterns is tracked as a polish follow-up (§7); it lands **after** §3–§6 ship so doctor doesn't fire on a smell the tooling hasn't yet eliminated.
6. **No CLI text-mode renderer.** §5 emits JSON only. Pretty-printed text output is a deferred polish item.
7. **No support for unresolved symbols.** If `lazuli inspect host.Banana` is invoked and `Banana` doesn't exist anywhere, the CLI returns a non-zero exit and a structured error (§5.3); it does NOT degrade to a best-effort response.
8. **No `lazuli symbol <symbol>` alternative subcommand.** §5.1 picks `inspect` extension over a new subcommand and justifies.
9. **No mobile / RN / Expo concerns.** The LSP runs against `.lzi` source regardless of target.

---

## §3. The information flow

```
                                ┌─────────────────────────────────┐
                                │  Author writes host.lzi:        │
                                │    uses account                 │
                                │    resource Host                │
                                │      field gender: Gender ...   │
                                └─────────────────────────────────┘
                                              │
                                              ▼
   ┌──────────────────────────────────────────────────────────────┐
   │ lazuli_syntax — parses; both `account.lzi` and `host.lzi`    │
   │ produce FeatureSkeletons with `uses: Vec<String>`            │
   │ (crates/lazuli_ir/src/lib.rs:314).                           │
   └──────────────────────────────────────────────────────────────┘
                                              │
                                              ▼
   ┌──────────────────────────────────────────────────────────────┐
   │ lazuli_analyzer — lower_feature_skeleton                     │
   │ (crates/lazuli_analyzer/src/lib.rs:2059) produces            │
   │ ir::Feature with `enums`, `resources`, `records`,            │
   │ `commands`, `queries`, `events` each carrying SpanRef.       │
   │                                                              │
   │ NEW: after lowering every feature, build                     │
   │ ir::SymbolOriginIndex (§6.1):                                │
   │   - Walk every enum/resource/record/scalar declaration.      │
   │   - Index `(feature, name) -> SymbolOrigin { defined_at,     │
   │     kind, previous_names }` keyed on the OWNING feature.     │
   │   - Walk every `Feature.uses` entry. For each pair           │
   │     `(importer, imported)`, record an ImportEdge with the    │
   │     SpanRef of the `uses` clause.                            │
   └──────────────────────────────────────────────────────────────┘
                                              │
                            ┌─────────────────┴─────────────────┐
                            ▼                                   ▼
   ┌────────────────────────────────────┐  ┌────────────────────────────────┐
   │ LSP hover handler                  │  │ CLI `lazuli inspect <sym>`     │
   │ (crates/lazuli_lsp/src/lib.rs:137) │  │ (crates/lazuli_cli/src/main.rs │
   │                                    │  │  :119, :3686)                  │
   │ 1. word_at_position → "Gender".    │  │ 1. Parse `<feature>.<name>`    │
   │ 2. Lower current document.         │  │    or bare `<name>`.           │
   │ 3. Query SymbolOriginIndex:        │  │ 2. Lower workspace.            │
   │    resolve(current_feature,        │  │ 3. Query SymbolOriginIndex.    │
   │    "Gender") -> SymbolOrigin.      │  │ 4. Emit JSON record per §5.2.  │
   │ 4. Append Origin block to hover    │  │                                │
   │    Markdown.                       │  │                                │
   └────────────────────────────────────┘  └────────────────────────────────┘
                            │                                   │
                            ▼                                   ▼
                  Editor renders enriched           stdout: structured JSON
                  Markdown hover bubble.            consumed by humans, agents,
                                                    or future audit-skill.
```

### §3.1 What the analyzer exposes

The new artifact is **`ir::SymbolOriginIndex`** (§6.1). It is a **sidecar** to `ir::Module`, populated by a new analyzer pass and serialized to `<module>.symbol-origin.json` when `--with-symbol-origin` is requested. Sidecar-not-embedded mirrors the existing `SourceMap` decision documented at `crates/lazuli_ir/src/lib.rs:42-53` (ADR-3 — keeps the IR JSON ABI stable and avoids cascading snapshot churn).

The index has three exposed operations:

| Operation | Signature | Used by |
|---|---|---|
| `resolve(current_feature, name)` | `(&str, &str) -> Option<&SymbolOrigin>` | LSP hover (resolves the symbol-under-cursor in the current file's scope) |
| `lookup(qualified)` | `(&str /* "account.Gender" or "Gender" */) -> Option<&SymbolOrigin>` | CLI `inspect <sym>` |
| `imports_for(importer)` | `&str -> &[ImportEdge]` | LSP hover (resolves which `uses` clause brought in the symbol) |

**`SymbolOrigin` shape** (full Rust definition in §6.2):

```rust
pub struct SymbolOrigin {
    pub feature: String,       // owning feature; "account"
    pub name: String,          // "Gender"
    pub kind: SymbolKind,      // Enum / Resource / Record / Scalar / Semantic / Command / Query / Event / Aggregate
    pub defined_at: SourceLocation,  // file + line; same path used by SourceMap
    pub previous_names: Vec<String>, // preserves rename history; surfaces as "formerly: Sex"
}

pub struct ImportEdge {
    pub importer: String,      // "host"
    pub imported: String,      // "account"
    pub uses_at: SourceLocation,  // line of the `uses account` clause in host.lzi
}
```

`SourceLocation` is a `{ file: String, line: u32, column: u32 }` wrapper that resolves a `SpanRef` against the `SourceMap` companion (`crates/lazuli_ir/src/lib.rs:50-62`). The analyzer builds it once during the pass; LSP/CLI consume the resolved form.

### §3.2 What the LSP/CLI consume

Both consumers receive the **same** `SymbolOrigin` record. The presentation differs (Markdown for hover, JSON for CLI) but the data is identical. This is the contract that prevents drift: if a future caller adds a third surface (an MCP tool, an audit-skill rule, a docs-as-IR-projection emitter), it consumes the same struct.

---

## §4. LSP hover shape

### §4.1 Current behavior

The hover handler at `crates/lazuli_lsp/src/lib.rs:137-174` returns Markdown only for closed-catalog keywords (`command`, `query.list`, `api`, `policy`, `effect`, etc.) via `rich_keyword_hover` (`:13822`) or the brief one-liner via `keyword_description` (`:13030`). For user-defined names like `Gender` or `Address`, `word_at_position` (`:12825`) returns the word, but neither catalog hits, and the handler returns `Ok(None)` at `:163-164`.

### §4.2 New behavior — cross-feature reference

When `word_at_position` returns a word that is **not** in the closed-catalog hover map AND the current document declares it via `uses <feature>` AND `SymbolOriginIndex::resolve(current_feature, word)` returns `Some(origin)`, the handler returns a Markdown block:

```
`Gender` (enum)

**Origin**
- Defined in: `features/account/account.lzi:42`
- Imported via: `uses account` at `features/host/host.lzi:4`

**Variants:** `female`, `male`, `non_binary`, `prefer_not_to_say`
```

The block has three sections, each optional:

1. **Header line** — symbol name + `(kind)`. Always present.
2. **Origin** — `Defined in:` always; `Imported via:` only when the resolved origin is in a different feature than the current document.
3. **Body** — kind-specific extras (enum variants, resource fields, record fields, command input shape, query result shape). Bounded; for record/resource, the body lists the **first 6 fields** + `… N more` if longer. The LSP hover is a hint, not a full documentation viewer.

### §4.3 New behavior — local reference

When the resolved origin is in the **same** feature as the current document, the `Imported via:` line is omitted:

```
`Booking` (resource)

**Origin**
- Defined in: `features/booking/booking.lzi:18`

**Fields:** `id: ID`, `listing_id: ID`, `guest_id: ID`, `check_in: Date`, `check_out: Date`, `status: BookingStatus` … 3 more
```

### §4.4 New behavior — closed-catalog vocabulary unchanged

If `rich_keyword_hover(word)` or `keyword_description(word)` returns `Some`, the existing path is unchanged. The Origin block is appended **only** when the resolution succeeds AND the symbol is an analyzer-tracked declaration (enum/resource/record/scalar/semantic/command/query/event/aggregate). This preserves all existing hover output bit-for-bit.

### §4.5 Cascade order

The handler evaluates in this order:

1. **Closed catalog hit** (existing path) — return as today.
2. **`SymbolOriginIndex::resolve(current_feature, word)` hit** — return enriched Markdown per §4.2/§4.3.
3. **No hit** — return `None` as today.

The new behavior is purely additive at step 2; step 1 fires first so no closed-catalog hover regresses.

### §4.6 Performance

The LSP backend already holds `documents: HashMap<Url, String>` (`:65`). The new path:

- Lowers the current document on every hover **only if the document changed since last hover** (cache by hash of document text, invalidate on `did_change`).
- Looks up the SymbolOriginIndex in O(log n) via a `BTreeMap<(String, String), SymbolOrigin>`. Lowering is the dominant cost; index lookup is negligible.

Worst case: a 2000-line `.lzi` lowered on every hover request. The analyzer is fast (existing diagnostics path runs on every `did_change` already); the marginal cost is one hover-time index build per file change, not per hover. Acceptable.

---

## §5. CLI `lazuli inspect <qualified-symbol>` shape

### §5.1 Subcommand choice — extend `inspect`, not new `symbol`

Two candidates considered:

| Option | Surface | Verdict |
|---|---|---|
| **A. New `lazuli symbol <qualified-symbol>`** | One subcommand per concern. Symbol lookup gets its own name. | **REJECTED** — fragments the inspect surface. Authors already learned `lazuli inspect`; adding `lazuli symbol` doubles the API surface for one read mode. The audit-skill consumers `inspect` already; one tool, two modes is simpler than two tools. |
| **B. Extend `lazuli inspect`** | Same command; argument disambiguates between path-mode (existing) and symbol-mode (new). When the argument is `<feature>.<name>` or matches a known symbol, the command emits the symbol JSON. When the argument is a `.lzi` path or a project directory, the existing path-mode kicks in. | **SELECTED** — reuses the existing command grammar; same JSON contract style; same `--format` flag controls output. The disambiguation is **lexical** (`.lzi` suffix → path mode; absent → symbol mode), not contextual, so it remains deterministic for AI authoring per the rubric §"Determinism". |

### §5.2 JSON output shape

`lazuli inspect host.Gender` emits **exactly** this structure to stdout:

```json
{
  "symbol": "Gender",
  "feature": "host",
  "defined_in": {
    "file": "features/account/account.lzi",
    "line": 42,
    "column": 1,
    "kind": "enum"
  },
  "imported_via": {
    "file": "features/host/host.lzi",
    "line": 4,
    "column": 1,
    "uses": "account"
  },
  "type": "enum",
  "variants": ["female", "male", "non_binary", "prefer_not_to_say"],
  "previous_names": []
}
```

For a **locally-defined** symbol (no cross-feature import), `imported_via` is `null`:

```json
{
  "symbol": "Booking",
  "feature": "booking",
  "defined_in": {
    "source": "file",
    "file": "features/booking/booking.lzi",
    "line": 18,
    "column": 1,
    "kind": "resource"
  },
  "imported_via": null,
  "type": "resource",
  "fields": [
    { "name": "id", "type": "ID" },
    { "name": "listing_id", "type": "ID" },
    { "name": "guest_id", "type": "ID" },
    { "name": "check_in", "type": "Date" },
    { "name": "check_out", "type": "Date" },
    { "name": "status", "type": "BookingStatus" }
  ],
  "previous_names": []
}
```

**Note on `defined_in.source`:** the `source` discriminator is `"file"` for user-authored symbols and `"builtin"` for compiler-provided types (Money, Email, etc., per §8.3). The first example above (`Gender`) is also `source: "file"` — for brevity the first example was abridged; both forms always carry the discriminator.

### §5.2.1 Command, Query, Event, Aggregate shapes

Each symbol `kind` carries its own typed payload. The structure is closed per kind.

**Command** (`type: "command"`):

```json
{
  "symbol": "create_booking",
  "feature": "booking",
  "defined_in": {
    "source": "file",
    "file": "features/booking/booking.lzi",
    "line": 64,
    "column": 1,
    "kind": "command"
  },
  "imported_via": null,
  "type": "command",
  "command_kind": "Create",
  "input": [
    { "name": "listing_id", "type": "ID", "required": true },
    { "name": "check_in", "type": "Date", "required": true },
    { "name": "check_out", "type": "Date", "required": true }
  ],
  "effect": { "kind": "creates", "resource": "Booking" },
  "emits": ["BookingCreated"],
  "previous_names": []
}
```

**Query** (`type: "query"`):

```json
{
  "symbol": "list_mine",
  "feature": "booking",
  "defined_in": {
    "source": "file",
    "file": "features/booking/booking.lzi",
    "line": 102,
    "column": 1,
    "kind": "query"
  },
  "imported_via": null,
  "type": "query",
  "query_kind": "List",
  "returns": "Booking[]",
  "filters": [
    { "name": "guest_id", "type": "ID" },
    { "name": "status", "type": "BookingStatus" }
  ],
  "previous_names": []
}
```

**Event** (`type: "event"`):

```json
{
  "symbol": "BookingCreated",
  "feature": "booking",
  "defined_in": {
    "source": "file",
    "file": "features/booking/booking.lzi",
    "line": 128,
    "column": 1,
    "kind": "event"
  },
  "imported_via": null,
  "type": "event",
  "payload": "BookingCreatedPayload",
  "producers": ["booking.command.create_booking"],
  "consumers": ["notifications.job.notify_host", "platform.job.audit_log"],
  "previous_names": []
}
```

**Aggregate** (`type: "aggregate"`):

```json
{
  "symbol": "BookingLifecycle",
  "feature": "booking",
  "defined_in": {
    "source": "file",
    "file": "features/booking/booking.lzi",
    "line": 156,
    "column": 1,
    "kind": "aggregate"
  },
  "imported_via": null,
  "type": "aggregate",
  "root": "Booking",
  "contains": ["BookingMessage", "BookingPayment"],
  "previous_names": []
}
```

**`Scalar`** (reserved) — populated once L0 #4 ships scalar aliases. The shape includes `base_type: <TypeRef>` plus the constraint chain (`min`, `max`, `pattern`, etc.).

### §5.3 Argument resolution

| Input | Resolved to | Behavior |
|---|---|---|
| `lazuli inspect host.Gender` | `(feature="host", name="Gender")` | Symbol mode. Looks up `Gender` in scope of `host` (must resolve through `host`'s `uses` clauses). |
| `lazuli inspect account.Gender` | `(feature="account", name="Gender")` | Symbol mode. Looks up `Gender` declared in `account`. |
| `lazuli inspect Gender` | `(feature=None, name="Gender")` | Symbol mode. Walks every feature; if exactly one declares `Gender`, emits the record. If two or more declare it (collision), emits the error shape per §5.4. If none, emits the not-found shape. |
| `lazuli inspect features/host/host.lzi` | path mode | Existing behavior at `crates/lazuli_cli/src/main.rs:3686-3712`. **Unchanged.** |
| `lazuli inspect .` | path mode | Existing behavior. **Unchanged.** |

The disambiguation rule: if the argument **contains a path separator** (`/` or `\`) **or** ends in `.lzi` **or** is `.` **or** points to an existing file/directory, it is treated as path mode. Otherwise, symbol mode. This is encoded once in `inspect_command` (§9 Cell C.1) and tested.

### §5.4 Error shapes

`lazuli inspect host.Banana` where `Banana` doesn't exist anywhere:

```json
{
  "error": {
    "code": "SYMBOL_NOT_FOUND",
    "message": "no declaration named `Banana` in scope of `host` or any imported feature",
    "searched_features": ["host", "account", "billing", "catalog"]
  }
}
```

Exit code: `2`.

`lazuli inspect Money` where `Money` is declared in two features (collision):

```json
{
  "error": {
    "code": "SYMBOL_AMBIGUOUS",
    "message": "the name `Money` is declared in 2 features; qualify with `<feature>.Money`",
    "candidates": ["billing.Money", "accounting.Money"]
  }
}
```

Exit code: `2`. (Note: collisions across features are already tracked by the codegen `CrossFeatureIndex.ambiguous` map at `crates/lazuli_codegen_go/src/emitter/cross_feature.rs:56-58`; this proposal lifts that capability to the analyzer surface, §6.4.)

`lazuli inspect host.Gender` where `host` doesn't import `account`:

```json
{
  "error": {
    "code": "SYMBOL_UNREACHABLE",
    "message": "the name `Gender` is declared in `account` but `host` does not include `uses account`",
    "defined_in": "account.Gender",
    "hint": "add `uses account` to features/host/host.lzi"
  }
}
```

Exit code: `2`. This is a real failure mode that proves the resolver is doing scope analysis, not just global symbol lookup.

### §5.5 Output format flag

The existing `--format <json|lazuli>` flag (`crates/lazuli_cli/src/main.rs:125-126` + `:455-460`) is honored in symbol mode:

- `--format json` (default): emits §5.2 / §5.4 shape.
- `--format lazuli`: emits a minimal `.lzi` fragment showing the symbol declaration as it appears in its source feature (e.g. the `enum Gender { ... }` block). Useful for agent context-pulls. **Deferred** to follow-up; v0.1 ships `--format json` only and `--format lazuli` returns a `FORMAT_NOT_SUPPORTED_IN_SYMBOL_MODE` error (exit code `2`). Tracked in §13.

### §5.6 Behavior contract

The JSON output is treated as a **stable contract**. Field names locked, additive evolution only:

- Adding a new field is allowed (consumers should ignore unknown fields).
- Removing or renaming a field is a breaking change requiring a versioned `--inspect-version` flag.
- The shape is documented in `docs/inspect-contract.md` (Cell F.2) as the canonical reference for downstream consumers.

---

## §6. Implementation in the IR / analyzer layer

### §6.1 New module — `crates/lazuli_analyzer/src/symbol_origin.rs`

The pass that builds `SymbolOriginIndex` lives in a new module to keep `lazuli_analyzer/src/lib.rs` (already 7000+ lines) from growing further. The new module exposes:

```rust
pub fn build_symbol_origin_index(
    module: &ir::Module,
    source_map: &ir::SourceMap,
) -> ir::SymbolOriginIndex;
```

It walks `module.features`, indexes every declared symbol (`enums`, `resources`, `records`, `commands`, `queries`, `events`, `aggregates`), and folds the `uses: Vec<String>` declarations into a list of `ImportEdge`. The pass is **side-effect-free** — no mutation of `ir::Module`. The result is held in a sidecar artifact.

### §6.2 Updates to `crates/lazuli_ir/src/lib.rs`

Add **three** new types, **none** of which embed in `Module`:

```rust
/// Sidecar to `Module`. Resolves cross-feature symbol references.
/// Built by `lazuli_analyzer::build_symbol_origin_index`.
/// EXPERIMENTAL: shape may grow additive fields before 1.0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolOriginIndex {
    pub symbols: BTreeMap<QualifiedName, SymbolOrigin>,
    pub imports: BTreeMap<String, Vec<ImportEdge>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolOrigin {
    pub feature: String,
    pub name: String,
    pub kind: SymbolKind,
    pub defined_at: SourceLocation,
    pub previous_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportEdge {
    pub importer: String,
    pub imported: String,
    pub uses_at: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Enum,
    Resource,
    Record,
    Scalar,      // reserved; populated post-L0 #4 scalar aliases
    Semantic,    // built-in `@semantic.*` types. Closed catalog at `docs/canonical-semantics.md` §Reference Namespaces — Email, Phone, Url, Uuid, Currency, GeoPoint, Money (core); locale plugins (e.g. `@plugin/scalars-br`) add BrazilianCPF / BrazilianCNPJ / BrazilianCEP via the same kind.
    Command,
    Query,
    Event,
    Aggregate,
}

/// Where a symbol is defined. Discriminated by `source`:
/// - `{ "source": "file", "file": "...", "line": N, "column": N }` for user-authored symbols
/// - `{ "source": "builtin" }` for compiler-provided types (Money, Email, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SourceLocation {
    File {
        file: String,   // canonical relative path, e.g. "features/account/account.lzi"
        line: u32,      // 1-indexed
        column: u32,    // 1-indexed
    },
    Builtin,
}
```

The `Builtin` variant carries no payload — a builtin's identity is its name (e.g. `SemanticMoney`) plus its `kind` field on `SymbolOrigin`. Documentation for each builtin lives in `docs/canonical-semantics.md`; the index doesn't duplicate it.

`QualifiedName` is **unchanged** (`crates/lazuli_ir/src/lib.rs:875-882`). The new index keys on `QualifiedName` because cross-feature collisions require feature qualification to disambiguate (one `Money` in `billing`, another in `accounting`).

### §6.3 The pass

```rust
pub fn build_symbol_origin_index(
    module: &ir::Module,
    source_map: &ir::SourceMap,
) -> ir::SymbolOriginIndex {
    let mut symbols = BTreeMap::new();
    let mut imports = BTreeMap::new();

    for feature in &module.features {
        // Index every declared symbol.
        for r#enum in &feature.enums {
            let qn = QualifiedName { feature: Some(feature.name.clone()), name: r#enum.name.clone() };
            symbols.insert(qn, SymbolOrigin {
                feature: feature.name.clone(),
                name: r#enum.name.clone(),
                kind: SymbolKind::Enum,
                defined_at: resolve_span(r#enum.span_ref, source_map),
                previous_names: r#enum.previous_names.clone(),
            });
        }
        // ... resources, records, commands, queries, events, aggregates symmetric.

        // Index every uses clause. `feature.uses: Vec<String>` carries the
        // feature names; the uses-clause SpanRef comes from a new
        // FeatureSkeleton.uses_spans: Vec<SpanRef> field added in §6.5.
        let edges: Vec<ImportEdge> = feature.uses.iter().enumerate().map(|(i, imported)| {
            ImportEdge {
                importer: feature.name.clone(),
                imported: imported.clone(),
                uses_at: resolve_span(feature.uses_spans.get(i).copied(), source_map),
            }
        }).collect();
        imports.insert(feature.name.clone(), edges);
    }

    SymbolOriginIndex { symbols, imports }
}
```

### §6.4 Ambiguity policy

When `lookup` is called with a bare name (no feature qualifier) and the name resolves to **two or more** features, the function returns `Err(LookupError::Ambiguous { candidates })`. This mirrors the existing `CrossFeatureIndex.ambiguous` policy at `crates/lazuli_codegen_go/src/emitter/cross_feature.rs:51-58`. The CLI converts the error to the §5.4 `SYMBOL_AMBIGUOUS` response.

### §6.5 Source spans for `uses` clauses

`syntax::FeatureSkeleton.uses` is `Vec<String>` today. To track the span of each `uses` clause, the parser is extended to carry `uses_spans: Vec<SpanRef>` parallel to `uses` (one entry per import). The analyzer copies into `ir::Feature.uses_spans: Vec<SpanRef>` (new field, `#[serde(default, skip_serializing_if = "Vec::is_empty")]` to keep the JSON ABI back-compat). Cell A.2.

### §6.6 Sidecar serialization

`ir::SymbolOriginIndex` is **not** embedded in `Module`. It serializes to `<workspace>/.lazuli/symbol-origin.json` (cache; gitignored per `CLAUDE.md` §"Folder conventions"). The CLI/LSP rebuild the sidecar on demand from the lowered module; the disk file is a cache, not a contract.

The pattern matches `SourceMap` exactly (ADR-3 at `crates/lazuli_ir/src/lib.rs:42-53`). Embedding `SymbolOriginIndex` in `Module` would inflate the JSON snapshot tests across `crates/lazuli_codegen_*/tests/` by ~5 KB per fixture; the sidecar keeps the existing snapshot stability.

### §6.7 Public API

The new types are exported from `lazuli_ir` and `lazuli_analyzer`:

```rust
// lazuli_ir
pub use crate::SymbolOriginIndex;
pub use crate::SymbolOrigin;
pub use crate::ImportEdge;
pub use crate::SymbolKind;
pub use crate::SourceLocation;

// lazuli_analyzer
pub use crate::symbol_origin::build_symbol_origin_index;
```

The LSP crate imports `lazuli_analyzer::build_symbol_origin_index` directly (already depends on `lazuli_analyzer` via the diagnostics path); the CLI does the same.

---

## §7. Doctor / lint angle — deferred to follow-up

Once `lazuli inspect <sym>` and LSP hover surface origin **automatically**, the procedence-comment habit becomes obsolete. A follow-up doctor rule can then enforce its removal:

**Proposed rule code:** `COMMENTS-AS-VOCABULARY-PROCEDENCE-001`

**Trigger:** a `# <comment>` line in `.lzi` whose body matches `<Symbol> imported from <feature>` (regex: `(?i)^([A-Z][A-Za-z0-9]*)\s+imported\s+from\s+([a-z][a-z0-9_]*)\s*$`).

**Severity:** `warn`.

**Diagnostic:** `procedence comment for \`<Symbol>\` is redundant — LSP hover and \`lazuli inspect <feature>.<Symbol>\` surface this automatically. Remove the comment.`

**Why deferred to follow-up:** firing this rule before §3–§6 ship would create the bad UX of "doctor says don't write the comment but the IDE doesn't tell you anything either." Order matters: ship the surface first, then enforce the absence of the workaround. Tracked in `docs/next-checklist.md` as a polish item once this proposal lands.

The rule is **not part of this proposal's acceptance criteria**. It is documented here so the grader sees the closure of the loop opened by `project_comments_are_vocabulary_smell_2026-05-15`.

---

## §8. Examples — Hostpoint-shaped

### §8.1 Cross-feature enum reference

`features/host/host.lzi`:

```lazuli
feature host
  uses account
  uses billing

  resource Host
    field id: ID required
    field user_id: ID required
    field gender: Gender required
    field default_currency: Currency required
    field nightly_rate: Money required
    field address: Address required
    field bio: Text
```

`features/account/account.lzi:42`:

```lazuli
enum Gender
  female
  male
  non_binary
  prefer_not_to_say
```

**Hover on `Gender` in host.lzi:**

```
`Gender` (enum)

**Origin**
- Defined in: `features/account/account.lzi:42`
- Imported via: `uses account` at `features/host/host.lzi:2`

**Variants:** `female`, `male`, `non_binary`, `prefer_not_to_say`
```

**`lazuli inspect host.Gender` stdout:**

```json
{
  "symbol": "Gender",
  "feature": "host",
  "defined_in": { "file": "features/account/account.lzi", "line": 42, "column": 1, "kind": "enum" },
  "imported_via": { "file": "features/host/host.lzi", "line": 2, "column": 1, "uses": "account" },
  "type": "enum",
  "variants": ["female", "male", "non_binary", "prefer_not_to_say"],
  "previous_names": []
}
```

### §8.2 Cross-feature record reference

`features/account/account.lzi:88`:

```lazuli
record Address
  field street: Text required
  field number: Text required
  field complement: Text
  field neighborhood: Text required
  field city: Text required
  field state: Text required
  field postal_code: Text required
  field country: Text required
```

**Hover on `Address` in host.lzi:**

```
`Address` (record)

**Origin**
- Defined in: `features/account/account.lzi:88`
- Imported via: `uses account` at `features/host/host.lzi:2`

**Fields:** `street: Text`, `number: Text`, `complement: Text`, `neighborhood: Text`, `city: Text`, `state: Text` … 2 more
```

**`lazuli inspect host.Address` stdout:**

```json
{
  "symbol": "Address",
  "feature": "host",
  "defined_in": { "file": "features/account/account.lzi", "line": 88, "column": 1, "kind": "record" },
  "imported_via": { "file": "features/host/host.lzi", "line": 2, "column": 1, "uses": "account" },
  "type": "record",
  "fields": [
    { "name": "street", "type": "Text" },
    { "name": "number", "type": "Text" },
    { "name": "complement", "type": "Text" },
    { "name": "neighborhood", "type": "Text" },
    { "name": "city", "type": "Text" },
    { "name": "state", "type": "Text" },
    { "name": "postal_code", "type": "Text" },
    { "name": "country", "type": "Text" }
  ],
  "previous_names": []
}
```

### §8.3 Built-in semantic type

`Money` is a built-in semantic type (`crates/lazuli_analyzer/src/lib.rs:1306`), not a user-declared symbol. The index records it with `kind: semantic` and `defined_at: SourceLocation::Builtin` (no file/line/column payload — see §6.2 type definition).

**Hover on `Money` in host.lzi:**

```
`Money` (semantic type)

**Origin**
- Defined in: Lazuli core semantic catalog (built-in)

**Surface:** currency-aware decimal. Codegen emits paired `<field>_currency TEXT` column automatically. See `docs/proposals/semantic-types-money-brazilian.md`.
```

**`lazuli inspect host.Money` stdout:**

```json
{
  "symbol": "Money",
  "feature": "host",
  "defined_in": { "source": "builtin", "kind": "semantic" },
  "imported_via": null,
  "type": "semantic",
  "documentation": "currency-aware decimal; emits paired <field>_currency TEXT column",
  "previous_names": []
}
```

Built-in semantics have `imported_via: null` because no `uses` clause brings them in — they're globally available. The CLI distinguishes built-ins from user-declared by `defined_in.source == "builtin"` — a discriminated typed contract, not a string-sentinel literal in the `file` field.

### §8.3.1 `.lzx` symbol reference (cross-feature surface)

`.lzx` surfaces reference `.lzi` symbols via `source` and `submit` clauses. The same `lazuli inspect` mechanism resolves them. Given:

```lzx
# features/booking/booking.web.lzx
surface booking web
  uses feature booking

  audience guest
    view list my_bookings at "/bookings"
      source booking.query.list_mine
      fields listing_title, status, check_in
      actions cancel
```

**`lazuli inspect booking.query.list_mine`** (qualified query reference):

```json
{
  "symbol": "list_mine",
  "feature": "booking",
  "defined_in": {
    "source": "file",
    "file": "features/booking/booking.lzi",
    "line": 102,
    "column": 1,
    "kind": "query"
  },
  "imported_via": null,
  "type": "query",
  "query_kind": "List",
  "returns": "Booking[]",
  "filters": [
    { "name": "guest_id", "type": "ID" },
    { "name": "status", "type": "BookingStatus" }
  ],
  "previous_names": [],
  "referenced_by": [
    {
      "file": "features/booking/booking.web.lzx",
      "line": 7,
      "column": 14,
      "context": "view list my_bookings source"
    }
  ]
}
```

The `referenced_by` field is populated by the analyzer pass when the inspected symbol has known `.lzx` consumers; it's `[]` for symbols with no surface references. Hover on the `source booking.query.list_mine` token in `.lzx` produces the same JSON minus `referenced_by` (since the hover IS the reference site). This closes the `.lzx` ↔ `.lzi` resolution gap surfaced in acceptance criterion #11.

### §8.4 Locally-declared symbol (no cross-feature import)

`features/booking/booking.lzi:18`:

```lazuli
feature booking
  resource Booking
    field id: ID required
    field listing_id: ID required
    ...
```

**Hover on `Booking` inside booking.lzi:**

```
`Booking` (resource)

**Origin**
- Defined in: `features/booking/booking.lzi:18`

**Fields:** `id: ID`, `listing_id: ID`, `guest_id: ID`, `check_in: Date`, `check_out: Date`, `status: BookingStatus` … 3 more
```

`Imported via:` is omitted because the origin and the current document are in the same feature.

---

## §9. Implementation cells

Per `feedback_wave_workflow_lucas_preferred.md` (Claude orchestrates; Codex executes mechanical L2; up to 5 Codex agents in parallel; never Codex on shared files).

### §9.1 Cell table

| Cell | Owner | Scope | Risk |
|---|---|---|---|
| **A.1** | Claude | Add `ir::SymbolOriginIndex`, `SymbolOrigin`, `ImportEdge`, `SymbolKind`, `SourceLocation` types to `crates/lazuli_ir/src/lib.rs`. Export from `lib.rs`. Add `crates/lazuli_ir/tests/symbol_origin_serde.rs` for round-trip serde. | Low (type definitions; no behavior) |
| **A.2** | Claude | Extend `syntax::FeatureSkeleton.uses_spans: Vec<SpanRef>` (parser change). Mirror into `ir::Feature.uses_spans: Vec<SpanRef>` (additive field, default-empty serde). | Medium (touches parser + IR; back-compat via `#[serde(default)]`) |
| **A.3** | Codex | New module `crates/lazuli_analyzer/src/symbol_origin.rs`. Implement `build_symbol_origin_index` per §6.3. Tests against `examples/marketplace-mini/` covering enum/resource/record/command/query/event/aggregate paths. | Medium (single-file; ~150-200 LOC) |
| **A.4** | Codex | Sidecar serializer: `lazuli_analyzer::write_symbol_origin_sidecar(index, path)` writes `<workspace>/.lazuli/symbol-origin.json` per §6.6. Tests round-trip. | Low (deterministic JSON write) |
| **B.1** | Claude | LSP hover handler in `crates/lazuli_lsp/src/lib.rs:137-174` extended per §4.5 cascade. New helper `enriched_user_defined_hover(source: &str, word: &str) -> Option<String>` builds the Markdown block. Document text caching by hash per §4.6. | Medium (cascade ordering; cache invalidation) |
| **B.2** | Codex | LSP hover tests in `crates/lazuli_lsp/src/lib.rs` (existing `#[cfg(test)]` mod) covering: cross-feature enum, cross-feature record, locally-declared resource, built-in semantic, unresolved symbol (returns `None`), closed-catalog override path (returns existing rich hover, no enrichment). 6 test cases. | Low (test-only file) |
| **C.1** | Claude | Extend `crates/lazuli_cli/src/main.rs::Commands::Inspect` to accept either a `PathBuf` or a string symbol argument. Add `inspect_symbol_command` function. Disambiguation rule per §5.3 encoded once. | Medium (Clap arg parsing; back-compat with existing path mode) |
| **C.2** | Codex | New module `crates/lazuli_cli/src/inspect_symbol.rs` containing the symbol-mode JSON emitter (§5.2). Reuses `lazuli_analyzer::build_symbol_origin_index`. Variants/fields/columns extracted from `ir::Module`. | Medium (~250 LOC; deterministic JSON formatting) |
| **C.3** | Codex | Error shapes (§5.4) — `SYMBOL_NOT_FOUND`, `SYMBOL_AMBIGUOUS`, `SYMBOL_UNREACHABLE`. Three test cases in `crates/lazuli_cli/tests/inspect_symbol_errors.rs`. | Low (deterministic) |
| **C.4** | Codex | CLI smoke test in `crates/lazuli_cli/tests/inspect_symbol_smoke.rs` — runs `lazuli inspect host.Gender` against `examples/marketplace-mini/` and asserts the JSON shape (§5.2). | Low |
| **D.1** | Claude | Wire `lazuli inspect` symbol mode into `examples/marketplace-mini/` fixture. Add the cross-feature reference (one enum + one record) to the fixture if not present. | Medium (touches a fixture other tests depend on) |
| **D.2** | Claude | Snapshot test in `crates/lazuli_cli/tests/inspect_symbol_snapshot.rs` — `lazuli inspect marketplace.Money` (or whatever cross-feature symbol the fixture exposes) → byte-identical JSON. | Low |
| **E.1** | Claude | `docs/inspect-contract.md` — new file documenting the JSON shape (§5.2/§5.4) as the stable contract per §5.6. Normative-only per `feedback_normative_not_narrative_2026-05-15`. | Low |
| **E.2** | Claude | `docs/architecture.md` — add a one-paragraph entry under §"LSP/CLI surfaces" referencing this proposal. | Low |
| **F.1** | Claude | Update `docs/error-contract.md` catalog with the three CLI error codes (§5.4). | Low |
| **G** (polish, deferred) | — | `COMMENTS-AS-VOCABULARY-PROCEDENCE-001` doctor rule per §7. Tracked separately in `docs/next-checklist.md`. | — |
| **H** (polish, deferred) | — | `--format lazuli` for symbol mode per §5.5. Tracked separately. | — |

### §9.2 Wave layout

- **Wave 1 — IR + analyzer** (A.1, A.2, A.3, A.4): 4 cells. A.1 + A.2 sequenced (both touch `lazuli_ir`); A.3 + A.4 in parallel (Codex on `symbol_origin.rs`; Codex on sidecar serializer). Critical path: A.1 → A.2 → A.3.
- **Wave 2 — LSP** (B.1, B.2): 2 cells. B.1 sequenced (touches shared `lib.rs`); B.2 runs after B.1.
- **Wave 3 — CLI** (C.1, C.2, C.3, C.4): 4 cells. C.1 sequenced (touches `main.rs`); C.2 + C.3 + C.4 in parallel (Codex; each touches a distinct new file).
- **Wave 4 — Fixture + docs** (D.1, D.2, E.1, E.2, F.1): 5 cells. D.1 → D.2 sequenced; E.1 + E.2 + F.1 in parallel.

**Total: 15 cells across 4 waves.** Critical path: A.1 → A.2 → A.3 → B.1 → C.1 → C.2 → D.1 → D.2. Estimated ~1.5 days of orchestrator-bound work + ~6 Codex hours.

### §9.3 Shared-file conflict avoidance

Per `CLAUDE.md` §"Division of labor": Codex agents must not touch shared files in parallel.

- `crates/lazuli_ir/src/lib.rs` — touched by A.1 only (Claude).
- `crates/lazuli_lsp/src/lib.rs` — touched by B.1 only (Claude); B.2 adds test cases to the existing `#[cfg(test)]` block in a single dedicated commit after B.1.
- `crates/lazuli_cli/src/main.rs` — touched by C.1 only (Claude).
- All Codex cells (A.3, A.4, B.2, C.2, C.3, C.4) target distinct new files.

---

## §10. Open questions

1. **What about decorator references (`@policy.<name>`, `@cap.<name>`, `@semantic.<name>`)?** v0.1 covers user-declared types only. Decorator references are closed-catalog and already get hover via `rich_keyword_hover`. **Resolution:** out of scope for v0.1; if pilots need it, track a follow-up cell that surfaces the closed-catalog entry's procedence (e.g. `@cap.Hashed` → "core capability, see `docs/capabilities.md#hashed`").

2. **`uses` chains (transitive imports)?** If `host` uses `account`, and `account` uses `core`, does hovering `CoreThing` from `host` resolve via the transitive chain? **Resolution:** **no**. `uses` is non-transitive by `docs/invariants.md`. If `host` references `CoreThing`, it must `uses core` directly. The resolver returns `SYMBOL_UNREACHABLE` per §5.4 if the importer doesn't declare the import — this is the same scope rule the analyzer already enforces; this proposal surfaces it, doesn't relax it.

3. **What if a symbol is renamed mid-pilot (`previous_names: ["Sex"]` on `Gender`)?** The index records `previous_names`. Future LSP enhancement: when the author types `Sex` (a previous name), the hover surfaces both the current name AND the rename history. v0.1 ships the data (§6.2) but the hover only renders `formerly: Sex` if `previous_names` is non-empty. CLI emits `previous_names: ["Sex"]` in the JSON.

4. **Should the hover include the surrounding context (e.g. the `feature account` declaration line)?** **No.** Hover stays bounded — `Defined in:` is one line. The `lazuli inspect` JSON is the deep-read mode.

5. **What about `.lzx` references to `.lzi` symbols?** A `.lzx` `view list ...` references `<feature>.query.<name>`. **Resolution:** `.lzx` hover is the same handler (`crates/lazuli_lsp/src/lib.rs:137`) and shares the SymbolOriginIndex. The CLI symbol-mode accepts `<feature>.query.<name>` and emits the query record. Tracked in §11 acceptance criteria #11.

6. **Should the LSP send hover proactively (push-based) instead of on-request?** **No.** Hover is pull-based per LSP protocol. The marginal cost of on-demand lowering is acceptable (§4.6).

7. **What about workspace-level `lazuli inspect` (no feature context)?** `lazuli inspect Gender` (bare name, no feature) walks all features. If ambiguous, emits `SYMBOL_AMBIGUOUS` (§5.4). If unique, emits the record with `feature` set to the **declaring** feature. The `imported_via` is `null` because there's no importer context.

8. **Does the sidecar file `<workspace>/.lazuli/symbol-origin.json` need versioning?** **Yes**, additive field `version: u32` at the JSON top level. v0.1 ships `version: 1`. Tracked in §6.6.

9. **Should `previous_names` show the rename source (which proposal/commit)?** **No.** Procedence belongs to the changelog, not the IR (`feedback_normative_not_narrative_2026-05-15`). The index records only the names; the audit trail lives elsewhere.

10. **What about cross-workspace references (one workspace imports another's `.lzi`)?** Out of scope. The current `uses` model is intra-workspace. If `Lazurite.toml`-level workspace federation lands, the resolver extends; v0.1 is workspace-local.

---

## §11. References

- `docs/architecture.md` — three-layer architecture; this proposal extends the LSP/CLI consumer surface without touching the IR contract.
- `docs/invariants.md` — closed-catalog discipline (no new `@-namespace`).
- `docs/design-principles.md` — Rule Zero (Vocabulary Over Mechanism): the procedence-comment habit was a missing-tooling smell, not a missing-vocabulary smell; this proposal closes the gap at the tooling layer.
- `docs/next-checklist.md` §"From cross-feature symbol resolution review (2026-05-16)" — the tracked item this proposal closes.
- `docs/grading-rubric.md` — target ≥ 9.0.
- `docs/proposals/mobile-target.md` — structural reference for v0.2 PASS proposal shape.
- `crates/lazuli_ir/src/lib.rs:875-882` — the existing `QualifiedName` struct (unchanged by this proposal).
- `crates/lazuli_ir/src/lib.rs:42-62` — `SourceMap` ADR-3 pattern (mirrored by this proposal's sidecar approach).
- `crates/lazuli_analyzer/src/lib.rs:4144-4157` — existing `lower_qualified_name`.
- `crates/lazuli_analyzer/src/lib.rs:2059-2174` — existing `lower_feature_skeleton`.
- `crates/lazuli_lsp/src/lib.rs:137-174` — current hover handler (the cascade extension point).
- `crates/lazuli_lsp/src/lib.rs:13822` — `rich_keyword_hover` (preserved unchanged).
- `crates/lazuli_cli/src/main.rs:119-127` + `:3686-3712` — current `inspect` command (extended in §5).
- `crates/lazuli_codegen_go/src/emitter/cross_feature.rs:50-100` — the existing codegen-only `CrossFeatureIndex` lifted to the analyzer surface.
- Memory: `feedback_normative_not_narrative_2026-05-15` (procedence is metadata), `project_comments_are_vocabulary_smell_2026-05-15` (the smell this proposal closes), `feedback_scope_discipline_2026-05-14` (target-closing vs boundary-moving), `feedback_wave_workflow_lucas_preferred` (parallel waves), `project_strategic_pivot_2026-05-15` (Hostpoint pilot driver).

---

## §12. Acceptance criteria

L0 PASS condition: this proposal answers, deterministically, for any Lazuli capsule with cross-feature references:

1. **What changes in the IR grammar?** → **Nothing.** `QualifiedName` (`crates/lazuli_ir/src/lib.rs:875-882`) is unchanged. The new types (`SymbolOriginIndex`, `SymbolOrigin`, `ImportEdge`, `SymbolKind`, `SourceLocation`) are **sidecar additions**, not embedded in `Module` (§6.2 + §6.6).
2. **What changes in the surface `.lzi`/`.lzx` grammar?** → **Nothing.** Authors write `Gender` exactly as they always have.
3. **What new `@-namespace` entries are introduced?** → **None.** The proposal honors the closed-catalog discipline of `docs/invariants.md`.
4. **What new doctor rules in v0.1?** → **None.** `COMMENTS-AS-VOCABULARY-PROCEDENCE-001` is deferred to §7 follow-up after the surface ships, per the ordering discipline (don't fire the rule before the workaround becomes obsolete).
5. **What does the LSP hover return for `Gender` in host.lzi (cross-feature)?** → Markdown block per §4.2 with `Defined in:` AND `Imported via:` sections, plus enum variants list bounded to 6 entries + `… N more`.
6. **What does the LSP hover return for `Booking` in booking.lzi (local)?** → Markdown block per §4.3 with `Defined in:` only (no `Imported via:`).
7. **What does the LSP hover return for closed-catalog keywords (`command`, `query.list`, etc.)?** → Existing behavior unchanged (§4.4). The cascade per §4.5 evaluates closed catalog **first**.
8. **What is the JSON shape returned by `lazuli inspect host.Gender`?** → Locked in §5.2: top-level keys `symbol`, `feature`, `defined_in`, `imported_via`, `type`, `variants` (for enums) or `fields` (for resources/records), `previous_names`. For local symbols, `imported_via: null`.
9. **What is the JSON shape for an unresolved symbol?** → §5.4 `SYMBOL_NOT_FOUND` with exit code `2`.
10. **What is the JSON shape for an ambiguous symbol?** → §5.4 `SYMBOL_AMBIGUOUS` with `candidates: [...]` and exit code `2`.
11. **What is the JSON shape for an unreachable symbol (importer doesn't `uses` the source)?** → §5.4 `SYMBOL_UNREACHABLE` with `hint` and exit code `2`.
12. **Why extend `lazuli inspect` instead of adding `lazuli symbol`?** → §5.1: reuse the existing command grammar; one tool, two modes is simpler than two tools; disambiguation is lexical, deterministic.
13. **Where does the analyzer's resolution metadata live (embedded vs sidecar)?** → Sidecar `<workspace>/.lazuli/symbol-origin.json` (§6.6); mirrors `SourceMap` ADR-3 to keep IR JSON ABI stable.
14. **How does the LSP avoid re-lowering on every hover?** → §4.6 cache-by-document-hash; rebuild on `did_change` only.
15. **What is the fixture validating the end-to-end path?** → `examples/marketplace-mini/` (§9 Cell D.1/D.2) + a snapshot test asserting byte-identical JSON for one cross-feature symbol.
16. **How does the proposal honor `feedback_normative_not_narrative_2026-05-15`?** → §7 — the procedence-comment habit is a `.lzi` polluter; this proposal eliminates the trigger by surfacing the metadata at the tooling layer (LSP hover, CLI JSON) instead of in source comments.
17. **How does the proposal honor Rule Zero (Vocabulary Over Mechanism)?** → No new vocabulary, no new mechanism. The data already exists in the analyzer's resolution path (`crates/lazuli_codegen_go/src/emitter/cross_feature.rs:60-100`); this proposal **wires** it through to consumers per the founding wire-thin principle.

If all 17 answers are mechanical from the proposal text, L0 passes.

---

## §13. Tracked cuts (post-PASS)

If this proposal lands PASS, the following items are tracked separately in `docs/next-checklist.md`:

- **G — `COMMENTS-AS-VOCABULARY-PROCEDENCE-001` doctor rule** (§7). Fires after surface ships; warns on `# <Symbol> imported from <feature>` patterns.
- **H — `--format lazuli` for symbol mode** (§5.5). Emits a minimal `.lzi` fragment showing the declaration as it appears in source. Useful for agent context-pulls.
- **I — Decorator-reference hover** (§10.1). Surface procedence for `@policy.*`, `@cap.*`, `@semantic.*` referencing the closed-catalog catalog files.
- **J — Rename-history hover** (§10.3). When `previous_names` is non-empty, hover renders `formerly: Sex` and the CLI flags this in a separate `rename_history` JSON field.
- **K — Workspace federation** (§10.10). Cross-workspace symbol resolution if/when `Lazurite.toml` workspace federation lands.
