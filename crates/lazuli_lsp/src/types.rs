//! LSP-side type definitions shared across handlers, diagnostics, and code
//! action producers.
//!
//! ## `SecurityProfile` now lives in `lazuli_doctor_config`
//!
//! The 3-value profile enum was relocated to `lazuli_doctor_config` as
//! `DoctorProfile` so that the single doctor severity resolver
//! (`lazuli_doctor_config::effective_severity`) can be shared by both the
//! CLI doctor and the LSP without an inverted `lazuli_cli -> lazuli_lsp`
//! dependency. `lazuli_lsp` re-exports it as `SecurityProfile` so the
//! documented ABI (the canonical pilot VSCode extension and
//! `lazuli_cli::doctor` import `lazuli_lsp::SecurityProfile`) is
//! preserved byte-for-byte — the variants (`Prototype` / `Strict` /
//! `Production`) are identical.
//!
//! ## Why these types live here (and not in `lazuli_ir`)
//!
//! The remaining LSP-side types describe the *server's* view of the
//! world — diagnostic data payloads carried in `Diagnostic.data` — not
//! the IR's view of the source tree. Putting them in `lazuli_ir` would
//! force every IR consumer (codegen, doctor CLI, packs) to depend on
//! `tower-lsp` transitively through type definitions they never touch.

/// Selector for which subset of diagnostic codes the LSP backend emits.
///
/// Relocated to `lazuli_doctor_config::DoctorProfile`; re-exported here
/// as `SecurityProfile` for ABI stability. The default for
/// `diagnostics_for_source` is `Strict`. `Prototype` down-grades
/// production-only codes; `Production` upgrades a handful of warnings to
/// errors. The CLI `doctor` reads this off the project profile.
pub use lazuli_doctor_config::DoctorProfile as SecurityProfile;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_distinct_profiles() {
        // Smoke test: the three profile variants are not aliases. If a
        // future merge accidentally collapses two variants this catches
        // it immediately.
        assert_ne!(SecurityProfile::Prototype, SecurityProfile::Strict);
        assert_ne!(SecurityProfile::Strict, SecurityProfile::Production);
        assert_ne!(SecurityProfile::Prototype, SecurityProfile::Production);
    }
}
