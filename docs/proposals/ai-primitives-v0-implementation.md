# Cut A Implementation Plan — AI Primitives v0

**Status**: Draft. Sequencing the design from
`docs/proposals/ai-primitives-v0.md` (Cut A approved by
`lazuli-language-architect` second pass) into a phased implementation
ordered against the actual state of the codebase. Submit for approval
before any code lands.

**Scope**: Cut A only — `tools` child of `agent`, discriminated
`output`, `evals` block. Cut B (`flow`, `budget tokens`, `knowledge`,
`quota cost`) is gated on pilot evidence and does not appear here.

## 0. Discovery

A cold map of the codebase shows that Cut A's prerequisite is more
foundational than the proposal implied. The current state of the
`agent` surface, walked end-to-end:

| Layer | File | Today | Cut A target |
|---|---|---|---|
| Pest grammar | `crates/lazuli_syntax/src/grammar.pest` (37 lines) | Legacy brace MVP. Knows `aggregate { ... }`, `command { ... }`, `query { ... }`, `surface { ... }`. **No `agent`. No canonical indent form.** | Indented `agent` block with all Cut A children. |
| Parser | `crates/lazuli_syntax/src/parser.rs` `parse_document` | Drives pest grammar. Parses canonical fixtures only because the *legacy* surface still appears in some examples. Canonical-indent fixtures fail to parse. | Indent-aware parsing for `feature` body (or at least for `agent`). |
| AST | `crates/lazuli_syntax/src/ast.rs` | `Document`, `Aggregate`, `Field`, `Command`, `Query`, `Surface`. **No `Agent` node.** | `Agent` AST node with Cut A children. |
| IR | `crates/lazuli_ir/src/lib.rs` (1710 lines) | `Feature`, `Resource`, `Field`, `Command`, `Query`, `Workflow`, `Job`, `Webhook`, `Auth`, `App*`, `Workspace*`, `Contract*`. **No `Agent`. Zero matches for `agent`.** | `Agent` IR node + Cut A fields (`tools`, `evals`, `output_kind`, `output_discriminator`). |
| Analyzer (lowering) | `crates/lazuli_analyzer/src/lib.rs` `lower_document` | Lowers `Document → ir::Module`. **No agent path.** | Lower `Agent` AST to IR; resolve cross-feature tool refs in expand pass. |
| Doctor | `crates/lazuli_cli/src/doctor.rs` (3868 lines) | App contract, workspace, gateway, integrations, calls, profiles, packs, routes. **Zero matches for `agent`.** Cross-feature checks today walk source text. | Cross-feature `tool` ref resolution; policy compatibility check; PII-coverage warnings. |
| LSP | `crates/lazuli_lsp/src/lib.rs` (13931 lines) | `agent_contract_diagnostics` (line 3297, ~150 lines) is text-pattern: walks indent depth, checks required children (`policy`/`output`/`model`/`prompt`), validates `temperature`/`top_p`/`max_tokens`/`seed` ranges. | Text-pattern remains valid for *file-local* warnings; *cross-feature* checks must move to doctor. New diagnostics for `tools`, `evals`, discriminator. |
| Inspect | `crates/lazuli_cli/src/main.rs` and projection helpers | `--expand=summary,refs,events,policies,locators,dependencies,security` over IR + source. **Agent currently appears via text scan, not IR projection.** | `summary` adds tools/evals/output_kind once IR carries them; new `--expand=tools` projection. |
| Inline tests | `crates/lazuli_lsp/src/lib.rs` lines 12517–12569 | `agent_accepts_canonical_declaration`, `agent_rejects_missing_required_children`, `agent_rejects_non_llm_model_reference`. Snapshot-style. | New tests for tools/evals/discriminator; existing snapshots remain valid. |

**Implication**: Cut A as designed assumes a typed IR for `Agent`.
The IR does not exist. Building Cut A against text-pattern LSP only
would extend the technical-debt items already on
`docs/language-backlog.md:204-207`:

> [ ] Lower the new canonical surface into typed IR instead of LSP-only
>     text diagnostics.
> [ ] Add parser support for canonical indentation syntax beyond the
>     legacy brace MVP.

This plan treats Cut A as the **first construct that flows
end-to-end** through a canonical-indent parser → AST → IR → doctor
pipeline. Other constructs continue text-pattern until later cuts
migrate them.

## 1. Implementation strategies

