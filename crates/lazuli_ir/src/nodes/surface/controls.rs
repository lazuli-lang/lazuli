//! Surface controls — search, filter, sort, selection.
//!
//! Each control is its own typed shape — search is not a free-form string,
//! filter is not a duck-typed property, selection isn't "anything goes."
//! That tightness is what lets the LLM compose the whole surface cold from
//! a one-line spec.
//!
//! ## Closed-catalog discipline
//!
//! Several enums in this family are intentionally narrow: [`SelectionMode`]
//! (`none / single / multi`), [`SortDir`], [`SearchMode`] (`columns /
//! segmented`), [`FilterCardinality`] (`single / multi`). Doctor enforces
//! every catalog with proposal-level cost to extend.

use serde::{Deserialize, Serialize};

use super::core::CommandRef;
use crate::SpanRef;

/// `filter <name>: <Type> [single|multi] from query` declaration.
/// Surfaces one filter control on a list view; the `url_sync` flag
/// controls whether filter state is reflected in the URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterDecl {
    pub name: String,
    /// Raw type label as authored (e.g. `ItemType`, `Text`). Resolution
    /// to a concrete enum-on-resource or scalar happens in lowering.
    pub type_ref: String,
    pub cardinality: FilterCardinality,
    /// `from query` flag: true if filter state syncs to URL params.
    pub url_sync: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog distinguishing single-value, multi-value, and
/// date-range filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterCardinality {
    /// One selected value at a time.
    Single,
    /// Multiple values selected concurrently.
    Multi,
    /// Paired from/to date picker (GAP-UX-07). Lowers to two query params
    /// `<name>_from` / `<name>_to`; the backing field must be Date / DateTime.
    DateRange,
}

/// `search { ... }` declaration on a list view. Mode picks the UI shape
/// (one input across columns, or a segmented per-column control);
/// `fields` lists the searchable columns; `free_text_target` binds a
/// global free-text input to a specific filter or input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchDecl {
    pub mode: SearchMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SearchField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_text_target: Option<BindingRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of search UI shapes. `Columns` lists explicit
/// searchable columns; `Segmented` shows a per-column segmented control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchMode {
    /// v1 behavior already represented by `ViewList.search` today.
    Columns { columns: Vec<String> },
    /// Per-column segmented search control.
    Segmented,
}

/// One entry inside [`SearchDecl::fields`] — names the search key and
/// binds it to a typed target (a filter or source input).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchField {
    pub key: String,
    pub binds_to: BindingRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of binding targets a search/filter control can point at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BindingRef {
    /// `filters.<name>`.
    Filter { name: String },
    /// `source.<input-name>`.
    SourceInput { name: String },
    /// `selection` in single-selection mode.
    SelectionScalar,
}

/// `sort { allowed = [...], default = <field> asc|desc }` declaration.
/// Closed list of orderable columns + the initial state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortDecl {
    pub allowed: Vec<String>,
    pub default_field: String,
    pub default_dir: SortDir,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of sort directions on a [`SortDecl`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    /// Ascending (A → Z, 1 → 9).
    Asc,
    /// Descending (Z → A, 9 → 1).
    Desc,
}

/// `selection { mode = ..., bulk_actions = [...] }` declaration.
/// Names the selection cardinality and the commands invokable on the
/// selected row set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionDecl {
    pub mode: SelectionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bulk_actions: Vec<CommandRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of selection cardinalities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    /// Selection disabled.
    None,
    /// Exactly one row may be selected.
    Single,
    /// Multiple rows may be selected.
    Multi,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_dir_round_trips() {
        let s = serde_json::to_string(&SortDir::Asc).unwrap();
        assert_eq!(s, "\"asc\"");
    }
}
