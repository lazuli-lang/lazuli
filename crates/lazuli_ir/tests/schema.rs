use lazuli_ir::LZIR_SCHEMA;

#[test]
fn lzir_schema_constant_is_0_12_0() {
    assert_eq!(LZIR_SCHEMA, "0.12.0");
}
