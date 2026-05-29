//! Freshness gate: the committed `docs/keyword-reference.md` MUST be
//! byte-identical (modulo line endings) to what `gen-keyword-reference`
//! produces from `lazuli_keywords::ALL`. A hand-edit of the generated doc, or
//! a registry change without a regen, fails here.
//!
//! Equivalent to `cargo run -p xtask -- gen-keyword-reference --check`, but
//! wired into `cargo test --workspace` so the whole-repo gate catches drift.

#[test]
fn keyword_reference_is_fresh() {
    if let Err(e) = xtask::keyword_reference::run(true) {
        panic!(
            "docs/keyword-reference.md is stale — run \
             `cargo run -p xtask -- gen-keyword-reference`.\n{e}"
        );
    }
}
