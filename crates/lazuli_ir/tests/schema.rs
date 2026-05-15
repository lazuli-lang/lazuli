use lazuli_ir::LZIR_SCHEMA;

#[test]
fn lzir_schema_constant_is_0_14_0() {
    // CL.C.4 — bumped from 0.13.0 by the additive `Field.slug`,
    // `Resource.invariants`, `Feature.aggregates` slots + new
    // `Aggregate` and `Invariant` types. See `crates/lazuli_ir/src/lib.rs`.
    assert_eq!(LZIR_SCHEMA, "0.14.0");
}
