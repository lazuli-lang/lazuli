# Proposal: Cut A.5 — `safety` accepts a list with PII coverage check

**Status**: Draft proposal. Depends on Cut A
(`docs/proposals/ai-primitives-v0.md`) IR shape; lands as a small
follow-on after Cut A's first cut ships.

**Owner**: TBD. **Target version**: same `LZI_LANG` minor that ships
Cut A, or the next one. Depends on registry-side IR landing.

## Motivation

Cut A's `tools` block lets an agent fan-in to multiple capabilities
that may return PII fields. The fan-in side is checked: doctor
collects `Agent.tools[].resolved_pii_classes` from the underlying
queries and registry tool entries, and warns when a `read` tool
returns `@pii.*` fields and the agent has no `safety` validator
(`agent_pii_unsafetied_warning`).

The fan-out side — the `safety` slot — is currently zero or one
validator. In any real product where agents touch more than one
classification of PII (`@pii.contact` for emails, `@pii.credential`
for API keys, `@pii.behavioral` for derived signals), a single
validator quickly becomes "the validator for all PII", which is
exactly the kind of catch-all the architect's discipline rejects.

The fix is small and additive: `safety` accepts a list, and doctor
checks that the union of validator coverage spans the union of tool
PII classes. The IR shape Cut A already chose (`Agent.safety:
Vec<QualifiedName>`, not `Option<...>`) is forward-compatible. This
proposal closes the missing lattice check.

## Scope

- `safety @validator.<name>, @validator.<name>` (list form).
- Doctor diagnostic
  `agent_safety_pii_coverage_gap_diagnostics`.
- Registry-side `ValidatorExt.covers_pii_classes: Vec<PiiClass>`.
- No language-vocabulary additions; no new keywords; no new namespace.

## Promotion gate

Cut A.5 lands when **a pilot product surfaces evidence of the
catch-all anti-pattern**: a code review where an author wrote a
single `safety @validator.pii_scrub` covering two or more `@pii.*`
classes via a single validator, and the reviewer flagged it as
"this validator is doing too much; we should split."

