---
id: 0024
title: VOCAB-RUNTIME-REINVENTED-001 — doctor catches handlers that reinvent the runtime
type: adr
status: accepted
created: 2026-06-01
supersedes: —
---

# ADR — One generic rule, a parameterized reinvention table, two detector families; sibling of ESC-RAWSQL

## Context
- The reinvention audit proved the damage is a CLASS, not isolated cases: 31 confirmed across two pilots, concentrated in crypto/auth (import-detectable) and lifecycle/scalar shapes (pattern-detectable). A rule-per-case is the wrong answer (the user's explicit fear: "se pra cada caso desses nós termos que criar uma regra... fudeu").
- The handler-`.go`-scanning machinery already exists: `ESC-RAWSQL-IN-HANDLER-001` (0010) walks `<feature>/handlers/*.go`, matches markers, honors `# doctor:allow`, dispatches via the run aggregator. The new rule is its sibling — same plumbing, different match table.
- The wire-thin principle has a precise blind spot: it keys on "many LOC + zero external imports". Reinvented crypto IMPORTS an external lib (argon2), so it slips through. The import-signal detector closes exactly that hole.
- The false-positive rate in the raw audit was 47.5% — so the rule must be PRECISE: a `crypto/*` import alone isn't proof; it must pair with a runtime equivalent. A `UPDATE` alone isn't proof; it must pair with the `RowsAffected==0` sentinel shape.

## Decision
- **One rule, `VOCAB-RUNTIME-REINVENTED-001`, parameterized by a REINVENTION TABLE.** The table is a `const &[ReinventionRule]` where each row = `{ trigger: ImportSet | BodyShape, equivalent: &str (runtime sym or .lzi primitive), family: &str }`. Adding a family = adding a row. No per-case functions.
- **Two detector families:**
  - **Import-signal** (high precision): handler imports a crypto/encoding lib from the table's `ImportSet` → fire, naming the runtime equivalent. Seed set: `golang.org/x/crypto/argon2`+`crypto/subtle`+`encoding/base64` → `auth.HashPassword`/`auth.VerifyPassword`; `crypto/sha256`+`encoding/hex` in a token context → `auth.HashSessionToken`/`auth.HashWithArgon2`; `crypto/rand` minting an opaque token → `auth.MintSessionToken`/`auth.RequestPasswordReset`.
  - **Shape-signal** (regex/substring over the body): `UPDATE`...`status IN (`...`RowsAffected`...`== 0` → "lifecycle transition: declare a `transition` in .lzi (runtime owns TransitionAdvance + LifecycleStateMismatchError)"; `^#?[0-9A-Fa-f]{6}$` regex literal → "`@semantic.HexColor`"; `>= 0`+`<= 100`+`Decimal` validation → "`@semantic.Percentage`"; hand-typed `deleted_at IS NULL` → "`soft_delete by` (declarative filter)".
- **Reuse 0010's scanner verbatim** (`scan handlers/*.go`, `source_contains_doctor_allow`, the `check(feature, feature_dir)` signature, the aggregator wiring). The new rule lives at `crates/lazuli_doctor/src/vocab/runtime_reinvented_001.rs` (vocab family, where the module_headers meta-lint applies — gives the trigger-cue header for free).
- **Advisory, never gates.** Some findings are gated on an upstream primitive (parser GAP for partial-unique, transition-name collision bug) — those carry `# doctor:allow ... reason`. The rule nudges; it never blocks a pilot whose reinvention is a tracked gap.
- **Precision over recall.** Each row requires BOTH the trigger AND a named equivalent. A handler importing `crypto/hmac` for a vendor webhook signature (legitimate — no runtime equivalent) does NOT fire because no table row claims an equivalent for it. Better to miss a borderline case than reproduce the 47.5% noise.

## Alternatives considered
- **A rule per reinvention family** (one for crypto, one for lifecycle, one for scalars) — rejected: that's 4+ rules now and N later; the table-parameterized single rule is the same coverage with O(1) growth.
- **AST-parse the Go handlers** (full go/parser) — rejected: heavy dependency, and the import-list + body-substring signals are sufficient for the families found. The audit's own detectors were grep-level and hit 52% precision pre-verification; pairing trigger+equivalent gets the rest. Revisit only if substring FPs appear.
- **Make it a hard error** — rejected: gated-on-upstream cases exist; advisory + `doctor:allow` is correct. Escalating is a config choice (preset), not a default.
- **Fold into ESC-RAWSQL** — rejected: different intent (raw SQL visibility vs runtime reinvention); separate code keeps each message sharp. They share the scanner, not the semantics.

## Consequences
**We accept:** substring/import heuristics can theoretically miss an obfuscated reinvention or false-fire on an unusual-but-legitimate handler — mitigated by requiring trigger+equivalent pairing and by `doctor:allow` for the rare legit case. The table must be kept in sync as the runtime grows (a row per new commodity primitive).
**We gain:** the audit oracle — `lazuli doctor` now reproduces the 31-finding audit at lint time; every batch fix self-verifies (rule goes silent); the wire-thin blind spot (external-import reinvention) is closed; new families are one-row additions. The user's nightmare ("a rule per case") is structurally avoided — it's a table per class.
**We watch:** if the false-positive rate on real pilots exceeds ~10%, tighten the trigger+equivalent pairing (require a more specific body context per import). If agents start `doctor:allow`-ing it en masse without reasons, that's a signal the equivalent is wrong or missing — fix the table, not the waiver.
