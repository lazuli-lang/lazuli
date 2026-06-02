//! Per-construct LSP completion providers.
//!
//! Each sub-module owns the completion provider for one Lazuli
//! construct family. They are surfaced through `lib.rs` via
//! `pub(crate) use completion::<name>::*;` re-exports so existing
//! `crate::<fn>` paths (and external `lazuli_lsp::<fn>` paths for
//! `pub fn` entry points) keep working unchanged.
//!
//! Mirrors `diagnostics/` and `code_actions/`: small, Rails-style
//! files anchored to one construct each.

pub(crate) mod auth_refresh;
pub(crate) mod cap_file;
pub(crate) mod context;
pub(crate) mod error_page;
pub(crate) mod error_vocab;
pub(crate) mod input_field;
pub(crate) mod namespace;
pub(crate) mod owner_axis;
pub(crate) mod transition;
