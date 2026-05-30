//! Lazuli doctor — file-local diagnostic checks.
//!
//! Extracted from `lazuli_cli` on 2026-05-15 so the LSP can import these
//! checks and emit them as live squiggles, not just on `lazuli doctor` CLI
//! runs. This crate intentionally only contains file-local, portable
//! diagnostics — the full doctor pipeline (cross-file checks, manifest
//! enforcement, registry parsing, etc.) stays in `lazuli_cli::doctor` and
//! re-exports these sub-modules so existing call sites keep working.
//!
//! See `editors/vscode/AUDIT-LSP-DOCTOR-GAP.md` for the audit that
//! motivated the extraction (R2.F).
//!
//! `lzx/` is intentionally NOT here yet — it still couples to a private
//! `ir_stub::Feature` shape that needs an adapter; cells A.1/A.2/A.3 retire
//! that stub, after which `lzx/` can move too.

// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
pub mod allow_comment;
pub mod config_noise;
pub mod correctness;
pub mod coverage;
pub mod cross_feature;
pub mod design;
pub mod domain;
pub mod encryption;
pub mod error_handling;
pub mod error_vocab;
pub mod handler_path;
pub mod handler_walker;
pub mod internal_hygiene;
pub mod lifecycle;
pub mod lzi_hygiene;
pub mod poller;
pub mod report;
pub mod route_guard;
pub mod rule_category;
pub mod severity;
pub mod test_discipline;
pub mod vocab;

pub use rule_category::RuleCategory;
pub use severity::DoctorSeverity;
