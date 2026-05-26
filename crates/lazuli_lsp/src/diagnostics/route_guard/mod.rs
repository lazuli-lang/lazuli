//! Diagnostics, completions, hovers, and code-action backends for the
//! IR Route-Guards family (`policy`, `on_unauthenticated`,
//! `on_unauthorized`, `default_policy`, `default_*_redirect`).
//!
//! Mirror structure of `diagnostics/lifecycle/`: the language-layer
//! support for `route_guard` lives here, and the code-action frontend
//! in `code_actions/route_guard.rs` calls into these helpers via the
//! `pub(crate) use diagnostics::route_guard::*;` re-export in lib.rs.
//!
//! Two public entry points are surfaced to `lib.rs::server`:
//! [`route_guard_completions`] and [`route_guard_hover`]. The rest are
//! `pub(crate)`.
//!
//! Sub-concerns:
//!
//! | Module | Concern |
//! |---|---|
//! | [`context`] | Block-scope facts (`enclosing_view_block`, `app_route_guard_block`, …) used by every other layer. |
//! | [`completions`] | `route_guard_completions` + item builders. |
//! | [`hover`] | `route_guard_hover` + `@policy.<name>` atom resolution + backend-alignment lookup. |
//!
//! Shared helpers (`block_kind_at`, `feature_name`, `leading_spaces`,
//! `position_at_line_start`, `simple_edit_action`,
//! `collect_policy_categories_for_feature`) stay in lib.rs because
//! they're consumed by sibling modules.

mod completions;
mod context;
mod hover;

#[allow(unused_imports)]
pub use completions::route_guard_completions;
#[allow(unused_imports)]
pub(crate) use completions::{
    policy_ref_completion_items, query_ref_completion_items, redirect_trigger_has_open_quote,
    route_guard_default_clause_completion_items, route_guard_redirect_path_trigger,
    route_path_completion_items, snippet_completion, ROUTE_GUARD_DEFAULT_CLAUSES,
};
#[allow(unused_imports)]
pub(crate) use context::{
    app_route_guard_block, at_app_child_completion_line, collect_route_paths,
    enclosing_audience_block, enclosing_named_block, enclosing_view_block, find_block_end,
    first_quoted_value, in_app_body_context, in_app_route_guard_block,
    in_guard_policy_child_context, in_view_or_audience_guard_context, route_guard_context_feature,
    RouteGuardBlock, RouteGuardViewBlock,
};
#[allow(unused_imports)]
pub use hover::route_guard_hover;
#[allow(unused_imports)]
pub(crate) use hover::{
    backend_policy_for_ref, hosted_backend_refs_for_view, resolve_policy_atoms,
    route_guard_backend_alignment, route_guard_policy_ref_hover,
};
