//! Concrete view shapes — `ViewList`, `ViewDetail`, `ViewCreate`.
//!
//! Closed catalog of the three view kinds a surface admits today. New
//! kinds enter via a Lazuli core proposal plus a minor IR bump (Rule Zero).
//!
//! `OnSuccessSpec` captures the post-submit orchestration for `ViewCreate`
//! views — `back`, `redirect`, `flash`, `invalidates`, `replace`. The
//! shape is a *declaration of intent*, not a callback: codegen emits the
//! navigation moves; the language stays out of the routing library.

use serde::{Deserialize, Serialize};

use crate::{InvalidatesSpec, SpanRef, TranslationKeyRef, is_false};

use super::{
    controls::{FilterDecl, SearchDecl, SelectionDecl, SortDecl},
    core::{CellBinding, CommandRef, QueryRef, RouteParam},
    settings_and_drawer::{DrawerSubView, SettingDecl},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewList {
    pub name: String,
    /// Optional `at "<path>"` route binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// `source <feature>.query.<name>`.
    pub source: QueryRef,
    /// How this list renders its rows (`columns` table form or grid `cells` slot).
    pub render: ListRender,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<FilterDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<CommandRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawer: Option<DrawerSubView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<SortDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<SelectionDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingDecl>,
    /// `view ... fields <X> redacted` — field names that this view
    /// must mask before emission. Codegen emits a redaction wrapper.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDetail {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub source: QueryRef,
    /// `route <name>: <Type> from path` declarations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_params: Vec<RouteParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<CommandRef>,
    /// `view ... fields <X> redacted` — field names that this view
    /// must mask before emission. Codegen emits a redaction wrapper.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCreate {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// `submit <feature>.command.<name>` (required).
    pub submit: CommandRef,
    /// Submit-success orchestration lowered from `.lzx on_success`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_success: Option<OnSuccessSpec>,
    /// `fields <name>, <name>` — subset of the command's input slots.
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    /// `view ... fields <X> redacted` — field names that this view
    /// must mask before emission. Codegen emits a redaction wrapper.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnSuccessSpec {
    #[serde(default, skip_serializing_if = "is_false")]
    pub back: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flash: Option<FlashSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates: Vec<InvalidatesSpec>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub replace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashSpec {
    pub kind: String,
    pub message_key: TranslationKeyRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListRender {
    Table {
        columns: Vec<String>,
    },
    /// Grid form: slot identifier after `@client.`.
    Cells {
        slot: String,
    },
}
