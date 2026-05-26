//! `filters` block tests for `view list`.
//!
//! Sibling of `mod.rs`. Raw-string fixtures preserved verbatim.

#[cfg(test)]
mod filters_tests {
    use super::super::super::parse_surface_document;
    use crate::{FilterCardinalityAst, ViewAst};

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
}
