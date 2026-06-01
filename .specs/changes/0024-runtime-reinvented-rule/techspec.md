---
id: 0024
title: VOCAB-RUNTIME-REINVENTED-001 — doctor catches handlers that reinvent the runtime
type: techspec
track: prove (reinvention defense)
depends_on: []
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_doctor runtime_reinvented && cargo test --workspace"
agent: unassigned
---

# TechSpec — Runtime-reinvented doctor rule

## Approach
A sibling of `ESC-RAWSQL-IN-HANDLER-001` (0010): reuse its `handlers/*.go` scanner + `# doctor:allow` honoring + aggregator wiring verbatim, swap the SQL-marker match for a parameterized REINVENTION TABLE of `{trigger (import-set OR body-shape), equivalent (runtime sym / .lzi primitive), family}`. Two detector families (import-signal, shape-signal), each requiring trigger+equivalent so precision stays high. Advisory severity. The audit's 31 confirmed findings (in `/c/tmp/reinvention-damage-report.json`) are the correctness oracle.

## Surface
**Create:**
- `crates/lazuli_doctor/src/vocab/runtime_reinvented_001.rs` — the rule: `REINVENTION_TABLE` const, `scan_handler(source) -> Vec<Hit>`, `check(feature, feature_dir) -> Vec<Finding>` (mirror rawsql's signature), `Finding{ file, handler, family, equivalent, waived }` + `message()`.
- `crates/lazuli_doctor/tests/runtime_reinvented.rs` — fixtures + the audit-oracle tests.

**Modify:**
- `crates/lazuli_doctor/src/vocab/mod.rs` — `pub mod runtime_reinvented_001;`.
- `crates/lazuli_keywords/src/registry/facets.rs` — add `df("VOCAB-RUNTIME-REINVENTED-001", "warning", "vocabulary")` to the vocab facet group (P_FN or a P_RUNTIME group — match where VOCAB-HANDLER-HEAVY-001 lives).
- `crates/lazuli_doctor_run/src/doctor/aggregators/...` — wire `runtime_reinvented_001::check` into the same aggregator that dispatches the escape_hatch / vocab handler rules (find where `ESC-RAWSQL-IN-HANDLER-001` or `VOCAB-HANDLER-HEAVY-001` is dispatched and follow it).
- `docs/diagnostics/README.md` — register the code + one-line help.

## Contracts
**The reinvention table (seed rows — extensible; this IS the deliverable shape):**
```rust
struct ReinventionRule { trigger: Trigger, equivalent: &'static str, family: &'static str }
enum Trigger {
    Imports(&'static [&'static str]),   // ALL must be present in the handler import block
    BodyShape(&'static [&'static str]),  // ALL substrings must appear in the body
}
const REINVENTION_TABLE: &[ReinventionRule] = &[
  // import-signal (crypto/auth)
  R{ Imports(&["golang.org/x/crypto/argon2"]), "auth.HashPassword / auth.VerifyPassword (argon2 + concurrency cap + rate-limit are runtime-owned)", "auth.password-hash" },
  R{ Imports(&["crypto/sha256","encoding/hex"]), "auth.HashSessionToken / auth.HashWithArgon2 via @cap.Hashed", "auth.token-hash" },
  R{ Imports(&["crypto/rand"]) /* + opaque-token body context */, "auth.MintSessionToken / auth.RequestPasswordReset (runtime mints+hashes+TTL)", "auth.opaque-token" },
  // shape-signal (lifecycle/scalar/soft-delete)
  R{ BodyShape(&["UPDATE","status IN (","RowsAffected"]), "declare a `transition` in .lzi — runtime owns TransitionAdvance + LifecycleStateMismatchError", "lifecycle.transition" },
  R{ BodyShape(&["#?[0-9A-Fa-f]{6}"]), "@semantic.HexColor", "scalar.hexcolor" },
  R{ BodyShape(&["deleted_at IS NULL"]), "`soft_delete by` (declarative deleted_at filter)", "query.soft-delete" },
];
```
PRECISION GUARD: a row fires ONLY when its trigger matches AND the handler is NOT a vendor-signature/legitimate case the table has no equivalent for. `crypto/hmac` (vendor webhook) has NO row → never fires. `crypto/rand` requires the opaque-token body context (a `token`/`reset`/`session` substring nearby) so a random-id mint for an unrelated purpose doesn't false-fire.

**`check(feature: &Feature, feature_dir: &Path) -> Vec<Finding>`** — IDENTICAL signature/shape to `rawsql_in_handler_001::check`. Scans `<feature_dir>/handlers/*.go`, runs each table row, honors `source_contains_doctor_allow(src, CODE)`.

**`Finding::CODE = "VOCAB-RUNTIME-REINVENTED-001"`**, severity warning, `message()` = `handler '<file>' reinvents <family> — use <equivalent>. The Lazuli runtime owns this mechanism; see docs/lazuli_way/delegate-to-runtime.md. (# doctor:allow VOCAB-RUNTIME-REINVENTED-001 — reason "..." if this is a tracked upstream gap.)`

## Plan — for the executing agent
1. Read `crates/lazuli_doctor/src/escape_hatch/rawsql_in_handler_001.rs` IN FULL — it is your template (scanner, `check`, `source_contains_doctor_allow`, `Finding`, the test fixtures shape). Read its aggregator wiring in `lazuli_doctor_run`.
2. Read the audit's confirmed findings at `/c/tmp/reinvention-damage-report.json` (the `confirmedFindings[]` array) — these are your oracle. Note which families appear: auth.password-hash (4), auth.token/session/reset (7), scalar hex/percentage (3), lifecycle transition (7), soft-delete (1).
3. Read 2-3 real flagged handlers to ground the detectors: `C:/Users/lucas/dev/pauta-web-monorepo/app/features/account/handlers/hash_password.go` (argon2 import), `C:/Users/lucas/hostpoint/app/features/operations/handlers/accept_proposal.go` (UPDATE...IN...RowsAffected shape), `C:/Users/lucas/dev/pauta-web-monorepo/app/features/agency/handlers/validate_hex_color.go` (hex regex).
4. Write `runtime_reinvented_001.rs` with the `REINVENTION_TABLE` + `check`. Mirror rawsql's module structure + `//!` header with a `fires when` trigger cue (module_headers).
5. Register the facet + wire the aggregator + diagnostics README.
6. TDD: write `tests/runtime_reinvented.rs` FIRST — fixtures for each family (a fires-on + a must-NOT-fire negative: e.g. `crypto/hmac` vendor-signature handler stays silent; a legit `UPDATE` without the RowsAffected sentinel stays silent), plus the `doctor:allow` suppression test.
7. GATE: `cargo test -p lazuli_doctor runtime_reinvented` + `cargo test -p lazuli_keywords` (facet parity) + `cargo test -p lazuli_diagnostics_registry` (bridge) + `cargo test -p lazuli_doctor --test module_headers` + **`cargo test --workspace`** (FULL sweep, 0 failures REQUIRED) + `cargo build --workspace`.
8. LIVE ORACLE (read-only): build the CLI, run `lazuli doctor` on hostpoint + pauta, confirm the rule fires on the audit's confirmed handlers (esp. the 11 auth findings + the 7 lifecycle + the hex validators) and does NOT fire on obvious-legitimate handlers (mercadopago vendor HTTP, hoxo sync). Report the count it catches vs the audit's 31.
9. TEACH: add a stub `docs/lazuli_way/delegate-to-runtime.md` (0026 fills it fully) — for now, a header + the rule reference so the message's link resolves.
10. Commit on `loop-serial`.

## Tests first (TDD)
- [ ] `argon2_import_fires` — a handler importing `golang.org/x/crypto/argon2` → fires, names `auth.HashPassword`.
- [ ] `vendor_hmac_does_not_fire` — a handler importing `crypto/hmac` for a webhook signature (no table equivalent) → silent (precision guard).
- [ ] `lifecycle_shape_fires` — `UPDATE ... status IN (...) ... RowsAffected == 0` body → fires, names `transition`.
- [ ] `plain_update_does_not_fire` — a normal `UPDATE` without the RowsAffected sentinel → silent.
- [ ] `hexcolor_regex_fires` — a `^#?[0-9A-Fa-f]{6}$` regex literal → fires, names `@semantic.HexColor`.
- [ ] `doctor_allow_suppresses` — `# doctor:allow VOCAB-RUNTIME-REINVENTED-001` on the file → waived flag set, message records it.
- [ ] `table_is_extensible` — adding a row to `REINVENTION_TABLE` catches a new fixture without touching `check` (proves the parameterization).

## Gate

### Definition of Done (reinvention-defense gate)
1. BUILD: implemented; **`cargo test --workspace` green (FULL sweep)** + facet/bridge/module_headers green.
2. ORACLE: `lazuli doctor` on hostpoint+pauta fires on the audit's confirmed reinventions (report caught-vs-31) and stays silent on legit handlers.
3. TEACH: `docs/lazuli_way/delegate-to-runtime.md` stub exists (0026 fills it); diagnostics README registers the code.
4. ENFORCE: the rule IS the enforcement; the `table_is_extensible` + per-family tests prevent regression.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_doctor runtime_reinvented` (all 7 TDD) + `cargo test --workspace` 0 failures + facet parity + bridge + module_headers.
2. **ORACLE** — `lazuli doctor` hostpoint+pauta catches ≥ the high/med auth + lifecycle + scalar findings from the audit; near-zero false fire on vendor/sync handlers. Report the number.
3. **TEACH** — README + lazuli_way stub.
4. **ENFORCE** — extensibility test green.

## Risks & rollback
- **Substring false-positives** (a legit handler containing `deleted_at IS NULL` for a real reason) → mitigation: pair triggers, advisory severity + `doctor:allow`; tune the body-shape substrings to require the full sentinel context. The `plain_update_does_not_fire` + `vendor_hmac_does_not_fire` tests guard precision.
- **The table drifts from the runtime** as new primitives land → mitigation: it's a const table; a row addition is a one-line PR. Document the table as the extension point in the module header.
- **Oracle catches fewer than 31** → that's FINE and expected: some findings are shape-subtle (the stdlib `itoa` ones, the record-typed gated ones) and out of the seed table's scope; the rule targets the high-value families (crypto/auth, lifecycle, scalars). Report honestly what it catches; the batch fixes cover the rest.

**Rollback:** `git revert` — additive rule + facet row + aggregator line + doc stub; absent it, behavior is today's (no detection). No pilot file touched.