The naïve form of this gate ("first pilot with multi-class PII fan-
in") is satisfiable on any cut that touches `customer.email` plus
`customer.api_key` — that's pressure-shaped, not evidence-shaped.
Real promotion requires that the catch-all anti-pattern actually
emerged and was rejected in review, which proves the lattice check
earns its weight. Without that evidence, Cut A.5 is speculative
coverage: a check whose value cannot be measured.

Until the gate fires, the language continues to allow
`safety @validator.<single>` and doctor's existing
`agent_pii_unsafetied_warning` covers the zero-validator case.

## Syntax

Single validator (Cut A baseline, unchanged):

```lazuli
agent summarize_customer
  ...
  safety @validator.pii_scrub
```

Multiple validators (Cut A.5):

```lazuli
agent summarize_customer
  ...
  safety @validator.pii_email_scrub, @validator.pii_credential_scrub
  tools
    customer.query.by_id            # returns @pii.contact (email)
    customer.query.api_keys         # returns @pii.credential
```

## Rules (normative)

- **Header shape**: `safety @validator.<name> [, @validator.<name>]*`.
  One or more validator references on a single line, comma-separated.
  Multi-line form (one validator per line under `safety` block) is
  reserved and not in this cut.
- **Coverage union (doctor, error)**: doctor computes `tool_classes
  = union(Agent.tools[].resolved_pii_classes)` and `validator_classes
  = union(Agent.safety[].covers_pii_classes)` from the registry-side
  validator declaration. If `tool_classes` ⊄ `validator_classes`,
  emit `agent_safety_pii_coverage_gap_diagnostics` with both sets
  spelled out and the missing classes highlighted.
- **Registry-side `covers_pii_classes`**: every `validator <name>:
  Validator[Text]` (or any `Validator[<scope>]`) declaration in
  `extensions` may declare `covers_pii @pii.<class>, @pii.<class>` on
  a child line. If the declaration is omitted, the validator is
  treated as `covers_pii = []` (covers nothing) and any agent that
  lists it as `safety` for an agent with PII-returning tools will
  fail the coverage check.
- **Backwards compatibility**: existing single-validator agents do
  not change. Agents whose tools return PII but whose validator does
  not declare `covers_pii` start to fail. This is intentional — it
  surfaces the silent "single validator covers everything" assumption.
  A grace cut may emit the failure as warning before turning to
  error; recommendation: ship as warning in the first cut, error in
  the second.
- **Inspect**: `--expand=security` reports per-agent
  `safety_coverage` with `tool_classes`, `validator_classes`, and
  any gap.

## IR delta

Zero changes on `Agent` because Cut A's plan
(`docs/proposals/ai-primitives-v0-implementation.md §4.1`) already
sets `Agent.safety: Vec<QualifiedName>`.

One additive field on the existing `Extension` wrapper struct
(`crates/lazuli_ir/src/lib.rs:1370`). The metadata lives on the
outer wrapper, not on the `ExtensionContract::Validator` variant
(line 1394–1395), because `covers_pii` describes the *extension
entry*, not the validator's type signature:

```rust
// crates/lazuli_ir/src/lib.rs — extends the existing Extension struct
pub struct Extension {
    pub name: String,
    pub contract: ExtensionContract,    // unchanged: Validator { type_arg: TypeRef }
    pub resolved_path: PathRef,
    pub previous_names: Vec<String>,
    pub span_ref: Option<SpanRef>,
    // NEW (Cut A.5):
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covers_pii_classes: Vec<QualifiedName>,  // @pii.*; non-empty only for Validator extensions
}
```

The field is `Vec<QualifiedName>` typed for any extension but
populated only for `ExtensionContract::Validator { .. }`. Doctor
consumers walk the wrapper, not the variant. Other variants
(`Hook`, `Function`, etc.) ignore the field; lowering checks that
`covers_pii_classes` is empty for non-validator extensions and
emits a warning otherwise.

`LZIR_SCHEMA`: minor bump (additive). `LZI_LANG`: minor bump.

## Doctor diagnostic shape

```text
agent_safety_pii_coverage_gap_diagnostics  error
  Agent `summarize_customer` consumes tools whose result fields
  carry PII classes that are not covered by any declared safety
  validator.

  Tool PII classes resolved:
    customer.query.by_id        : @pii.contact
    customer.query.api_keys     : @pii.credential

  Safety validators declared:
    @validator.pii_email_scrub  : @pii.contact

  Missing coverage:
    @pii.credential

  Add a validator that declares `covers_pii @pii.credential`, or
  remove the `@pii.credential`-returning tool from this agent.
```

Lives in `crates/lazuli_cli/src/doctor.rs` next to the Cut A agent
diagnostics. Uses the same `Agent.tools[].resolved_pii_classes`
field landing in Cut A Phase 2.

## Inspect delta

Extend `--expand=security` per agent:

```json
{
  "agent": "summarize_customer",
  "safety_coverage": {
    "tool_classes": ["@pii.contact", "@pii.credential"],
    "validator_classes": ["@pii.contact"],
    "missing": ["@pii.credential"]
  }
}
```

The `safety_coverage` block appears only when `tool_classes` is
non-empty. Single-validator and zero-PII-tool agents see a no-op.

## Why language, not pack

Three reasons:

1. The check uses the same closed `@pii.*` namespace the language
   already enforces. Putting the lattice check in a pack would
   require the pack to re-derive what "pii" means, in source.
2. The check is doctor-shaped (cross-feature, package-wide), not
   runtime-shaped. It runs once at check-time and emits a typed
   error.
3. The IR fields (`tools[].resolved_pii_classes`,
   `safety[].covers_pii_classes`) are already in scope from Cut A's
   landing. No new IR shape; the lattice walks existing fields.

## Why not earlier (in Cut A)

The architect's first re-grade explicitly suggested Cut A.5 as a
follow-on, *not* part of Cut A itself. Reason given: bundling six
primitives in one cut violated the promotion lifecycle. The pattern
holds here — Cut A.5 stays a standalone follow-on so that:

- Cut A's IR delta lands on its own merits without the registry-side
  `covers_pii_classes` extension blocking it.
- The pilot-evidence gate ("first product with multi-class PII fan-
  in") is honored.
- Each cut keeps a single dominant concern.

## Acceptance criteria

- Pilot product confirmed to have multi-class PII fan-in on at least
  one agent.
- Cut A's `Agent.safety: Vec<QualifiedName>` IR shape has shipped.
- Cut A's `Agent.tools[].resolved_pii_classes` derivation works.
- Doctor diagnostic implemented and tested with three cases:
  - all classes covered (passes)
  - one class missing (errors)
  - no PII tools at all (no-op)
- Registry-side `covers_pii_classes` parses, lowers, and is
  honored.
- Inspect `--expand=security` reports `safety_coverage` correctly.
- `quickref-write.md` (or `quickref.md` if not split) gains a one-
  line note under the `safety` row of the Agent section.
- `docs/invariants.md` agent invariant updates "single validator" to
  "one or more validators".
- `docs/design-decisions.md` records two entries:
  1. *`safety` accepts a list because multi-class PII fan-in via
     `tools` made the single-validator form the catch-all the
     discipline rejects.*
  2. *Cut A.5 ships its diagnostic as a warning first, then promotes
     to error in the next minor cut. Direct error-on-arrival would
     break every existing validator that omits `covers_pii`; the
     warning cycle gives teams time to backfill without breaking
     CI. The warning→error pattern is reserved for additive
     diagnostics whose enforcement requires authoring metadata that
     does not yet exist.*

## Non-goals

- Multi-line `safety` block syntax (reserved; not needed for v1).
- Validator dispatch order. Doctor checks coverage; runtime
  composition (sequential vs parallel) is runtime/adapter concern.
- Validator failure modes (which validator fired, which class the
  failure was on). Runtime / observability concern.
- Auto-suggesting validators by class. The author writes the
  validator list; doctor only checks coverage.

## Migration impact

Existing single-validator agents on agents without PII-returning
tools: zero impact.

Existing single-validator agents on agents with PII-returning tools:
- if the validator's registry entry declares `covers_pii` for those
  classes, zero impact.
- if the validator's registry entry omits `covers_pii`, the agent
  starts producing the new diagnostic. This is the intended
  behavior.

Recommended migration: ship Cut A.5 as a *warning* in the first
release, allowing teams to backfill `covers_pii` declarations on
existing validators. Promote to error in the next minor release.

## Release timing

Ship one cycle **behind** Cut A. Same release would conflate two
`LZI_LANG` minor bumps with different landing risks: Cut A's
parser-slice is foundational and may surface unforeseen issues; Cut
A.5's only friction is the registry-side IR sibling. Bundling tempts
skipping the pilot gate, which inverts the proposal's whole value
proposition (evidence-gated coverage). One cycle of pilot exposure
between Cut A and Cut A.5 is the discipline this proposal invokes
for itself.

## Reserved

- Multi-line `safety` block (`safety NEWLINE INDENT validator-ref+
  DEDENT`).
- Per-class validator routing (`safety @validator.x for
  @pii.contact, @validator.y for @pii.credential`). Reserved for if
  the simple coverage union proves insufficient.
- Validator coverage *intersection* checks (warn if two validators
  both claim to cover the same class). Defer until evidence shows
  redundancy is actually a problem.
