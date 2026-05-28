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

use super::core::{CellBinding, CommandRef, QueryRef};
use crate::SpanRef;

/// One `setting <name> { ... }` knob inside a [`super::ViewList`].
/// Three typed value spaces (enum / bool / int with bounds) keep the
/// surface tight so codegen can render the right control without
/// dynamic dispatch.
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

/// Closed catalog of setting value spaces. Each variant carries the
/// minimal shape needed to render the control (enum values, bool
/// switch, integer bounds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingValueSpace {
    /// Enum knob — closed list of admitted string values.
    Enum { values: Vec<String> },
    /// Boolean toggle.
    Bool,
    /// Integer knob with hard `[min, max]` bounds.
    Int { min: i64, max: i64 },
}

/// Closed catalog of setting persistence scopes. `None` is ephemeral
/// (lives in component state); `Local` survives reload via local
/// storage; `Workspace` is the planned cross-device scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingPersistence {
    /// Ephemeral — gone on reload.
    None,
    /// Persisted in local storage; survives reload on the same device.
    Local,
    /// v0.2: declared but lowering warns until the cell ships.
    Workspace,
}

/// One drawer sub-view opened from a host list. Has its own query
/// source, optional route binding (e.g. `from selection`), and the
/// same composition surface as a detail view (sections + cells +
/// actions).
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

/// Closed catalog of drawer opening behaviours. `Select` opens the
/// drawer when the user clicks a host row; `ManualOpen` requires
/// explicit `.open(id)` calls from author code.
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

/// Closed catalog of drawer route-binding sources. v0 admits
/// `Selection` only — additional sources require a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerBindingSource {
    /// Bind the drawer route slot to the host's selected item.
    Selection,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawer_trigger_round_trips() {
        let s = serde_json::to_string(&DrawerTrigger::Select).unwrap();
        assert_eq!(s, "\"select\"");
    }
}
