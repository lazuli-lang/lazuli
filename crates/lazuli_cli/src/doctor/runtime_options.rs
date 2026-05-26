//! Public CLI-surface runtime options for `lazuli doctor`.
//!
//! `DoctorRuntimeOptions` is the wire-level shape the CLI parser
//! (`main.rs`) and the watcher (`doctor_watch.rs`) populate when
//! routing flags from the `lazuli doctor` clap subcommand into the
//! doctor pipeline. Keeping it in its own file isolates the
//! "what flags exist?" surface from the dispatch logic in
//! `doctor/mod.rs` and from the per-feature aggregator dispatch in
//! `DoctorPackage::diagnostics`.
//!
//! Every field is `pub` because callers construct this struct
//! directly — there is no builder. Two waves of the doctor proposal
//! landed flags here:
//!
//!   - **Wave 2 (CLI surface)** — `format` (text/json/ndjson/auto)
//!     and `fail_on` (rule/category/severity gate).
//!   - **Wave 6 (coverage)** — `coverage` adds the per-layer
//!     coverage rollup to text output and the JSON envelope.
//!   - **W3 (`--self`)** — `self_audit` short-circuits the standard
//!     IR pipeline and walks `crates/lazuli_*/src/` for the framework's
//!     own `INTERNAL-*` rules.
//!
//! Re-exported from `doctor/mod.rs` as `crate::doctor::DoctorRuntimeOptions`
//! so the existing `pub use`-shaped call sites in `main.rs` and
//! `doctor_watch.rs` keep compiling unchanged.

/// Wave 2 (CLI surface) + Wave 6 (coverage) — agent-first CLI surface
/// runtime options. `format` selects text vs JSON output; `coverage`
/// emits the per-layer coverage report; `fail_on` composes the
/// non-zero exit gate.
#[derive(Debug, Clone, Default)]
pub struct DoctorRuntimeOptions {
    pub format: Option<String>,
    pub coverage: bool,
    pub fail_on: Vec<String>,
    /// W3 — when `true`, run the `internal_hygiene` rules (file size,
    /// undoc pub, no-example, test-pairing) against the framework's
    /// own Rust source at `crates/lazuli_*/src/`. The input path is
    /// treated as the workspace root.
    pub self_audit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_silent_text_no_coverage() {
        let opts = DoctorRuntimeOptions::default();
        assert!(opts.format.is_none());
        assert!(!opts.coverage);
        assert!(opts.fail_on.is_empty());
        assert!(!opts.self_audit);
    }
}