### Strategy A — Architectural (recommended)

Build a *narrow* canonical-indent parser slice for `agent` blocks
only, lower to a new `Agent` IR node, run doctor checks against IR,
keep `agent_contract_diagnostics` for file-local warnings only, and
move cross-feature checks to doctor.

**Pros**:
- Pays down a documented backlog item for one construct.
- Establishes the migration pattern for the next constructs (`flow`,
  `notification`, etc.).
- Cross-feature `tool` resolution is genuinely doctor-shaped (it needs
  the package set, like the existing `app_contract_diagnostics`).

**Cons**:
- ~3× the LOC of Strategy B before any new feature ships.
- Two parser paths coexist (pest brace MVP + new canonical-indent for
  agent) until the rest of the language migrates.

### Strategy B — Tactical (documented fallback)

Extend `agent_contract_diagnostics` with new text-pattern handlers
for `tools`, `evals`, and `discriminator`. Add cross-feature checks
in doctor by walking source files line-by-line (matching the existing
doctor style for app/workspace/integrations).

**Pros**:
- Smallest code delta. Likely 2-3 days to ship Cut A's user-facing
  surface.
- No new pipeline.

**Cons**:
- Extends the text-pattern debt the backlog wants to retire.
- Cross-feature tool resolution as text scan is brittle (the existing
  doctor's `collect_feature_commands` shows the pain).
- Inspect projections cannot include resolved tool effects without IR.
- Architect note in the proposal grade explicitly required `Agent.tools[]
  .resolved_effect`/`resolved_policy`/`resolved_pii_classes` as derived
  IR fields. Strategy B cannot deliver that.

### Strategy C — Hybrid

Two parser paths for `agent` (text-pattern existing children,
canonical-indent new children). **Rejected** before this plan: the
parser-handoff seam is a permanent maintenance tax.

### Recommendation

**Strategy A**, acknowledging the foundational cost. The architect's
proposal grade and the backlog both point this direction. Cut A
becomes the migration vehicle.

If the team decides Strategy B for delivery speed, this plan still
applies — Phases 1, 4, 5, 6 collapse and Phase 3 grows. A separate
fallback plan would need to spell out the text-pattern surface; this
document does not.

## 2. Phase sequencing (Strategy A)

```
  Phase 1: AST + parser slice
     ↓ (deliverable: indented `agent` block parses to AST)
  Phase 2: IR Agent node + lowering
     ↓ (deliverable: `lazuli inspect --format=json` reports agent shape from IR)
  Phase 3: Doctor cross-feature checks
     ↓ (deliverable: `lazuli doctor` rejects bad tool refs and policy gaps)
  Phase 4: LSP migration + new diagnostics
     ↓ (deliverable: LSP text-pattern shrinks; new diagnostics for tools/evals)
  Phase 5: Inspect projections
     ↓ (deliverable: --expand=summary, --expand=tools, --expand=security extended)
  Phase 6: Fixture + docs
     ↓ (deliverable: full-capsule fixture uses Cut A; quickref/invariants updated)
  Phase 7: MCP + codegen acknowledgment
     ↓ (deliverable: MCP read tools surface tools/evals; Go codegen acknowledges or stubs)
```

Phases 1–3 are the load-bearing chain. Phases 4–7 fan out from IR
once it lands.

## 3. Phase 1 — AST + parser slice

**Goal**: indented `agent` block parses to an `Agent` AST node.

### 3.1 Files touched

- `crates/lazuli_syntax/src/grammar.pest` — new productions or a
  parallel grammar (TBD; see open question Q-impl-1).
- `crates/lazuli_syntax/src/ast.rs` — new `Agent`, `AgentTool`,
  `AgentEvalCase`, `AgentEvalAssertion`, `AgentOutputKind` enums.
- `crates/lazuli_syntax/src/parser.rs` — `parse_agent`,
  `parse_agent_tools`, `parse_agent_evals`, `parse_agent_output`.
  Hooked into `parse_document` once feature-level parsing exists.

### 3.2 AST shape

```rust
// crates/lazuli_syntax/src/ast.rs

pub struct Agent {
    pub name: String,
    pub span_ref: Option<Span>,
    pub input: Option<Vec<TypedSlot>>,
    pub context: Option<TargetExpr>,
    pub policy: Option<PolicyAtomList>,
    pub rate_limit: Option<String>,
    pub output: AgentOutput,
    pub model: NamespaceRef,        // @llm.<name>
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    pub seed: Option<i64>,
    pub prompt: String,             // path literal
    pub safety: Vec<NamespaceRef>,         // @validator.* (Cut A.5 ready: list; today 0..1)
    pub tools: Option<Vec<AgentTool>>,
    pub evals: Option<Vec<AgentEvalCase>>,
}

pub struct AgentTool {
    pub reference: ToolRef,         // <feature>.<kind>.<name> or @tool.<name>
    pub span_ref: Option<Span>,
}

pub enum AgentOutput {
    Stream(TypeRef),
    Text(TypeRef),                  // legacy default; lowering emits warn
    DiscriminatorEnum(QualifiedName),
    DiscriminatedRecord(QualifiedName), // record with one discriminator field
}

pub struct AgentEvalCase {
    pub name: String,
    pub span_ref: Option<Span>,
    pub assertions: Vec<AgentEvalAssertion>,
}

pub struct AgentEvalAssertion {
    pub kind: AgentEvalKind,        // Requires | Forbids
    pub predicate: AgentEvalPredicate,
    pub span_ref: Option<Span>,
}

pub enum AgentEvalPredicate {
    Closed(Predicate),              // existing closed predicate language
    Contains { lhs: Ref, rhs: ContainsRhs },
    ToolsCalls { op: ToolsCallsOp, target: ToolRef },
}

pub enum ContainsRhs {
    Literal(String),                // substring match
    SemanticType(QualifiedName),    // @semantic.<Type> membership
}

pub enum ToolsCallsOp {
    Includes,
    Excludes,
}
```

### 3.3 Parser strategy (decided)

**Decision**: hand-written line-walker, mirroring the existing
`parse_lzx_document` shape in `parser.rs:65`. The LZX parser already
walks `SourceLine` arrays; the agent block follows the same pattern.

The alternative — adding a pest INDENT/DEDENT preprocessor and
extending the pest grammar — is the right *long-term* shape for the
canonical-indent migration, but it is a separate
`docs/language-backlog.md:204-207` item with its own schedule. Cut A
should not be coupled to that migration. When the preprocessor lands,
the agent line-walker collapses into the pest grammar in a future cut.

### 3.4 Feature-skeleton scope (decided)

**Decision**: the Phase 1 slice includes a minimum-viable feature
skeleton — `feature <name>` header + indented body — that yields a
`FeatureSkeleton { name, agents: Vec<Agent> }`. Other feature children
(commands, queries, workflows, etc.) are NOT parsed by this slice and
remain in the existing pipeline. The analyzer locates agents through
the skeleton.

This bounds the slice to what Cut A actually needs without leaking
into a full canonical-indent feature parser.

### 3.5 Snapshot tests

Add to `crates/lazuli_syntax/src/parser.rs` test module:

- `agent_with_tools_block_parses`.
- `agent_with_evals_parses` — full case/requires/forbids shape.
- `agent_with_discriminator_output_parses` — `output discriminator
  Intent`.
- `agent_with_discriminated_record_output_parses` — `output Action`
  + record with `discriminator` marker.
- `agent_rejects_unknown_output_kind` — `output stream` plus a
  `discriminator` keyword should error.

(Eval determinism check — `temperature 0` AND `seed N` — is an
analyzer/doctor concern, not a parser concern. Tests for it live in
Phase 2 and Phase 3, not here.)

## 4. Phase 2 — IR Agent node + lowering

**Goal**: `Agent` AST lowers to typed IR. `lazuli inspect
--format=json` reports agent shape from IR rather than text scan.

### 4.1 Files touched

- `crates/lazuli_ir/src/lib.rs` — add structs after the existing
  `Auth` block (around line 1631, before `TestBlock`):

  ```rust
  pub struct Agent {
      pub name: String,
      pub feature: String,
      pub input: Vec<TypedSlot>,
      pub context: Option<TargetExpr>,
      pub policy: Option<PolicyRef>,
      pub rate_limit: Option<String>,
      pub output_kind: AgentOutputKind,
      pub output_type: TypeRef,
      pub output_discriminator: Option<DiscriminatorRef>,
      pub model: QualifiedName,           // @llm.<name>
      pub temperature: Option<f64>,
      pub max_tokens: Option<u32>,
      pub top_p: Option<f64>,
      pub seed: Option<i64>,
      pub prompt_path: String,             // file path; bare String matches existing IR convention
      pub safety: Vec<QualifiedName>,     // @validator.*  (Cut A: 0 or 1; Cut A.5 ready)
      pub tools: Vec<ToolBinding>,
      pub evals: Vec<EvalCase>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
  }

  pub enum AgentOutputKind {
      Text,
      Stream,
      DiscriminatedEnum,
      DiscriminatedRecord,
  }

  pub enum DiscriminatorRef {
      Enum(QualifiedName),                // for DiscriminatedEnum
      RecordField {                        // for DiscriminatedRecord
          record: QualifiedName,
          field: String,
          enum_type: QualifiedName,
      },
  }

  pub struct ToolBinding {
      pub reference: QualifiedToolRef,
      #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
      // resolved_* fields populated by expand, not lowering:
      pub resolved_effect: Option<ToolEffect>,
      pub resolved_policy: Option<PolicyRef>,
      pub resolved_pii_classes: Vec<QualifiedName>,  // @pii.*
  }

  pub enum QualifiedToolRef {
      Local { kind: ToolKind, name: String },
      CrossFeature { feature: String, kind: ToolKind, name: String },
      Adapter { dotted: Vec<String> }, // @tool.<x>.<y>
  }

  pub enum ToolKind {
      QueryList,
      QueryLookup,
      QuerySql,
      Command,
      Api,
  }

  pub enum ToolEffect {
      Read,
      Write,
  }

  pub struct EvalCase {
      pub name: String,
      pub assertions: Vec<EvalAssertion>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
  }

  pub struct EvalAssertion {
      pub kind: EvalAssertionKind,
      pub predicate: EvalPredicate,
      #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
  }

  pub enum EvalAssertionKind {
      Requires,
      Forbids,
  }

  pub enum EvalPredicate {
      Closed(PredicateExpr),
      Contains { lhs: PathExpr, rhs: ContainsRhs },
      ToolsCalls { op: ToolsCallsOp, target: QualifiedToolRef },
  }
  // ContainsRhs / ToolsCallsOp mirror the AST.
  ```

- `crates/lazuli_ir/src/lib.rs` — `Feature` struct gains
  `pub agents: Vec<Agent>` (currently `Feature` is at line 198; add
  the field next to existing children). The `Vec` shape (not
  `BTreeMap`) matches `Feature.commands`, `queries`, `workflows`
  already in the struct — same convention.

- `crates/lazuli_analyzer/src/lib.rs` — add `lower_agent(ast::Agent)
  -> Result<ir::Agent, AnalyzeError>`. Called from the existing
  feature lowering path.

### 4.2 IR delta classification

- `LZIR_SCHEMA`: minor bump (additive fields).
- `LZI_LANG`: minor bump.
- Backward compat: agents without `tools`/`evals` produce `Vec::new()`
  for those fields. `output_kind` defaults to `Stream` when the
  source uses `output stream <T>` and to `Text` when the source uses
  `output <T>` (legacy form, deprecated soft-warn in Phase 6).

### 4.3 Resolution in expand (not lowering)

`ToolBinding.resolved_effect`/`resolved_policy`/`resolved_pii_classes`
are populated by an expand pass that has the workspace IR loaded.
Lowering produces `None`/`vec![]`. This matches the architect's
single-pass-base + expand-resolution constraint.

The expand pass lives in `crates/lazuli_cli/src/main.rs` next to the
existing `--expand=summary` handler. New private function:

```rust
fn resolve_agent_tools(
    agent: &mut ir::Agent,
    workspace: &WorkspaceIr,
) -> Result<()>;
```

### 4.4 Snapshot tests

- `lower_agent_with_tools_resolves_to_ir`.
- `lower_agent_with_evals_resolves_to_ir`.
- `lower_agent_with_discriminator_output_resolves`.
- `lower_agent_with_discriminated_record_resolves`.
- `lower_agent_evals_without_temperature_zero_is_marked_nondeterministic`
  — analyzer flags the agent's evals as informational-only when
  `temperature != 0` or `seed.is_none()`.
- `expand_agent_tools_resolves_cross_feature_effect`.
- `expand_agent_tools_marks_unknown_tool_as_unresolved`.
- `expand_agent_tools_resolves_pii_classes_from_source_resource`.
- `lower_registry_tool_entry_with_effect_and_pii_classes`.

### 4.5 Registry-side IR extension (in scope for Phase 2)

The proposal commits `RegistryToolEntry.effect: Read | Write`
(required) and `RegistryToolEntry.pii_class: Vec<PiiClass>` (optional).
Without these registry fields, Phase 3's
`tool_registry_effect_required_diagnostics`,
`agent_tool_policy_diagnostics`, and `agent_pii_unsafetied_warning`
have no field to read. The registry IR delta is therefore part of
Phase 2, not a separate phase.

Files touched (incremental, alongside §4.1):

- `crates/lazuli_ir/src/lib.rs` — locate the existing registry IR
  shape (`AppIntegration`, `RegistryPack`, etc.) and add:

  ```rust
  pub struct RegistryToolEntry {
      pub name: String,                     // dotted path under @tool.*
      pub effect: ToolEffect,               // required
      #[serde(default, skip_serializing_if = "Vec::is_empty")]
      pub pii_classes: Vec<QualifiedName>,  // @pii.*
      pub adapter: Option<QualifiedName>,   // @adapter.*
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub span_ref: Option<SpanRef>,
  }
  ```

- `crates/lazuli_analyzer/src/lib.rs` — extend the registry lowering
  path to populate these fields from `registry.lzi`.

- `crates/lazuli_cli/src/doctor.rs` — registry-side validation
  (`tool_registry_effect_required_diagnostics`) reads
  `RegistryToolEntry.effect`. Lives next to the existing pack /
  integration registry checks.

The registry-side delta is small and aligned with how `RegistryPack`
already carries `provides`/`requires` shapes. No structural drift.

## 5. Phase 3 — Doctor cross-feature checks

**Goal**: `lazuli doctor` produces the diagnostics the proposal
declared, working off IR.

### 5.1 Files touched

- `crates/lazuli_cli/src/doctor.rs` — add a new module section after
  `policy_reachability_diagnostics` (around line 944):

  ```rust
  fn agent_tool_diagnostics(
      facts: &PackageFacts,
  ) -> Vec<DoctorDiagnostic>;

  fn agent_eval_diagnostics(
      facts: &PackageFacts,
  ) -> Vec<DoctorDiagnostic>;

  fn agent_discriminator_diagnostics(
      facts: &PackageFacts,
  ) -> Vec<DoctorDiagnostic>;
  ```

- `crates/lazuli_cli/src/doctor.rs` — extend `PackageFacts` (the
  collected canonical facts struct, declared near line 470) with
  `agents: Vec<AgentFacts>` populated from IR.

### 5.2 Diagnostics

| Id | Severity | Source primitive | Implementation |
|---|---|---|---|
| `tool_registry_effect_required_diagnostics` | error | A1 | Read `RegistryToolEntry.effect`; rejected if missing. Lives near the existing registry diagnostics in doctor. |
| `agent_tool_policy_diagnostics` | error | A1 | Compare `Agent.policy` lattice rank to `ToolBinding.resolved_policy`. Lattice helper already exists for `policy_reachability_diagnostics` — reuse. |
| `agent_tool_write_unguarded_diagnostics` | error | A1 | Filter `tools[]` by `resolved_effect == Write`; require `Agent.safety.is_empty().not()` OR an idempotency hint on the agent (Cut A: agents do not declare `idempotency by`; default is `safety` required). |
| `agent_pii_unsafetied_warning` | warning | A1 | If any `tools[].resolved_pii_classes` is non-empty and `Agent.safety` is empty, emit. |
| `agent_discriminator_target_invalid_diagnostics` | error | A2 | Resolve `output_discriminator` against `feature.enums` / `feature.records`; emit if target missing. |
| `agent_discriminator_field_invalid_diagnostics` | error | A2 | For `DiscriminatedRecord`, exactly one field must carry the marker, and its type must be an enum. |
| `eval_ordered_op_invalid_diagnostics` | error | A3 | When the assertion uses `<` `<=` `>` `>=`, both sides must resolve to numeric type. Walk `EvalPredicate::Closed` and check. |
| `eval_nondeterministic_warning` | warning | A3 | `Agent.evals` non-empty AND (`temperature.is_none() || temperature != 0.0` OR `seed.is_none()`). |

### 5.3 Doctor test pattern

Snapshot tests in `crates/lazuli_cli/src/doctor.rs` `mod tests`
mirror the existing pattern (look for `policy_reachability_*` tests):

- `doctor_rejects_tool_with_stricter_policy_than_agent`
- `doctor_rejects_write_tool_without_safety`
- `doctor_warns_pii_tool_without_safety`
- `doctor_rejects_unknown_discriminator_target`
- `doctor_warns_evals_without_determinism_pin`

### 5.4 Open questions for Phase 3

- **Q-impl-4**: The proposal allows `safety` *or* `idempotency by`
  for the write-tool guard. Cut A agents do not yet declare
  `idempotency by`. Decision: in Cut A, only `safety` satisfies the
  guard; `idempotency by` for agents is a Cut B concern bundled with
  `flow`. Document in the diagnostic message.

## 6. Phase 4 — LSP file-local additions

**Goal**: extend the file-local LSP surface for Cut A. The existing
`agent_contract_diagnostics` is already file-local-only (required
children, scalar config ranges; no cross-feature awareness). This
phase **adds** file-local Cut A diagnostics; cross-feature checks
were never in LSP and remain in doctor. The phase does not "shrink"
existing LSP — it grows it minimally and pushes any cross-feature
ambition to Phase 3.

### 6.1 Files touched

- `crates/lazuli_lsp/src/lib.rs`:
  - `agent_contract_diagnostics` (line 3297): keep file-local checks
    only; remove anything that requires cross-feature awareness
    (none today, but the function will gain pressure as Cut A's
    diagnostics are added — push them to doctor).
  - New private functions:

    ```rust
    fn agent_tools_diagnostics(source: &str) -> Vec<Diagnostic>;
    fn agent_evals_diagnostics(source: &str) -> Vec<Diagnostic>;
    fn agent_discriminator_diagnostics(source: &str) -> Vec<Diagnostic>;
    ```

  - These new functions are file-local: they reject tool entries
    whose *shape* is wrong (not a valid `<feature>.<kind>.<name>`),
    eval cases whose *predicate language* is malformed, and
    discriminator references whose target *cannot exist in scope by
    name shape*. Cross-feature reachability is doctor's job.

  - Hook into the diagnostics dispatch around line 333.

### 6.2 LSP test additions

In the existing test module of `lazuli_lsp/src/lib.rs`:

- `agent_tools_accepts_canonical_block`
- `agent_tools_rejects_unknown_kind_segment`
- `agent_evals_accepts_case_with_requires_forbids`
- `agent_evals_rejects_given_expect_legacy_vocabulary`
- `agent_discriminator_rejects_when_marker_outside_record`
- `agent_evals_warns_without_temperature_zero_seed`

Tests live near `agent_accepts_canonical_declaration` (line 12517).

## 7. Phase 5 — Inspect projections

**Goal**: `lazuli inspect --expand=...` reports Cut A IR shape.

### 7.1 Files touched

- `crates/lazuli_cli/src/main.rs` (the `inspect` subcommand and its
  expand handlers):
  - `--expand=summary`: extend `AgentSummary` to include
    `tools: Vec<String>`, `evals: Vec<String>`, `output_kind`,
    `output_discriminator`.
  - `--expand=security`: extend the agent section with per-tool
    `effect`, `policy_gap`, write-guard status, PII propagation
    status, eval-determinism status.
  - **New**: `--expand=tools`: emit per-agent dispatch graph keyed
    by tool ref with resolved fields.

### 7.2 Single-pass guarantee

Per the proposal's "Pass model" note: `inspect` base mode (no
`--expand`) must remain single-pass over the file. Cross-feature
resolution runs only under `--expand=tools` and `--expand=security`.

