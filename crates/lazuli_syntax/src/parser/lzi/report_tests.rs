
    use super::super::parse_feature_skeletons;
    use crate::ast::ReportColumnSourceAst;

    #[test]
    fn report_full_block_parses() {
        let source = r#"
feature customer
  report monthly_audit
    source customer.query.list
    columns
      id from row.id
      name from row.name
      tier from row.tier label "Plano"
      ltv from @fn.lifetime_value(row.id) label "Valor de vida"
      created_at from row.created_at format "yyyy-mm-dd"
    formats csv, xlsx
    storage object_storage.files
    visibility signed
    signed_ttl 1h
    filename "monthly_audit_{ctx.now:yyyymm}.{format}"
    policy @policy.global_read
    rate_limit "10 per hour per user"
    audit actor, ctx.now, source.params
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].reports.len(), 1);
        let report = &features[0].reports[0];
        assert_eq!(report.name, "monthly_audit");
        assert_eq!(report.source, "customer.query.list");
        assert_eq!(report.columns.len(), 5);
        assert!(matches!(
            &report.columns[0].source,
            ReportColumnSourceAst::RowField(f) if f == "id"
        ));
        assert!(matches!(
            &report.columns[3].source,
            ReportColumnSourceAst::FnCall { name, args }
                if name == "lifetime_value" && args == &["row.id"]
        ));
        assert_eq!(report.columns[2].label.as_deref(), Some("Plano"));
        assert_eq!(report.columns[4].format.as_deref(), Some("yyyy-mm-dd"));
        assert_eq!(report.formats, vec!["csv".to_owned(), "xlsx".to_owned()]);
        assert_eq!(report.storage.as_deref(), Some("object_storage.files"));
        assert_eq!(report.visibility.as_deref(), Some("signed"));
        assert_eq!(report.signed_ttl.as_deref(), Some("1h"));
        assert_eq!(report.policy.as_deref(), Some("@policy.global_read"));
        let audit = report.audit.as_ref().expect("audit");
        assert_eq!(
            audit.subjects,
            vec![
                "actor".to_owned(),
                "ctx.now".to_owned(),
                "source.params".to_owned()
            ]
        );
    }

    #[test]
    fn report_input_block_parses() {
        // W5 GAP-REPORT-01 — canonical `report input { … }` syntax.
        let source = r#"
feature billing
  report billing_summary
    input
      period_start: Date required
      period_end: Date required
      format: CSV
    source billing.query.billing_rows
    columns
      id from row.id
    formats csv
    policy @policy.global_read
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let report = &features[0].reports[0];
        assert_eq!(report.input.len(), 3);
        assert_eq!(report.input[0].name, "period_start");
        assert_eq!(report.input[0].type_text, "Date");
        assert!(report.input[0].required);
        assert_eq!(report.input[1].name, "period_end");
        assert!(report.input[1].required);
        assert_eq!(report.input[2].name, "format");
        assert_eq!(report.input[2].type_text, "CSV");
        assert!(!report.input[2].required);
        // `source` / `columns` still parse alongside the new block.
        assert_eq!(report.source, "billing.query.billing_rows");
        assert_eq!(report.columns.len(), 1);
    }

    #[test]
    fn report_without_input_block_has_empty_input() {
        let source = r#"
feature customer
  report monthly_audit
    source customer.query.list
    columns
      id from row.id
    formats csv
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        assert!(features[0].reports[0].input.is_empty());
    }

    #[test]
    fn report_missing_source_errors() {
        let source = r#"
feature customer
  report broken
    columns
      id from row.id
    formats csv
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("source"));
    }

    #[test]
    fn report_missing_formats_errors() {
        let source = r#"
feature customer
  report broken
    source customer.query.list
    columns
      id from row.id
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("formats"));
    }

    #[test]
    fn report_column_unknown_source_errors() {
        let source = r#"
feature customer
  report broken
    source customer.query.list
    columns
      id from bogus.id
    formats csv
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("row.<field>") || err.to_string().contains("@fn"));
    }
