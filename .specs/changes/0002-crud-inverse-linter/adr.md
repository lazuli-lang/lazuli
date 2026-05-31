---
id: 0002
title: crud-inverse-linter
kind: adr
track: prove
depends_on: [0001]
parallel_safe: true
status: ready
created: 2026-05-31
---

# ADR — Inverse-synth detection as an advisory doctor facet

- Status: Accepted
- Date: 2026-05-31
- Deciders: Lazuli core
- Relates: docs/architecture-decisions/0002-resource-conventions-crud-me.md

## Context

ADR 0002 (resource conventions) shipped `crud`/`me` synth. The synth pass lives
in `crates/lazuli_analyzer/src/conventions/mod.rs` and tags each member it
produces `ConventionOrigin::Synthesized(ConventionRef::Crud)`, while a
name-colliding author member becomes `AuthorOverride(...)`. Today's doctor
(`crates/lazuli_doctor/src/vocab/conventions.rs`) only validates *existing*
opt-ins (`crud_synth_signature_mismatch`, `crud_synth_policy_not_found`, …);
nothing nudges a resource that hand-rolls the whole surface to adopt the
convention.

Doctor facet registration: rule codes are declared as `DiagnosticFacet` rows in
`crates/lazuli_keywords/src/registry/facets.rs` (e.g. `P_CONVENTIONS =
&[df("conventions_unknown", "warning", "vocabulary")]`). Vocab rules are
`pub mod` entries under `crates/lazuli_doctor/src/vocab/mod.rs`, each a sub-module
with a `check`/`run` function. `# doctor:allow` suppression is handled centrally
by `crates/lazuli_doctor/src/allow_comment.rs`.

## Decision

Add a new vocab rule, code `VOCAB-CRUD-SYNTH-AVAILABLE-001`, registered as a
`DiagnosticFacet` with base severity **`warning`** in the conventions facet group
(`P_CONVENTIONS`), and **excluded from gating** — `lazuli check`/`doctor` exit
codes never change because of it (advisory category).

Detection = the existing crud synth **run backwards**:

1. For each resource that does **not** already carry `ConventionRef::Crud` in its
   `conventions`, compute the canonical synth member names for its name
   (`create_<r>`, `update_<r>`, `delete_<r>`, `read_<r>`, `list_<r>` — same
   spelling the synth pass uses; snake-cased per the existing convention).
2. Intersect those names against the resource feature's hand-written
   commands/queries by name.
3. If the matched set covers the create + update core, emit the diagnostic with a
   suggestion payload naming `conventions [crud]` and the exact members it would
   replace.
4. **Soft-delete carve-out:** if the matched `delete_<r>` command sets
   `deleted_at`/carries the resource's `soft_delete`/`retention` posture, exclude
   it from the suggested replacement set and note that delete stays explicit until
   the soft-delete convention lands (0015), because the canonical synth delete is
   a hard delete.

The diagnostic is anchored on the resource declaration span so `# doctor:allow
VOCAB-CRUD-SYNTH-AVAILABLE-001` placed on the resource suppresses it through the
existing `allow_comment` path.

## Alternatives considered

- **Make it gate (Error).** Rejected: adoption is a recommendation, not a
  correctness rule; gating would break every pre-convention pilot on upgrade.
- **Auto-rewrite the source.** Rejected: out of scope; doctor advises, it does
  not mutate. A future codemod can consume the machine-readable suggestion.
- **Detect by field-shape instead of synth member names.** Rejected: the synth
  keys off the resource name; matching the synth's own member names is the exact
  inverse and avoids guessing intent.
- **Reuse the existing `crud_synth_*` family.** Rejected: those validate opt-ins
  (fire when you HAVE the convention); this fires when you DON'T. Opposite
  trigger, opposite default posture — a distinct code keeps `# doctor:allow`
  scoping clean.

## Consequences

- Authors (and agents reading `--json`) get a concrete adoption nudge listing the
  members, closing the gap ADR 0002 flagged.
- Safe to ship dark: advisory + suppressible means no build breaks.
- Soft-delete resources get a *partial* suggestion (create/update only), matching
  exactly what 0003 does to Pauta — the linter and the migration agree.
- Adds one facet to the catalog; no change to synth or analyzer behavior.
