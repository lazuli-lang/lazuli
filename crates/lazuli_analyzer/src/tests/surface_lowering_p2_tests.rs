    #[test]
    fn span_ref_attached_after_lowering() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        assert!(surface.span_ref.is_some());
        assert!(surface.audiences[0].span_ref.is_some());
    }

    #[test]
    fn audience_view_count_preserves_source_order() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list b\n      source slug.query.mine\n      columns key\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let names: Vec<&str> = surface.audiences[0]
            .views
            .iter()
            .map(|v| v.name())
            .collect();
        assert_eq!(names, vec!["b", "a"]);
    }

    // ── Wave-W6 surface UX primitives ──────────────────────────────────────

    #[test]
    fn lowers_date_range_filter_cardinality() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        created: date_range\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filter[0].name, "created");
        assert_eq!(view.filter[0].cardinality, ir::FilterCardinality::DateRange);
    }

    #[test]
    fn lowers_wizard_steps_and_tab_group_on_view() {
        let surface = parse(
            "surface item web\n  audience admin\n    view detail d at \"/d/:id\"\n      source item.query.by_id\n      route id: Text from path\n      wizard_steps 3 current step\n      tab_group derived_from kind\n        case TV, RADIO -> tab \"Broadcast\"\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::Detail(v) => v,
            _ => unreachable!(),
        };
        let steps = view.ux.wizard_steps.as_ref().expect("wizard_steps");
        assert_eq!(steps.total, 3);
        assert_eq!(steps.current_field, "step");
        let group = view.ux.tab_group.as_ref().expect("tab_group");
        assert_eq!(group.derived_from, "kind");
        assert_eq!(group.cases[0].variants, vec!["TV", "RADIO"]);
        assert_eq!(group.cases[0].label, "Broadcast");
    }

    #[test]
    fn lowers_view_mode_and_inline_table() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      view_mode\n        table\n        kanban\n      view.inline_table on_change update_row\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.ux.view_modes,
            vec![ir::RenderMode::Table, ir::RenderMode::Kanban]
        );
        let inline = view.ux.inline_table.as_ref().expect("inline_table");
        assert_eq!(inline.on_change.feature, "item");
        assert_eq!(inline.on_change.name, "update_row");
    }

    #[test]
    fn lowers_board_and_repeatable_group() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      view.board activity_board\n        lanes derived_from status\n      repeatable input installments group days: Integer, percentage: Decimal validates sum(percentage) = 100\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let board = view.ux.board.as_ref().expect("board");
        assert_eq!(board.name, "activity_board");
        assert_eq!(board.lanes_source, "status");
        assert_eq!(view.ux.repeatable_groups.len(), 1);
        let g = &view.ux.repeatable_groups[0];
        assert_eq!(g.name, "installments");
        assert_eq!(g.fields.len(), 2);
        assert_eq!(g.fields[1].name, "percentage");
        assert_eq!(g.fields[1].type_name, "Decimal");
        assert_eq!(g.sum_field, "percentage");
        assert_eq!(g.sum_target, "100");
    }

    #[test]
    fn unknown_view_mode_is_lowering_error() {
        let ast = parse_surface_document(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      view_mode\n        hologram\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxUnknownRenderMode { .. }));
    }

    #[test]
    fn lowers_audience_tabs_and_wizard() {
        let surface = parse(
            "surface job web\n  audience admin\n    view detail detail at \"/j/:id\"\n      source job.query.by_id\n      route id: Text from path\n    tabs\n      tab \"Details\" -> view detail\n    wizard flow steps\n      step 1: detail\n",
        );
        let aud = &surface.audiences[0];
        assert_eq!(aud.ux.tabs.len(), 1);
        assert_eq!(aud.ux.tabs[0].entries[0].view, "detail");
        assert_eq!(aud.ux.wizards.len(), 1);
        assert_eq!(aud.ux.wizards[0].name, "flow");
        assert_eq!(aud.ux.wizards[0].steps[0].ref_name, "detail");
    }
