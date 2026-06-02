//! .lzi + .lzx source-text parser — thin router.
//!
//! Wave 4.4 of the rails-style refactor split the original 23k-LOC
//! `parser.rs` into per-source-kind modules so future contributors
//! don't bisect a single megafile when tracking down a parse rule.
//! Stage 2 of W4.4 R2 carved the `.lzx` dialects into `lzx.rs`;
//! stage 3 carved the `.lzi` parsers into `lzi.rs`. This module is
//! now a re-export router only — nothing parses here.
//!
//! ## What lives where
//!
//! - `error.rs` — the `ParseError` envelope returned by every entry
//!   point. Spans for Pest-flavour errors are authored-side; analyzer
//!   diagnostics upgrade those into coded findings downstream.
//! - `common.rs` — shared leaf mechanics: `SourceLine`, `source_lines`,
//!   trivia / inline-comment skippers, depth- and quote-aware token
//!   scanners, the `line_error` constructors, ident validators. Every
//!   parser walks the same line-stream contract, so these helpers stay
//!   crate-private.
//! - `lzx.rs` — both `.lzx` dialects (app-surface + feature-surface):
//!   `parse_lzx_document` for `app.lzx` and `parse_surface_document`
//!   for `features/<feat>/<feat>.{web,mobile}.lzx`. Also owns the
//!   shared policy expression machinery (`try_parse_policy_expr`,
//!   `looks_like_policy_expr`, `parse_policy_atom`, `PolicyExprParser`)
//!   consumed by the `.lzi` parsers.
//! - `lzi.rs` — every `.lzi` parser: `parse_feature_skeletons`,
//!   resources / commands / queries / jobs / webhooks / agents /
//!   notifications / pollers / events / translations / RBAC catalog,
//!   plus the design / plan / package skeleton parsers. Also hosts the
//!   `Lifecycle*` and `Poller*` AST struct types that re-export through
//!   `lazuli_syntax::*`.
//!
//! ## Cross-module bridges
//!
//! - `parse_invalidates_entry` and `parse_translation_key_token` live
//!   in `lzi.rs` and carry `pub(super)` so `lzx.rs`'s
//!   `parse_on_success_block` can call them.
//! - `try_parse_policy_expr` / `looks_like_policy_expr` /
//!   `parse_policy_atom` live in `lzx.rs` and carry `pub(super)` so
//!   every `.lzi` `policy <expr>` parser can call them.
//! - `parse_invariant_form` lives in `lzi/helpers.rs` and is `pub(crate)`;
//!   the lifecycle parser calls it to reject unknown invariant forms at
//!   parse time (closed catalog per `docs/proposals/lifecycle-vocab.md` §3.4).
//!
//! ## Public ABI
//!
//! The crate root (`crates/lazuli_syntax/src/lib.rs`) re-exports the
//! public surface area: the seven `parse_*` entry points, the
//! `ParseError` envelope, and the `Lifecycle*` / `Poller*` AST types.
//! Everything else stays crate-private.
//!
//! ## See also
//!
//! - `docs/canonical-semantics.md` — the source-of-truth grammar
//!   reference these parsers implement.
//! - `lazuli_ir::nodes` — the typed IR shapes the analyzer lowers
//!   parser AST into.

mod common;
mod error;
mod highlight;
mod lzi;
mod lzx;

pub use error::ParseError;
pub use highlight::{ClassifiedToken, classify_tokens};
/// Spec 0028 — `@doctor.allow(CODE, reason: "...")` waiver recognizers, shared
/// by the parser-capture path, the `lazuli_doctor` string scanner, and the
/// migration codemod.
pub use lzi::doctor_allow;
pub use lzi::{
    LifecycleBlockAst, LifecycleInvariantAst, LifecycleInvariantForm, LifecycleStateAst,
    LifecycleTransitionAst, PollerBlockAst, PollerCursorAst, PollerRetryAst, PollerRetryQuirkAst,
    PollerStateAst, PollerTickAst, parse_design_document, parse_feature_gates,
    parse_feature_skeletons, parse_package_skeleton, parse_plan_blocks,
};
pub use lzx::{parse_lzx_document, parse_surface_document};
