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

/// Public entry point — CLI and LSP both call this.
///
/// Constructs the default [`FixRegistry`] (every action exported from
/// `actions::*`) and dispatches on `request.rule`. Honors
/// `request.apply` — see [`preview`] for a forced no-write variant.
///
/// ## Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use lazuli_fix::{execute, FixRequest};
///
/// let request = FixRequest {
///     rule: "TEST-MISSING-AUTHORED-001".to_owned(),
///     path: PathBuf::from("examples/full-capsule/full-capsule.lzi"),
///     line: 12,
///     column: 1,
///     apply: false,
/// };
/// let result = execute(&request).expect("execute returns FixResult");
/// println!("{}", result.preview);
/// ```
pub fn execute(request: &FixRequest) -> anyhow::Result<FixResult> {
    let registry = FixRegistry::default();
    registry.execute(request)
}

/// Preview the fix without writing to disk.
///
/// Forces `apply = false` regardless of the caller's request. CLI calls
/// this when `--apply` is absent; LSP uses it to populate the
/// `WorkspaceEdit` it returns to the editor.
///
/// ## Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use lazuli_fix::{preview, FixOutcome, FixRequest};
///
/// let request = FixRequest {
///     rule: "TEST-MISSING-AUTHORED-001".to_owned(),
///     path: PathBuf::from("examples/full-capsule/full-capsule.lzi"),
///     line: 12,
///     column: 1,
///     apply: true, // forced to false inside preview()
/// };
/// let result = preview(&request).expect("preview returns FixResult");
/// assert!(matches!(result.outcome, FixOutcome::Preview | FixOutcome::Skipped | FixOutcome::NoChange));
/// ```
pub fn preview(request: &FixRequest) -> anyhow::Result<FixResult> {
    let mut request = request.clone();
    request.apply = false;
    let registry = FixRegistry::default();
    registry.execute(&request)
}
