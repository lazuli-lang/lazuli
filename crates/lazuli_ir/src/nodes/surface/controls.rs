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

use crate::SpanRef;

use super::core::CommandRef;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterCardinality {
    Single,
    Multi,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchMode {
    /// v1 behavior already represented by `ViewList.search` today.
    Columns {
        columns: Vec<String>,
    },
    Segmented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchField {
    pub key: String,
    pub binds_to: BindingRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortDecl {
    pub allowed: Vec<String>,
    pub default_field: String,
    pub default_dir: SortDir,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionDecl {
    pub mode: SelectionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bulk_actions: Vec<CommandRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    None,
    Single,
    Multi,
}
