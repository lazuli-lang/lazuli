//! `lazuli_doctor_run` — the package-wide `lazuli doctor` orchestration
//! engine, lifted out of `lazuli_cli` (Wave D3) so the LSP can run the
//! same engine in-editor with full Layer-2 parity.
//!
//! This crate owns the **package** run: load every `.lzi` / `.lzx`
//! source, lift the Tier-3 fact bundle (+ auth / plan-gate facts), fan
//! out every cross-feature / project-level aggregator, and return the
//! sorted + suppressed diagnostic stream. It deliberately does **not**
//! depend on `lazuli_cli` or `lazuli_lsp`:
//!
//! - The **file-local** "shape" diagnostics (the LSP's
//!   `diagnostics_for_source_with_profile`) are **injected at the
//!   boundary** via the [`FileLocalInjector`] closure passed to
//!   [`run_package`] — the engine never imports the provider.
//! - Package-level findings that need the CLI's TS-codegen module loader
//!   (today: `SCHEMA-RICH-001`) are computed by the caller and passed in
//!   as the `injected` argument, folded into the same sorted stream.
//!
//! The CLI keeps the formatting / exit-code / `--coverage` / `--fail-on`
//! / `--check-release` / `--self` layer on top.

// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
mod doctor;

// `folder::vocab_client_src_001` is consumed by the CLI's frontend
// scaffold invariants test (`crate::doctor::folder::…`), re-exported here
// so that call path keeps resolving through the CLI's thin shim.
pub use doctor::{
    DoctorConstruct, DoctorDiagnostic, DoctorFix, DoctorPackage, DoctorSeverity, FileLocalInjector,
    folder, run_package,
};

/// Helpers reused by the CLI's `lazuli doctor` entry layer (`--self`,
/// `--check-release`) which stays in `lazuli_cli`. These are doctor
/// internals, not part of the engine's main `run_package` contract, but
/// the two CLI-only sub-commands build [`DoctorDiagnostic`] rows by hand
/// and reach for the same project-root / version / recipe-walk helpers
/// the engine uses, so they're surfaced here rather than duplicated.
pub mod entry_support {
    pub use crate::doctor::{
        collect_recipe_dirs, doctor_project_root, extract_lzir_schema, major_minor,
        resolve_error_handling_severity, resolve_internal_hygiene_severity,
    };
}