```rust
fn inspect_summary(feature: &ir::Feature) -> SummaryProjection;
fn inspect_tools(feature: &ir::Feature, workspace: &WorkspaceIr) -> ToolsProjection;
fn inspect_security(feature: &ir::Feature, workspace: &WorkspaceIr) -> SecurityProjection;
```

The `summary` handler does not load the workspace.

### 7.3 Snapshot tests

JSON snapshot golden tests for:

- `inspect_summary_includes_agent_tools_evals_output_kind`
- `inspect_tools_resolves_cross_feature_effect`
- `inspect_security_reports_policy_gap`

Goldens live under `crates/lazuli_cli/tests/inspect/` (existing
pattern — survey `tests/inspect/` to find the canonical layout).

## 8. Phase 6 — Fixture + docs

**Goal**: the canonical fixture exercises Cut A, and the normative
docs reflect it.

### 8.1 Files touched

- `examples/full-capsule/full-capsule.lzi`: extend the existing
  `agent summarize_customer` (lines 316–329) with:
  - `temperature 0` and `seed N` (currently 0.2; needed for evals to
    gate).
  - `tools` block referencing local queries.
  - `evals` block with 2–3 cases.
  - Optional: a second agent declaring `output discriminator
    <Enum>` to exercise A2.
- `tools/generate-fixtures.ps1`: confirm it does not strip Cut A
  shapes; add normalization rules if it does.
