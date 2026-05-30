//! Surface AST — modern hand-written mirror for the per-target lzx
//! ViewModel files (`features/<feat>/<feat>.{web,mobile}.lzx`).
//!
//! Reference: `docs/proposals/lzx-integration-codegen.md` §5 (closed
//! keyword catalog) + §5.1 (per-view-kind matrix). Field-level type
//! references are kept as raw text; the analyzer lifts them to `ir::*`
//! in `lower_surface`. The indentation-based parser populates this via
//! `parse_surface_decl`.
//!
//! The closed view-kind catalog (`ViewAst`) is `List | Detail | Create`.
//! Each shape has its own struct so the analyzer can switch on kind at
//! lowering time without re-walking the AST. The catalog is intentionally
//! small — adding a view kind is an IR + analyzer change requiring a
//! proposal.
//!
//! Authoring shape (excerpt):
//!
//! ```text
//! surface customer_management web
//!   uses feature customer
//!   audience admin
//!     requires @scope.admin
//!     view list customers
//!       source customer.query.list
//!       route "/customers"
//!       columns name, email, owner
//!       filters
//!         tier: @semantic.CustomerTier multi url_sync
//!         status: @semantic.CustomerStatus single
//!       search columns name, email
//!       sort
//!         allowed name, email, created_at
//!         default created_at desc
//!       selection multi
//!         bulk_actions assign_owner, archive
//!       settings
//!         columns: enum [name, email, owner] default name persistence local
//!       drawer
//!         trigger select
//!         source customer.query.lookup.by_id
//!         sections summary
//!         actions edit_customer
//!       cells owner @client.OwnerCell
//!       actions create_customer
//! ```
//!
//! `RouteParamAst` doubles as the typed-route-param surface for both
//! lzx routes and view-detail/view-create headers (`route id: ID from
//! path`). It's exported from this file because it lives closest to the
//! views that consume it.

use serde::{Deserialize, Serialize};

use super::{InvalidatesDecl, PolicyAtomAst, Span, TranslationKeyRefAst};

include!("surface_p1.rs");
include!("surface_p2.rs");
