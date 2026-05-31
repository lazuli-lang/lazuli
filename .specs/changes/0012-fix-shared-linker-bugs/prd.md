---
id: 0012
title: Fix shared linker bugs — four VOCAB false positives waived identically by both pilots
type: prd
status: ready
created: 2026-05-31
depends_on: []
parallel_safe: false
track: evolve/prove
---

# PRD — Fix shared linker bugs

## Problem
Two unrelated pilots (hostpoint + pauta-web) carry NEAR-IDENTICAL `# doctor:allow` waivers — with matching reason text, often citing each other ("same waiver as account/agency") — for the same four `lazuli_doctor` vocab diagnostics. Two independent apps routing around the same diagnostic with the same justification is the signal that the **diagnostic/linker is the bug**, not the pilots. The actual root cause of each (verified by reading the rule sources + the pilot `.lzi` shapes, NOT the waiver prose) is:

- **`VOCAB-EVENT-PAYLOAD-001`** — the rule (`vocab_event_payload_001.rs:148`) builds its `declared` map ONLY from `feature.events` (flat, top-level). Both pilots declare events INSIDE `event_group <name> on <Resource>` blocks (e.g. pauta `account.lzi:109` `event_group account_lifecycle on User` with nested `event user_registered`). Commands `emits account_<x>`; the rule never looks in `feature.event_groups`, so every grouped event is reported `Undeclared`. False positive — the events ARE declared. (pilot ref: pauta BT-04 in `docs/lazuli-gaps.md:126`; waivers in account/agency/admin_panel/media_*/job_*/etc.)

- **`VOCAB-TESTS-MISSING-001`** — the rule (`vocab_tests_missing_001.rs`) ALREADY honors `# doctor:allow` and checks command/rule/workflow/lifecycle `tests` blocks via `block_has_substance` (non-empty assertions). The bug is UPSTREAM in `crates/lazuli_analyzer/src/test_lowering.rs`: inline command/rule `tests` blocks that use forms the lowerer doesn't recognize must lower to `TestAssertion::Raw` (non-empty) per its own NOTE (2026-05-27), but the pilots report the block still reads empty (BT-03/BT-11 in pauta `docs/lazuli-gaps.md`). Either a lowering path is dropping lines or the pilots' `tests` syntax isn't reaching the `Raw` fallback. Root cause is in lowering, NOT the rule's adjacency. Confirm before fixing.

- **`VOCAB-DERIVED-READ-001`** — the rule (`vocab_derived_read_001.rs`) collects write-sites ONLY from declarative `CommandEffect::Creates/Updates` assignments + `from_input`. Fields written by `@fn` HANDLERS (e.g. pauta `job_steps_activities` `instantiate_workflow_steps` seeds `step_template_id`; `compute_activity_due_date` sets `due_date_override`) and by the native `notification <name>` primitive's dispatch (pauta `notifications` `entity_type`/`entity_id`) are invisible to the static walk → flagged "never written". False positive for handler-written + primitive-written columns; the rule cannot see imperative Go.

- **`VOCAB-SHADOW-RECORD-001`** — the rule (`vocab_shadow_record_001.rs`) fires on declarations sharing ≥4 `(name,type)` pairs at ≥50% ratio (resource-vs-command-input + input-vs-input). In both pilots this fires on create/update command inputs that genuinely have ~120 lines of field-by-field overlap with the resource (hostpoint `Host` create/update share 10 fields; pauta `customer_management` create/update inputs). This may be a GENUINE positive: the missing primitive is a shared create/update input `record`, owned by specs 0003/0015 — NOT a false positive. The shadow-record relationship to that ~120-line copy is real.

## Goal
For each of the four, find the TRUE root cause (verified against rule source + pilot shape), fix the genuine false positives, prove each fix with `cargo test -p lazuli_doctor` (and `lazuli_analyzer` where the feed is the bug), then REMOVE the now-unneeded waivers from BOTH pilots and confirm `lazuli doctor .` is clean WITHOUT them. For any rule that is a GENUINE positive (SHADOW-RECORD is the prime suspect), DOWNGRADE it to a tracked backlog item, KEEP the waiver, and document why — do not fake a fix.

