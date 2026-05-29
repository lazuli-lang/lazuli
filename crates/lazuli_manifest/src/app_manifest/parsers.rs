//! Shared line-level parsers and predicates for the `app_manifest` sub-tree.
//!
//! Every entry point in this module (`parse_app_manifest`, `parse_app_registry`,
//! `parse_app_workspace`, `parse_app_contracts`, `parse_app_profiles`) walks
//! source line by line, dispatches on indent depth, and then matches headers
//! and field shapes. The leaf-level recognizers — `is_identifier`,
//! `is_type_name`, `unquote`, `leading_spaces`, `split_items`,
//! `parse_quoted_prefix`, the child-block dispatchers (`app_child`,
//! `registry_child`, `workspace_child`, `profile_child`), and the per-shape
//! field parsers (`parse_contract_field`, `parse_app_env_var`,
//! `parse_app_pack_use`, etc.) — are pulled here so each entry-point file can
//! stay focused on its own block-level state machine.
//!
//! Everything in this file is intentionally `pub(super)` (not `pub`): the
//! helpers are internal to the `app_manifest` sub-tree. The crate boundary
//! only sees the five `parse_app_*` entry points re-exported from `mod.rs`.
//!
//! Implementation layout — the helpers live in concern-specific
//! sibling files (`parsers_common`, `parsers_workspace`,
//! `parsers_contracts`, `parsers_app`, `parsers_registry`,
//! `parsers_profiles`) so each file stays small. This module is a
//! flat re-exporter so existing `use super::parsers::{...}` imports
//! across the `app_manifest/` sub-tree keep compiling verbatim — no
//! callsite touched the day of the split.
//!
//! See: `lazuli_ir::nodes::app_manifest`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

pub(super) use super::parsers_app::*;
pub(super) use super::parsers_common::*;
pub(super) use super::parsers_contracts::*;
pub(super) use super::parsers_profiles::*;
pub(super) use super::parsers_registry::*;
pub(super) use super::parsers_workspace::*;
