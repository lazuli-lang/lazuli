---
id: 0024
title: VOCAB-RUNTIME-REINVENTED-001 — doctor catches handlers that reinvent the runtime
type: prd
stage: 1 of 3 (Reinvention defense)
status: ready
created: 2026-06-01
---

# PRD — Runtime-reinvented doctor rule

## Problem
Pilot agents reinvent runtime/language primitives by hand because the runtime surface is invisible to them. A 100% audit of pauta+hostpoint (165 handlers, adversarially verified) found **31 confirmed reinventions** — 12 re-implement a runtime symbol (11 of them in `auth`: argon2 hashing, session tokens, password-reset/verify tokens), 19 hand-roll a language primitive (lifecycle transitions, semantic scalars, declarative CRUD/constraints). The flagship: `account/handlers/hash_password.go` re-derives argon2id PHC encoding by hand when `runtime/go/lazuli/auth.HashPassword` exists and is even already wired in hostpoint's own `binding_fns.go`. Nothing catches this at lint time. The wire-thin founding principle (per CLAUDE.md) only fires on "100+ LOC with ZERO external imports" — `hash_password.go` PASSES it because it imports `golang.org/x/crypto/argon2`. There is no doctor rule that scans handler `.go` files for reinvention; it only surfaces in a manual audit.

## Why now (or why ever)
This is the AUDIT ORACLE for the whole reinvention-defense effort. Once it exists, `lazuli doctor` reproducibly flags the 31 findings, and every batch fix proves itself when the rule goes silent on that handler — turning "fix and hope" into "fix and the linter confirms." Without it, the smart-stubs (0025) and surface-index (0026) mechanisms have no regression backstop: a future agent that bypasses the stub still reinvents silently. One generic rule, parameterized by a reinvention table, is the net under everything. It is also the cheapest of the three mechanisms and unblocks auditable verification of the rest.

## Outcome — done means
1. A new doctor rule `VOCAB-RUNTIME-REINVENTED-001` (advisory/warning) scans each feature's `handlers/*.go` and fires when a handler reinvents a known runtime/language primitive, naming the exact symbol it should call.
2. It fires via TWO detector families, parameterized by a single REINVENTION TABLE (not hardcoded per-case):
   - **Import-signal:** a handler imports `crypto/*`, `golang.org/x/crypto/*`, `encoding/base64`, `encoding/hex`, or `crypto/subtle` AND the runtime exports an equivalent (auth.HashPassword / auth.HashWithArgon2 / auth.MintSessionToken / auth.HashSessionToken). The strongest signal — these imports in an app handler almost always mean reinvented crypto.
   - **Shape-signal:** a handler body matches a reinvention shape — `UPDATE ... status IN (...)` + `RowsAffected == 0` sentinel (lifecycle transition reinvented); regex `^#?[0-9A-Fa-f]{6}$` (HexColor reinvented); `AND deleted_at IS NULL` typed by hand (soft_delete reinvented).
3. The message names the runtime symbol or `.lzi` primitive to use and points at `docs/lazuli_way/delegate-to-runtime.md` (created by 0026's teach cell — name it as incoming if not yet present).
4. Running `lazuli doctor` on hostpoint + pauta flags the confirmed findings from the audit (the rule's correctness oracle: it must light up the 31, not the 28 false positives).
5. The rule honors `# doctor:allow VOCAB-RUNTIME-REINVENTED-001` (some reinventions are gated on an upstream primitive — those carry a waiver with a reason).

## Non-goals
- Auto-fixing the handlers (that's the batch-fix work, separate). This rule only DETECTS.
- Smart stubs (0025) / the surface index (0026) — sibling mechanisms; this is the detector.
- A complete reinvention table covering every runtime symbol — Pareto: seed it with the families the audit actually found (crypto/auth, lifecycle-transition shape, hex/percentage scalars, soft-delete shape). The table is EXTENSIBLE (adding a row catches a new family) — that extensibility is the deliverable, not exhaustiveness.
- Catching reinvention in `.lzi` (that's the language-primitive linters from the 18-spec loop). This is the Go-handler net.

## User stories
- As a pilot dev, `lazuli doctor` tells me "handler hashes a password by hand — call `auth.HashPassword` (runtime owns argon2 + concurrency cap + rate-limit)" instead of me shipping inferior crypto.
- As the reinvention-fix executor, after I rewrite a handler to delegate, the rule goes silent on it — confirming the fix landed correctly.
- As a framework maintainer, adding one row to the reinvention table makes the rule catch a whole new family, no per-case code.

## Constraints
- Reuse the handler-`.go`-scanning shape already shipped by `ESC-RAWSQL-IN-HANDLER-001` (spec 0010, `crates/lazuli_doctor/src/escape_hatch/rawsql_in_handler_001.rs`): scan `<feature_dir>/handlers/*.go`, honor `# doctor:allow`, wire into the run aggregator. Do NOT invent a new scanning mechanism.
- Register the diagnostic code in `lazuli_keywords` facets (or the bridge test fails) + `//!` trigger-cue header (module_headers).
- Advisory severity — never gates the build (these are nudges; some are gated on upstream primitives).

## Open questions
None. The two detector families + the seed table are decided in the ADR.
