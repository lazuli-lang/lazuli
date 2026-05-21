use lazuli_ir::LZIR_SCHEMA;

#[test]
fn lzir_schema_constant_is_0_16_0() {
    // `ir-rate-limit-env-aware` cell 1 — bumped from 0.15.0 by the
    // shape change on the `rate_limit` slot of `Command`, `Api`,
    // `Agent`, `Report`, and `AuthPassword`. The slot's JSON
    // representation moves from a bare string (`"5 per 10 minutes
    // per ip"`) to an object (`{"default": "5 per 10 minutes per ip",
    // "by_env": []}`). The source-level shape `rate_limit "X"` keeps
    // 100% back-compat — the parser lowers it to `RateLimitSpec {
    // default: "X", by_env: [] }` (see proposal §4.1 + §8). Consumers
    // of the IR JSON must read the object form.
    assert_eq!(LZIR_SCHEMA, "0.16.0");
}