- `docs/quickref.md` (or `quickref-write.md` post-split): add Cut A
  primitives to the agent section. ~30 lines.
- `docs/canonical-semantics.md`: add a Cut A subsection under
  `## Working With Agents` (line 1043) documenting tools/evals/
  discriminator. ~80 lines.
- `docs/invariants.md`: extend the `agent <name>` invariant (line
  106) with the Cut A fields — replace "optional tool list" with the
  normative shape.
- `docs/design-decisions.md`: add three entries documented in the
  proposal (tools effect derived not declared; evals separate from
  tests; discriminator before flow).

### 8.2 No-op verification

`tools/generate-fixtures.ps1 -Check` must pass after the fixture
update. If it does not, the script needs a Cut A normalization pass
added.

## 9. Phase 7 — MCP + codegen acknowledgment

**Goal**: MCP read tools surface Cut A; Go codegen acknowledges or
stubs the new fields.

### 9.1 Files touched

- `crates/lazuli_mcp/src/lib.rs` (currently 3 lines — empty stub).
  No write API changes (`write_dsl_feature(new_text)` remains the
  only writer per `docs/mcp-abi.md:24`). Read tools may add a `tools`
  projection; this is a minor MCP bump per `docs/mcp-abi.md:103`.
- `crates/lazuli_codegen_go/src/lib.rs` (277 lines): the
  agent-rendering path (if any) should carry `tools` and `output_kind`
  so the generated Go server scaffolds the dispatch table. If
  there's no agent codegen yet (likely — Agent IR doesn't exist),
  add a TODO entry pointing at this plan.
