//! Wave-W6 surface UX primitive emitters (GAP-UX-01..04).
//!
//! These emit small hook-state + return-field fragments for the view-level
//! primitives carried on `ViewUx` (`wizard_steps`, `tab_group`, `view_mode`,
//! `view.inline_table`). Per the founding wire-not-reimplementation principle,
//! the output is just declarative config + runtime-helper invocations
//! (`useWizardSteps`, `useTabGroup`, `useViewMode`, `useInlineTable`); the
//! runtime owns the React state machines.

use std::fmt::Write;

use super::casing::lower_camel;
use crate::lzx::{RenderMode, RepeatableGroup, ViewUx, command_ident};

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
    if let Some(board) = &ux.board {
        // GAP-UX-05 — kanban board keyed on the lane source field/relation.
        writeln!(
            s,
            "  const board = useBoard(query.data, {{ lanesFrom: \"{}\" }});",
            board.lanes_source
        )
        .ok();
    }
    for group in &ux.repeatable_groups {
        // GAP-UX-05 — repeatable field array + cross-row sum constraint. The
        // form library enforces the constraint; we emit it declaratively.
        writeln!(
            s,
            "  const {} = useRepeatableGroup({{ fields: {}, validates: {{ sum: \"{}\", equals: {} }} }});",
            repeatable_ident(&group.name),
            emit_repeatable_fields(group),
            group.sum_field,
            group.sum_target
        )
        .ok();
    }
    s
}

/// The `@lazuli/runtime/react` hook names a `ViewUx` invokes, in import
/// order. Empty when the view declares no W6 primitive. Kept in lockstep
/// with `emit_ux_const` so every emitted call site is imported and the
/// generated file typechecks (W1-7 VIEW-HOOKS-UNDEFINED).
pub(crate) fn ux_runtime_hook_imports(ux: &ViewUx) -> Vec<&'static str> {
    let mut hooks = Vec::new();
    if ux.wizard_steps.is_some() {
        hooks.push("useWizardSteps");
    }
    if ux.tab_group.is_some() {
        hooks.push("useTabGroup");
    }
    if !ux.view_modes.is_empty() {
        hooks.push("useViewMode");
    }
    if ux.inline_table.is_some() {
        hooks.push("useInlineTable");
    }
    if ux.board.is_some() {
        hooks.push("useBoard");
    }
    if !ux.repeatable_groups.is_empty() {
        hooks.push("useRepeatableGroup");
    }
    hooks
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
    if ux.board.is_some() {
        writeln!(s, "    board,").ok();
    }
    for group in &ux.repeatable_groups {
        writeln!(s, "    {},", repeatable_ident(&group.name)).ok();
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

/// The hook-local lowerCamel identifier a repeatable group binds to.
fn repeatable_ident(name: &str) -> String {
    lower_camel(name)
}

/// Render a repeatable group's fields as a TS array literal of
/// `{ name, type }` rows.
fn emit_repeatable_fields(group: &RepeatableGroup) -> String {
    let rows: Vec<String> = group
        .fields
        .iter()
        .map(|f| format!("{{ name: {:?}, type: {:?} }}", f.name, f.type_name))
        .collect();
    format!("[{}]", rows.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lzx::{
        Board, CommandRef, InlineTable, RenderMode, RepeatableField, RepeatableGroup, TabGroup,
        TabGroupCase, ViewUx, WizardSteps,
    };

    #[test]
    fn empty_ux_emits_nothing() {
        let ux = ViewUx::default();
        assert_eq!(emit_ux_const(&ux), "");
        let mut s = String::new();
        emit_ux_return_fields(&mut s, &ux);
        assert_eq!(s, "");
        assert!(ux_runtime_hook_imports(&ux).is_empty());
    }

    #[test]
    fn hook_imports_track_every_emitted_call_site() {
        // W1-7 VIEW-HOOKS-UNDEFINED — the import set must name every hook
        // emit_ux_const calls so the generated file typechecks.
        let ux = ViewUx {
            wizard_steps: Some(WizardSteps {
                total: 2,
                current_field: "step".to_owned(),
                span_ref: None,
            }),
            tab_group: Some(TabGroup {
                derived_from: "kind".to_owned(),
                cases: vec![],
                span_ref: None,
            }),
            view_modes: vec![RenderMode::Table, RenderMode::Kanban],
            inline_table: Some(InlineTable {
                on_change: CommandRef {
                    feature: "jobs".to_owned(),
                    name: "update_row".to_owned(),
                },
                span_ref: None,
            }),
            board: Some(Board {
                name: "b".to_owned(),
                lanes_source: "status".to_owned(),
                span_ref: None,
            }),
            repeatable_groups: vec![RepeatableGroup {
                name: "installments".to_owned(),
                fields: vec![],
                sum_field: "pct".to_owned(),
                sum_target: "100".to_owned(),
                span_ref: None,
            }],
        };
        assert_eq!(
            ux_runtime_hook_imports(&ux),
            vec![
                "useWizardSteps",
                "useTabGroup",
                "useViewMode",
                "useInlineTable",
                "useBoard",
                "useRepeatableGroup",
            ]
        );
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
    fn board_emits_hook_and_return() {
        let ux = ViewUx {
            board: Some(Board {
                name: "activity_board".to_owned(),
                lanes_source: "status".to_owned(),
                span_ref: None,
            }),
            ..Default::default()
        };
        let out = emit_ux_const(&ux);
        assert!(out.contains("const board = useBoard(query.data, { lanesFrom: \"status\" });"));
        let mut s = String::new();
        emit_ux_return_fields(&mut s, &ux);
        assert!(s.contains("board,"));
    }

    #[test]
    fn repeatable_group_emits_field_array_and_sum_constraint() {
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
        let out = emit_ux_const(&ux);
        assert!(out.contains("const installments = useRepeatableGroup("));
        assert!(out.contains("{ name: \"days\", type: \"Int\" }"));
        assert!(out.contains("{ name: \"percentage\", type: \"Decimal\" }"));
        assert!(out.contains("validates: { sum: \"percentage\", equals: 100 }"));
        let mut s = String::new();
        emit_ux_return_fields(&mut s, &ux);
        assert!(s.contains("installments,"));
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
