//! Cells + drawer + route-binding tests for `view list`.
//!
//! Sibling of `mod.rs`. Raw-string fixtures preserved verbatim so the
//! parser's canonical-indent contract continues to hold.

#[cfg(test)]
mod drawer_tests {
    use super::super::super::parse_surface_document;
    use crate::{DrawerBindingSourceAst, DrawerTriggerAst, ViewAst};

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
}
