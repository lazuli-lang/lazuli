//! LSP-side type definitions shared across handlers, diagnostics, and code
//! action producers.
//!
//! ## Why this lives here (and not in `lazuli_ir`)
//!
//! These types describe the *LSP server's* view of the world — diagnostic
//! data payloads carried in `Diagnostic.data`, security-profile selectors
//! that gate which diagnostic codes fire — not the IR's view of the source
//! tree. Putting them in `lazuli_ir` would force every IR consumer
//! (codegen, doctor CLI, packs) to depend on `tower-lsp` transitively
//! through type definitions they never touch.
//!
//! ## ABI guarantee
//!
//! `SecurityProfile` is re-exported from the crate root via `pub use
//! types::SecurityProfile;` so external consumers (notably the the canonical pilot
//! VSCode extension and `lazuli_cli::doctor`) continue to import it as
//! `lazuli_lsp::SecurityProfile` exactly as before.

/// Selector for which subset of diagnostic codes the LSP backend emits.
///
/// The default for `diagnostics_for_source` is `Strict`. `Prototype`
/// down-grades production-only codes; `Production` upgrades a handful of
/// warnings to errors. The CLI `doctor` reads this off the project
/// profile in `profiles.lzi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityProfile {
    /// Pre-production sandbox. Production-only codes are demoted so
    /// scaffolds can iterate without churn from rules that don't apply
    /// until deploy.
    Prototype,
    /// Default. Every catalog code fires at its declared severity.
    Strict,
    /// Production lock-in. A handful of warnings escalate to errors so
    /// `doctor` blocks deploy on weak postures (missing redact, open
    /// CORS, etc.).
    Production,
}

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
