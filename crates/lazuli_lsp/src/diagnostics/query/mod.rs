//! Diagnostics for the `query.*` family (list / lookup / sql / view).
//!
//! Largest single Lazuli vocabulary surface in canonical source — every
//! read path is declared here. Sub-concerns split per shape:
//!
//! | Module | Concern |
//! |---|---|
//! | [`mode`] | every query declaration uses an explicit mode (`query.list`, `query.lookup`, `query.sql`, `query.view`); legacy bare `query` is rejected. |
//! | [`previously`] | `previously migrated|alias` must follow the right shape per scope (field / header / transition / other). |
//! | [`list`] | `query.list` defaults (order, paginate, filter-index, search syntax, `active_sessions` temporal guard). |
//! | [`lookup`] | single-key `query.lookup` shorthand + `typed_param` / `lookup_key_assignment` parsers. |
//!
//! `is_identifier` / `is_type_name` live in `lib.rs` since 40+ other
//! catalogs depend on them; `typed_param` lives under `lookup` here
//! and is re-exported because other catalogs (`command`, `event`,
//! `security`) consume it through the crate-root glob.

mod list;
mod lookup;
mod mode;
mod previously;

#[allow(unused_imports)]
pub(crate) use list::{
    active_session_query_diagnostics, active_session_query_facts_diagnostics,
    filter_index_field, generated_query_filter_indexes, normalize_index_value,
    query_block_filter_index_fields, query_block_has_scope_override,
    query_filter_index_diagnostics, query_order_default_diagnostics,
    query_pagination_diagnostics, query_search_syntax_diagnostics, single_tenancy_axis,
    ActiveSessionQueryFacts,
};
#[allow(unused_imports)]
pub(crate) use lookup::{
    lookup_key_assignment, lookup_query_diagnostics, lookup_shorthand_diagnostics, typed_param,
    LookupQueryFacts,
};
#[allow(unused_imports)]
pub(crate) use mode::query_mode_diagnostics;
#[allow(unused_imports)]
pub(crate) use previously::{
    inline_previously_kind, previously_mode_diagnostics, InlinePreviouslyKind,
};