- `crates/lazuli_codegen_ts/src/lib.rs` (577 lines): same — TS
  codegen for agent client may need `output_kind` to type the response.

### 9.2 MCP delta

- `read_inspect` schema gains optional `tools` projection.
- `LZIR_SCHEMA` minor bump per `docs/ir-abi.md` cascades into MCP
  schema — minor MCP bump.

### 9.3 Coordination with the runtime

The runtime spike (Phase B/C in commits 34af967 / e69319e) does not
yet handle agents. The Cut A IR landing should be visible to the
runtime team via the IR JSON projection; runtime work to materialize
agent dispatch is a separate phase that follows IR.

## 10. Estimates

Honest budget after second-pass review. The earlier "13 days"
estimate underweighted Phase 1's eval-predicate parser and Phase 2's
registry-side IR work.

| Phase | Effort | Blockers |
|---|---|---|
| 1 — AST + parser slice | 5–7 days | none (Q-impl-1, Q-impl-2 decided) |
| 2 — IR + lowering + registry extension | 3 days | Phase 1 |
| 3 — Doctor diagnostics | 3 days | Phase 2; Q-impl-4 (write-guard contract) |
| 4 — LSP file-local additions | 1.5 days | none |
| 5 — Inspect projections | 1.5 days | Phase 2 |
| 6 — Fixture + docs | 1 day | Phase 2 (docs reflect IR shape) |
| 7 — MCP + codegen acks | 0.5 day | Phase 2 |
| **Total** | **16–18 days for one engineer** | — |

