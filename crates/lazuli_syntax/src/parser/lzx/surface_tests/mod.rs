//! Surface ViewModel parser tests — top-level container.
//!
//! Exercise `parse_surface_document` end-to-end across audience, view
//! list/detail/create, drawer, filters, search, sort, and settings.
//! Preserved as `mod surface_parser_tests { ... }` so multi-line raw
//! string fixtures keep their original indentation — de-indenting
//! corrupts the canonical-indent contract that the parser asserts.
//!
//! Topic-specific sub-files (search, drawer/cells, filters, sort +
//! selection + settings) live as siblings to keep every file under the
//! 500-LOC ceiling. The general / core tests stay here.

#[cfg(test)]
mod drawer;
#[cfg(test)]
mod filters;
#[cfg(test)]
mod search;
#[cfg(test)]
mod sort_selection_settings;

#[cfg(test)]
mod surface_parser_tests {
    use super::super::parse_surface_document;
    use crate::{SearchModeAst, SurfaceTargetAst, ViewAst};

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
}
