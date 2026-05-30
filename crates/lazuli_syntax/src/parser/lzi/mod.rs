//! `.lzi` source-text parser — every declarative slot a feature file
//! can carry: resources, commands, queries, jobs, webhooks, agents,
//! notifications, pollers, events, translations, defaults, RBAC
//! catalog, plus the design / plan / package-skeleton top-level parsers
//! that share the same line-stream contract.
//!
//! ## Entry points (public ABI)
//!
//! - `parse_feature_skeletons` — every `feature <name>` block in a
//!   `.lzi` source, expanded to typed children. The workhorse — called
//!   by the analyzer, doctor, LSP, and codegen.
//! - `parse_design_document` — `design.lzi` (color tokens, typography,
//!   shadows, motion, custom tokens).
//! - `parse_plan_blocks` — top-level `plan <name>` blocks.
//! - `parse_feature_gates` — top-level `gate` directives.
//! - `parse_package_skeleton` — top-level RBAC catalog
//!   (`permission` + `role` + `grants`).
//!
//! ## What's NOT here
//!
//! - `.lzx` files are in `lzx.rs` (both app-surface and feature-surface
//!   dialects). The bridge helpers `looks_like_policy_expr` /
//!   `try_parse_policy_expr` / `parse_policy_atom` live in `lzx.rs` as
//!   `pub(super)` items consumed by every `policy <expr>` parser here.
//! - Shared leaf mechanics (`SourceLine`, `source_lines`, error
//!   constructors, ident validators, depth-aware scanners) are in
//!   `common.rs`.
//! - The `ParseError` envelope is in `error.rs`.
//!
//! ## Cross-module bridges
//!
//! - `parse_invalidates_entry` and `parse_translation_key_token` are
//!   `pub(super)` so `lzx.rs`'s `parse_on_success_block` can call them.
//! - `parse_invariant_form` / `InvariantForm` live in `helpers.rs` and
//!   are `pub(crate)`; the lifecycle parser calls them at parse time to
//!   enforce the closed catalog from `docs/proposals/lifecycle-vocab.md`
//!   §3.4 — no silent coercion downstream.
//!
//! ## Grammar source-of-truth
//!
//! Hand-rolled two-space indentation. No Pest grammar — each parser IS
//! the spec. `docs/canonical-semantics.md` is the prose reference.

mod agent;
mod api;
mod auth;
pub mod cache;
mod command;
mod defaults;
pub mod design;
mod enums;
pub mod event;
mod feature_errors;
mod feature_prelude;
mod field_constraints;
mod helpers;
mod iron_hand_context;
mod job;
mod lifecycle;
mod locale;
pub mod mcp;
pub mod notification;
mod numerics;
pub mod package;
pub mod plan;
mod policy;
mod poller;
mod query;
pub mod record;
pub mod report;
mod resource;
pub mod translation;
pub mod types;
mod webhook;

#[cfg(test)]
mod lifecycle_emits_split_tests;

// Hand-rolled line-walker for `feature <name>` blocks. Lives in a sibling
// so this module root stays thin; it dispatches to every other parser in
// `lzi/*.rs`.
mod feature_walker;

pub use feature_walker::parse_feature_skeletons;
pub(super) use feature_walker::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD,
    AGENT_INDENT_GREAT_GRANDCHILD,
};

pub(super) use helpers::{
    is_policy_identifier, parse_named_args, split_call_signature, split_first_token,
    split_top_level_commas, take_identifier, take_quoted_string,
};

pub(super) use command::parse_invalidates_entry;
pub(super) use defaults::parse_defaults_tenancy;
pub(super) use translation::parse_translation_key_token;

// `parse_resource_field_decl` is re-exported here (not `pub(super)`)
// because the only outside caller is `lzi/record.rs`, which reaches it
// via `super::parse_resource_field_decl` — i.e., from inside `lzi`,
// where private items are already visible.
use resource::parse_resource_field_decl;
// `fold_rate_limit_line` / `parse_rate_limit_line_body` flow through lzi's
// namespace so siblings like `report.rs` can write `super::*` instead of
// pulling them directly from `numerics`.
use numerics::{fold_rate_limit_line, parse_rate_limit_line_body};

pub use design::parse_design_document;
pub use package::parse_package_skeleton;
pub use plan::{parse_feature_gates, parse_plan_blocks};
pub use types::{
    LifecycleBlockAst, LifecycleInvariantAst, LifecycleInvariantForm, LifecycleStateAst,
    LifecycleTransitionAst, PollerBlockAst, PollerCursorAst, PollerRetryAst, PollerRetryQuirkAst,
    PollerStateAst, PollerTickAst,
};
