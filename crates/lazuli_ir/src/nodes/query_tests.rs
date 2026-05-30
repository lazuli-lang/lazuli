
    use super::*;

    #[test]
    fn query_name_dispatches_across_kinds() {
        let v = Query::List(ListQuery {
            name: "all".into(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            order: vec![],
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        });
        assert_eq!(v.name(), "all");
    }

    #[test]
    fn sql_query_kind_default_is_sql() {
        assert!(SqlQueryKind::default().is_sql());
        assert!(!SqlQueryKind::View.is_sql());
    }

    #[test]
    fn cache_ttl_literal_round_trips_minutes() {
        let v = CacheTtl::Literal(CacheTtlLiteral::Minutes(5));
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"kind\":\"Literal\""));
        assert!(s.contains("\"unit\":\"Minutes\""));
        assert!(s.contains("\"amount\":5"));
        let back: CacheTtl = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn path_from_segments_collects_strs() {
        let p = Path::from_segments(["a", "b", "c"]);
        assert_eq!(p.segments, vec!["a", "b", "c"]);
    }

    #[test]
    fn predicate_comparison_round_trips() {
        let v = Predicate::Comparison {
            left: Expr::Path(Path::from_segments(["x"])),
            op: CompareOp::Eq,
            right: Expr::Integer(7),
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: Predicate = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn order_dir_round_trips() {
        let v = OrderDir::Asc;
        let s = serde_json::to_string(&v).expect("serialize");
        let back: OrderDir = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }
