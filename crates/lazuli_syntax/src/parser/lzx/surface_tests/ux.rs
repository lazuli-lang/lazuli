//! Wave-W6 surface UX primitive parser tests (GAP-UX-01..04).
//!
//! Sibling of `mod.rs`. Raw-string fixtures preserved verbatim so the
//! canonical-indent contract holds.

#[cfg(test)]
mod ux_tests {
    use super::super::super::parse_surface_document;
    use crate::ViewAst;

    // ── GAP-UX-01: wizard_steps ────────────────────────────────────────────

    #[test]
    fn wizard_steps_on_detail_view() {
        let source = r#"surface onboarding web
  audience admin
    view detail registration at "/registration"
      source onboarding.query.by_id
      route id: Text from path
      wizard_steps 3 current registration_step
"#;
        let surface = parse_surface_document(source).expect("parses wizard_steps");
        let detail = match &surface.audiences[0].views[0] {
            ViewAst::Detail(v) => v,
            other => panic!("expected detail, got {other:?}"),
        };
        let steps = detail.ux.wizard_steps.as_ref().expect("wizard_steps");
        assert_eq!(steps.total, 3);
        assert_eq!(steps.current_field, "registration_step");
    }

    #[test]
    fn wizard_steps_rejects_zero_total() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      wizard_steps 0 current step\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("positive integer"));
    }

    #[test]
    fn wizard_steps_rejects_missing_current() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      wizard_steps 3\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("current <field>"));
    }

    // ── GAP-UX-02: tab_group ───────────────────────────────────────────────

    #[test]
    fn tab_group_derived_from_with_cases() {
        let source = r#"surface ad web
  audience admin
    view list ads at "/ads"
      source ad.query.list
      columns title
      tab_group derived_from vehicle_type
        case TV, RADIO -> tab "Broadcast"
        case PRINT -> tab "Print"
"#;
        let surface = parse_surface_document(source).expect("parses tab_group");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {other:?}"),
        };
        let group = list.ux.tab_group.as_ref().expect("tab_group");
        assert_eq!(group.derived_from, "vehicle_type");
        assert_eq!(group.cases.len(), 2);
        assert_eq!(group.cases[0].variants, vec!["TV", "RADIO"]);
        assert_eq!(group.cases[0].label, "Broadcast");
        assert_eq!(group.cases[1].variants, vec!["PRINT"]);
        assert_eq!(group.cases[1].label, "Print");
    }

    #[test]
    fn tab_group_requires_derived_from() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      tab_group vehicle_type\n        case TV -> tab \"B\"\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("derived_from"));
    }

    #[test]
    fn tab_group_requires_at_least_one_case() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      tab_group derived_from t\n      actions x\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    // ── GAP-UX-04: view_mode + view.inline_table ───────────────────────────

    #[test]
    fn view_mode_block_and_inline_table() {
        let source = r#"surface jobs web
  audience admin
    view list jobs at "/jobs"
      source jobs.query.list
      columns title, status
      view_mode
        table
        kanban
      view.inline_table on_change update_row
"#;
        let surface = parse_surface_document(source).expect("parses view_mode");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(list.ux.view_modes, vec!["table", "kanban"]);
        let inline = list.ux.inline_table.as_ref().expect("inline_table");
        assert_eq!(inline.on_change, "update_row");
    }

    #[test]
    fn view_mode_rejected_in_detail() {
        let source = "surface a web\n  audience admin\n    view detail d at \"/d/:id\"\n      source a.query.by_id\n      route id: Text from path\n      view_mode\n        table\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("only valid in `view list`"));
    }

    #[test]
    fn inline_table_rejects_retired_at_command_sigil() {
        // SPEC-02 — the `@command.<name>` sigil is retired; commands are bare.
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      view.inline_table on_change @command.update_row\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("E-AT-COMMAND-RETIRED"));
    }

    #[test]
    fn inline_table_accepts_bare_command() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      view.inline_table on_change update_row\n";
        let surface = parse_surface_document(source).expect("bare command accepted");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(
            list.ux.inline_table.as_ref().unwrap().on_change,
            "update_row"
        );
    }

    #[test]
    fn view_create_rejects_ux_primitives() {
        let source = "surface a web\n  audience admin\n    view create c\n      submit a.command.create\n      fields k\n      wizard_steps 2 current step\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("not valid in `view create`"));
    }

    // ── GAP-UX-03: tabs + wizard (audience-level) ──────────────────────────

    #[test]
    fn audience_static_tabs() {
        let source = r#"surface job web
  audience admin
    view detail detail at "/job/:id"
      source job.query.by_id
      route id: Text from path
    view list history at "/job/:id/history"
      source job.query.history
    tabs
      tab "Details" -> view detail
      tab "History" -> view history audience admin
"#;
        let surface = parse_surface_document(source).expect("parses tabs");
        let aud = &surface.audiences[0];
        assert_eq!(aud.ux.tabs.len(), 1);
        let tabs = &aud.ux.tabs[0];
        assert_eq!(tabs.entries.len(), 2);
        assert_eq!(tabs.entries[0].label, "Details");
        assert_eq!(tabs.entries[0].view, "detail");
        assert_eq!(tabs.entries[0].audience, None);
        assert_eq!(tabs.entries[1].label, "History");
        assert_eq!(tabs.entries[1].view, "history");
        assert_eq!(tabs.entries[1].audience.as_deref(), Some("admin"));
    }

    #[test]
    fn audience_multi_step_wizard() {
        let source = r#"surface job web
  audience admin
    wizard job_create steps
      step 1: job_basics
      step 2: job_targeting
"#;
        let surface = parse_surface_document(source).expect("parses wizard");
        let aud = &surface.audiences[0];
        assert_eq!(aud.ux.wizards.len(), 1);
        let wiz = &aud.ux.wizards[0];
        assert_eq!(wiz.name, "job_create");
        assert_eq!(wiz.steps.len(), 2);
        assert_eq!(wiz.steps[0].index, 1);
        assert_eq!(wiz.steps[0].ref_name, "job_basics");
        assert_eq!(wiz.steps[1].index, 2);
        assert_eq!(wiz.steps[1].ref_name, "job_targeting");
    }

    #[test]
    fn tabs_requires_view_target() {
        let source = "surface a web\n  audience admin\n    tabs\n      tab \"X\" -> detail\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("view <name>"));
    }

    #[test]
    fn wizard_requires_steps_keyword() {
        let source = "surface a web\n  audience admin\n    wizard job_create\n      step 1: a\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("wizard <name> steps"));
    }

    // ── GAP-UX-05: view.board ──────────────────────────────────────────────

    #[test]
    fn view_board_lanes_derived_from() {
        let source = r#"surface activity web
  audience admin
    view list activity at "/activity"
      source activity.query.list
      columns title, status
      view.board activity_board
        lanes derived_from status
"#;
        let surface = parse_surface_document(source).expect("parses view.board");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {other:?}"),
        };
        let board = list.ux.board.as_ref().expect("board");
        assert_eq!(board.name, "activity_board");
        assert_eq!(board.lanes_source, "status");
    }

    #[test]
    fn view_board_rejected_in_detail() {
        let source = "surface a web\n  audience admin\n    view detail d at \"/d/:id\"\n      source a.query.by_id\n      route id: Text from path\n      view.board b\n        lanes derived_from status\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("only valid in `view list`"));
    }

    #[test]
    fn view_board_requires_lanes_line() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      view.board b\n      actions x\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("lanes derived_from"));
    }

    // ── GAP-UX-05: repeatable input group ──────────────────────────────────

    #[test]
    fn repeatable_input_group_with_sum_validation() {
        let source = r#"surface billing web
  audience admin
    view list plans at "/plans"
      source billing.query.list
      columns title
      repeatable input installments group days: Integer, percentage: Decimal validates sum(percentage) = 100
"#;
        let surface = parse_surface_document(source).expect("parses repeatable input");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(list.ux.repeatable_groups.len(), 1);
        let g = &list.ux.repeatable_groups[0];
        assert_eq!(g.name, "installments");
        assert_eq!(g.fields.len(), 2);
        assert_eq!(g.fields[0].name, "days");
        assert_eq!(g.fields[0].type_name, "Integer");
        assert_eq!(g.fields[1].name, "percentage");
        assert_eq!(g.fields[1].type_name, "Decimal");
        assert_eq!(g.sum_field, "percentage");
        assert_eq!(g.sum_target, "100");
    }

    #[test]
    fn repeatable_input_requires_group_block() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      repeatable input installments validates sum(percentage) = 100\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("group"));
    }

    #[test]
    fn repeatable_input_requires_numeric_sum_target() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      repeatable input g group percentage: Decimal validates sum(percentage) = whole\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("number literal"));
    }

    #[test]
    fn repeatable_input_requires_validates_clause() {
        let source = "surface a web\n  audience admin\n    view list v\n      source a.query.l\n      columns k\n      repeatable input g group percentage: Decimal\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("validates"));
    }

    #[test]
    fn repeatable_input_on_detail_view_is_allowed() {
        let source = r#"surface billing web
  audience admin
    view detail plan at "/plan/:id"
      source billing.query.by_id
      route id: Text from path
      repeatable input fees group amount: Decimal validates sum(amount) = 50
"#;
        let surface = parse_surface_document(source).expect("parses on detail");
        let detail = match &surface.audiences[0].views[0] {
            ViewAst::Detail(v) => v,
            other => panic!("expected detail, got {other:?}"),
        };
        assert_eq!(detail.ux.repeatable_groups.len(), 1);
        assert_eq!(detail.ux.repeatable_groups[0].sum_target, "50");
    }
}
