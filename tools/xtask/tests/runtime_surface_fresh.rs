//! Freshness gate: the committed `docs/lazuli_way/runtime-surface.md` MUST be
//! byte-identical (modulo line endings) to what `gen-runtime-surface` produces
//! from `runtime/go/lazuli/**/*.go`. A hand-edit of the generated doc, or a
//! runtime surface change without a regen, fails here.
//!
//! Equivalent to `cargo run -p xtask -- gen-runtime-surface --check`, but wired
//! into `cargo test --workspace` so the whole-repo gate catches drift. This is
//! the twin of `keyword_reference_is_fresh`.

#[test]
fn runtime_surface_is_fresh() {
    if let Err(e) = xtask::runtime_surface::run(true) {
        panic!(
            "docs/lazuli_way/runtime-surface.md is stale — run \
             `cargo run -p xtask -- gen-runtime-surface`.\n{e}"
        );
    }
}
