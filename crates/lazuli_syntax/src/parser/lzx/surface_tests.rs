//! Surface ViewModel parser tests.
//!
//! Exercise `parse_surface_document` end-to-end across audience, view
//! list/detail/create, drawer, filters, search, sort, and settings.
//! Preserved as `mod surface_parser_tests { ... }` so multi-line raw
//! string fixtures keep their original indentation — de-indenting
//! corrupts the canonical-indent contract that the parser asserts.

#[cfg(test)]
mod surface_parser_tests {
    use super::super::parse_surface_document;
    use crate::{
        BindingRefAst, DrawerBindingSourceAst, DrawerTriggerAst, FilterCardinalityAst,
        SearchModeAst, SelectionModeAst, SettingPersistenceAst, SettingValueSpaceAst, SortDirAst,
        SurfaceTargetAst, ViewAst,
    };

    #[test]
    fn minimal_surface_one_audience_one_view_list() {
        let source = r#"
surface slug web
  audience admin
    view list slug_list
      source slug.query.mine
      columns key, title
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, SurfaceTargetAst::Web);
        assert_eq!(surface.uses_feature, None);
        assert_eq!(surface.audiences.len(), 1);
        let audience = &surface.audiences[0];
        assert_eq!(audience.name, "admin");
        assert_eq!(audience.requires.len(), 0);
        assert_eq!(audience.views.len(), 1);
        let view = match &audience.views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected ViewAst::List, got {:?}", other),
        };
        assert_eq!(view.name, "slug_list");
        assert_eq!(view.route, None);
        assert_eq!(view.source, "slug.query.mine");
        assert_eq!(view.columns, vec!["key", "title"]);
    }

    #[test]
    fn parses_full_section_13_1_demo_fixture() {
        // Section 13.1 verbatim from
        // `docs/proposals/lzx-integration-codegen.md`.
        let source = r#"surface slug web
  uses feature slug

  audience admin
    requires @scope.workspace_admin

    view list slug_list at "/slugs"
      source slug.query.mine
      columns key, title, tags, created_at
      search key, title
      filter tags
      cells tags @client.type_badge
      actions create, update, delete

    view detail slug_detail at "/slugs/:key"
      source slug.query.by_key
      route key: Text from path
      sections header, metadata, related_items
      cells tags @client.type_badge
      actions update, delete

    view create slug_create at "/slugs/new"
      submit slug.command.create
      fields key, title, description, tags
      cells tags @client.type_badge

  audience public
    requires @scope.workspace_member

    view list public_slug_list at "/browse"
      source slug.query.mine
      columns key, title
      search key, title
"#;
        let surface = parse_surface_document(source).expect("parses §13.1 fixture");
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, SurfaceTargetAst::Web);
        assert_eq!(surface.uses_feature.as_deref(), Some("slug"));
        assert_eq!(surface.audiences.len(), 2);

        // admin audience.
        let admin = &surface.audiences[0];
        assert_eq!(admin.name, "admin");
        assert_eq!(admin.requires.len(), 1);
        assert_eq!(admin.requires[0].namespace, "scope");
        assert_eq!(admin.requires[0].name, "workspace_admin");
        assert_eq!(admin.views.len(), 3);

        let list = match &admin.views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(list.name, "slug_list");
        assert_eq!(list.route.as_deref(), Some("/slugs"));
        assert_eq!(list.columns, vec!["key", "title", "tags", "created_at"]);
        match &list.search.as_ref().expect("search").mode {
            SearchModeAst::Columns(columns) => assert_eq!(columns, &vec!["key", "title"]),
            other => panic!("expected columns search, got {other:?}"),
        }
        assert_eq!(list.filter, vec!["tags"]);
        assert_eq!(list.cells.len(), 1);
        assert_eq!(list.cells[0].field, "tags");
        assert_eq!(list.cells[0].slot, "type_badge");
        assert_eq!(list.actions, vec!["create", "update", "delete"]);

        let detail = match &admin.views[1] {
            ViewAst::Detail(v) => v,
            other => panic!("expected detail, got {:?}", other),
        };
        assert_eq!(detail.name, "slug_detail");
        assert_eq!(detail.route.as_deref(), Some("/slugs/:key"));
        assert_eq!(detail.source, "slug.query.by_key");
        assert_eq!(detail.route_params.len(), 1);
        assert_eq!(detail.route_params[0].name, "key");
        assert_eq!(detail.route_params[0].type_ref, "Text");
        assert_eq!(detail.sections, vec!["header", "metadata", "related_items"]);
        assert_eq!(detail.actions, vec!["update", "delete"]);

        let create = match &admin.views[2] {
            ViewAst::Create(v) => v,
            other => panic!("expected create, got {:?}", other),
        };
        assert_eq!(create.name, "slug_create");
        assert_eq!(create.route.as_deref(), Some("/slugs/new"));
        assert_eq!(create.submit, "slug.command.create");
        assert_eq!(create.fields, vec!["key", "title", "description", "tags"]);

        // public audience.
        let public = &surface.audiences[1];
        assert_eq!(public.name, "public");
        assert_eq!(public.requires.len(), 1);
        assert_eq!(public.requires[0].name, "workspace_member");
        assert_eq!(public.views.len(), 1);
    }

    #[test]
    fn search_segmented_block_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key
      search segmented
        field slug binds filters.slug
        field type binds filters.type
        field tag binds filters.tags
        free text into source.q
