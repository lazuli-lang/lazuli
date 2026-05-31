---
id: 0012
title: Fix shared linker bugs — four VOCAB false positives waived identically by both pilots
type: adr
status: ready
created: 2026-05-31
depends_on: []
parallel_safe: false
track: evolve/prove
---

# ADR — Fix shared linker bugs

## Context
Four `lazuli_doctor` vocab diagnostics are waived with matching reason text by two unrelated pilots, several citing each other. Identical cross-app waivers are the canonical false-positive signal. Reading the rule SOURCES (not the waiver prose) pins the true root cause of each — and they are NOT the same root cause, nor are all four genuinely false:
- EVENT-PAYLOAD: rule (`vocab_event_payload_001.rs:148`) indexes only flat `feature.events`, ignores events nested in `event_group ... on <Resource>` (confirmed: pauta `account.lzi:94` `event_group account_* on User` with nested `event signed_up` etc., command `emits account_signed_up`) → grouped events report `Undeclared`. False positive.
- TESTS-MISSING: rule already honors the allow + checks `tests` blocks; the gap is upstream in `test_lowering.rs` — `allows when`/`denies when` forms "intentionally lower to nothing" (module NOTE) pending the closed-predicate parser, so command `tests` using those forms yield an empty block and the rule still fires. False positive; fix is in the analyzer feed, not the rule.
- DERIVED-READ: write-site walk (`collect_write_sites`) sees only declarative `creates/updates`; handler-written (`@fn`) + notification-primitive-written columns look "never written". False positive the static walk structurally cannot see without a new signal.
- SHADOW-RECORD: fires on create/update inputs that genuinely duplicate ~120 lines of resource fields (hostpoint `Host` create/update share 10 fields; pauta `customer_management`). Likely a TRUE positive whose real fix is the shared input-record primitive (specs 0003/0015), not a rule change.

## Decision
Fix the genuine false positives at their true root cause: index `event_groups` for EVENT-PAYLOAD; fix `test_lowering.rs` for TESTS-MISSING; add a handler/primitive write signal for DERIVED-READ (or escape-valve it). Prove each with paired `cargo test` cases (fires-on-genuine + quiet-on-legit), then remove ONLY the false-positive waivers from both pilots. Treat SHADOW-RECORD as a true-positive suspect: only relax it if overlap is provably divergent-by-design; otherwise retain its waiver and file a backlog entry pointing at 0003/0015. Never fake a fix to zero out waivers.

## Options considered
1. **Fix each at its true root cause; remove only false-positive waivers; backlog the genuine gaps (chosen).** Pro: restores linter trust precisely; one fix per pilot pair; tests lock corrected behavior; honest about SHADOW-RECORD. Con: requires per-rule source analysis; not a uniform mechanical fix.
2. **Mechanical guesses (adjacency `_test.go` scan / aggregate-projection / `@derives(Record)` / strict-subset-input).** Rejected: reading the sources disproved these — there is no `analyzer.rs`/`scan_adjacent_tests` (each rule's `check(feature, path)` is the detector); EVENT-PAYLOAD is event_group indexing; TESTS-MISSING is test-lowering; DERIVED-READ is handler-write invisibility; SHADOW-RECORD is a real gap.
3. **Lower all four to info/off severity.** Rejected: hides the rules' genuine catches; doesn't fix root cause.
4. **Leave waivers, build the missing primitives.** Rejected for the false positives (no primitive is missing — the rule/feed is wrong); retained ONLY as the escape valve for SHADOW-RECORD (and DERIVED-READ if unsignalable).

## Consequences
- EVENT-PAYLOAD becomes group-aware; both pilots' grouped-event waivers drop.
- TESTS-MISSING stops firing once inline `tests` lower to non-empty assertions; the fix benefits every feature, not just the waived ones.
- DERIVED-READ stops flagging handler/primitive-written columns once it has a write signal — or stays, with a documented backlog entry, if the IR can't express it.
- SHADOW-RECORD likely KEEPS its waiver with a backlog pointer to 0003/0015 (shared input record) — this spec does NOT build that primitive, so it ships no code change there and there is nothing to roll back for that rule.
- **Trade:** `parallel_safe: false` — waiver-removal edits contended pilot `.lzi` files (`customer_management.lzi` is touched by 0003/0004/0005/0015/0017); serialize waiver-removal cells per pilot. The framework BUILD cells (rule/analyzer fixes) ARE parallel-safe.

## Decision drivers
- A linter waived identically by two apps is the bug — fix the rule OR its feed, at the TRUE root cause.
- Read the source, not the waiver prose: the four root causes differ.
- Never disable a rule to silence it; keep the genuine-catch test.
- Truth over zero-waivers: a real gap (SHADOW-RECORD) gets a backlog item + retained waiver, not a fake fix.

## Rollback
Each rule fix is one analyzer/rule function + its test — revert per-rule and restore that rule's waivers in both pilots. No data migration. SHADOW-RECORD ships no code change here, so nothing to roll back for it.
