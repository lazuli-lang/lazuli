# Test Completeness Lints (L0)

**Status**: L0 v0.1 design proposal. No DSL surface change — extends the existing `lazuli doctor` rule catalog with a new **category** of lints (`TEST-*`) that detect when authored `tests` blocks drift from the canonical authored-vs-generated split named in [docs/invariants.md §Tests](../invariants.md).

**Date**: 2026-05-16.

**Author**: Claude Opus 4.7 (orchestrator).

**Pilot bucket**: doctor / cli cross-cutting. Not a new bucket; widens an existing one. Sibling to [doctor-vocabulary-lints.md](doctor-vocabulary-lints.md) — same shape, different concern (vocabulary fitness vs test-vocabulary discipline).

**Companion**:
- [docs/invariants.md §Tests (lines 530-548)](../invariants.md) — names the authored-vs-generated split this proposal operationalises. **The proposal exists because that section already separated `allows`/`denies` (authored, predicate-only) from `permits`/`forbids` (generated from `policy` matrix), but the doctor does not yet enforce the split.**
- [docs/proposals/doctor-vocabulary-lints.md](doctor-vocabulary-lints.md) — direct stylistic sibling. Severity model, override knob, and module shape mirror this catalog 1:1.
- [docs/design-principles.md §"Rule Zero: Vocabulary Over Mechanism"](../design-principles.md) — three of the five rules are Rule Zero applied to the test surface ("don't ad-lib the test predicate; the construct already declared it").
- [crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs](../../crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs) — existing **feature-level coarse** test-coverage lint. The catalog below is **per-construct fine-grained**; the two are orthogonal (see §"Relationship to VOCAB-TESTS-MISSING-001").

**First consumer**: the active pilot capsule (private downstream product). Every authored `tests` block today is expected to trigger at least one rule below; the diagnostic-driven refactor lands before the Phase 2-4 generators (see §"Roadmap pointer") so authored-shadow-of-generated tests don't accumulate as the surface ages.

---

## Problem

Lazuli already names the authored-vs-generated split for tests. From [docs/invariants.md:530-548](../invariants.md):

> - `tests` blocks are optional and inline.
> - Tests are allowed only on commands, workflow transitions, rules, and extensible views.
> - **Tests cover decisions with inference: rule predicates, transition validity, and anchor allowlists. They do not restate effects or emitted events.**
> - **Command actor-matrix tests are generated from the effective `policy @policy.*`; authored command tests should cover only predicate behavior beyond policy.**
> - **Generated command actor-matrix tests use `permits` and `forbids`; authored predicate and transition tests use `allows` and `denies`.**
> - Keep tests vocabulary scoped by construct. Rule predicates, workflow edges, workflow actor checks, and anchor allowlists are different semantic decisions; do not flatten them into one generic `allow/deny` dialect unless it preserves the same static rejection power and readability.
> - Command tests use `target` when a loaded target exists. Rule and workflow tests use `self`.
> - Tests use the same closed predicate language as rules and filters; no fixtures, mocks, or `given/when/then` framing.

**Observed drift across the pilot capsule** (2026-05-16 audit, the trigger for this proposal):

| Drift | What was authored | What the invariant says |
|---|---|---|
| Transition with `denies as @role.traveler` hand-written | Manual actor coverage | Actor matrix is **generated** from `policy @policy.*` — `permits`/`forbids` only. Hand-written `allows as @role.X` / `denies as @role.X` competes with the generator. |
| Transition test `denies when target.org != ctx.actor.org` | Hand-written tenant boundary | Tenancy boundary derives from `tenancy <axis>` + `scope <…>` — **generated** matrix territory, not authored. |
| Transition test `allows when self.cpf = "12345678901" AND self.phone = "+5511..."` | Data-shaped fixture inside `when` predicate | The closed predicate language admits literals, but a multi-digit literal that looks like real-world data is a fixture in disguise — Jest creep. Boundary is `!= nil` / enum-eq / catalog predicate, not literal data. |
| Transition test `allows when self.published_at != nil` after the transition sets `timestamps published_at` | Restating the construct's effect | The construct's own `updates` / `timestamps` / `emits` declaration is the contract; `tests` is for inference, not for re-asserting the effect (invariants.md:534). |
| Transition with 1 `allows from <source>` and 3 `denies from <other>` | Re-stating the transition's own `from` declaration | Source-state coverage is **generated** from the transition's own header — `from <state>` is already a contract. Authored source-state-only tests are pure shadow. |

Every drift is grammatical today. The doctor does not flag any of them, and the proposal-author's first instinct ("we need richer authored tests across N dimensions") feeds the drift further. Without doctor feedback the drift compounds in three concrete ways:

- **Test surface grows monotonically.** Each new transition prompts copy-paste of the same 4-line `allows from / denies from` stanza, none of which infer anything beyond the transition header.
- **Authored-shadow-of-generated tests block the generators.** Once Phases 2-4 (see §"Roadmap pointer") ship the actor-matrix / tenancy-matrix / transition-matrix generators, every authored `denies as @role.X` becomes a duplicate that the codegen must either skip (drift) or fail on (churn). Land the lint first, refactor the surface, then ship the generator.
- **The predicate language erodes.** Each fixture-shaped literal in a test predicate is a step toward `given/when/then` framing — exactly what invariants.md:548 forbids.

**Cost compounds the same way the vocab-lints proposal cost compounds**: every product port repeats the rewrite cycle, every fresh agent session re-introduces the same drift, and the doctor's silence reads as endorsement.

---

## Relationship to `VOCAB-TESTS-MISSING-001`

