//! Shared fix-action library used by `lazuli fix` (CLI) and the LSP
//! `code_action` handler.
//!
//! Wave 2.3 of the TDD/BDD-first proposal (2026-05-23): until now, every
//! LSP quick-fix was hardcoded inside `crates/lazuli_lsp/src/lib.rs` with no
//! CLI counterpart. This crate is the canonical home for fix builders so
//! both surfaces invoke identical logic.
//!
//! The proposal's acceptance bar accepts this PR as PARTIAL: the crate
//! skeleton, the trait surface, and two extracted actions (insert tests
//! block + scaffold errors block) ship here as proof-of-concept. The full
//! migration of all 12 LSP actions is a follow-up cell.

pub mod actions;
pub mod registry;
pub mod request;
pub mod result;

pub use registry::FixRegistry;
pub use request::FixRequest;
pub use result::{FixOutcome, FixResult};

/// Public entry point — CLI and LSP both call this. The default registry is
/// constructed lazily and includes every action exported from
/// `actions::*`.
pub fn execute(request: &FixRequest) -> anyhow::Result<FixResult> {
    let registry = FixRegistry::default();
    registry.execute(request)
}

/// Preview the fix without writing to disk. CLI invokes this when `--apply`
/// is absent; LSP uses it to populate the `WorkspaceEdit` it returns to the
/// editor.
pub fn preview(request: &FixRequest) -> anyhow::Result<FixResult> {
    let mut request = request.clone();
    request.apply = false;
    let registry = FixRegistry::default();
    registry.execute(&request)
}
