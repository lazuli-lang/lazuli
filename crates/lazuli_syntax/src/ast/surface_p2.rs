/// Closed catalog of persistence scopes for a [`SettingDeclAst`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingPersistenceAst {
    /// `persistence none` — ephemeral, per-session.
    None,
    /// `persistence local` — per-browser via localStorage.
    Local,
    /// `persistence workspace` — server-stored, follows the user's workspace.
    Workspace,
}

// ===========================================================================
// Wave-W6 surface UX primitives (GAP-UX-01..04). Mirrors `lazuli_ir::ux`.
// ===========================================================================

/// Aggregate of view-level W6 primitives carried on a list/detail view.
/// Defaults to "no extra UX surface".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewUxAst {
    /// `wizard_steps <total> current <field>` (GAP-UX-01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wizard_steps: Option<WizardStepsAst>,
    /// `tab_group derived_from <field> { ... }` (GAP-UX-02).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_group: Option<TabGroupAst>,
    /// `view_mode { table; kanban }` (GAP-UX-04).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_modes: Vec<String>,
    /// `view.inline_table on_change @command.X` (GAP-UX-04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_table: Option<InlineTableAst>,
    /// `view.board <name> / lanes derived_from <field>` (GAP-UX-05).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardAst>,
    /// `repeatable input <name> group <f>: <T>, … validates sum(<f>) = <n>`
    /// (GAP-UX-05).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeatable_groups: Vec<RepeatableGroupAst>,
}

impl ViewUxAst {
    /// True when no W6/GAP-UX-05 view primitive is declared.
    pub fn is_empty(&self) -> bool {
        self.wizard_steps.is_none()
            && self.tab_group.is_none()
            && self.view_modes.is_empty()
            && self.inline_table.is_none()
            && self.board.is_none()
            && self.repeatable_groups.is_empty()
    }
}

/// Aggregate of audience-level W6 containers (`tabs`, `wizard`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceUxAst {
    /// `tabs { tab "X" -> view v }` static containers (GAP-UX-03).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabsAst>,
    /// `wizard <name> steps { step N: ref }` containers (GAP-UX-03).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wizards: Vec<WizardAst>,
}

impl AudienceUxAst {
    /// True when the audience declares no tabs or wizards.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty() && self.wizards.is_empty()
    }
}

/// `wizard_steps <total> current <field>` — step indicator (GAP-UX-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardStepsAst {
    pub total: u32,
    pub current_field: String,
    pub span: Span,
}

/// `tab_group derived_from <field>` runtime-derived tabs (GAP-UX-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroupAst {
    pub derived_from: String,
    pub cases: Vec<TabGroupCaseAst>,
    pub span: Span,
}

/// One `case <V1, V2> -> tab "<label>"` arm of a [`TabGroupAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroupCaseAst {
    pub variants: Vec<String>,
    pub label: String,
    pub span: Span,
}

/// `tabs { tab "<label>" -> view <name> [audience <a>] }` (GAP-UX-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabsAst {
    pub entries: Vec<TabEntryAst>,
    pub span: Span,
}

/// One `tab "<label>" -> view <name>` row of a [`TabsAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabEntryAst {
    pub label: String,
    pub view: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    pub span: Span,
}

/// `wizard <name> steps { step N: <ref> }` multi-step container (GAP-UX-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardAst {
    pub name: String,
    pub steps: Vec<WizardStepAst>,
    pub span: Span,
}

/// One `step <N>: <ref>` row of a [`WizardAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardStepAst {
    pub index: u32,
    pub ref_name: String,
    pub span: Span,
}

/// `view.inline_table on_change @command.<name>` (GAP-UX-04). `on_change`
/// is kept as raw `@command.<name>` text; the analyzer normalizes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineTableAst {
    pub on_change: String,
    pub span: Span,
}

/// `view.board <name>` + `lanes derived_from <field>` (GAP-UX-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardAst {
    /// Optional board name from the header (`view.board <name>`); empty when
    /// omitted.
    pub name: String,
    /// `lanes derived_from <field>` — the enum field / has_many relation.
    pub lanes_source: String,
    pub span: Span,
}

/// `repeatable input <name> group <fields> validates sum(<f>) = <n>`
/// (GAP-UX-05). The `sum_target` is kept verbatim (parser-validated numeric).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatableGroupAst {
    pub name: String,
    pub fields: Vec<RepeatableFieldAst>,
    pub sum_field: String,
    pub sum_target: String,
    pub span: Span,
}

/// One `<name>: <Type>` field inside a [`RepeatableGroupAst`]'s `group { … }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatableFieldAst {
    pub name: String,
    pub type_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_target_ast_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(SurfaceTargetAst::Web).unwrap(),
            serde_json::json!("web")
        );
        assert_eq!(
            serde_json::to_value(SurfaceTargetAst::Mobile).unwrap(),
            serde_json::json!("mobile")
        );
    }

    #[test]
    fn selection_mode_ast_default_via_serde_token() {
        assert_eq!(
            serde_json::to_value(SelectionModeAst::Multi).unwrap(),
            serde_json::json!("multi")
        );
    }

    #[test]
    fn binding_ref_ast_filter_serde_carries_name() {
        let r = BindingRefAst::Filter {
            name: "status".into(),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["kind"], "filter");
        assert_eq!(v["value"]["name"], "status");
    }
}
