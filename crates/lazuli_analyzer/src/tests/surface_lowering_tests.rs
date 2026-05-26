    use crate::{AnalyzeError, lower_surface};
    use lazuli_ir as ir;
    use lazuli_syntax::parse_surface_document;

    fn parse(src: &str) -> ir::Surface {
        let ast = parse_surface_document(src).expect("parses");
        lower_surface(&ast).expect("lowers")
    }

    fn parse_requires(atom: &str) -> ir::PolicyAtom {
        let source = format!("surface slug web\n  audience admin\n    requires {atom}\n");
        let surface = parse(&source);
        surface.audiences[0].requires[0].clone()
    }

    #[test]
    fn lowers_minimal_surface() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, ir::SurfaceTarget::Web);
        assert_eq!(surface.audiences.len(), 1);
        assert_eq!(surface.audiences[0].views.len(), 1);
    }

    #[test]
    fn session_fresh_policy_atom_lowers() {
        let atom = parse_requires("@session.fresh(15m)");
        assert_eq!(atom.namespace, "session");
        assert_eq!(atom.name, "fresh");
        assert_eq!(atom.args.as_deref(), Some("15m"));
    }

    #[test]
    fn rate_budget_policy_atom_lowers() {
        let atom = parse_requires("@rate_budget.password_reset");
        assert_eq!(atom.namespace, "rate_budget");
        assert_eq!(atom.name, "password_reset");
        assert!(atom.args.is_none());
    }

    #[test]
    fn time_policy_atom_lowers() {
        let atom = parse_requires("@time.business_hours_brasilia(tz:America/Sao_Paulo)");
        assert_eq!(atom.namespace, "time");
        assert_eq!(atom.name, "business_hours_brasilia");
        assert_eq!(atom.args.as_deref(), Some("tz:America/Sao_Paulo"));
    }

    #[test]
    fn view_redacted_fields_lower() {
        let surface = parse(
            "surface customer web\n  audience admin\n    view create invite\n      submit customer.command.invite\n      fields email redacted\n",
        );
        let ir::View::Create(view) = &surface.audiences[0].views[0] else {
            panic!("expected create view");
        };
        assert_eq!(view.fields, vec!["email".to_owned()]);
        assert_eq!(view.redacted_fields, vec!["email".to_owned()]);
    }

    #[test]
    fn list_view_lowers_table_render_search_and_legacy_filter_names() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key, title\n      search key\n      filter title\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.render,
            ir::ListRender::Table {
                columns: vec!["key".into(), "title".into()]
            }
        );
        assert_eq!(
            view.search.as_ref().map(|search| &search.mode),
            Some(&ir::SearchMode::Columns {
                columns: vec!["key".into()]
            })
        );
        assert_eq!(view.filter.len(), 1);
        assert_eq!(view.filter[0].name, "title");
    }

    #[test]
    fn list_view_lowers_cells_render() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list cards\n      source item.query.search\n      cells @client.item_card\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.render,
            ir::ListRender::Cells {
                slot: "item_card".into()
            }
        );
    }

    #[test]
    fn lowers_filter_decl_block_to_typed_ir() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from query\n        tags: list of Text\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filter.len(), 2);
        assert_eq!(view.filter[0].name, "slug");
        assert_eq!(view.filter[0].type_ref, "Text");
        assert_eq!(view.filter[0].cardinality, ir::FilterCardinality::Single);
        assert!(view.filter[0].url_sync);
        assert_eq!(view.filter[1].cardinality, ir::FilterCardinality::Multi);
    }

    #[test]
    fn lowers_segmented_search_decl_bindings() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      search segmented\n        field slug binds filters.slug\n        field q binds source.search\n        free text into selection\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, ir::SearchMode::Segmented);
        assert_eq!(
            search.fields[0].binds_to,
            ir::BindingRef::Filter {
                name: "slug".into()
            }
        );
        assert_eq!(
            search.fields[1].binds_to,
            ir::BindingRef::SourceInput {
                name: "search".into()
            }
        );
        assert_eq!(
            search.free_text_target,
            Some(ir::BindingRef::SelectionScalar)
        );
    }

    #[test]
    fn lowers_drawer_subview() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer item_detail on select\n        source item.query.by_id\n        route key from selection\n        sections header, meta\n        cells owner @client.owner_card\n        actions update\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let drawer = view.drawer.as_ref().expect("drawer");
        assert_eq!(drawer.name, "item_detail");
        assert_eq!(drawer.trigger, ir::DrawerTrigger::Select);
        assert_eq!(drawer.source.name, "by_id");
        assert_eq!(drawer.route_binding.as_ref().unwrap().target, "key");
        assert_eq!(drawer.sections, vec!["header", "meta"]);
        assert_eq!(drawer.cells[0].slot, "owner_card");
        assert_eq!(drawer.actions[0].name, "update");
    }

    #[test]
    fn lowers_sort_selection_and_settings() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title, updated\n        default updated desc\n      selection multi\n      bulk_actions delete\n      settings\n        grid_size: Enum [sm, md] default sm\n          persist local\n        page_size: Int min 10 max 200 default 25\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let sort = view.sort.as_ref().expect("sort");
        assert_eq!(sort.allowed, vec!["title", "updated"]);
        assert_eq!(sort.default_dir, ir::SortDir::Desc);
        let selection = view.selection.as_ref().expect("selection");
        assert_eq!(selection.mode, ir::SelectionMode::Multi);
        assert_eq!(selection.bulk_actions[0].name, "delete");
        assert_eq!(view.settings.len(), 2);
        assert_eq!(
            view.settings[0].value_space,
            ir::SettingValueSpace::Enum {
                values: vec!["sm".into(), "md".into()]
            }
        );
        assert_eq!(view.settings[0].persistence, ir::SettingPersistence::Local);
        assert_eq!(
            view.settings[1].value_space,
            ir::SettingValueSpace::Int { min: 10, max: 200 }
        );
    }

    #[test]
    fn detail_view_lifts_route_params_and_sections() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.by_key\n      route key: Text from path\n      sections header, metadata\n",
        );
        let detail = match &surface.audiences[0].views[0] {
            ir::View::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.route.as_deref(), Some("/s/:key"));
        assert_eq!(detail.route_params.len(), 1);
        assert_eq!(detail.route_params[0].name, "key");
        assert_eq!(detail.route_params[0].type_ref, "Text");
        assert_eq!(detail.sections, vec!["header", "metadata"]);
    }

    #[test]
    fn create_view_lifts_submit_command_and_fields() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view create n at \"/s/new\"\n      submit slug.command.create\n      fields key, title\n",
        );
        let create = match &surface.audiences[0].views[0] {
            ir::View::Create(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(create.submit.feature, "slug");
        assert_eq!(create.submit.name, "create");
        assert_eq!(create.fields, vec!["key", "title"]);
    }

    #[test]
    fn create_view_lifts_on_success_to_ir() {
        let surface = parse(
            "surface host web\n  audience admin\n    view create edit_host\n      submit host.command.update_host_basic_details\n      fields title\n      on_success\n        back\n        flash success @translation.saved\n        invalidates query.lookup_my_host\n",
        );
        let create = match &surface.audiences[0].views[0] {
            ir::View::Create(v) => v,
            _ => unreachable!(),
        };
        let on_success = create.on_success.as_ref().expect("on_success");
        assert!(on_success.back);
        let flash = on_success.flash.as_ref().expect("flash");
        assert_eq!(flash.kind, "success");
        assert_eq!(flash.message_key.key, "saved");
        assert_eq!(on_success.invalidates.len(), 1);
        assert_eq!(
            on_success.invalidates[0].query.feature.as_deref(),
            Some("host")
        );
        assert_eq!(on_success.invalidates[0].query.name, "lookup_my_host");
    }

    #[test]
    fn requires_lifts_to_policy_atom() {
        let surface = parse(
            "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let req = &surface.audiences[0].requires[0];
        assert_eq!(req.namespace, "scope");
        assert_eq!(req.name, "workspace_admin");
    }

    #[test]
    fn query_ref_disambiguates_kind_via_prefix() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.lookup.by_key\n      route key: Text from path\n",
        );
        let detail = match &surface.audiences[0].views[0] {
            ir::View::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.source.feature, "slug");
        assert_eq!(detail.source.kind, ir::QueryKind::Lookup);
        assert_eq!(detail.source.name, "by_key");
    }

    #[test]
    fn query_ref_unqualified_defaults_to_list() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.source.kind, ir::QueryKind::List);
        assert_eq!(view.source.name, "mine");
    }

    #[test]
    fn actions_short_form_lifts_owning_feature() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions create, update\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions.len(), 2);
        for action in &view.actions {
            assert_eq!(action.feature, "slug");
        }
        assert_eq!(view.actions[0].name, "create");
        assert_eq!(view.actions[1].name, "update");
    }

    #[test]
    fn actions_qualified_form_keeps_explicit_feature() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions other.command.archive\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions[0].feature, "other");
        assert_eq!(view.actions[0].name, "archive");
    }

    #[test]
    fn cell_binding_lifts_to_ir_cell_binding() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns tags\n      cells tags @client.type_badge\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn route_param_orphan_error() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.by_key\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(
            err,
            AnalyzeError::LzxRouteParamMissingBinding { .. }
        ));
    }

    #[test]
    fn route_param_extra_without_placeholder_error() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view detail d at \"/s/x\"\n      source slug.query.by_key\n      route key: Text from path\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxRouteParamOrphan { .. }));
    }

    #[test]
    fn cell_slot_orphan_when_field_not_in_columns() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key, title\n      cells tags @client.type_badge\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxCellSlotOrphan { .. }));
    }

    #[test]
    fn bad_query_ref_rejected_at_lowering() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view list a\n      source bogus_thing\n      columns key\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxBadQueryRef { .. }));
    }

    #[test]
    fn lowers_full_section_13_1_fixture() {
        // Smoke: the proposal §13.1 fixture lowers cleanly end-to-end.
        let surface = parse(include_str!("../../tests/fixtures/slug_web_section_13_1.lzx"));
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.audiences.len(), 2);
        assert_eq!(surface.audiences[0].views.len(), 3);
        let admin_list = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(admin_list.cells[0].slot, "type_badge");
        assert_eq!(admin_list.actions.len(), 3);
    }

    #[test]
    fn mobile_target_lowers_to_mobile_variant() {
        let surface = parse(
            "surface item mobile\n  audience kiosk\n    view list a\n      source item.query.mine\n      columns key\n",
        );
        assert_eq!(surface.target, ir::SurfaceTarget::Mobile);
    }

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