The existing [vocab_tests_missing_001](../../crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs) lint fires **per feature, once**, when a feature declares ≥1 resource or command and zero inline `tests` blocks anywhere. It is a **feature-level coarse-grained** check answering "did the author write any tests at all?".

This proposal's `TEST-*` catalog is **per-construct fine-grained** answering "is each authored `tests` block shaped right?". The two are orthogonal:

| Question | Lint | Granularity |
|---|---|---|
| Did the feature author any tests? | `VOCAB-TESTS-MISSING-001` (exists) | Feature |
| Does each construct with a non-trivial predicate have a `tests` block? | `TEST-MISSING-AUTHORED-001` (proposed) | Construct (transition / rule / view) |
| Are the predicates covered by ≥1 `allows` + ≥1 `denies`? | `TEST-PREDICATE-UNCOVERED-001` (proposed) | Construct |
| Does the `tests` block restate the construct's effect? | `TEST-RESTATES-EFFECT-001` (proposed) | Construct |
| Does the `tests` block compete with `permits`/`forbids` generation? | `TEST-RESTATES-POLICY-001` (proposed) | Construct |
| Do test predicates contain fixture-shaped literals? | `TEST-FIXTURE-LITERAL-001` (proposed) | Test assertion |

`VOCAB-TESTS-MISSING-001` keeps its place; `TEST-MISSING-AUTHORED-001` is finer-grained but covers a different signal. A feature can pass the coarse lint with a single trivial `tests` block on one construct and still fail the catalog below on every other construct.

The catalog is also placed under a **new doctor category** (`RuleCategory::TestDiscipline`) rather than `Vocabulary` — these rules enforce **how the test vocabulary is used**, not **which primitive replaces a mechanism**. The vocab-lints proposal explicitly named `RuleCategory::Vocabulary` as scoped to "use the named primitive over the unnamed mechanism"; `TestDiscipline` is the sibling for "use the test vocabulary as the invariants section says, no improvisation".

---

## Proposal — `TEST-*` rule catalog

Add a **new severity-class** to doctor: **test-discipline**. Mechanically the same as the existing rule shape (rule fn walks the IR, emits a `Diagnostic` with code + span + suggestion); conceptually distinct from **correctness lints** (e.g. `HOOK-TARGET-001`), **vocabulary-fitness lints** (`VOCAB-*`), and **security lints** (`crypto-tier`).

### Rule code convention

`TEST-<CATEGORY>-NNN` where:

- `<CATEGORY>` names the discipline: `MISSING-AUTHORED`, `PREDICATE-UNCOVERED`, `RESTATES-EFFECT`, `RESTATES-POLICY`, `FIXTURE-LITERAL`, …
- `NNN` is the per-category index (`001` for the canonical case; `002+` for variants discovered downstream).

### Severity model

Per the user's calibrated note on first-draft severities, plus the doctor's three-profile model (`prototype` / `strict` / `production`):

| Rule | strict | production | Rationale |
|---|---|---|---|
| `TEST-MISSING-AUTHORED-001` | `warning` if construct declares ≥1 `requires` non-policy predicate; `info` otherwise | same | Predicate without coverage is a real gap; predicate-less construct is generation territory and not the lint's job. |
| `TEST-PREDICATE-UNCOVERED-001` | `info` | `info` | Predicate-boundary coverage is genuinely judgement-laden; over-firing risk is high. Stay informational until calibrated against ≥2 product capsules. |
| `TEST-RESTATES-EFFECT-001` | `warning` | `warning` | The construct's effect is already the contract; restating it is noise, not a correctness bug. Authors who insist should opt out with a comment. |
| `TEST-RESTATES-POLICY-001` | `warning` | `warning` | Authored actor coverage shadows the generated matrix. Warning today; promoted to `error` in the wave that ships the Phase 2 generator (see §"Retirement path"). |
| `TEST-FIXTURE-LITERAL-001` | `error` | `error` | This is the vector of Jest creep. Every authored fixture literal is a step toward `given/when/then`. Harden the boundary on day one — the alternative is the closed-predicate-language invariant erodes monotonically. |

The severity-override toml shape mirrors `[doctor.vocab]` from the sibling proposal:

```toml
[doctor.test_discipline]
# Brand-new transitions in the publishing feature carry obvious test
# placeholders during the in-flight refactor (2026-06).
"TEST-MISSING-AUTHORED-001" = "info"
```

Comment-required override is enforced by the existing `DOCTOR-OVERRIDE-NEEDS-REASON-001` rule.

### Auto-fix policy

**No auto-fix.** Each `TEST-*` rule changes the shape of authored assertions; auto-fix would either delete authored intent (dangerous) or generate placeholder predicates (worse). Every rule emits **a suggestion** in the diagnostic; the author refactors manually.

The TEST-RESTATES-* rules in particular benefit from human review because the "shadow of generation" judgement requires the author to confirm that the shadow IS the generator's territory (and not a legitimate carve-out the generator can't yet express).

---

## Starter catalog (5 rules — all v0.1)

### `TEST-MISSING-AUTHORED-001` — construct declares non-policy predicate without `tests`

**Detection heuristic** (IR walk, no source re-parse):

A construct that admits a `tests` block (transition / rule / view; see invariants.md:531) declares at least one **non-policy predicate gate** AND has no `tests` block. "Non-policy predicate gate" means:

