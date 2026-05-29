//! Freshness gate: the committed `#kw-*` keyword-alternation section of the
//! VS Code grammar MUST be byte-identical to what `gen-tmlanguage` produces
//! from `lazuli_keywords::ALL`. A hand-edit of a generated rule, or a registry
//! change without a regen, fails here.
//!
//! Equivalent to `cargo xtask gen-tmlanguage --check`, but wired into
//! `cargo test --workspace` so the whole repo gate catches drift.

#[test]
fn kw_section_is_fresh() {
    if let Err(e) = xtask::tmlanguage::run(true) {
        panic!("tmLanguage #kw-* section is stale — run `cargo xtask gen-tmlanguage`.\n{e}");
    }
}