Phases 1–3 are sequential (~12 days). Phases 4–7 fan out from Phase
3 and total ~5 days; with a second engineer they overlap with the
front of Phase 3, recovering ~4 days. End-to-end with two engineers:
~12 days.

## 11. Acceptance criteria

- `lazuli check examples/full-capsule/full-capsule.lzi` passes after
  the fixture update.
- `lazuli doctor examples/full-capsule` emits zero errors and no
  unexpected warnings.
- `lazuli inspect examples/full-capsule/full-capsule.lzi --expand=
  summary,security,tools --format=json` reports Cut A fields and
  resolved cross-feature tool effects.
- All new tests in Phases 1, 2, 3, 4, 5 pass.
- `tools/generate-fixtures.ps1 -Check` passes.
- `docs/quickref.md` (or `quickref-write.md`) reflects Cut A in the
  Agent section.
- `docs/invariants.md` agent invariant lists tools/evals/discriminator
  as canonical children.
- `docs/design-decisions.md` records the three Cut A decisions.
- A PR review run by `/lazuli-grade` returns a pass with all axes ≥
  7 and the AI-first axis ≥ 8 (matching the proposal acceptance
  criteria).

## 12. Open questions consolidated

Decisions Q-impl-1, Q-impl-2, Q-impl-3 from the prior draft were
resolved in §3.3, §3.4, and §4.1 respectively. Two remain open:

- **Q-impl-4** (Phase 3): write-tool guard accepts `safety` only in
  Cut A (not `idempotency by`; that is Cut B / `flow` territory).
  Document in the diagnostic message body. Decision: yes, `safety`
  only — pending review of one pilot product's tool surface to
  confirm the guard scales.
- **Q-impl-5** (Phase 6): legacy `output <T>` form (without `stream`
  or `discriminator`) — deprecate softly with warn or accept
  silently? Recommendation: emit a `agent_output_kind_default_warning`
  for one cut, then require explicit kind in the cut after.

## 13. Risks

- **Two parser paths coexisting** until canonical-indent migration
  finishes is a real cost. Mitigation: phase plan for migrating the
  next constructs (`flow`, `notification`, ...) following Cut A's
  pattern. Track in `docs/next-checklist.md`.
- **Cross-feature tool resolution accidentally re-loads the
  workspace from `inspect --expand=summary`**. Mitigation: explicit
  separation of `inspect_summary(feature)` from
  `inspect_tools(feature, workspace)` (Section 7.2).
- **Fixture drift** if Cut A primitives are added to fixtures before
  parser/IR support. Mitigation: land Phase 1 before Phase 6.
- **Codegen lag** if backend code is generated against IR that does
  not yet carry `tools`. Mitigation: Phase 7 ships TODO stubs;
  separate runtime cut implements dispatch.