1. A `requires <expr>` clause whose expression is NOT a single `@policy.<name>` reference (so plain `requires @policy.X` does NOT trigger — that's pure policy, covered by the actor matrix).
2. OR a rule's `deny <expr>` / `allow <expr>` whose expression is not a single `@policy.<name>` reference.
3. OR a lifecycle transition declaring `requires self.<field> ...` of any shape.

Bare grammar match: walk `feature.commands[*].requires`, `feature.rules[*].rule_body`, `feature.workflows[*].transitions[*].requires`, `feature.resources[*].lifecycle.transitions[*].requires`, plus the same for `extends @anchor.*` view bodies once they admit `requires`.

**Example trigger:**

```lzi
transition fill_basic_details
  from basic_details_pending
  to address_pending
  requires self.cpf != nil OR self.cnpj != nil
  requires self.phone != nil
  policy @policy.host_only
  audit default
  # ← no tests block
```

**Suggested refactor (in the diagnostic):**

```lzi
transition fill_basic_details
  from basic_details_pending
  to address_pending
  requires self.cpf != nil OR self.cnpj != nil
  requires self.phone != nil
  policy @policy.host_only
  audit default

  tests
    # cover the predicate boundary
    allows when self.cpf != nil AND self.phone != nil
    allows when self.cnpj != nil AND self.phone != nil
    denies when self.cpf = nil AND self.cnpj = nil
    denies when self.phone = nil
```

**False-positive cases (rule MUST not fire):**

- Construct declares only `policy @policy.X` — actor matrix is generated; no authored coverage needed.
- Construct declares only `from <state>` source — covered by transition-matrix generation (Phase 4).
- Rule declares only `deny @policy.X` — degenerate predicate; pure policy.
- View `extends @anchor.X` with no predicate gating — anchor allowlist matrix is generated.

**Severity**: `warning` (strict) / `warning` (production) when ≥1 non-policy predicate present; `info` (both) otherwise.

**Retirement path**: stays. Even after Phase 2-4 generators ship, this rule keeps catching authored predicate coverage gaps that generation cannot infer.

---

### `TEST-PREDICATE-UNCOVERED-001` — predicate boundary lacks both-sides coverage

**Detection heuristic** (IR walk + predicate AST decomposition):

For each `requires <expr>` / `deny <expr>` / `allow <expr>` on a construct with a `tests` block, decompose the predicate AST into a minimal Boolean expression over **atoms** (each leaf comparison or namespace-ref-truthiness counts as one atom). Each atom needs both `true` and `false` coverage across the construct's `allows`/`denies` assertions:

- `allows when` predicates must collectively map all atoms to `true` at least once each.
- `denies when` predicates must collectively map at least one atom to `false` each.

The "all atoms ≥1 true, ≥1 false" rule is the minimal cover; full truth-table cover is not required (would over-fire on N-ary predicates).

**Example trigger:**

```lzi
transition mark_published
  from publishing
  to published
  requires self.error_reason = nil
  requires self.scheduled_at <= ctx.now

  tests
    allows when self.error_reason = nil AND self.scheduled_at <= ctx.now
    # ← missing: `denies when self.error_reason != nil`
    # ← missing: `denies when self.scheduled_at > ctx.now`
```

**Suggested refactor:**

```lzi
  tests
    allows when self.error_reason = nil AND self.scheduled_at <= ctx.now
    denies when self.error_reason != nil
    denies when self.scheduled_at > ctx.now
```

**False-positive cases:**

- Predicate is a single atom and a `denies when NOT <atom>` is present — that's both-sides covered.
- Predicate has a `requires @validator.<name>` — validator atoms are not decomposed (the validator itself is an opaque boundary).
- Predicate references a namespace ref (`@actor.<name>` truthiness) — atom-level coverage cannot be authored declaratively; the generator handles it (Phase 2).

**Severity**: `info` (strict) / `info` (production). Held at info because predicate-decomposition false-positive risk is real — only escalate after calibration on ≥2 pilots.

**Retirement path**: stays — generation does not author predicate-side coverage.

---

### `TEST-RESTATES-EFFECT-001` — `tests` predicate names a field the construct writes

**Detection heuristic** (IR walk + cross-block field-name compare):

Collect the set of resource fields the construct WRITES in its same-block effect:

- For a command: the LHS field names in `creates <Resource> { … }` / `updates <Resource> { … }`.
- For a transition: the discriminator-field name + every name in `timestamps <field>` + every field name in `emits <event> payload <field>, <field>`.

Walk the same construct's `tests` block. For each `allows when` predicate, walk the predicate AST and flag every leaf comparison of the form `<rooted-ref>.<field> = <literal>` where `<field>` is in the WRITES set AND `<literal>` matches the construct's set value (literal-eq, enum-eq, `nil`).

**Example trigger:**

```lzi
transition mark_published
  from publishing
  to published
  timestamps published_at      # ← lifecycle stamps published_at = ctx.now
  emits publication_published

  tests
    allows when self.published_at != nil   # ← restates the effect; transition guarantees this
```

**Suggested refactor:**

```lzi
  tests
    allows when self.error_reason = nil    # ← assert inference, not restate the effect
    denies when self.error_reason != nil
```

**False-positive cases:**

- The test asserts the field is `nil` on the PRE-image (denies-shape) — that's a legit precondition check.
- The test's literal does NOT match the construct's set value — different value = legit boundary check.
- The construct's effect is itself conditional (e.g., `updates Foo when @validator.X` — not yet IR-shaped in v0; reserved for later refinement).
- Lifecycle invariants like `terminal_immutable` add IR-level guarantees that the test re-states — those should clear under invariant declarations, not authored tests.

**Severity**: `warning` (strict) / `warning` (production). The test is not wrong; it is noise. Warning gives the author the chance to delete; no production block.

**Retirement path**: stays — Phase 2-4 generators do not surface this drift; this rule is the only guard.

---

### `TEST-RESTATES-POLICY-001` — authored `as @role.X` competes with generated matrix

**Detection heuristic** (IR walk + policy resolution):

Walk each construct's `tests` block. For each assertion with an `as @role.<X>` or `as @actor.<X>` clause:

1. Resolve the construct's `policy` clause (`@policy.<name>`) against the feature's `policies` dictionary.
2. Collect the union of atoms named in the resolved policy entry (`@role.*`, `@scope.*`, `@actor.*`).
3. If the authored `as @role.<X>` is IN the resolved atom set (allow-shaped test) OR explicitly NOT IN it (deny-shaped test), AND the test assertion has no additional `when <expr>` predicate, fire.

The "no additional `when`" guard is critical — the actor-matrix generation only emits actor-only assertions. Once an author attaches a predicate, the test is a legit predicate-gated carve-out and the generator does not own it.

**Example trigger:**

```lzi
# feature has:  policies { update: @role.admin, @role.sales }

transition archive
  from active
  to archived
  policy @policy.update     # ← resolves to {@role.admin, @role.sales}

  tests
    denies as @role.viewer       # ← shadow: @role.viewer is NOT in @policy.update, generator emits this
    allows as @role.admin        # ← shadow: @role.admin IS in @policy.update, generator emits this
```

**Suggested refactor:**

```lzi
  tests
    # delete the shadow assertions — generator emits permits/forbids per the resolved policy
    denies from paused           # legit: predicate-only, source-state coverage stays authored until Phase 4
    allows from active as @role.admin when self.tier = enterprise
    # ↑ legit: actor + predicate combined; generator can't emit this
```

**False-positive cases:**

- Test assertion has a `when <expr>` clause — predicate-gated actor coverage is legit (combined dimension).
- Test asserts an actor NOT named in the resolved policy entry but the assertion is `allows` and the construct has a separate path to admit it (e.g., `requires @policy.<override>` raises the bar) — narrow case; lint stays conservative and respects the construct's own override.
- The construct uses `requires @policy.<X>` (transition override) — resolve THAT entry, not the construct's default `policy` clause.
- The feature has no `policies` dictionary (legacy minimal shape) — defer (Phase 0 of the existing actor-matrix generator's prerequisites).

**Severity**: `warning` (strict) / `warning` (production) **today**; promoted to `error` (strict) / `error` (production) in the wave that ships the Phase 2 actor-matrix generator.

**Retirement path**: stays in catalog forever, but the severity escalates with the generator. The rule's *job* shifts from "warn about pre-existing shadow" to "block re-introduction of shadow now that the generator emits the matrix".

---

### `TEST-FIXTURE-LITERAL-001` — predicate value is a fixture-shaped literal

**Detection heuristic** (IR walk + literal classification):

Walk each test assertion's `when <expr>` predicate AST. For every leaf comparison of the form `<ref> <op> <value>`, classify `<value>`:

| Literal class | Verdict |
|---|---|
| `nil`, `true`, `false` | clean |
| `""` (empty string) | clean |
| Integer `-1`, `0`, `1` | clean |
| Decimal `0.0`, `0.5`, `1.0` | clean |
| Enum value (`IDENT_LOWER` that resolves to a variant of the referenced field's enum type) | clean |
| Namespace ref (`@role.X`, `@semantic.Y`, etc.) | clean |
| Ref (`self.X`, `target.X`, `input.X`, `ctx.X`, …) | clean |
| Duration / Size literal | clean |
| `STRING` with > 0 chars AND not an enum variant of the LHS field's type | **fires** |
| `INTEGER` outside `{-1, 0, 1}` AND not a declared bound (`min N`, `max N`, `between A and B`) | **fires** |
| `DECIMAL` outside `{0.0, 0.5, 1.0}` AND not a declared bound | **fires** |

The "declared bound" carve-out scans the LHS field's `min` / `max` / `between` / `length` / `in` constraints (see invariants.md:495-528) and admits any literal that matches a declared bound — that's the boundary the field's contract names, not a fixture.

**Example trigger:**

```lzi
transition fill_basic_details
  from basic_details_pending
  to address_pending
  requires self.cpf != nil OR self.cnpj != nil

  tests
    allows when self.cpf = "12345678901"       # ← fixture: arbitrary 11-digit string
    allows when self.email = "ada@example.com" # ← fixture: arbitrary email
    denies when self.tier = "free"             # ← clean IF `free` is an enum variant of CustomerTier
```

**Suggested refactor:**

```lzi
  tests
    allows when self.cpf != nil
    denies when self.cpf = nil AND self.cnpj = nil
    # if a specific value matters, name it via the field's constraints or as an @validator
```

**False-positive cases:**

- LHS field's type has a closed `in [...]` constraint and the literal matches a member — that's the field's catalog, not a fixture.
- LHS field has `pattern STRING` and the test asserts pattern boundary — defer to a future `TEST-PATTERN-BOUNDARY-002` lint (out of v0.1 scope).
- The literal IS an enum variant (resolution succeeds) — clean.

**Severity**: `error` (strict) / `error` (production). This is the Jest-creep vector and must harden on day one; every fixture literal that ships becomes precedent. Lower severity gets ignored.

**Retirement path**: stays forever. Generation does not surface this drift; this is principle-level enforcement of the closed-predicate-language invariant.

---

## Active subset for v0.1

All five rules ship together. Unlike `doctor-vocabulary-lints.md` v0.1 (which split into "destination vocabulary exists?" yes/no buckets), every rule below names existing test vocabulary — there is no deferred set.

| Rule | Status | Ships in v0.1? | Cell |
|---|---|---|---|
| `TEST-MISSING-AUTHORED-001` | active (predicate vocabulary exists) | yes | T1 |
| `TEST-PREDICATE-UNCOVERED-001` | active (closed predicate language exists; AST decomposition is straightforward) | yes | T2 |
| `TEST-RESTATES-EFFECT-001` | active (effects + tests both IR-visible) | yes | T3 |
| `TEST-RESTATES-POLICY-001` | active (policies dictionary + actor refs IR-visible) | yes | T4 |
| `TEST-FIXTURE-LITERAL-001` | active (predicate value classification is IR-walkable) | yes | T5 |

**v0.1 ships 5 rules.** Cells T1-T5 are single-file detectors, each ~120-180 LOC including inline `#[cfg(test)] mod tests`. Codex-able in parallel; orchestrator (Claude) wires `mod.rs` in a single follow-up edit (the only shared-file write, per the [`feedback_claude_plans_codex_executes`](../../../.claude/projects/c--Users-lucas-lazuli/memory/feedback_claude_plans_codex_executes.md) pattern).

---

## Roadmap pointer

These lints exist **because the generators don't yet ship**. As Phases 2-4 from the parent test-roadmap discussion (see commit message of this proposal) land, the catalog evolves:

| Phase | Generator | Effect on this catalog |
|---|---|---|
| **Phase 2** — Actor-matrix `permits`/`forbids` generation from `policy @policy.*` | codegen-Go / codegen-TS emit per-target actor matrix tests | `TEST-RESTATES-POLICY-001` severity promotes to `error` strict + production. Authored `as @role.X` without `when` becomes a hard block, not a warning. |
| **Phase 3** — Tenancy-boundary `permits`/`forbids` generation from `tenancy <axis>` + `scope` | codegen emits cross-tenant assertions | New lint candidate: `TEST-RESTATES-TENANCY-001` (mirror of POLICY-001 for tenancy axis). Not in v0.1 — proposed as v0.2. |
| **Phase 4** — Transition-matrix `permits from <every-other-state>` generation from lifecycle/workflow | codegen emits source-state matrix | New lint candidate: `TEST-RESTATES-SOURCE-STATE-001` (deletes ~75% of currently-authored transition tests). Not in v0.1 — proposed as v0.2. |

The five v0.1 rules are designed to **survive** the generators arriving. None of them prescribe authoring the generator's territory; they all guard the authoring boundary in a direction generation cannot infer (predicate coverage, predicate shape, fixture vector). The catalog grows monotonically by adding TENANCY / SOURCE-STATE shadows in v0.2 once those generators exist.

---

## Implementation status

The `RuleCategory::TestDiscipline` enum variant is **new**. Existing categories (`Correctness`, `Vocabulary`, plus the security family) stay unchanged. `lazuli inspect --expand=doctor` lists rules grouped by category — `TestDiscipline` becomes a fourth heading.

Full dispatch into `DoctorPackage::diagnostics()` follows the same gap pattern the vocab catalog has today: rule modules register in `mod.rs` and exercise their logic via inline `#[cfg(test)] mod tests`, but the IR-loading adapter that turns `Finding` into `DoctorDiagnostic` lands in a **separate follow-up cell** (a single ~200-LOC edit reusing the `Tier3FeatureFacts` walk used by `VOCAB-*`). The v0.1 cells T1-T5 ship the detection logic; the follow-up cell wires them into the CLI surface. This mirrors the v0.1 status of `doctor-vocabulary-lints.md` (4 v0.1 rules registered but not yet surfaced in `lazuli check` output).

---

## Implementation shape

### File layout

One file per rule under `crates/lazuli_doctor/src/test_discipline/`:

```
crates/lazuli_doctor/src/test_discipline/
  mod.rs                              # registry of all TEST-* rules
  test_missing_authored_001.rs        # T1
  test_predicate_uncovered_001.rs     # T2
  test_restates_effect_001.rs         # T3
  test_restates_policy_001.rs         # T4
  test_fixture_literal_001.rs         # T5
```

`mod.rs` shape (registry):

```rust
//! TEST-* rules — Test Discipline (authored vs generated split).
//! See docs/proposals/test-completeness-lints.md for the design.

pub mod test_fixture_literal_001;
pub mod test_missing_authored_001;
pub mod test_predicate_uncovered_001;
pub mod test_restates_effect_001;
pub mod test_restates_policy_001;

pub fn all_rules() -> Vec<Box<dyn DoctorRule>> {
    vec![
        Box::new(test_missing_authored_001::TestMissingAuthored001),
        Box::new(test_predicate_uncovered_001::TestPredicateUncovered001),
        Box::new(test_restates_effect_001::TestRestatesEffect001),
        Box::new(test_restates_policy_001::TestRestatesPolicy001),
        Box::new(test_fixture_literal_001::TestFixtureLiteral001),
    ]
}
```

The `lazuli_doctor/src/lib.rs` registers the new module alongside `vocab` / `correctness` / `cross_feature` / `design` / `domain` / `encryption` / `lifecycle` / `poller` / `report`. Single-line additive edit.

Each rule file exports:

```rust
pub struct TestRestatesEffect001;

impl DoctorRule for TestRestatesEffect001 {
    fn code(&self) -> &'static str { "TEST-RESTATES-EFFECT-001" }
    fn category(&self) -> RuleCategory { RuleCategory::TestDiscipline }
    fn check(&self, feature: &Feature, ctx: &DoctorCtx) -> Vec<Diagnostic> { ... }
}

#[cfg(test)]
mod tests {
    // positive: detects on transition that restates `timestamps`-set field
    // negative: pre-image precondition check on same field clears
    // negative: differing literal clears
}
```

### Configuration parsing

`Lazurite.toml` `[doctor.test_discipline]` table is parsed in the same site (`crates/lazuli_cli/src/lazurite_manifest.rs`) as `[doctor.vocab]`. Each entry maps a rule code to `off | info | warning | error`. The parser rejects unknown rule codes.

Comment-required override is enforced by the pre-existing `DOCTOR-OVERRIDE-NEEDS-REASON-001` rule — no new wiring needed.

### Predicate AST helpers

`TEST-PREDICATE-UNCOVERED-001` and `TEST-FIXTURE-LITERAL-001` both decompose predicate ASTs. The closed predicate language (invariants.md §20) is parsed into a typed AST today (`PredicateNode` / `ComparisonNode` in `lazuli_ir`). Helper functions live in `crates/lazuli_doctor/src/test_discipline/predicate_walk.rs` (one new shared helper file) — atom decomposition, literal classification, ref-root extraction. Both rules import the helpers.

---

## Diagnostic shape

Every `TEST-*` diagnostic renders with the existing doctor diagnostic structure (`crypto-tier`, `VOCAB-*`, etc.). Verbatim example for `TEST-RESTATES-POLICY-001`:

```
features/host/host.lzi:142:7: warning [TEST-RESTATES-POLICY-001]: actor-only test shadows generated matrix
  --> transition `archive` declares `policy @policy.update` which resolves
      to {@role.admin, @role.sales}. The authored assertion
      `denies as @role.viewer` adds no predicate beyond actor and will be
      emitted by the actor-matrix generator (Phase 2). Authored tests should
      cover predicate behavior beyond policy.
  --> suggestion:
        # delete the shadow assertion — generator emits forbids per the resolved policy
        # keep only assertions that combine actor with predicate:
        allows from active as @role.admin when self.tier = enterprise
  --> false-positive guards:
        - Assertion has a `when <expr>` clause: rule does not fire (predicate-gated actor coverage is legit).
        - Construct uses `requires @policy.<override>`: rule resolves THAT entry, not the default `policy` clause.
        - See docs/proposals/test-completeness-lints.md §TEST-RESTATES-POLICY-001 for the full carveout list.
  Severity in production-profile: warning (promotes to error in the wave that ships the actor-matrix generator).
```

LLM agents pattern-matching on the diagnostic see: rule code, location, the named primitive (`policy` / generation), a verbatim refactor suggestion, explicit "does not fire" guards, **and an explicit retirement-path notice** so the author understands the severity contract. The "promotes to error" sentence is load-bearing — it tells future agents that the warning is in-flight, not permanent.

All `TEST-*` rules follow this shape: trigger description, suggestion block, false-positive guards, severity-in-production, retirement path. The diagnostic emitter lives in the shared doctor formatter — no per-rule rendering code.

---

## Acceptance

A new TEST-* rule lands when:

1. The rule's behavior is named in `docs/invariants.md §Tests` (lines 530-548) or in a corollary invariant the rule operationalises (e.g., predicate-language closure for `FIXTURE-LITERAL`).
2. A single-file detector module is added under `crates/lazuli_doctor/src/test_discipline/` with the exact name `test_<lowercased>_<NNN>.rs`.
3. The detector has positive + ≥2 negative test cases (false-positive guards).
4. `crates/lazuli_doctor/src/test_discipline/mod.rs` registers the rule in the catalog.
5. `cargo check --all-targets` green; `cargo test -p lazuli_doctor --lib test_discipline` green.
6. **Hostpoint-corpus calibration**: the rule runs against the full active pilot capsule (private repo) AND every `examples/*` fixture in this repo. The author records:
   - **Fire count** per rule per capsule.
   - **Confirmed-true-positive rate** (drift the author agrees should be refactored).
   - **Confirmed-false-positive rate** (cases the author rejects as legitimate authoring; each gets either a doctor-override entry with reason, OR a new false-positive carve-out in the rule's heuristic).

   The rule ships only when **false-positive rate ≤ 10%** on the pilot corpus. The 10% threshold is empirical — calibrated against the vocab-lints proposal acceptance bar (paraphrased as "≥2 negative test cases AND a real-world false-positive case from the v2 dogfood"); for TEST-* rules whose detection is more judgement-laden, the explicit numeric ceiling forces honest counting.

7. `docs/proposals/test-completeness-lints.md` (this file) appends the rule to the active subset table.
8. The pilot capsule passes `lazuli check` strict-profile under the new rule, OR the author explicitly accepts the warnings as in-flight refactor (tracked as cells, mirroring the vocab-lints acceptance bar).

A rule is **rejected** when:

- It fires on legitimate test authoring with no replacement (e.g., predicate-shape that the closed language cannot express).
- It requires source-text inspection that the IR doesn't preserve (e.g., comment-based intent markers; no comment-aware lints in v0.1).
- Its false-positive rate against the pilot corpus exceeds 10%.

---

## Verification

For each candidate rule, the L2 cell author runs:

1. `cargo test -p lazuli_doctor --lib test_discipline::<rule_module>` — inline test pack.
2. `cargo test -p lazuli_doctor` — full doctor suite, no regression.
3. **Corpus calibration**: the rule's `check` function is invoked against every Feature IR from:
   - The active pilot capsule (private repo; orchestrator-side check).
   - `examples/full-capsule/`, `examples/auth-roundtrip/`, `examples/auth-multi-tenant/`, `examples/marketplace-mini/`, `examples/binary-smoke/`, `examples/lazurite-multifrontend/`, `examples/smoke-hello/`.
   - Each TEST-* rule's positive/negative `EXAMPLES/` skill fixtures (once authored — paralleling the audit-skill `EXAMPLES/` pattern at [skills/audit/EXAMPLES/](../../skills/audit/EXAMPLES/)).

4. A new rule must NOT change verdict (PASS → BLOCK) on any pre-existing fixture without an accompanying fixture refactor PR. Either the fixture is updated to use the right test shape in the same wave, OR the fixture is exempted with a comment explaining why the authored form is the right call there (escape hatch, paired with a TODO link).

---

## Out of scope

- **Auto-fix.** Each rule emits a suggestion; the human/agent refactors.
- **Test runners.** No `*.test.lzi` separate files. Tests stay inline on the construct that owns them (invariants.md:531). The proximity is what keeps the relation between intent and verification visible.
- **Fixture support.** No `let user = Host(...)` blocks, no factory shapes, no shared fixture pools. The predicate language stays closed (invariants.md:548).
- **Mocks / spies / call assertions.** Effects are declared; the runtime guarantees them; tests are for inference. Anything that looks like `expect(emits).toHaveBeenCalled(...)` is rejected at the grammar layer; the lint catalog does not need to relitigate.
- **Given/when/then framing.** Invariants.md:548 explicit ban. The closed predicate language is the only authoring surface.
- **Property-based testing.** Generators are code, not DSL. If a project needs property-based tests, they live in handler-side `*.go` / `*.ts` files alongside the `extensions` block — not in `.lzi`.
- **Setup / teardown / lifecycle blocks.** No `before` / `after` / `setup` in `tests`. Each assertion is a pure expression.
- **Behaviour/trace analysis.** Rules walk the IR only. No "this handler does X, so the test should assert Y" — that would require interpreting handler Go/TS, which the doctor explicitly does not.
- **AI auto-rewrite of failing tests.** Tempting; exactly wrong direction. The diagnostic is for the agent to refactor deliberately, not silently.

---

## Risks / blockers

1. **False positives erode trust on `TEST-PREDICATE-UNCOVERED-001`.** Predicate decomposition is judgement-laden (N-ary AND/OR trees, validator-call atoms, ref truthiness). A rule that flags every "missing coverage" overshoots quickly. **Mitigation**: ship at `info` severity, calibrate against ≥2 product capsules before any promotion, and reserve `warning` for the explicit case where ZERO `denies` cover a `requires` predicate (the unambiguous gap).

2. **`TEST-RESTATES-EFFECT-001` over-fires on legitimate pre-image checks.** Many transitions legitimately assert that the pre-image had a specific shape (e.g., `denies when self.deleted_at != nil`). **Mitigation**: rule fires only on `allows when` (allow-shape), only when literal-eq matches the construct's set value, and only when the field is in the WRITES set. `denies` shape is exempt.

3. **`TEST-FIXTURE-LITERAL-001` over-fires on declared bounds.** Inline `min N` / `max N` / `between A and B` / `length N` / `in [...]` constraints (invariants.md:495-528) name legitimate literal boundaries. **Mitigation**: the literal-classification helper scans the LHS field's constraint list and admits any literal matching a declared bound. Documented as the `# declared bound` carve-out.

4. **`TEST-RESTATES-POLICY-001` needs policies-dict resolution.** Resolution depends on the feature having a `policies` block (invariants.md:309-328). Legacy features without one cannot resolve `@policy.X` → atom set. **Mitigation**: rule short-circuits (does not fire) when the feature has no `policies` entry for the construct's policy ref. Lower-quality detection on legacy capsules; same trade-off as the actor-matrix generator will face.

5. **Severity escalation surprise.** Promoting `TEST-RESTATES-POLICY-001` from `warning` to `error` in the Phase 2 wave will fail builds for any project that has accumulated authored shadow. **Mitigation**: the Phase 2 proposal MUST include (a) a one-wave grace period at `warning` after the generator ships, (b) a `lazuli doctor --explain TEST-RESTATES-POLICY-001` companion that points at every offending construct with a refactor suggestion.

6. **Performance.** Doctor walks today are fast (~ms for typical projects). Each new rule adds a constant. **Mitigation**: budget — if the entire `TEST-*` catalog adds >20ms on a typical fixture, parallelise rule dispatch (rayon iter, already used elsewhere in the doctor).

7. **`RuleCategory::TestDiscipline` enum addition surface.** Every doctor consumer (LSP, inspect projection, CLI summary) must handle the new variant. **Mitigation**: the variant is additive; non-exhaustive matches stay non-exhaustive; LSP / inspect / CLI updates are mechanical follow-up cells, paired with this proposal's landing wave.

---

## Companion docs to update

When this proposal lands:

- [docs/invariants.md](../invariants.md) §"Tests" — append a one-line pointer: "Doctor enforces these via the `TEST-*` rule catalog (see `docs/proposals/test-completeness-lints.md`)."
- [docs/design-principles.md](../design-principles.md) §"Doctor enforces Rule Zero" — append: "The `TEST-*` rule catalog (test-completeness-lints) is the test-surface application of the same principle."
- [skills/audit/RULES.md](../../skills/audit/RULES.md) — append 5 entries (one per rule) mirroring the LLM-readable projection pattern already used for `VOCAB-*`.
- [skills/audit/EXAMPLES/](../../skills/audit/EXAMPLES/) — add one `.lzi` fixture per rule (`test-missing-authored-001.lzi`, etc.) showing positive trigger + canonical fix.
- [crates/lazuli_doctor/src/lib.rs](../../crates/lazuli_doctor/src/lib.rs) — module-level docstring lists the rule categories (correctness, security, vocabulary, test-discipline, …).
- [CLAUDE.md](../../CLAUDE.md) — mention under "Doctor green" that test-discipline lints fire at the severities declared in this proposal.

---

## Future expansion

The catalog grows when:

1. **A generator ships** (Phase 2-4). Each new generator surfaces a new "authored shadow of generated" pattern; each pattern earns a `TEST-RESTATES-<dim>-001` lint companion. Catalog expansion is gated on the generator existing — same Rule Zero discipline as the vocab-lints proposal (do not enforce absence).

2. **A new authoring pattern proves systemic across ≥2 product capsules.** If, after v0.1, a third drift pattern surfaces (e.g., authoring source-state assertions when transition-matrix generation is planned), that pattern earns a v0.2 lint with the same shape.

3. **A test grammar extension is proposed.** Any proposal to extend `test_clause` beyond the current `from <state>` / `as <actor>` / `when <expr>` closed set must include a companion `TEST-*` lint that detects misuse of the new shape — or explicitly justify why no lint is appropriate (the new shape is purely additive and has no obvious misuse).

The catalog is a **closed catalog over the test vocabulary** — bounded by the closed predicate language's surface, not by user creativity. It is the same shape as `VOCAB-*`: lint catalog grows only when vocabulary grows or when an undisputed drift pattern accumulates.

---

## Grade gates

This proposal goes through the standard grade-then-fix loop with the `lazuli-language-architect` agent before commit. Gate: ≥ 8.5/10 with all dimensions ≥ 7. Target ≥ 9.0/10.

**Pre-architect grade**: the orchestrator (the user) will grade this proposal first (see the conversation that produced it). Architect grade is the second pass.

Per the AI-first rubric, the dimensions most at risk for this proposal:

- **Vocabulary Over Mechanism (D1)** — meta-application; the lint catalog itself must not open a predicate-shape engine in disguise. The five rules each prescribe *one* IR-walkable heuristic; none asks the doctor to interpret authored intent.
- **Generation/Authoring boundary (D-NEW)** — implicitly D6 (surface area): the catalog must honor the invariants.md:530-548 split. Score requires that NO rule in v0.1 prescribes authoring in territory the generators (Phase 2-4) will own.
- **Doctor coverage (D9)** — auto-relevant; the lints themselves are the surface being graded.
- **Escape-hatch hygiene (D5)** — `[doctor.test_discipline]` overrides are the escape hatch. Requires justification (comment hint, audit).
- **Severity calibration (D-NEW)** — implicitly D7 (operational fitness): the proposed severities (especially `error` for `TEST-FIXTURE-LITERAL-001`) must withstand the Hostpoint-corpus calibration. If the rule fires > 10% false-positive against the pilot corpus, severity drops and the carve-out grows.

A BLOCK at < 8.5 or any dimension < 7 returns the proposal to the design author with annotated blockers. Re-grade as v0.2.

---

## References

- [docs/invariants.md §Tests (lines 530-548)](../invariants.md) — the canonical authored-vs-generated split.
- [docs/design-principles.md](../design-principles.md) — Rule Zero applied to the test surface.
- [docs/proposals/doctor-vocabulary-lints.md](doctor-vocabulary-lints.md) — direct stylistic sibling; severity model + override knob + module shape mirror this proposal.
- [docs/proposals/lifecycle-vocab.md](lifecycle-vocab.md) §3.3 — lifecycle `tests` block grammar (same closed `allows`/`denies` clauses, mirroring workflow tests).
- [crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs](../../crates/lazuli_doctor/src/vocab/vocab_tests_missing_001.rs) — existing feature-level coarse test-coverage lint; orthogonal to this catalog.
- [skills/audit/RULES.md](../../skills/audit/RULES.md) — LLM-readable projection of doctor rule catalog; this proposal will append 5 entries mirroring the pattern.
- [skills/audit/EXAMPLES/](../../skills/audit/EXAMPLES/) — positive/negative .lzi fixtures for each rule, paralleling the vocab-lints fixture practice.

---

## Appendix — sample diagnostic walkthrough on the pilot capsule

The orchestrator's 2026-05-16 audit (the trigger for this proposal) identified the following authored test assertions in the pilot capsule's host-lifecycle feature. Each is mapped to the rule that catches it; the table is acceptance evidence for §"Verification".

| Authored assertion (paraphrased) | Rule | Verdict |
|---|---|---|
| `allows from basic_details_pending` on `transition fill_basic_details` | none | clean (source-state coverage; legit until Phase 4 generator ships) |
| `denies from intermediation_terms_pending` on same transition | none | clean (same) |
| `denies from complete` on same transition | none | clean (same) |
| no `tests` block on `transition fill_basic_details` despite `requires self.cpf != nil OR self.cnpj != nil` (the rule fires; the user proposed the right `requires` in the same conversation) | `TEST-MISSING-AUTHORED-001` | fires (warning) |
| no `denies when self.cpf = nil AND self.cnpj = nil` (predicate uncovered) | `TEST-PREDICATE-UNCOVERED-001` | fires (info) |
| (hypothetical, from 13:58 analysis) `allows when self.cpf = "12345678901"` | `TEST-FIXTURE-LITERAL-001` | fires (error) |
| (hypothetical, from 13:58 analysis) `creates IntermediationTermsAcceptance with version = input.version` inside `tests` | grammar error today; the lint catches the precursor (effect-restating predicate `allows when self.published_at != nil` after `timestamps`) | fires (warning, when grammar admits it) |
| (hypothetical, from 13:58 analysis) `denies as @role.traveler` on a `policy @policy.host_only` transition | `TEST-RESTATES-POLICY-001` | fires (warning, escalates to error in Phase 2 wave) |

Five rules; eight fires across one pilot feature. The acceptance bar is **at most one false positive per rule** in the full pilot corpus — calibrated against the rule's `Verification` heuristic.
