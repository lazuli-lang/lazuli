---
id: 0002
title: crud-inverse-linter
type: techspec
track: prove
depends_on: [0001]
parallel_safe: true
status: ready
created: 2026-05-31
test_gate: "cargo test -p lazuli_doctor crud_synth_available"
agent: unassigned
---

# TechSpec — CRUD inverse linter

## Approach

New advisory doctor vocab rule, `VOCAB-CRUD-SYNTH-AVAILABLE-001`, that detects a
resource hand-rolling the crud surface and suggests `conventions [crud]`. It is
the existing crud synth run backwards. No grammar/AST/IR change; pure doctor +
one facet registry row. Advisory (never gates), suppressible via `# doctor:allow`.

## Grounding (verified by reading)

- AST: `crates/lazuli_syntax/src/ast/resource_p1.rs` — `ResourceDecl { name,
  fields, conventions: Vec<ResourceConventionAst>, soft_delete, retention, ... }`;
  `enum ResourceConventionAst { Crud, Me }`.
- Synth pass: `crates/lazuli_analyzer/src/conventions/mod.rs` — gates on
  `resource.conventions.contains(&ir::ConventionRef::Crud)`; tags members
  `ConventionOrigin::Synthesized(ConventionRef::Crud)` vs `AuthorOverride(...)`.
  Synth emits 3 commands (`create/update/delete_<r>`) + 2 queries
  (`read/list_<r>` — `crates/lazuli_analyzer/src/feature.rs` §5).
- IR convention enum: `crates/lazuli_ir/src/nodes/resource/convention.rs`
  (`ConventionRef::{Crud, Me}`); catalog mirror `CONVENTION_CATALOG = &["crud",
  "me"]` in `crates/lazuli_analyzer/src/errors.rs`.
- Existing validate-only family: `crates/lazuli_doctor/src/vocab/conventions.rs`
  (`crud_synth_signature_mismatch`, `crud_synth_policy_not_found`,
  `crud_synth_no_required_fields`, `me_synth_*`) — the message/Finding pattern to
  mirror.
- Vocab module registry: `crates/lazuli_doctor/src/vocab/mod.rs` (`pub mod` list).
- Facet registry: `crates/lazuli_keywords/src/registry/facets.rs` — `df(code,
  base_severity, category)` rows; conventions group is
  `P_CONVENTIONS = &[df("conventions_unknown", "warning", "vocabulary")]`.
- Suppression: `crates/lazuli_doctor/src/allow_comment.rs` (the `# doctor:allow`
  path — every emitted finding flows through it).

## Surface

**Create:**
- `crates/lazuli_doctor/src/vocab/crud_synth_available.rs` — the rule: a
  `CrudSynthAvailableFinding` struct (mirrors the `*Finding` shape in
  `vocab/conventions.rs`: `path`, `resource`, `matched: Vec<String>`,
  `delete_excluded: bool`) + `check(...)` producing findings + the `CODE` const +
  inline `#[cfg(test)] mod tests`.
- (tests live in the same file's `mod tests`, named so
  `cargo test -p lazuli_doctor crud_synth_available` selects them.)

**Modify:**
- `crates/lazuli_doctor/src/vocab/mod.rs` — add `pub mod crud_synth_available;`.
- `crates/lazuli_keywords/src/registry/facets.rs` — extend `P_CONVENTIONS`:
  `df("VOCAB-CRUD-SYNTH-AVAILABLE-001", "warning", "vocabulary")`.
- the vocab dispatcher/aggregator that walks resources and collects vocab
  findings (the same wiring site that calls the existing `conventions` family) —
  call `crud_synth_available::check` per feature-resource and fold its findings in.

## Contracts

**Synth-name set (the inverse key).** For a resource named `<R>` with snake form
`<r>`, the canonical synth member names are exactly:
`create_<r>`, `update_<r>`, `delete_<r>` (commands), `read_<r>`, `list_<r>`
(queries) — identical spelling to what `conventions/mod.rs` synthesizes. The rule
MUST derive these from the same snake-casing helper the synth uses, not a private
copy, so the inverse can never drift from the forward pass.

**Trigger.** Emit `VOCAB-CRUD-SYNTH-AVAILABLE-001` iff ALL hold:
1. the resource does NOT already list `Crud` in `conventions`;
2. the feature hand-rolls (by name) at least `create_<r>` AND `update_<r>`;
3. the matched-name set after the soft-delete carve-out is non-empty.

**Soft-delete carve-out.** If `delete_<r>` is matched AND the resource declares
`soft_delete` or `retention ... then ...` (LGPD posture), `delete_<r>` is dropped
from the suggested replacement set and `delete_excluded = true`. The message then
says create/update (+read/list when matched) are replaceable; delete stays
explicit until 0015.

