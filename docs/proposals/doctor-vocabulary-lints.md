# Doctor Vocabulary Lints (L0)

**Status**: L0 design proposal. No DSL surface change — extends the existing `lazuli doctor` rule catalog with a new **category** of lints (`VOCAB-*`) that detect when authored `.lzi` re-implements semantics the language already names.

**Audience**: Lazuli compiler team, doctor rule authors, downstream product authors writing `.lzi`, AI agents emitting `.lzi`.

**Date**: 2026-05-14.

**Pilot bucket**: doctor / cli cross-cutting. Not a new bucket; widens an existing one.

**Companion**:
- `docs/design-principles.md` §"Rule Zero: Vocabulary Over Mechanism" — the principle this proposal operationalises.
- `docs/invariants.md` §"Source And Derived Views" — names existing vocabulary (`derived from`, `union`, `has_many`, `audit`, `lifecycle`-future, semantic types).
- `docs/architecture.md` §"Founding principle" — wire-thin discipline at the runtime level; this is the same discipline at the **DSL-author** level.
- `crates/lazuli_cli/src/doctor.rs` (or equivalent) — current rule catalog (`crypto-tier`, `security-opt-out`, `app-url-contract`, ~30 others).

**First consumer**: the three v2 dogfood products (`lucasrgt/pleiades`, `lucasrgt/atelier`, `lucasrgt/erudito`). All three would currently trigger at least one `VOCAB-*` warning after this lands; that's the intended forcing function — `.lzi` declarations refactor to use existing vocabulary, handlers get smaller or disappear.

---

## Problem

Lazuli's "Vocabulary Over Mechanism" principle (Rule Zero) is enforced today by **convention** — proposals get graded, AGENTS.md tells humans + agents what to prefer, design docs cite the rule. But the DSL itself accepts perfectly grammatical `.lzi` that re-implements semantics the language already names.

Concrete drift observed across the v2 triple-dogfood (2026-05-13):