"#;
        let surface = parse_surface_document(source).expect("parses segmented search");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {other:?}"),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, SearchModeAst::Segmented);
        assert_eq!(search.fields.len(), 3);
        assert_eq!(search.fields[0].key, "slug");
        assert_eq!(
            search.fields[0].binds_to,
            BindingRefAst::Filter {
                name: "slug".to_owned()
            }
        );
        assert_eq!(
            search.free_text_target,
            Some(BindingRefAst::SourceInput {
                name: "q".to_owned()
            })
        );
    }

    #[test]
    fn search_columns_v1_form_still_parses() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search key, title\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        match &search.mode {
            SearchModeAst::Columns(columns) => assert_eq!(columns, &vec!["key", "title"]),
            other => panic!("expected columns search, got {other:?}"),
        }
        assert!(search.fields.is_empty());
        assert!(search.free_text_target.is_none());
    }

    #[test]
    fn search_segmented_rejects_inline_content() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented foo\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("takes no inline list"));
    }

    #[test]
    fn search_at_most_once() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search key
      search segmented
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn search_field_rejects_duplicate_key() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search segmented
        field slug binds filters.slug
        field slug binds source.slug
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("more than once"));
    }

    #[test]
    fn search_free_text_at_most_once() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search segmented
        free text into source.q
        free text into source.query
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("free text into"));
    }

    #[test]
    fn search_binding_ref_filter_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field slug binds filters.slug\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::Filter {
                name: "slug".to_owned()
            }
        );
    }

    #[test]
    fn search_binding_ref_source_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field q binds source.q\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::SourceInput {
                name: "q".to_owned()
            }
        );
    }

    #[test]
    fn search_binding_ref_selection_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field selected binds selection\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::SelectionScalar
        );
    }

    #[test]
    fn search_binding_ref_invalid() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field slug binds foo.bar\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("binding references"));
    }

    #[test]
    fn search_segmented_empty_block() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n";
        let surface = parse_surface_document(source).expect("parses empty segmented search");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, SearchModeAst::Segmented);
        assert!(search.fields.is_empty());
        assert!(search.free_text_target.is_none());
    }

    #[test]
    fn view_list_requires_source() {
        let source = "surface slug web\n  audience admin\n    view list bad\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("view list requires"));
    }

    #[test]
    fn view_list_no_columns_is_not_parse_time_error() {
        let source =
            "surface slug web\n  audience admin\n    view list bad\n      source slug.query.mine\n";
        let surface = parse_surface_document(source).expect("parses without columns");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert!(view.columns.is_empty());
        assert!(view.cells_slot.is_none());
    }

    #[test]
    fn view_create_requires_submit() {
        let source = "surface slug web\n  audience admin\n    view create bad\n      fields key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("view create requires"));
    }

    #[test]
    fn view_create_parses_on_success_block() {
        let source = r#"surface host web
  audience admin
    view create edit_host
      submit host.command.update_host_basic_details
      fields title
      on_success
        back
        redirect "/host/property/{result.id}"
        flash success @translation.saved
        invalidates query.lookup_my_host
        replace
"#;
        let surface = parse_surface_document(source).expect("parses on_success");
        let create = match &surface.audiences[0].views[0] {
            ViewAst::Create(v) => v,
            other => panic!("expected create, got {other:?}"),
        };
        let on_success = create.on_success.as_ref().expect("on_success");
        assert!(on_success.back);
        assert_eq!(
            on_success.redirect.as_deref(),
            Some("/host/property/{result.id}")
        );
        let flash = on_success.flash.as_ref().expect("flash");
        assert_eq!(flash.kind, "success");
        assert_eq!(flash.message_key.key, "saved");
        assert_eq!(on_success.invalidates.len(), 1);
        assert_eq!(on_success.invalidates[0].query, "query.lookup_my_host");
        assert!(on_success.replace);
    }

    #[test]
    fn view_create_parses_on_success_redirect_only() {
        let source = r#"surface host web
  audience admin
    view create create_property
      submit host.command.create_property
      fields title
      on_success
        redirect "/host/property/{result.id}"
"#;
        let surface = parse_surface_document(source).expect("parses redirect-only on_success");
        let create = match &surface.audiences[0].views[0] {
            ViewAst::Create(v) => v,
            other => panic!("expected create, got {other:?}"),
        };
        let on_success = create.on_success.as_ref().expect("on_success");
        assert!(!on_success.back);
        assert_eq!(
            on_success.redirect.as_deref(),
            Some("/host/property/{result.id}")
        );
        assert!(on_success.flash.is_none());
        assert!(on_success.invalidates.is_empty());
        assert!(!on_success.replace);
    }

    #[test]
    fn on_success_rejects_invalid_flash_kind() {
        let source = r#"surface host web
  audience admin
    view create edit_host
      submit host.command.update_host_basic_details
      fields title
      on_success
        flash warning @translation.saved
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("kind must be"));
    }

    #[test]
    fn mobile_target_recognised() {
        let source = "surface item mobile\n  audience kiosk\n    view list item_list\n      source item.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses mobile");
        assert_eq!(surface.target, SurfaceTargetAst::Mobile);
    }

    #[test]
    fn rejects_unknown_target() {
        let source = "surface slug desktop\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("surface target must be"));
    }

    #[test]
    fn rejects_top_level_indentation() {
        let source = "  surface slug web\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("top-level"));
    }

    #[test]
    fn cells_binding_parses() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      columns tags\n      cells tags @client.type_badge\n";
        let surface = parse_surface_document(source).expect("parses cells");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.cells.len(), 1);
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn view_list_accepts_cells_at_client_slot_grid_form() {
        let source = "surface item web\n  audience admin\n    view list foo at \"/\"\n      source f.query.q\n      cells @client.item_card\n";
        let surface = parse_surface_document(source).expect("parses cells grid form");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot.as_deref(), Some("item_card"));
        assert!(view.columns.is_empty());
        assert!(view.cells.is_empty());
    }

    #[test]
    fn view_list_rejects_cells_at_client_slot_with_trailing_tokens() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.foo extra\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("accepts only one slot identifier"));
    }

    #[test]
    fn view_list_rejects_double_cells_grid_form() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.item_card\n      cells @client.other_card\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn view_list_v1_per_column_cells_still_works() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      cells tags @client.type_badge\n      columns key, title\n";
        let surface = parse_surface_document(source).expect("parses per-column cells");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot, None);
        assert_eq!(view.columns, vec!["key", "title"]);
        assert_eq!(view.cells.len(), 1);
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn view_list_no_longer_requires_columns_if_cells_slot_present() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.item_card\n";
        let surface = parse_surface_document(source).expect("parses without columns");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot.as_deref(), Some("item_card"));
        assert!(view.columns.is_empty());
    }

    #[test]
    fn view_list_empty_grid_and_no_columns_does_not_error_at_parse_time() {
        let source =
            "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n";
        let surface = parse_surface_document(source).expect("parses without render declaration");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert!(view.cells_slot.is_none());
        assert!(view.columns.is_empty());
    }

    #[test]
    fn cells_binding_requires_at_client_prefix() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      columns tags\n      cells tags @server.type_badge\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("cell slot must be `@client."));
    }

    #[test]
    fn view_list_with_drawer_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key, title
      drawer item_detail on select
        source item.query.by_id
        route key from selection
        sections header, content, metadata
        cells related @client.related_items
        actions update, delete
