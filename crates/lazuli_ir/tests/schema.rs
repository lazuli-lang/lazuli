use lazuli_ir::LZIR_SCHEMA;

#[test]
fn lzir_schema_constant_is_0_15_0() {
    // Phase L Tier 4b — bumped from 0.14.0 by the additive
    // `Command.rate_limit`/`audit`/`approval`/`invalidates`/
    // `external_calls`, plus the new `Api`, `AuditSpec`,
    // `ApprovalSpec`, `ApprovalThen`, `InvalidatesSpec` types and
    // `Feature.apis`. `JobDeclarative` swap from `raw_*` strings to
    // typed `target`/`lets`/`effect` is the JSON ABI risk; covered
    // by `#[serde(default, skip_serializing_if = "…")]` on every
    // additive slot. See `crates/lazuli_ir/src/lib.rs`.
    assert_eq!(LZIR_SCHEMA, "0.15.0");
}
