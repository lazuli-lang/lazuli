//! Per-catalog diagnostic producers.
//!
//! The LSP dispatcher (`crate::dispatch::diagnostics_for_with_profile_inner`)
//! fans a single source string across ~70 catalog-specific producer
//! functions. Each producer owns one Lazuli construct family — `agent`,
//! `event`, `query.*`, `cache`, `webhook`, `auth`, `policy`, etc. — and
//! returns a `Vec<Diagnostic>` for that family alone.
//!
//! Rails-style layout: one file per catalog. The shape per file is:
//!
//! ```text
//! //! Diagnostics for the `<catalog>` family.
//! use tower_lsp::lsp_types::{Diagnostic, ...};
//! use crate::{...shared helpers...};
//!
//! pub(crate) fn <catalog>_<concern>_diagnostics(source: &str) -> Vec<Diagnostic> {
//!     ...
//! }
//! ```
//!
//! ## ABI contract
//!
//! Every `pub(crate) fn ..._diagnostics(...)` here is re-exported at the
//! crate root via `pub(crate) use diagnostics::<module>::*;` in `lib.rs`,
//! so the dispatcher and out-of-process consumers keep their existing
//! `crate::<fn>` paths. Moving a producer to its own module is strictly
//! additive — no public API change.
//!
//! ## Shared helpers
//!
//! Tiny helpers used by 2+ catalogs (`is_identifier`, `closest_kind`,
//! `unquote_lzx_literal`, …) stay in `lib.rs` with `pub(crate)` visibility
//! and are imported by each catalog module. Catalog-private helpers stay
//! inside their catalog file.
//!
//! ## See also
//! * `crate::dispatch` — the flat `.extend(...)` registry that calls
//!   into each catalog producer.
//! * `crate::catalogs` — the closed lists of legal kind/keyword tokens
//!   (e.g. `FEATURE_BODY_KINDS`) consumed by the producers below.
//! * `crate::code_actions` — code-action providers; structurally
//!   parallel to this module (one file per family).

pub(crate) mod agent;
pub(crate) mod api;
pub(crate) mod auth;
pub(crate) mod cache;
pub(crate) mod crypto;
pub(crate) mod error;
pub(crate) mod http_headers;
pub(crate) mod notification;
pub(crate) mod policy;
pub(crate) mod query;
pub(crate) mod webhook;
