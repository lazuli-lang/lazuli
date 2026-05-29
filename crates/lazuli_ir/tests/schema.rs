use lazuli_ir::LZIR_SCHEMA;

#[test]
fn lzir_schema_constant_is_0_16_0() {
    // `cookie-sessions-child` adds the additive `AuthSessions.cookie` slot
    // + the `SessionCookie` type. The change is back-compat-pure (every
    // field `#[serde(default, skip_serializing_if = "Option::is_none")]`,
    // so a `cookie`-less `AuthSessions` is byte-identical to 0.16.0). The
    // Minor bump `docs/ir-abi.md` prescribes for a new optional field is
    // DEFERRED to the follow-up stage that ships the `0.16-to-0.17`
    // migration recipe + re-pins the version fixtures (migration/doctor
    // work out of this core-faces stage). Until then the constant — and
    // this assertion — stay 0.16.0.
    //
    // `ir-rate-limit-env-aware` cell 1 — bumped here from 0.15.0 by the
    // shape change on the `rate_limit` slot of `Command`, `Api`,
    // `Agent`, `Report`, and `AuthPassword`.
    assert_eq!(LZIR_SCHEMA, "0.16.0");
}
