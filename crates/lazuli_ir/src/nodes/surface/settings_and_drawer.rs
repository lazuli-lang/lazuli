//! View settings + drawer sub-view declarations.
//!
//! Settings give a `ViewList` a closed catalog of session knobs (enum,
//! bool, int). [`SettingPersistence`] decides whether the knob survives
//! tab reload (`local`), reaches workspace scope (`workspace`, planned),
//! or stays ephemeral (`none`).
//!
//! Drawer sub-views are nested views opened from a host list. They have
//! their own query source and route binding (typically `from selection`).

use serde::{Deserialize, Serialize};

use crate::SpanRef;

use super::core::{CellBinding, CommandRef, QueryRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingDecl {
    pub name: String,
    pub value_space: SettingValueSpace,
    /// Raw token, e.g. `sm`, `true`, or `42`.
    pub default: String,
    pub persistence: SettingPersistence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingValueSpace {
    Enum { values: Vec<String> },
    Bool,
    Int { min: i64, max: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingPersistence {
    None,
    Local,
    /// v0.2: declared but lowering warns until the cell ships.
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawerSubView {
    pub name: String,
    pub trigger: DrawerTrigger,
    pub source: QueryRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_binding: Option<DrawerRouteBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<CommandRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerTrigger {
    /// Click on host cell opens drawer with that item.
    Select,
    /// User code calls `.open(id)` explicitly.
    ManualOpen,
}

/// `route <slot> from selection` binds the drawer's source query input
/// to the host's selection state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawerRouteBinding {
    /// The sub-query input name, e.g. `key`.
    pub target: String,
    pub source: DrawerBindingSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerBindingSource {
    Selection,
}