**Severity / gating.** Base severity `warning`; the rule's category keeps it out
of the gating set (exit code unaffected). Suppressible: the finding is anchored on
the resource span and flows through `allow_comment`, so `# doctor:allow
VOCAB-CRUD-SYNTH-AVAILABLE-001` on the resource silences it.

**Message (canonical phrasing, mirrors `vocab/conventions.rs` style):**
```
Resource `<R>` hand-rolls <n> command(s) the `crud` convention would synthesize
(<matched>). Add `conventions [crud]` to the resource and delete them.
<when delete_excluded:> Keep `delete_<r>` explicit — it is a soft-delete; the
synthesized delete is hard (see spec 0015).
```

### Behavior table

| resource shape | emits? | suggested members |
|----------------|--------|-------------------|
| full hand-rolled CRUD, hard delete | yes | create, update, delete, read, list |
| full hand-rolled, `soft_delete`/`retention` delete | yes | create, update, read, list (delete excluded) |
| only create + update hand-rolled | yes | create, update |
| only read + list hand-rolled (no create/update) | no | — (no core) |
| already `conventions [crud]` | no | — (opted in) |
| `# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001` on resource | no | — (suppressed) |

## Plan — for the executing agent

1. Add the `P_CONVENTIONS` facet row in `facets.rs`; confirm the registry still
   compiles and the code is discoverable.
2. Write `vocab/crud_synth_available.rs`: `CODE` const, `*Finding` struct +
   `message()`, and `check(feature, resource)` implementing the contract. Reuse
   the synth's snake-casing helper for the name set (import it; do not re-implement).
3. Add `pub mod crud_synth_available;` to `vocab/mod.rs`.
4. Wire `check` into the resource-walking vocab aggregator next to the existing
   `conventions` family; ensure findings pass through `allow_comment`.
5. Write the inline tests (below). Run `test_gate`.

## Tests first (TDD)

Inline `#[cfg(test)] mod tests` in `crud_synth_available.rs` (selected by
`cargo test -p lazuli_doctor crud_synth_available`):

- [ ] `flags_full_handrolled_crud` — resource with hand-rolled create/update/
      delete/read/list and no convention ⇒ one finding, matched = all 5.
- [ ] `silent_when_opted_in` — resource carrying `conventions [crud]` ⇒ no finding.
- [ ] `excludes_soft_delete_from_suggestion` — `soft_delete` resource ⇒ finding
      present, `delete_excluded == true`, `delete_<r>` not in matched.
- [ ] `requires_create_and_update_core` — only read+list hand-rolled ⇒ no finding.
- [ ] `respects_doctor_allow` — `# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001` on
      the resource ⇒ suppressed (no finding survives the allow_comment pass).
- [ ] `severity_is_advisory` — the facet row's base severity is `warning` and the
      code is not in the gating set.

## Gate — Definition of Done (Lazuli feature gate)

> Embedded verbatim from `0001-teaching-spine/techspec.md`, made concrete for 0002.

```
## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.
```

Concrete for 0002:
1. **BUILD** — `cargo test -p lazuli_doctor crud_synth_available` green (all six
   tests above); facet registered with base severity `warning`, non-gating.
2. **MIGRATE** — no pilot edit is owned by THIS spec (that is 0003). The rule must
   run clean (advisory-only) on the framework's own example/template app:
   `lazuli check` still exits 0 with the advisory present.
3. **TEACH** — `docs/lazuli_way/crud-by-convention.md` (scaffolded by 0001) gains a
   "How doctor nudges you" section naming `VOCAB-CRUD-SYNTH-AVAILABLE-001`: its
   trigger, the soft-delete carve-out, and the `# doctor:allow` escape hatch.
   (Real pilot before/after excerpts are added by 0003.)
4. **ENFORCE** — `VOCAB-CRUD-SYNTH-AVAILABLE-001` fires on a fully hand-rolled CRUD
   resource (proven by `flags_full_handrolled_crud`); the rule code is named in the
   idiom doc.

## Risks & rollback

- The inverse name set drifts from the forward synth → mitigation: import the
  synth's snake helper instead of duplicating; `flags_full_handrolled_crud` pins
  the exact 5 names.
- Soft-delete detection false-negative (a soft delete not flagged) suggests a hard
  delete → data-loss advice. Mitigation: carve-out keys on BOTH `soft_delete` and
  `retention`, and the advice is non-binding (Warning) + 0003 reviews each delete.

**Rollback:** `git revert` — additive (one new file + one facet row + one mod line
+ one wiring call). Nothing downstream depends on it at runtime.

## Parallel-safety

`parallel_safe: true` — touches only `lazuli_doctor` (new file + mod line +
aggregator call) and one `P_CONVENTIONS` row in `lazuli_keywords`. No overlap with
0003, which edits only the Pauta repo.