"#;
        let surface = parse_surface_document(source).expect("parses drawer");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        let drawer = view.drawer.as_ref().expect("drawer populated");
        assert_eq!(drawer.name, "item_detail");
        assert_eq!(drawer.trigger, DrawerTriggerAst::Select);
        assert_eq!(drawer.source, "item.query.by_id");
        let route = drawer.route_binding.as_ref().expect("route binding");
        assert_eq!(route.target, "key");
        assert_eq!(route.source, DrawerBindingSourceAst::Selection);
        assert_eq!(drawer.sections, vec!["header", "content", "metadata"]);
        assert_eq!(drawer.cells.len(), 1);
        assert_eq!(drawer.cells[0].field, "related");
        assert_eq!(drawer.cells[0].slot, "related_items");
        assert_eq!(drawer.actions, vec!["update", "delete"]);
    }

    #[test]
    fn drawer_rejects_unknown_trigger() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on hover\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer trigger must be `select` or `open`")
        );
    }

    #[test]
    fn drawer_rejects_columns_inside() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        columns a, b\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer body lines are"));
    }

    #[test]
    fn drawer_rejects_filters_inside() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        filters status\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer body lines are"));
    }

    #[test]
    fn drawer_rejects_nested() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        drawer bar on select\n          source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer cannot be nested"));
    }

    #[test]
    fn view_list_at_most_one_drawer() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n      drawer bar on open\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most one `drawer`"));
    }

    #[test]
    fn drawer_grid_form_cells_rejected() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        cells @client.item_card\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer cells use `cells <field> @client.<slot>`")
        );
    }

    #[test]
    fn view_detail_rejects_drawer() {
        let source = "surface item web\n  audience admin\n    view detail item_detail\n      source item.query.by_id\n      route key: Text from path\n      drawer foo on select\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("`drawer` is only valid in `view list` bodies")
        );
    }

    #[test]
    fn route_key_from_selection_parses() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        route key from selection\n";
        let surface = parse_surface_document(source).expect("parses drawer route");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let route = view
            .drawer
            .as_ref()
            .and_then(|drawer| drawer.route_binding.as_ref())
            .expect("route binding");
        assert_eq!(route.target, "key");
        assert_eq!(route.source, DrawerBindingSourceAst::Selection);
    }

    #[test]
    fn route_key_from_path_inside_drawer_rejected() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        route key from path\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer route binding source must be `from selection`")
        );
    }

    #[test]
    fn view_list_filters_block_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key
      filters
        type: ItemType
        status: ItemStatus
        confidence: Confidence
        tags: list of Text
        slug: Text from query
