//! Wave-W6 surface UX primitives — wizard step indicator, runtime tab
//! groups, static tabs, multi-step wizard container, multi-mode view
//! toggle, and inline-editable tables.
//!
//! These close pauta UX gaps GAP-UX-01..04. To keep the heavily-constructed
//! [`ViewList`](super::views::ViewList) / [`ViewDetail`](super::views::ViewDetail)
//! / [`Audience`](super::core::Audience) literals churn-free, the new data is
//! grouped into two `#[derive(Default)]` aggregates — [`ViewUx`] (carried on
//! list/detail views) and [`AudienceUx`] (carried on the audience) — added as
//! a single `#[serde(default)]` field per parent. Empty aggregates serialize
//! to nothing, so existing JSON goldens are unaffected.

use serde::{Deserialize, Serialize};

use super::core::CommandRef;
use crate::SpanRef;

/// Wave-W6 presentation primitives attached to a single list/detail view.
/// All slots are optional; the default is "no extra UX surface".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewUx {
    /// `wizard_steps <total> current <field>` — step indicator (GAP-UX-01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wizard_steps: Option<WizardSteps>,
    /// `tab_group derived_from <field> { ... }` — runtime-derived tabs
    /// (GAP-UX-02).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_group: Option<TabGroup>,
    /// `view_mode { table; kanban }` — user-toggleable render modes
    /// (GAP-UX-04). Empty = single fixed render mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_modes: Vec<RenderMode>,
    /// `view.inline_table on_change X` — inline-editable rows
    /// (GAP-UX-04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_table: Option<InlineTable>,
    /// `view.board <name> / lanes derived_from <field>` — kanban-style board
    /// whose lanes are derived from an enum field or has_many relation on the
    /// bound resource (GAP-UX-05).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<Board>,
    /// `repeatable input <name> group <f>: <T>, … validates sum(<f>) = <n>` —
    /// repeatable field-array control with a cross-row sum constraint
    /// (GAP-UX-05).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeatable_groups: Vec<RepeatableGroup>,
}

impl ViewUx {
    /// True when no W6/GAP-UX-05 primitive is declared — codegen can skip the
    /// whole surface.
    pub fn is_empty(&self) -> bool {
        self.wizard_steps.is_none()
            && self.tab_group.is_none()
            && self.view_modes.is_empty()
            && self.inline_table.is_none()
            && self.board.is_none()
            && self.repeatable_groups.is_empty()
    }
}

/// Wave-W6 audience-level containers (static tabs + multi-step wizards).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceUx {
    /// `tabs { tab "X" -> view v }` static tab containers (GAP-UX-03).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<Tabs>,
    /// `wizard <name> steps { step N: ref }` multi-step containers (GAP-UX-03).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wizards: Vec<Wizard>,
}

impl AudienceUx {
    /// True when the audience declares no tabs or wizards.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty() && self.wizards.is_empty()
    }
}

/// `wizard_steps <total> current <field>` — a step indicator bound to an
/// enum field selecting the active step (GAP-UX-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardSteps {
    /// Total number of steps; positive integer literal.
    pub total: u32,
    /// `current <field>` — the enum field whose value selects the active step.
    pub current_field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `tab_group derived_from <field> { case … -> tab "…" }` — tabs whose set
