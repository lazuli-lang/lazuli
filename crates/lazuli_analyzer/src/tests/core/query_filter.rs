    use lazuli_ir as ir;

    use lazuli_syntax::{parse_feature_skeletons, parse_lzx_document};

    use crate::auth::lower_auth_identity;
    use crate::query::parse_query_filter_line;
    use crate::resource::lower_validate_line;
    use crate::{
        AnalyzeError, lower_audit_block, lower_feature_skeleton, lower_lzx_document,
        lower_policy_atom_with_args, parse_cap_file_type, resolve_invalidates_targets,
        type_ref_from_syntax,
    };


    #[test]
    fn query_filter_line_lowers_dotted_path() {
        let filter = parse_query_filter_line("org_id = ctx.actor.org_id")
            .expect("dotted path filter parses");
        let ir::Predicate::Comparison { left, op, right } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        assert!(matches!(op, ir::CompareOp::Eq));
        assert_eq!(
            left,
            ir::Expr::Path(ir::Path::from_segments(["org_id".to_owned()]))
        );
        assert_eq!(
            right,
            ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "org_id".to_owned(),
            ]))
        );
        assert!(filter.when.is_none());
    }

    #[test]
    fn query_filter_line_lowers_bool_literal() {
        let filter = parse_query_filter_line("is_public = false").unwrap();
        let ir::Predicate::Comparison { right, .. } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        assert_eq!(right, ir::Expr::Boolean(false));
    }

    #[test]
    fn query_filter_line_lifts_bare_identifier_to_enum_literal() {
        // WAR-VOCAB-QUERY-ENUM-01 closure: `status = approved` must
        // lift `approved` to `Expr::Enum` so codegen emits a TEXT
        // const bind, NOT a runtime input lookup.
        let filter = parse_query_filter_line("status = approved").unwrap();
        let ir::Predicate::Comparison { right, .. } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        let literal = match right {
            ir::Expr::Enum(literal) => literal,
            other => panic!("expected Expr::Enum, got {other:?}"),
        };
        assert!(literal.type_name.is_none());
        assert_eq!(literal.variant, "approved");
    }

    #[test]
    fn query_filter_line_handles_inequality_operators() {
        let f1 = parse_query_filter_line("rating >= 4").unwrap();
        if let ir::Predicate::Comparison { op, .. } = f1.predicate {
            assert!(matches!(op, ir::CompareOp::Ge));
        } else {
            panic!("expected Comparison");
        }
        let f2 = parse_query_filter_line("status != cancelled").unwrap();
        if let ir::Predicate::Comparison { op, right, .. } = f2.predicate {
            assert!(matches!(op, ir::CompareOp::Ne));
            if let ir::Expr::Enum(literal) = right {
                assert_eq!(literal.variant, "cancelled");
            } else {
                panic!("expected Enum literal on RHS of !=");
            }
        } else {
            panic!("expected Comparison");
        }
    }

    #[test]
    fn query_filter_line_drops_blanks_and_comments() {
        assert!(parse_query_filter_line("").is_none());
        assert!(parse_query_filter_line("   ").is_none());
        assert!(parse_query_filter_line("# org_id = ctx.actor.org_id").is_none());
    }

    #[test]
    fn query_filter_line_lowers_quoted_string() {
        let filter = parse_query_filter_line("name = \"hello\"").unwrap();
        if let ir::Predicate::Comparison { right, .. } = filter.predicate {
            assert_eq!(right, ir::Expr::String("hello".to_owned()));
        } else {
            panic!("expected Comparison");
        }
    }

    #[test]
    fn query_filter_line_lowers_integer_and_nil() {
        let f1 = parse_query_filter_line("count >= 0").unwrap();
        if let ir::Predicate::Comparison { right, .. } = f1.predicate {
            assert_eq!(right, ir::Expr::Integer(0));
        } else {
            panic!("expected Comparison");
        }
        let f2 = parse_query_filter_line("deleted_at = nil").unwrap();
        if let ir::Predicate::Comparison { right, .. } = f2.predicate {
            assert_eq!(right, ir::Expr::Nil);
        } else {
            panic!("expected Comparison");
        }
    }