"#;
        let surface = parse_surface_document(source).expect("parses filters");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters.len(), 5);
        assert_eq!(view.filters[0].name, "type");
        assert_eq!(view.filters[0].type_ref, "ItemType");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Single);
        assert!(!view.filters[0].url_sync);
        assert_eq!(view.filters[3].name, "tags");
        assert_eq!(view.filters[3].cardinality, FilterCardinalityAst::Multi);
        assert!(!view.filters[3].url_sync);
        assert_eq!(view.filters[4].name, "slug");
        assert_eq!(view.filters[4].cardinality, FilterCardinalityAst::Single);
        assert!(view.filters[4].url_sync);
    }

    #[test]
    fn filters_single_from_query() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from query\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters[0].name, "slug");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Single);
        assert!(view.filters[0].url_sync);
    }

    #[test]
    fn filters_multi_from_query() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        tags: list of Text from query\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters[0].name, "tags");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Multi);
        assert!(view.filters[0].url_sync);
    }

    #[test]
    fn filters_rejects_from_path() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from path\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("from query"));
    }

    #[test]
    fn filters_rejects_duplicate_name() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        tags: list of Text\n        tags: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("duplicate filter `tags`"));
    }

    #[test]
    fn filters_rejects_empty_block() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n      actions update\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires at least one"));
    }

    #[test]
    fn view_detail_rejects_filters() {
        let source = "surface item web\n  audience admin\n    view detail a\n      source item.query.by_id\n      filters\n        slug: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("only valid in `view list`"));
    }

    #[test]
    fn view_create_rejects_filters() {
        let source = "surface item web\n  audience admin\n    view create a\n      submit item.command.create\n      fields key\n      filters\n        slug: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("only valid in `view list`"));
    }

    #[test]
    fn view_list_at_most_one_filters_block() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text\n      filters\n        tags: list of Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn filters_missing_type_ref() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug:\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires a type"));
    }

    #[test]
    fn multiple_audiences_per_surface() {
        let source = r#"surface slug web
  audience admin
    requires @scope.workspace_admin
    view list a
      source slug.query.mine
      columns key

  audience public
    requires @scope.workspace_member
    view list b
      source slug.query.mine
      columns key
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.audiences.len(), 2);
        assert_eq!(surface.audiences[0].name, "admin");
        assert_eq!(surface.audiences[1].name, "public");
    }

    #[test]
    fn multiple_views_per_audience() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
    view list b
      source slug.query.mine
      columns key
    view detail c at "/x/:id"
      source slug.query.by_key
      route id: Text from path
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.audiences[0].views.len(), 3);
    }

    #[test]
    fn empty_audience_parses_cleanly() {
        let source = "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n";
        let surface = parse_surface_document(source).expect("parses empty audience");
        assert_eq!(surface.audiences.len(), 1);
        assert_eq!(surface.audiences[0].views.len(), 0);
        assert_eq!(surface.audiences[0].requires.len(), 1);
    }

    #[test]
    fn actions_comma_separated_list() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions create, update, delete\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions, vec!["create", "update", "delete"]);
    }

    #[test]
    fn at_path_optional() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.route, None);
    }

    #[test]
    fn rejects_partial_overrides() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      columns += score\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("partial overrides"));
    }

    #[test]
    fn route_param_captures_type_text() {
        let source = "surface slug web\n  audience admin\n    view detail d at \"/s/:id\"\n      source slug.query.by_key\n      route id: Customer.ID from path\n";
        let surface = parse_surface_document(source).expect("parses");
        let detail = match &surface.audiences[0].views[0] {
            ViewAst::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.route_params[0].name, "id");
        assert_eq!(detail.route_params[0].type_ref, "Customer.ID");
    }

    #[test]
    fn uses_feature_override_captured() {
        let source = "surface slug web\n  uses feature slug\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.uses_feature.as_deref(), Some("slug"));
    }

    #[test]
    fn requires_scope_atom_captured() {
        let source = "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        let atom = &surface.audiences[0].requires[0];
        assert_eq!(atom.namespace, "scope");
        assert_eq!(atom.name, "workspace_admin");
    }

    #[test]
    fn requires_rejects_unknown_namespace() {
        let source = "surface slug web\n  audience admin\n    requires @group.workspace_admin\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("namespace"));
    }

    #[test]
    fn rejects_blank_document() {
        let source = "\n\n# comment only\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(matches!(
            err,
            super::super::super::super::error::ParseError::Expected { .. }
        ));
    }

    #[test]
    fn comments_and_blank_lines_skipped() {
        let source = r#"# header comment

surface slug web
  # mid comment
  audience admin

    view list a
      # explanatory
      source slug.query.mine
      columns key
"#;
        let surface = parse_surface_document(source).expect("parses with comments");
        assert_eq!(surface.audiences[0].views.len(), 1);
    }

    #[test]
    fn at_path_requires_leading_slash() {
        let source = "surface slug web\n  audience admin\n    view list a at \"slugs\"\n      source slug.query.mine\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("must begin with `/`"));
    }

    #[test]
    fn view_create_with_route_at() {
        let source = "surface slug web\n  audience admin\n    view create new at \"/slugs/new\"\n      submit slug.command.create\n      fields key\n";
        let surface = parse_surface_document(source).expect("parses");
        let create = match &surface.audiences[0].views[0] {
            ViewAst::Create(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(create.route.as_deref(), Some("/slugs/new"));
        assert_eq!(create.submit, "slug.command.create");
    }

    #[test]
    fn sort_block_parses() {
        let source = r#"surface item web
  audience admin
    view list terminal
      source item.query.search
      columns title
      sort
        by title, type, priority, updated
        default updated desc
"#;
        let surface = parse_surface_document(source).expect("parses sort");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let sort = list.sort.as_ref().expect("sort");
        assert_eq!(sort.allowed, vec!["title", "type", "priority", "updated"]);
        assert_eq!(sort.default_field, "updated");
        assert_eq!(sort.default_dir, SortDirAst::Desc);
    }

    #[test]
    fn sort_requires_by_line() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        default title asc\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires a `by`"));
    }

    #[test]
    fn sort_default_field_must_be_allowed() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title\n        default updated desc\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("must be listed"));
    }

    #[test]
    fn sort_default_requires_dir() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title\n        default title\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("default <field>"));
    }

    #[test]
    fn selection_single_and_multi_parse() {
        let source = r#"surface item web
  audience admin
    view list single_view
      source item.query.search
      columns title
      selection single
    view list multi_view
      source item.query.search
      columns title
      selection multi
"#;
        let surface = parse_surface_document(source).expect("parses selection");
        let single = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        let multi = match &surface.audiences[0].views[1] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(single.mode, SelectionModeAst::Single);
        assert_eq!(multi.mode, SelectionModeAst::Multi);
    }

    #[test]
    fn selection_none_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      selection none\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("omit the line"));
    }

    #[test]
    fn selection_unknown_mode_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      selection foo\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("selection single"));
    }

    #[test]
    fn bulk_actions_single_and_multi_parse() {
        let source = r#"surface item web
  audience admin
    view list one
      source item.query.search
      columns title
      selection multi
      bulk_actions delete
    view list many
      source item.query.search
      columns title
      selection multi
      bulk_actions delete, archive
"#;
        let surface = parse_surface_document(source).expect("parses bulk actions");
        let one = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        let many = match &surface.audiences[0].views[1] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(one.bulk_actions, vec!["delete"]);
        assert_eq!(many.bulk_actions, vec!["delete", "archive"]);
    }

    #[test]
    fn bulk_actions_duplicate_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      bulk_actions delete\n      bulk_actions archive\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("bulk_actions"));
    }

    #[test]
    fn bulk_actions_without_selection_is_not_parser_error() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      bulk_actions delete\n";
        let surface = parse_surface_document(source).expect("bulk-only parses");
        let selection = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(selection.mode, SelectionModeAst::None);
        assert_eq!(selection.bulk_actions, vec!["delete"]);
    }

    #[test]
    fn settings_full_example_parses() {
        let source = r#"surface item web
  audience admin
    view list terminal
      source item.query.search
      columns title
      settings
        grid_size: Enum [sm, md, lg] default sm
          persist local
        show_metadata: Bool default true
        page_size: Int min 10 max 200 default 25
          persist workspace
"#;
        let surface = parse_surface_document(source).expect("parses settings");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(list.settings.len(), 3);
        assert_eq!(list.settings[0].name, "grid_size");
        assert_eq!(
            list.settings[0].value_space,
            SettingValueSpaceAst::Enum(vec!["sm".into(), "md".into(), "lg".into()])
        );
        assert_eq!(list.settings[0].default, "sm");
        assert_eq!(list.settings[0].persistence, SettingPersistenceAst::Local);
        assert_eq!(list.settings[1].value_space, SettingValueSpaceAst::Bool);
        assert_eq!(
            list.settings[2].value_space,
            SettingValueSpaceAst::Int {
                min: Some(10),
                max: Some(200)
            }
        );
        assert_eq!(
            list.settings[2].persistence,
            SettingPersistenceAst::Workspace
        );
    }

    #[test]
    fn persist_outside_setting_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      persist local\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("persist"));
    }

    #[test]
    fn duplicate_setting_name_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        grid_size: Bool default true\n        grid_size: Bool default false\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("duplicate setting"));
    }

    #[test]
    fn enum_default_must_be_member() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        grid_size: Enum [sm, md] default lg\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("not in the enum"));
    }

    #[test]
    fn int_default_must_be_in_range() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        page_size: Int min 10 max 200 default 5\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("below"));
    }

    #[test]
    fn settings_empty_block_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at least one setting"));
    }

    #[test]
    fn list_only_keywords_rejected_in_detail_and_create() {
        let detail = "surface item web\n  audience admin\n    view detail terminal\n      source item.query.by_id\n      sort\n        by title\n        default title asc\n";
        let create = "surface item web\n  audience admin\n    view create terminal\n      submit item.command.create\n      selection multi\n";
        let detail_err = parse_surface_document(detail).unwrap_err();
        let create_err = parse_surface_document(create).unwrap_err();
        assert!(detail_err.to_string().contains("valid only in `view list`"));
        assert!(create_err.to_string().contains("valid only in `view list`"));
    }
}