/// depends on a field's runtime value (GAP-UX-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroup {
    /// `derived_from <field>` — the enum field keying the conditional tabs.
    pub derived_from: String,
    pub cases: Vec<TabGroupCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `case <V1, V2, …> -> tab "<label>"` arm of a [`TabGroup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroupCase {
    /// Enum variants (UPPER) that map to this tab.
    pub variants: Vec<String>,
    /// Human-facing tab label.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `tabs { tab "<label>" -> view <name> [audience <a>] }` — static tab
/// container (GAP-UX-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tabs {
    pub entries: Vec<TabEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `tab "<label>" -> view <name>` row of a [`Tabs`] container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabEntry {
    pub label: String,
    /// Declared view name this tab renders.
    pub view: String,
    /// Optional `audience <a>` scoping for the tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `wizard <name> steps { step N: <ref> }` — multi-step container
/// (GAP-UX-03). Distinct from [`WizardSteps`] (the indicator).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wizard {
    pub name: String,
    pub steps: Vec<WizardStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `step <N>: <ref>` row of a [`Wizard`] container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardStep {
    /// 1-based step index as authored.
    pub index: u32,
    /// Declared form/view this step references.
    pub ref_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of toggleable list render modes (`view_mode { … }`).
/// New modes enter via a Lazuli core proposal (Rule Zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    /// Tabular rows.
    Table,
    /// Column-of-cards board keyed on a status field.
    Kanban,
    /// Date-grid calendar.
    Calendar,
    /// Thumbnail / card gallery.
    Gallery,
}

impl RenderMode {
    /// Parse the authored keyword into a [`RenderMode`]. Returns `None`
    /// for an unknown mode — the caller raises `LZX-VIEW-MODE-001`.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "table" => Some(RenderMode::Table),
            "kanban" => Some(RenderMode::Kanban),
            "calendar" => Some(RenderMode::Calendar),
            "gallery" => Some(RenderMode::Gallery),
            _ => None,
        }
    }

    /// The lowercase wire keyword for this mode.
    pub fn as_str(self) -> &'static str {
        match self {
            RenderMode::Table => "table",
            RenderMode::Kanban => "kanban",
            RenderMode::Calendar => "calendar",
            RenderMode::Gallery => "gallery",
        }
    }
}

/// `view.inline_table on_change <name>` — inline-editable rows
/// bound to an update command (GAP-UX-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineTable {
    /// The update command edited rows submit to.
    pub on_change: CommandRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `view.board <name> / lanes derived_from <field>` — a kanban-style board
/// whose lanes come from an enum field (one lane per variant) or a has_many
/// relation on the view's bound resource (GAP-UX-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    /// Optional board name (from the `view.board <name>` header). Empty when
    /// the header omits a name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// `lanes derived_from <field>` — the enum field or has_many relation
    /// whose value set defines the board's lanes.
    pub lanes_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `repeatable input <name> group <fields> validates sum(<field>) = <n>`
/// — a repeatable group of input rows with a cross-row sum constraint
/// (e.g. installment percentages must total 100) (GAP-UX-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatableGroup {
    /// The group's identifier (`repeatable input <name>`).
    pub name: String,
    /// The fields declared after `group`, in source order.
    pub fields: Vec<RepeatableField>,
    /// `sum(<field>)` — the group field aggregated across rows.
    pub sum_field: String,
    /// `= <n>` — the numeric literal the sum must equal, kept verbatim (the
    /// parser guarantees it parses as a number; stored as the source string so
    /// the IR node stays `Eq`/`Hash`-friendly and codegen emits it as authored).
    pub sum_target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `<name>: <Type>` field inside a [`RepeatableGroup`]'s `group { … }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatableField {
    pub name: String,
    /// The declared type keyword (`Int`, `Decimal`, `Text`, …), kept verbatim;
    /// doctor `LZX-REPEATABLE-SUM-001` checks the summed field is numeric.
    pub type_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_ux_default_is_empty_and_serializes_to_empty_object() {
        let ux = ViewUx::default();
        assert!(ux.is_empty());
        assert_eq!(serde_json::to_value(&ux).unwrap(), serde_json::json!({}));
    }

    #[test]
    fn render_mode_round_trips() {
        assert_eq!(RenderMode::parse("kanban"), Some(RenderMode::Kanban));
        assert_eq!(RenderMode::parse("bogus"), None);
        assert_eq!(
            serde_json::to_value(RenderMode::Table).unwrap(),
            serde_json::json!("table")
        );
    }

    #[test]
    fn audience_ux_default_is_empty() {
        assert!(AudienceUx::default().is_empty());
    }

    #[test]
    fn board_marks_view_ux_non_empty_and_serializes() {
        let ux = ViewUx {
            board: Some(Board {
                name: "activity_board".to_owned(),
                lanes_source: "status".to_owned(),
                span_ref: None,
            }),
            ..Default::default()
        };
        assert!(!ux.is_empty());
        let json = serde_json::to_value(&ux).unwrap();
        assert_eq!(json["board"]["lanes_source"], serde_json::json!("status"));
    }

    #[test]
    fn repeatable_group_marks_view_ux_non_empty() {
        let ux = ViewUx {
            repeatable_groups: vec![RepeatableGroup {
                name: "installments".to_owned(),
                fields: vec![
                    RepeatableField {
                        name: "days".to_owned(),
                        type_name: "Int".to_owned(),
                    },
                    RepeatableField {
                        name: "percentage".to_owned(),
                        type_name: "Decimal".to_owned(),
                    },
                ],
                sum_field: "percentage".to_owned(),
                sum_target: "100".to_owned(),
                span_ref: None,
            }],
            ..Default::default()
        };
        assert!(!ux.is_empty());
    }
}
