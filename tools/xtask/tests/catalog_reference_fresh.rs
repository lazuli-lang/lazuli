//! Freshness gate: the committed `docs/closed-catalogs.md` MUST be
//! byte-identical (modulo line endings) to what `gen-catalog-reference`
//! produces from the `lazuli_keywords` closed catalogs. A hand-edit of the
//! generated doc, or a catalog change without a regen, fails here — so the
//! doc can never drift from the single source.
//!
//! Equivalent to `cargo run -p xtask -- gen-catalog-reference --check`, wired
//! into `cargo test --workspace`.

#[test]
fn catalog_reference_is_fresh() {
    if let Err(e) = xtask::catalog_reference::run(true) {
        panic!(
            "docs/closed-catalogs.md is stale — run \
             `cargo run -p xtask -- gen-catalog-reference`.\n{e}"
        );
    }
}
