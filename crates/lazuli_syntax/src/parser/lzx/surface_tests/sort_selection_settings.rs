//! `sort`, `selection`, `bulk_actions`, and `settings` tests for `view list`.
//!
//! Sibling of `mod.rs`. Raw-string fixtures preserved verbatim so the
//! parser's canonical-indent contract continues to hold.

#[cfg(test)]
mod sort_selection_settings_tests {
    use super::super::super::parse_surface_document;
    use crate::{
        SelectionModeAst, SettingPersistenceAst, SettingValueSpaceAst, SortDirAst, ViewAst,
    };

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
        let create = "surface item web\n  audience admin\n    view create terminal\n      submit item.command.create\n      fields key\n      selection multi\n";
        let detail_err = parse_surface_document(detail).unwrap_err();
        let create_err = parse_surface_document(create).unwrap_err();
        assert!(detail_err.to_string().contains("valid only in `view list`"));
        assert!(create_err.to_string().contains("valid only in `view list`"));
    }
}