| Drift | What we wrote | What the vocabulary says we could have written |
|---|---|---|
| `LearningNode` (Erudito) with `node_type: enum {deepening, bridge}` + optional `bridge_justification: Text` (only required when bridge) | `enum` + optional field + handler-side validation | Discriminated `union` — `union LearningNode { Deepening \| Bridge { justification: Text required, target_topic: Topic required } }` |
| `quiz.passing_score: Int default 70` with handler-side `0..100` clamp | `Int` + handler validation | A named semantic type (`@semantic.Percent`) when the catalog adds it |
| `command approve { updates Post; status = "approved"; approved_at = ctx.now; approved_by = ctx.user }` plus three more transition commands forming the state graph `draft → in_review → approved → archived` | N transition commands + status enum | (Future) `lifecycle approval { draft → in_review → approved → archived }` once the lifecycle vocabulary lands |
| `@fn.canonical_url(post)` computed in a handler, never written | `@fn.X` handler reference | `derived from <expr>` (already in vocabulary; agents/devs don't know it exists) |
| `PackMembership` resource with two FKs + `unique (pack, post)` + `added_at`/`added_by` | Plain resource | `has_many <pack>: Pack through PackMembership` once the through-form lands |

Every one of these drifts is grammatical today. Doctor doesn't say a word. The result is that handlers, schemas, and the runtime grow logic the language could have expressed declaratively — and the agent emitting `.lzi` doesn't know any better, because the doctor doesn't tell it.

**Cost compounds:**

- **Handler bloat** — handler-side validation, condition checks, derived-value calls grow because the DSL "didn't have" the expressive primitive. (It did; we just didn't surface that.)
- **Audit drift** — `lazuli inspect --expand=security` doesn't see the invariant "bridge nodes always have justification" because that invariant lives in a Go handler, not the IR.
- **AI agents iterate slowly** — agents emit primitive shapes by default (cheaper next-token), then humans rewrite. Without doctor feedback, every fresh session repeats the rewrite cycle.
- **Documentation rot** — the vocabulary primitives that *exist* but aren't *named* by doctor lints fall off the agent's working set. `derived from` is in `docs/invariants.md`; nobody uses it.

---

## Proposal — `VOCAB-*` rule catalog

Add a **new severity-class** to doctor: **vocabulary-fitness**. Mechanically the same as existing rule codes (rule fn walks the IR, emits a `Diagnostic` with code + span + suggestion), but conceptually distinct from **correctness lints** (e.g. `crypto-tier`, `app-url-contract`).

### Rule code convention

`VOCAB-<CATEGORY>-NNN` where:

- `<CATEGORY>` names the vocabulary primitive the rule advocates: `UNION`, `LIFECYCLE`, `DERIVED`, `MEMBERSHIP`, `SEMANTIC`, `AUDIT`, …
- `NNN` is the per-category index (000 reserved for the canonical case; 001+ for variants).

Example: `VOCAB-UNION-001` for "enum + optional fields conditional on enum tag → discriminated union".

### Default severity

- **`warning`** in strict-profile.
- **`error`** in production-profile.
- **`off`** in draft-profile (already exists; means "I'm sketching, don't lint me").

This mirrors `security-opt-out` (warning in strict, error in production). The point isn't to block — it's to surface drift so the agent/author sees the alternative before committing.

### Severity overrides

`Lazurite.toml` adds:

```toml
[doctor.vocab]
# Disable a specific rule globally (e.g. you genuinely want the primitive form)
"VOCAB-UNION-001" = "off"

# Or downgrade to info (logged but not warning)
"VOCAB-SEMANTIC-PERCENT-001" = "info"

# Or upgrade — vocabulary rule becomes error in strict (production-only by default)
"VOCAB-DERIVED-READ-001" = "error"
```

Doctor reports its effective severity per rule for `lazuli inspect --expand=doctor`. Project teams override with reason in the toml comment.

### Auto-fix policy

**No auto-fix.** Every `VOCAB-*` rule changes schema shape (DB columns, IR layout, generated Go types). Auto-fix would silently rewrite the user's persistence layer — that's a footgun, not a quality-of-life feature. Each rule emits **a suggestion** in the diagnostic; the author applies it manually.

ESLint's `--fix` works because `let → const` is semantics-preserving. Lazuli's vocabulary refactors are shape-changing. The diagnostic is the surface; the rewrite is human-driven.

---

## Starter catalog (6 rules)

### `VOCAB-UNION-001` — enum + correlated-optional-fields

**Detection heuristic:**

A resource declares an `enum`-typed field `kind` AND ≥1 field that is `optional` AND that field is only meaningful when `kind = <one specific tag>`. The "only meaningful when" signal is detected via: (a) handler reference (`@fn.X`) that branches on `kind`, OR (b) inline doctor pragma `# only-when kind=<tag>`, OR (c) heuristic match on the field name's tag prefix (`bridge_*` paired with `kind = bridge`).

**Example trigger:**

```lzi
resource LearningNode
  workspace: Workspace required
  node_type: LearningNodeType required        # enum {deepening, bridge}
  bridge_justification: Text                  # optional — only when bridge
  bridge_target_topic: Topic                  # optional — only when bridge
```

**Suggested refactor (in the diagnostic):**

```lzi
union LearningNode
  Deepening
    workspace: Workspace required
  Bridge
    workspace: Workspace required
    justification: Text required
    target_topic: Topic required
```

**False-positive cases (rule MUST not fire):**

- Optional fields that are universally optional (no correlation with any enum value). Tags, free-form notes, etc.
- Enum types with only ONE variant materially used (the conditional fields are deprecated leftovers; codegen can already see they're unused — different lint).
- Resources where the union form would *increase* total schema width by N×variants — the rule should report total-field-count for both forms and skip if the union form is wider.

**Severity**: `warning` (strict), `error` (production).

---

### `VOCAB-LIFECYCLE-001` — sequential-status transition commands

**Status**: **deferred** until the `lifecycle` vocabulary actually exists. Listed here for catalog completeness; documented as a Phase F follow-up.

**Sketch (for the eventual rule):**

A resource has `status: <Enum>` and ≥3 commands that all `updates <Resource>` setting `status = <next-value-in-enum-order>` and an `*_at: DateTime` field. The commands form a linear-ish DAG over the enum values. Suggestion: `lifecycle <name> { v1 → v2 → v3 → ... }`.

**Why deferred:** the lifecycle vocabulary itself doesn't exist yet. Adding the lint without the destination vocabulary is misalignment with Rule Zero (we'd be enforcing absence). Land the lifecycle proposal first, then this rule lands in the same wave.

---

### `VOCAB-DERIVED-READ-001` — handler-computed read-only field

**Detection heuristic:**

A field in a resource is referenced by a handler `@fn.X` that:

1. Reads the resource fields (anywhere in the handler trace)
2. Computes a value
3. Is referenced from an output/return projection but NEVER from `creates`/`updates`/`deletes` writes
4. The DB column for the field exists but is computed every read (signal: never appears in `creates.<field> =` or `updates.<field> =`)

**Example trigger:**

```lzi
resource Post
  workspace: Workspace required
  title: Text required
  slug: Text required
  canonical_url: Text                    # only ever read; computed in @fn.serialize_post
```

**Suggested refactor:**

```lzi
resource Post
  workspace: Workspace required
  title: Text required
  slug: Text required
  canonical_url: Text derived from "{{app.url}}/p/{{slug}}"
```

(Vocabulary reference: `docs/invariants.md:89-92` already specifies `derived from` for read-time computed fields; it's not persisted, no `default`/`required`/`optional`, no input/effect targets.)

**False-positive cases:**

- Fields written by `creates` once and never updated — those are persisted, NOT derived. The rule fires only when there is no `creates`/`updates` write site.
- Fields written by a job/migration (not a command) — still persisted, just by a different agent. The lint walks BOTH `command.*` and `job.*` write sites.
- Fields with an explicit `@cap.*` capability tier — capability tiers imply storage semantics that conflict with `derived from`.

**Severity**: `warning` (strict), `warning` (production). Lower severity than `VOCAB-UNION-001` because the read-time variant is more debatable; some projects materialise computed values intentionally for index-ability.

---

### `VOCAB-MEMBERSHIP-001` — through-resource pattern

**Status**: **deferred** until the `has_many <name>: <Target> through <ThroughResource>` form lands.

(Already named in `docs/invariants.md:98-100` as a stub; details TBD.)

**Sketch:**

A resource X has exactly two `required` FK fields to resources A and B, a `unique (a, b)` constraint, and timestamp+actor fields (`added_at`/`added_by` or similar). Suggestion: `A has_many B through X` once vocabulary is live.

**Why deferred:** same as `VOCAB-LIFECYCLE-001` — the destination vocabulary doesn't exist. Both land in the same wave (Phase F probably).

---

### `VOCAB-SEMANTIC-PERCENT-001` — bounded-int with explicit range

**Detection heuristic:**

A field declared as `Int` with a default value AND a handler-side `@validator.X` or doctor pragma `# range 0..100` that bounds the value to `[0, 100]` (or `[0.0, 1.0]` for float).

**Example trigger:**

```lzi
resource Quiz
  passing_score: Int default 70    # @validator.percent_0_100 enforced in handler
```

**Suggested refactor (post `@semantic.Percent` landing):**

```lzi
resource Quiz
  passing_score: @semantic.Percent default 70
```

**Status**: **partially deferred** — depends on `@semantic.Percent` (or whatever the canonical name turns out to be) being added to the semantic-type catalog. If the catalog already includes a closely-aligned semantic type, the rule fires today; otherwise it's a v0.1 stub that activates when the catalog grows.

**Detection reach caveat (important for L2 cell author):** the rule walks IR only and does NOT read handler Go code. Range enforcement that lives **inside** a `@fn.X` handler (e.g. `if score > 100 { score = 100 }`) is invisible to this lint — the rule fires only when the bound is named at the IR level (validator declaration, doctor pragma, or — once it lands — a typed range constraint on the field). Authors using handler-side clamps won't see a `VOCAB-SEMANTIC-PERCENT-001` until they surface the bound to the IR. Acceptable trade-off; matches the "no behaviour analysis" boundary at the bottom of this proposal.

**False-positive cases:**

- The Int genuinely represents a count, ID, age, etc. — not a bounded ratio. Detection MUST require explicit range evidence (validator name or doctor pragma), not heuristic-on-default-value.
- Float fields in `[0.0, 1.0]` may want a different semantic (`@semantic.Probability`?) — different rule code (`VOCAB-SEMANTIC-PROBABILITY-001`).

**Severity**: `warning` (strict), `warning` (production). Lower bar because the rule depends on the semantic catalog being complete.

---

### `VOCAB-AUDIT-001` — write command without `audit`

**Detection heuristic:**

A `command` block that contains `creates`/`updates`/`deletes` AND does NOT declare an `audit` child (default, custom field list, or `audit none`).

**Example trigger:**

```lzi
command update_status
  route id: ID
  input
    status: PublicationStatus required
  policy @policy.editor
  rate_limit "300 per hour per actor"
  updates Publication
    status = input.status
  # MISSING: audit default / audit <fields> / audit none
```

**Suggested refactor:**

```lzi
command update_status
  route id: ID
  ...
  audit default     # or: audit status / or: audit none (with documented reason)
```

(Vocabulary reference: `docs/invariants.md:93-97` — commands/queries/jobs/webhooks should declare an explicit `audit` child so audit-log generation has a typed contract, not event-name conventions.)

**False-positive cases:**

- `command` blocks that don't write any resource (pure compute / call-out). Detection MUST require an `creates`/`updates`/`deletes`/`emits` effect.
- `query.*` blocks (this rule applies only to mutating effects, not reads).

**Severity**: `warning` (strict), `error` (production). The audit contract is load-bearing for compliance surfaces; absence is a real cost.

---

### `VOCAB-EVENT-PAYLOAD-001` — `emits` without typed payload

**Detection heuristic:**

A `command` declares `emits <event.name>` AND that event name is not declared in any `emits` block at the feature level, OR the event has no `payload <Type>` declaration.

**Example trigger:**

```lzi
command post.archive
  ...
  emits post.archived               # but no `emits event post.archived` block declared

# (or)

emits
  event post.archived               # no `payload <Type>` declared
```

**Suggested refactor:**

```lzi
emits
  event post.archived
    payload Post
```

**False-positive cases:**

- Events intentionally payload-less (e.g. liveness heartbeats). Rule respects `event <name> payload none` as the explicit opt-out form (catalog-fixed sentinel, not a free predicate).

**Severity**: `warning` (strict), `warning` (production).

---

## Active subset for v0.1

The catalog above lists 6 rules; only the ones whose destination vocabulary **already exists** ship in v0.1. That's:

| Rule | Status | Ships in v0.1? |
|---|---|---|
| `VOCAB-UNION-001` | active (union exists) | ✅ |
| `VOCAB-LIFECYCLE-001` | deferred (lifecycle vocabulary doesn't exist) | ❌ |
| `VOCAB-DERIVED-READ-001` | active (`derived from` exists) | ✅ |
| `VOCAB-MEMBERSHIP-001` | deferred (`has_many through` doesn't exist) | ❌ |
| `VOCAB-SEMANTIC-PERCENT-001` | deferred (semantic-type catalog incomplete) | ❌ |
| `VOCAB-AUDIT-001` | active (audit vocabulary exists) | ✅ |
| `VOCAB-EVENT-PAYLOAD-001` | active (event/payload vocabulary exists) | ✅ |

**v0.1 ships 4 rules.** Deferred rules land in the wave that introduces their destination vocabulary — never separately. That order matters: a lint enforcing absent vocabulary is itself a Rule Zero violation (mechanism without name).

---

## Active subset for v0.2 (catalog v2)

Cells A1-A4 from the 2026-05-14 wave (16-worker batch).

| Rule | Status | Ships in v0.2? | Cell |
|---|---|---|---|
| `VOCAB-CAP-MISSING-001` | active (capability vocabulary exists) | ✅ | A1 |
| `VOCAB-GRAMMAR-FORM-001` | active (canonical indentation is law) | ✅ | A2 |
| `VOCAB-UNION-002` | active (union vocabulary exists; polymorphic-FK variant of UNION-001) | ✅ | A3 |
| `HOOK-TARGET-001` | active (extension-points vocabulary exists; correctness, not vocab) | ✅ | A4 |

**v0.2 adds 4 rules** — three vocabulary lints + one correctness rule (`HOOK-TARGET-001`). All ship with the same diagnostic shape + severity-override knob as v0.1.

### `VOCAB-CAP-MISSING-001` — PII-shaped field without `@cap.*`

Closed PII-name catalog: `ssn`, `cpf`, `cnpj`, `tax_id`, `tax_number`, `vat`, `vat_id`, `national_id`, `passport`, `passport_number`, `credit_card`, `card_number`, `iban`, `bank_account`, `annual_revenue`, `salary`, `income`, `date_of_birth`, `birth_date`, `dob`, `drivers_license`, `license_number`, `email`, `phone`, `phone_number`, `mobile`, `address_line`, `street_address`, `ip_address`.

Detection: case-insensitive exact or `_<token>` suffix match. False-positive guards: fields with `TypeRef::Capability(_)` of any tier skip; `@semantic.Email` / `@semantic.Phone` (semantic-typed) skip; FK-shape `user: User`, `owner: User` does not match the catalog.

Severity: `warning` (strict), `error` (production).

### `VOCAB-GRAMMAR-FORM-001` — legacy curly-brace dialect

Walks raw source, not IR (lowering strips the dialect signal). Closed catalog of top-level keywords: `aggregate`, `resource`, `feature`, `domain`, `policies`, `command`, `query.list`, `query.lookup`, `query.sql`, `job`, `webhook`, `notification`. Fires when `<keyword> ... {` is the last non-whitespace token on a line. Per-line state machine for string literals; comment-line skip.

Severity: `warning` (strict), `warning` (production).

### `VOCAB-UNION-002` — polymorphic FK (enum discriminator + untyped id)

Sibling of `VOCAB-UNION-001`. Closed discriminator-name catalog: `target`, `subject`, `attachment_target`, `parent_target`. Fires when resource has `<name>: <Enum>` + sibling `<name>_id: ID` AND enum has ≥2 variants AND the FK is `BuiltinType::Id`/`Text` (untyped). Suggests discriminated union OR typed-FK sibling resources.

Severity: `warning` (strict), `error` (production).

### `HOOK-TARGET-001` — extension hook references undefined target

Correctness rule, not vocab. Fires when `hook x: Hook[Foo]` references `Foo` and no command/query/job/record/event/resource of that name exists in the same feature. Cross-feature references (`other.Foo`) skipped (different rule).

Severity: `error` (strict and production). Dangling references are correctness bugs.

---

## Active subset for v0.3 (catalog v3)

Cells A5-A8 from the same wave. Driven by triple-dogfood NEW rule candidates surfaced in `project_product_vocab_audits_2026-05-14.md`.

| Rule | Status | Ships in v0.3? | Cell |
|---|---|---|---|
| `VOCAB-EVENT-ORPHAN-001` | active (event/payload vocab exists; inverse of EVENT-PAYLOAD-001) | ✅ | A5 |
| `VOCAB-EVENT-PRODUCER-001` | active (emits/event vocab exists) | ✅ | A6 |
| `VOCAB-AUDIT-002` | active (audit + @cap.* vocab exists; sibling of AUDIT-001) | ✅ | A7 |
| `VOCAB-JSON-TYPED-001` | active (record/union vocab exists) | ✅ | A8 |
| `COMMAND-INPUT-SHADOWS-FIELD-001` | correctness rule (paired with HOOK-TARGET-001 in `correctness/`) | ✅ | A12 |

### `VOCAB-EVENT-ORPHAN-001` — event declared but never emitted

Inverse of `VOCAB-EVENT-PAYLOAD-001`. Walks `Feature.events` + `Feature.commands.iter().flat_map(|c| &c.emits)`. Fires when an event with `payload_none: false` has no emitter in the same feature. Cross-feature emission out of scope; `EventKind::External` (where applicable) skipped.

Severity: `warning` (strict), `warning` (production).

### `VOCAB-EVENT-PRODUCER-001` — mutating command without IR-visible emits

Walks `Feature.commands` looking for `c.emits.is_empty()` AND non-Returns effect AND ≥1 matching event-name on the targeted resource (`<resource_lower>.*`). Surfaces handler-side event emission that the IR can't see. Audit-none commands skipped.

Severity: `warning` (strict), `warning` (production).

### `VOCAB-AUDIT-002` — handler-only command on `@cap.*` fields lacks audit

Sibling of `VOCAB-AUDIT-001` (which catches mutation-effect commands without audit). This variant catches Returns/None-effect commands whose `invalidates` list touches a resource carrying `@cap.Encrypted`/`@cap.Token`/`@cap.Hashed`/`@cap.PII` fields.

Severity: `warning` (strict), `error` (production).

### `VOCAB-JSON-TYPED-001` — untyped JSON bag + sibling closed-catalog enum

Fires when a resource has `field: JSON` (untyped bag) with a sibling enum that thematically matches (heuristic: enum name contains field name OR `<resource>Type`) but is referenced nowhere else. Surfaces documenting-but-not-constraining shapes.

Severity: `warning` (strict), `warning` (production).

### `COMMAND-INPUT-SHADOWS-FIELD-001` — input slot shadows resource field with different type

Correctness rule. Fires when a typed command input has the same name as a field on the command's `creates`/`updates` target resource but a different declared `TypeRef`. Surfaces silent type narrowing in handlers. Walks `CommandInput::Typed(slots)`; skips `Short`/`Empty`/`Returns`/cross-feature.

Severity: `warning` (strict), `error` (production).

---

## Implementation status (post-wave)

After the 2026-05-14 16-worker wave, the rule MODULES are registered (`crates/lazuli_doctor/src/vocab/mod.rs`, `crates/lazuli_doctor/src/correctness/mod.rs`) and each rule's `#[cfg(test)] mod tests` exercises the logic. **Full dispatch into `DoctorPackage::diagnostics()` is a separate follow-up cell** (~+500 LOC of IR loading + Finding → DoctorDiagnostic adapter) — none of the 4 v0.1 rules nor the 7 new ones currently surface in `lazuli check` output. The pre-existing v0.1 rules had the same gap; this wave doesn't widen it but doesn't close it either.

Note: rule modules were extracted from `crates/lazuli_cli/src/doctor/vocab/` to `crates/lazuli_doctor/src/vocab/` on 2026-05-15 so the LSP can import them (see `crates/lazuli_cli/src/doctor.rs:9-11` re-exports). This document updated 2026-05-16 to reflect the current path; all subsequent `crates/lazuli_doctor/src/vocab/` references in this file are the canonical layout.

The follow-up cell shape (when it ships):

```rust
fn vocab_diagnostics(features: &[(PathBuf, &Feature)], severity: SecurityProfile) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for (path, feature) in features {
        diagnostics.extend(adapt(vocab_audit_001::check(feature, path), DoctorSeverity::Warning));
        diagnostics.extend(adapt(vocab_union_001::check(feature, path), DoctorSeverity::Warning));
        // …all 11 rules…
    }
    diagnostics
}

fn adapt(findings: Vec<impl FindingLike>, severity: DoctorSeverity) -> Vec<DoctorDiagnostic> {
    findings.into_iter().map(|f| DoctorDiagnostic {
        path: f.path(),
        line: 0,  // until source-map landing, line/col stay 0
        column: 0,
        severity,
        code: f.code().to_owned(),
        message: f.message(),
    }).collect()
}
```

This is mechanical — a separate Codex cell can ship it once the IR-loading site is decided (probably reusing `Tier3FeatureFacts` walk).

---

## Implementation shape

### File layout

One file per rule under `crates/lazuli_doctor/src/vocab/`:

```
crates/lazuli_doctor/src/vocab/
  mod.rs                          # registry of all VOCAB-* rules
  vocab_union_001.rs              # VOCAB-UNION-001 detector
  vocab_derived_read_001.rs       # VOCAB-DERIVED-READ-001 detector
  vocab_audit_001.rs              # VOCAB-AUDIT-001 detector
  vocab_event_payload_001.rs      # VOCAB-EVENT-PAYLOAD-001 detector
```

`crates/lazuli_cli/src/doctor.rs:9-11` re-exports the module so CLI-side
call sites keep their `vocab::*` qualified references unchanged.

`mod.rs` is the catalog registry — a 5-line stub that other doctor wiring imports:

```rust
//! VOCAB-* rules — Rule Zero enforcement (Vocabulary Over Mechanism).
//! See docs/proposals/doctor-vocabulary-lints.md for the design.

pub mod vocab_union_001;
pub mod vocab_derived_read_001;
pub mod vocab_audit_001;
pub mod vocab_event_payload_001;

pub fn all_rules() -> Vec<Box<dyn DoctorRule>> {
    vec![
        Box::new(vocab_union_001::VocabUnion001),
        Box::new(vocab_derived_read_001::VocabDerivedRead001),
        Box::new(vocab_audit_001::VocabAudit001),
        Box::new(vocab_event_payload_001::VocabEventPayload001),
    ]
}
```

New rules append to `all_rules()` in a single-line diff per cell — that's the only shared-file edit (per-cell orchestrator-driven, not Codex-driven).

Each rule file exports:

```rust
pub struct VocabUnion001;

impl DoctorRule for VocabUnion001 {
    fn code(&self) -> &'static str { "VOCAB-UNION-001" }
    fn category(&self) -> RuleCategory { RuleCategory::Vocabulary }
    fn check(&self, module: &Module, ctx: &DoctorCtx) -> Vec<Diagnostic> { ... }
}

#[cfg(test)]
mod tests {
    // positive: detects on enum + correlated-optional-fields
    // negative: skips when correlation absent
    // negative: skips when union form would be wider
}
```

The `RuleCategory::Vocabulary` enum variant is new; correctness rules stay `RuleCategory::Correctness` (or whatever it's named today). `lazuli inspect --expand=doctor` lists rules grouped by category.

### Integration with the doctor walker

The doctor today walks the IR once and dispatches each rule. The new vocab rules join the same walk — no new pass. Cost: one extra branch per rule per resource/command (cheap).

### Configuration parsing

`Lazurite.toml` `[doctor.vocab]` table is parsed by `crates/lazuli_cli/src/manifest.rs` (or the equivalent toml-load site). Each entry maps a rule code to one of `off | info | warning | error`. The parser rejects unknown rule codes (you can't override a rule that doesn't exist).

**Comment-required check on overrides.** Each override entry MUST have a comment on either the line above OR inline on the same line. The toml parser raises a doctor warning `DOCTOR-OVERRIDE-NEEDS-REASON-001` when an override lacks a neighbouring comment. Rationale: overrides leak design debates into product configs (risk #2 below); requiring a justification line keeps the override discoverable when someone audits the project's vocab posture later.

Example valid form:

```toml
[doctor.vocab]
# brand-system fields hold deliberately-flat JSON; union form would
# force per-category resource splits we don't want yet (2026-05-14).
"VOCAB-UNION-001" = "off"
```

Example rejected form (warning, not error — wave-1 friendly):

```toml
[doctor.vocab]
"VOCAB-UNION-001" = "off"          # ← no comment line above; DOCTOR-OVERRIDE-NEEDS-REASON-001 fires
```

The override-reason rule itself is a doctor-correctness rule (NOT a `VOCAB-*` rule) since it lints the config file, not `.lzi` source.

---

## Diagnostic shape

Every `VOCAB-*` diagnostic renders with the same structure as existing doctor codes (`crypto-tier`, `security-opt-out` etc.). Verbatim example for `VOCAB-UNION-001`:

```
features/erudito/learning_node.lzi:42:5: warning [VOCAB-UNION-001]: enum-discriminated optional fields
  --> resource LearningNode declares `node_type: LearningNodeType` (enum {deepening, bridge})
      plus `bridge_justification: Text` and `bridge_target_topic: Topic`, both optional and
      conditional on `node_type = "bridge"`. The discriminated-union vocabulary expresses this
      with stronger guarantees (required fields per variant; type system rules out drift).
  --> suggestion:
        union LearningNode
          Deepening
            workspace: Workspace required
          Bridge
            workspace: Workspace required
            justification: Text required
            target_topic: Topic required
  --> false-positive guards:
        - Optional fields universally optional (no enum-tag correlation): rule does not fire.
        - Union form would be wider than enum form (variants × fields): rule does not fire.
        - See docs/proposals/doctor-vocabulary-lints.md §VOCAB-UNION-001 for the full carveout list.
  Severity in production-profile: error.
```

LLM agents pattern-matching on the diagnostic see: rule code, location, the named primitive (`union`), a verbatim refactor block, and explicit "does not fire" guards. The rendered form is the agent's training signal — see `docs/architecture.md` §"AI ergonomics" for why diagnostics double as agent docs.

All `VOCAB-*` rules follow this shape: trigger description, suggestion block, false-positive guards, severity-in-production. Diagnostic emitter lives in the shared doctor formatter — no per-rule rendering code.

---

## Acceptance

A new VOCAB-* rule lands when:

1. The rule's destination vocabulary primitive **already exists** in the language (don't enforce absence).
2. A single-file detector module is added under `crates/lazuli_doctor/src/vocab/` with the exact name `vocab_<lowercased>_<NNN>.rs`.
3. The detector has positive + ≥2 negative test cases (false-positive guards).
4. `crates/lazuli_doctor/src/vocab/mod.rs` registers the rule in the catalog.
5. `cargo check --all-targets` green; `cargo test -p lazuli_doctor --lib vocab` green.
6. `docs/proposals/doctor-vocabulary-lints.md` (this file) appends the rule to the active subset table.
7. The three v2 dogfood products (`pleiades`, `atelier`, `erudito`) pass `lazuli check` strict-profile under the new rule, OR Lucas explicitly accepts the warnings as in-flight refactor (tracked as cells).

A rule is **rejected** when:

- It fires on legitimate primitive uses with no replacement vocabulary.
- It requires runtime/execution-trace data (only IR-walkable patterns ship; no behaviour analysis).
- It introduces severity escalation (production = error) on a pattern that's still genuinely up-for-debate in the design.

---

## Verification

For each candidate rule, the L2 cell author runs `lazuli check` against the v2 triple-dogfood plus the framework's own fixture suite:

- `examples/full-capsule/`
- `examples/auth-roundtrip/`
- `examples/auth-multi-tenant/`
- `examples/marketplace-mini/`
- `examples/binary-smoke/`
- `examples/lazurite-multifrontend/`
- `examples/smoke-hello/`

A new rule must NOT change verdict (PASS → BLOCK) on any pre-existing fixture without an accompanying fixture refactor PR. Either the fixture is updated to use the recommended vocabulary in the same wave, OR the fixture is exempted with a comment explaining why the primitive form is the right call there (escape hatch, paired with a TODO link).

---

## Out of scope

- **Auto-fix.** Each rule emits a suggestion; the human/agent applies it. Schema-changing fixes never run unattended.
- **Behaviour/trace analysis.** Rules walk the IR only. No "this handler does X, so vocabulary Y applies" — that would require interpreting handler Go/TS code, which the doctor explicitly does not.
- **Cross-project rules.** Each rule runs against a single Lazuli package. Cross-package vocabulary fitness (e.g. "your `slug` resource looks like another project's `key`") is a separate, much harder problem; not in v0.1.
- **Rule severity machine-learning.** No "agent learns from your accepted/dismissed warnings". Severity is declared, not inferred.
- **AI auto-rewrite.** Tempting but exactly the wrong direction; the rule is meant to surface drift TO the agent so it can refactor *deliberately*, not silently.

---

## Risks / blockers

1. **False positives erode trust.** A rule that fires on 30% of legitimate uses becomes noise; humans/agents start ignoring all `VOCAB-*` warnings. Mitigation: each rule ships with ≥2 negative test cases AND a real-world false-positive case from the v2 dogfood (or proven not to apply). If a rule can't pass the "ran on Pleiades + Atelier + Erudito without a false positive that the project genuinely accepts" test, it doesn't ship in v0.1.

2. **Severity overrides leak design debates into product configs.** If every project starts disabling `VOCAB-UNION-001`, the language is wrong. Mitigation: `Lazurite.toml` `[doctor.vocab]` overrides require a comment (parser warns when override has no neighbouring comment line); periodic audit of override frequency across known products.

3. **Vocabulary-rule sprawl.** With every new vocabulary primitive shipping a companion lint, the catalog grows monotonically. Mitigation: each rule must justify itself in the proposal that introduces the vocabulary; rules retire when their vocabulary retires. The catalog isn't open-ended like ESLint's; it's bounded by the count of primitives.

4. **AI agents over-correct.** An agent reading "you should use union" might rewrite a primitive case that was legitimately primitive. Mitigation: diagnostics include the false-positive cases explicitly ("this rule does NOT apply when X / Y / Z"). The suggestion is informative, not imperative.

5. **Performance.** Doctor walks today are fast (~ms for typical projects). Each new rule adds a constant. Mitigation: budget — if the entire `VOCAB-*` catalog adds >50ms on a typical fixture, parallelise rule dispatch (rayon iter, already used elsewhere in the cli).

---

## Companion docs to update

When this proposal lands:

- `docs/design-principles.md` — append §"Doctor enforces Rule Zero" with a one-paragraph pointer to the `VOCAB-*` catalog.
- `docs/invariants.md` — add an entry under "Doctor invariants" noting that the vocabulary-fitness category exists.
- `docs/agents/architect.md` (if it exists yet) — note that grades may cite `VOCAB-*` rules when reviewing `.lzi` proposals.
- `crates/lazuli_cli/src/doctor.rs` (or successor) — module-level docstring listing the rule categories (correctness, security, vocabulary, …).
- `CLAUDE.md` — mention under "Doctor green" that vocabulary lints are warnings in strict, errors in production.

---

## Future expansion

The catalog grows when:

1. A new vocabulary primitive lands. The proposal that introduces the primitive MUST include the companion `VOCAB-*` rule, OR explicitly justify why no rule is appropriate (the primitive is purely additive and never replaces an existing pattern).

2. The triple-dogfood pressure-test reveals a primitive that gets re-implemented in handlers consistently across products. That's the "name what's repeating" signal Rule Zero names.

3. Cell L0 grades on future proposals catch drift patterns the existing rules miss. The grading rubric's `Doctor coverage` dimension already asks "can `lazuli doctor` detect a misconfigured or partial use of this primitive?" — that signal upgrades to "and is there a corresponding `VOCAB-*` rule for the lazy alternative?".

The catalog is a **closed catalog over the vocabulary** — bounded by primitive count, not by user creativity. It is the ESLint analogy inverted: ESLint enforces unwritten JS style across an open syntax surface; Lazuli's `VOCAB-*` rules enforce the use of named primitives across a closed syntax surface.

---

## Grade gates

This proposal goes through the standard grade-then-fix loop with the `lazuli-language-architect` agent before commit. Gate: ≥ 8.5/10 with all dimensions ≥ 7. Target ≥ 9.0/10.

Per the AI-first rubric, the dimensions most at risk for this proposal:

- **Vocabulary Over Mechanism (D1)** — meta-application of the principle. Score requires that the lint catalog itself doesn't open a predicate engine / mechanism in disguise.
- **Surface area (D6)** — adding a rule category is surface growth. Score requires that the category is well-scoped (no overlap with existing correctness/security categories).
- **Doctor coverage (D9)** — auto-relevant; the lints' detection heuristics are the surface being graded.
- **Escape-hatch hygiene (D5)** — `[doctor.vocab]` overrides are the escape hatch. Score requires that overrides require justification (comment hint, audit).

A BLOCK at < 8.5 or any dimension < 7 returns the proposal to the design author with annotated blockers. Re-grade as v0.2.