## Users & jobs
- **Authoring agent**: trusts doctor warnings. Job: "a VOCAB nudge must mean I actually hand-rolled something; routine identical waivers mean the rule is lying and I stop reading warnings."
- **Framework maintainer**: keeps the linter honest. Job: "a diagnostic waived identically by two apps is a false positive — fix the rule/feed, not the apps."
- **Grader (RULE team)**: Job: "rules fire on the real anti-shape and stay quiet on the legitimate shape; the cross-pilot waiver census trends to zero."

## Scope
### In
- EVENT-PAYLOAD: index `feature.event_groups` (nested events) into the `declared` map; resolve `emits <group-prefixed name>` against grouped events.
- TESTS-MISSING: fix the upstream `test_lowering.rs` so recognized-or-raw inline `tests` blocks lower to non-empty assertions (or confirm the rule must read a coverage source it currently can't — then fix the feed). NOT the rule's existing allow/adjacency logic.
- DERIVED-READ: teach the write-site collector about handler-written + notification-primitive-written columns (e.g. honor a `handler writes <field>` signal, or treat a field bound by a `notification` block / `@fn` handler as written), OR — if the IR carries no such signal — declare it a genuine gap (escape valve).
- `cargo test -p lazuli_doctor` (+ `lazuli_analyzer`) paired cases: each fixed rule STILL fires on the genuine anti-shape AND stays quiet on the legitimate pilot shape.
- Remove the now-unneeded waivers from hostpoint (`C:\Users\lucas\hostpoint\app`) and pauta (`C:\Users\lucas\dev\pauta-web-monorepo\app`).
### Out
- Building the shared create/update input-record primitive (SHADOW-RECORD's real fix) — that is specs 0003/0015. Note the relationship; scope THIS spec to the false-positive linker fixes only. If SHADOW-RECORD is confirmed a true positive, it gets a retained waiver + backlog note here.
- Building `query.compose` derived-read capability (spec 0013).

## Behaviour
- After the fix, `lazuli doctor .` on each pilot with the FALSE-positive waivers DELETED produces zero of those codes.
- Each fixed rule still fires on a constructed genuine anti-shape (regression: not just disabled).
- **Genuine-gap escape valve:** if a rule correctly identifies a real missing primitive (SHADOW-RECORD → shared input record; possibly DERIVED-READ if no handler-write signal exists in IR), the plan does NOT force-remove that waiver — it downgrades the item to a tracked entry in `docs/language-backlog.md`, keeps the waiver, and documents the true-positive finding. Success = "no FALSE positives," not "zero waivers regardless of truth."

## Success metric
`cargo test -p lazuli_doctor` (+ `lazuli_analyzer` where the feed changed) green across the affected modules (each with fires-on-real + quiet-on-legit) AND `lazuli doctor .` clean in both pilots after the FALSE-positive waivers are removed — OR, for any rule proven a true positive, a `docs/language-backlog.md` entry + retained waiver + written justification.

## Risks
- "Fixing" a rule into uselessness (never fires). Mitigation: every rule keeps a fires-on-genuine-anti-shape test.
- Misclassifying SHADOW-RECORD as a false positive and silencing a real duplication. Mitigation: treat SHADOW-RECORD as a true-positive suspect by default; only "fix" it if the overlap is provably divergent-by-design (e.g. admin_panel's distinct SELECT projections), else retain waiver + backlog.
- DERIVED-READ has no IR signal for handler writes. Mitigation: escape valve — backlog + keep waiver + document, not a fake fix.

## Open questions
- Is the TESTS-MISSING root cause in `test_lowering.rs` (lines dropped) or in the pilots' `tests` syntax never reaching lowering? Resolved in the plan's reproduce step.
- Does the IR expose any handler-write / notification-write signal DERIVED-READ could consult? Resolved in the plan; escape valve covers "no."