- **Registry-side IR delta misclassified as ambient**. Mitigation:
  §4.5 folds `RegistryToolEntry.effect` and `pii_classes` into
  Phase 2; doctor cannot fire `tool_registry_effect_required_*` or
  `agent_pii_unsafetied_warning` without these.
- **Semantic-validator dispatch from `evals` straddles two
  pipelines.** `expect output contains @semantic.Email` resolves
  through `@validator.<auto>` per proposal §A3. `lazuli check`
  must validate the predicate *shape* without dispatching the
  validator (which would require an LLM run). `lazuli test --evals`
  does the dispatch. Mitigation: the parser produces
  `EvalPredicate::Contains { ..., rhs: ContainsRhs::SemanticType(_) }`
  and lowering records the semantic-type symbol; the validator
  binding is resolved at test-execution time, not at check-time.
  This must be documented inline in the predicate-validation code
  to prevent a future refactor from accidentally invoking the
  validator at check-time.

## 14. Cut A.5 readiness

The IR shape in this plan already supports Cut A.5 (`safety`
accepting a list with PII coverage union check):

- `Agent.safety: Vec<QualifiedName>` (this plan), not
  `Option<QualifiedName>`. Cut A.5 lands by adding the doctor
  diagnostic `agent_safety_pii_coverage_gap_diagnostics` and a
  registry-side `ValidatorExt.covers_pii_classes` field; no IR shape
  change.

## 15. Non-goals for Cut A implementation

- Migrating the entire language to canonical-indent parsing.
- Implementing `flow`, `budget tokens`, `knowledge`, or `quota cost`
  (Cut B / pack territory).
- Generating actual LLM dispatch wiring in Go/TS codegen (separate
  runtime phase).
- Changing the MCP write API.
- Lowering the legacy brace MVP fixtures into canonical-indent
  (`tools/generate-fixtures.ps1` keeps its current job).

## 16. Sequencing summary

```
Approve plan
   ↓
Phase 1 (3-5d) — parser slice
   ↓
Phase 2 (2d) — IR + lowering
   ↓
Phase 3 (3d) — doctor
   ↓
[Phases 4, 5, 6, 7 in parallel — 1.5+1.5+1+0.5 = 4.5d]
   ↓
Acceptance gate (re-grade by lazuli-language-architect)
   ↓
Cut A ships; Cut A.5 follow-on enters the queue
```
