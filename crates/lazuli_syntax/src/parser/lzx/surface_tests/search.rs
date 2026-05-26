//! `search` declaration tests for `view list` — segmented + columns modes,
//! binding refs, free-text targets, and the various malformed shapes
//! doctor surfaces.
//!
//! Sibling of `mod.rs`. Lives in its own file so the parent module stays
//! under the 500-LOC ceiling. Raw-string fixtures preserved verbatim —
//! de-indenting them corrupts the canonical-indent contract.

#[cfg(test)]
mod search_tests {
    use super::super::super::parse_surface_document;
    use crate::{BindingRefAst, SearchModeAst, ViewAst};

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
}
