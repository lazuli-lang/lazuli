//! `.lzx` source-text parser — thin router over the per-construct sub-modules.
//!
//! ## What lives here
//!
//! - **`policy_expr`**: cross-dialect policy expression machinery
//!   (`parse_policy_atom`, `try_parse_policy_expr`,
//!   `looks_like_policy_expr`, `PolicyExprParser`). Bridges `.lzx`
//!   audience `requires` lines and every `.lzi` `policy` payload, so
//!   `try_parse_policy_expr` is re-exported as `pub(super)` to keep
//!   the `parser::lzx::try_parse_policy_expr` path stable for `lzi`.
//! - **`app`** — `.lzx` app dialect: `parse_lzx_document` (top-level
//!   entry), `parse_lzx_app`, `parse_lzx_view_guard` family,
//!   `parse_lzx_error_page`. The view-guard helpers carry `pub(super)`
//!   so the sibling `route` / `experience` parsers can call them.
//! - **`route`** — single `route <name>` block parser.
//! - **`experience`** — `experience` / `resume` / `view <name>` /
//!   `extends @anchor.*` / `surface <experience> web|mobile` /
//!   `audience` / platform-view family.
//! - **`surface`** — feature-surface dialect: `parse_surface_document`
//!   (entry point) plus the per-construct `view list|detail|create`,
//!   `drawer`, `filters`, `search`, `sort`, `settings`, `on_success`
//!   sub-parsers. Tests live in the sibling `surface_tests.rs` file.
//!
//! ## What stays out
//!
//! - The shared infra (`SourceLine`, error ctors, ident validators,
//!   token scanners) is in `common.rs`.
//! - The `.lzi` feature parsers live under `crate::parser::lzi`. They
//!   consume `try_parse_policy_expr` re-exported from this module.
//!
//! ## Grammar source-of-truth
//!
//! Hand-rolled two-space indentation. No Pest grammar — each
//! `parse_*` function IS the spec. The proposal references
//! `docs/proposals/lzx-integration-codegen.md` §5 for the
//! feature-surface dialect and `docs/canonical-semantics.md` for the
//! app-surface dialect.

pub mod policy_expr;

pub(super) use policy_expr::try_parse_policy_expr;

pub mod app;

pub use app::parse_lzx_document;

pub mod route;

use route::parse_lzx_route;

pub mod experience;

use experience::{parse_lzx_experience, parse_lzx_surface};

pub mod surface;

pub use surface::parse_surface_document;
