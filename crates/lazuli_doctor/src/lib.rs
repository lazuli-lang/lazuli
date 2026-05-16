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

pub mod correctness;
pub mod cross_feature;
pub mod design;
pub mod domain;
pub mod encryption;
pub mod lifecycle;
pub mod poller;
pub mod report;
pub mod vocab;
