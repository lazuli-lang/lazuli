//! Wave-W6 surface UX primitive emitters (GAP-UX-01..04).
//!
//! These emit small hook-state + return-field fragments for the view-level
//! primitives carried on `ViewUx` (`wizard_steps`, `tab_group`, `view_mode`,
//! `view.inline_table`). Per the founding wire-not-reimplementation principle,
//! the output is just declarative config + runtime-helper invocations
//! (`useWizardSteps`, `useTabGroup`, `useViewMode`, `useInlineTable`); the
//! runtime owns the React state machines.

use std::fmt::Write;

use crate::lzx::{RenderMode, ViewUx, command_ident};

/// Emit hook-local `const` declarations for the view-level W6 primitives.
/// Returns the empty string when no W6 primitive is declared.
pub(crate) fn emit_ux_const(ux: &ViewUx) -> String {
    let mut s = String::new();
    if let Some(steps) = &ux.wizard_steps {
        writeln!(
            s,
            "  const wizardSteps = useWizardSteps({}, query.data?.{});",
            steps.total, steps.current_field
        )
        .ok();
    }
    if let Some(group) = &ux.tab_group {
        writeln!(
            s,
            "  const tabGroup = useTabGroup(query.data?.{}, {});",
            group.derived_from,
            emit_tab_group_cases(group)
        )
        .ok();
    }
    if !ux.view_modes.is_empty() {
        writeln!(
            s,
            "  const viewMode = useViewMode([{}] as const);",
            ux.view_modes
                .iter()
                .map(|m| format!("\"{}\"", m.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .ok();
    }
    if let Some(inline) = &ux.inline_table {
        writeln!(
            s,
            "  const inlineTable = useInlineTable({});",
            command_ident(&inline.on_change)
        )
        .ok();
    }
    s
}

/// Emit the `return {{ ... }}` fields for the view-level W6 primitives.
pub(crate) fn emit_ux_return_fields(s: &mut String, ux: &ViewUx) {
    if ux.wizard_steps.is_some() {
        writeln!(s, "    wizardSteps,").ok();
    }
    if ux.tab_group.is_some() {
        writeln!(s, "    tabGroup,").ok();
    }
    if !ux.view_modes.is_empty() {
        writeln!(s, "    viewMode,").ok();
    }
    if ux.inline_table.is_some() {
        writeln!(s, "    inlineTable,").ok();
    }
}

/// Render the `tab_group` case map as a TS array literal of
/// `{ variants, label }` rows.
fn emit_tab_group_cases(group: &crate::lzx::TabGroup) -> String {
    let rows: Vec<String> = group
        .cases
        .iter()
        .map(|case| {
            let variants = case
                .variants
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ variants: [{variants}], label: {:?} }}", case.label)
        })
        .collect();
    format!("[{}]", rows.join(", "))
}

/// The default render mode that a multi-mode toggle starts on — the first
/// declared mode, or `Table` when somehow empty.
pub(crate) fn default_render_mode(ux: &ViewUx) -> RenderMode {
    ux.view_modes.first().copied().unwrap_or(RenderMode::Table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lzx::{
        CommandRef, InlineTable, RenderMode, TabGroup, TabGroupCase, ViewUx, WizardSteps,
    };

    #[test]
    fn empty_ux_emits_nothing() {
        let ux = ViewUx::default();
        assert_eq!(emit_ux_const(&ux), "");
        let mut s = String::new();
        emit_ux_return_fields(&mut s, &ux);
        assert_eq!(s, "");
    }

    #[test]
    fn wizard_steps_emits_hook_and_return() {
        let ux = ViewUx {
            wizard_steps: Some(WizardSteps {
                total: 3,
                current_field: "registration_step".to_owned(),
                span_ref: None,
            }),
            ..Default::default()
        };
        let out = emit_ux_const(&ux);
        assert!(out.contains("useWizardSteps(3, query.data?.registration_step)"));
        let mut s = String::new();
        emit_ux_return_fields(&mut s, &ux);
        assert!(s.contains("wizardSteps,"));
    }

    #[test]
    fn tab_group_emits_case_map() {
        let ux = ViewUx {
            tab_group: Some(TabGroup {
                derived_from: "vehicle_type".to_owned(),
                cases: vec![
                    TabGroupCase {
                        variants: vec!["TV".to_owned(), "RADIO".to_owned()],
                        label: "Broadcast".to_owned(),
                        span_ref: None,
                    },
                    TabGroupCase {
                        variants: vec!["PRINT".to_owned()],
                        label: "Print".to_owned(),
                        span_ref: None,
                    },
                ],
                span_ref: None,
            }),
            ..Default::default()
        };
        let out = emit_ux_const(&ux);
        assert!(out.contains("useTabGroup(query.data?.vehicle_type"));
        assert!(out.contains("variants: [\"TV\", \"RADIO\"], label: \"Broadcast\""));
        assert!(out.contains("variants: [\"PRINT\"], label: \"Print\""));
    }

    #[test]
    fn view_mode_emits_const_tuple() {
        let ux = ViewUx {
            view_modes: vec![RenderMode::Table, RenderMode::Kanban],
            ..Default::default()
        };
        let out = emit_ux_const(&ux);
        assert!(out.contains("useViewMode([\"table\", \"kanban\"] as const)"));
        assert_eq!(default_render_mode(&ux), RenderMode::Table);
    }

    #[test]
    fn inline_table_emits_command_binding() {
        let ux = ViewUx {
            inline_table: Some(InlineTable {
                on_change: CommandRef {
                    feature: "jobs".to_owned(),
                    name: "update_row".to_owned(),
                },
                span_ref: None,
            }),
            ..Default::default()
        };
        let out = emit_ux_const(&ux);
        assert!(out.contains("useInlineTable("));
        let mut s = String::new();
        emit_ux_return_fields(&mut s, &ux);
        assert!(s.contains("inlineTable,"));
    }
}
