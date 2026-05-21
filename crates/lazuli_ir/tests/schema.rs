use lazuli_ir::LZIR_SCHEMA;

#[test]
fn lzir_schema_constant_is_0_16_0() {
    // `ir-rate-limit-env-aware` Cell 2 — bumped from 0.15.0 by the
    // `rate_limit: Option<String>` → `rate_limit: Option<RateLimitSpec>`
    // slot change on Command / AuthSessions.PasswordConfig / Agent /
    // Api / Report. Pre-0.16.0 JSON fixtures need re-lowering since
    // the slot value changes shape (string → object). See
    // `docs/proposals/ir-rate-limit-env-aware.md` §8 + `lib.rs`.
    assert_eq!(LZIR_SCHEMA, "0.16.0");
}
