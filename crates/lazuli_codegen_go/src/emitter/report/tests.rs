//! Tests for report emission. Split out of `mod.rs` to keep
//! production under the 500 LOC budget.

    use super::*;
    use lazuli_ir::{
        Defaults, FileVisibility, Policies, PolicyCategory, PolicyRef, QualifiedName, Report,
        ReportColumn, ReportColumnSource, ReportFormat, ReportSource,
    };

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            knowledge: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
                rate_limit: None,
                audit: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn mk_report(name: &str, formats: Vec<ReportFormat>) -> Report {
        Report {
            name: name.to_owned(),
            input: vec![],
            source: ReportSource::Query(QualifiedName {
                feature: None,
                name: "list".to_owned(),
            }),
            columns: vec![ReportColumn {
                name: "id".to_owned(),
                source: ReportColumnSource::RowField("id".to_owned()),
                label: None,
                format: None,
                span_ref: None,
            }],
            formats,
            storage: None,
            visibility: FileVisibility::Signed,
            signed_ttl: Some("1h".to_owned()),
            filename: None,
            policy: PolicyRef::Local("global_read".to_owned()),
            policy_expr: None,
            rate_limit: None,
            audit: None,
            span_ref: None,
        }
    }

    #[test]
    fn empty_feature_emits_nothing() {
        let feature = base_feature("customer");
        assert!(emit_reports_file("examples/x.lzi", &feature).is_none());
    }

    #[test]
    fn single_report_emits_contract_runner_and_init() {
        let mut feature = base_feature("customer");
        feature.reports.push(mk_report(
            "monthly_audit",
            vec![ReportFormat::Csv, ReportFormat::Xlsx],
        ));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");

        // Contract and Runner var still declared per report.
        assert!(out.contains("var MonthlyAuditReport = report.Contract{"));
        assert!(out.contains("var MonthlyAuditRunner report.Runner"));

        // Auto-mount init() block — registers the contract with a
        // closure that calls the package-level Runner each request.
        assert!(out.contains("func init() {"));
        assert!(out.contains(
            "report.Register(MonthlyAuditReport, func(ctx context.Context, format report.Format) (string, error) {"
        ));
        assert!(out.contains("if MonthlyAuditRunner != nil {"));
        assert!(out.contains("return MonthlyAuditRunner(ctx, format)"));
        assert!(out.contains("return \"\", report.ErrRunnerNotWired(\"monthly_audit\")"));

        // Context import threaded through for the closure signature.
        assert!(out.contains("\"context\""));

        // Existing Run<Name> entry point preserved (still 4-arg form).
        assert!(out.contains(
            "func RunMonthlyAudit(ctx *lazuli.Ctx, format report.Format, source report.SourceFn, store storage.ObjectStore) (string, error) {"
        ));
    }

    // W5 GAP-REPORT-01 — `report input { … }` codegen.

    #[test]
    fn report_with_input_emits_inputs_slice() {
        use lazuli_ir::{BuiltinType, TypeRef, TypedSlot};

        let mut feature = base_feature("billing");
        let mut report = mk_report("billing_summary", vec![ReportFormat::Csv]);
        report.input = vec![
            TypedSlot {
                name: "period_start".into(),
                type_ref: TypeRef::Builtin(BuiltinType::Date),
                required: true,
                constraints: Default::default(),
                validate_skip: false,
            },
            TypedSlot {
                name: "format".into(),
                type_ref: TypeRef::Unresolved("CSV".into()),
                required: false,
                constraints: Default::default(),
                validate_skip: false,
            },
        ];
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");

        // Inputs slice emitted into the Contract, in author order, with
        // the verbatim type token + required bool.
        assert!(out.contains("Inputs: []report.Input{"));
        assert!(out.contains(
            "{Name: \"period_start\", Type: \"Date\", Required: true},"
        ));
        assert!(out.contains("{Name: \"format\", Type: \"CSV\", Required: false},"));
    }

    #[test]
    fn report_without_input_omits_inputs_slice() {
        let mut feature = base_feature("customer");
        feature
            .reports
            .push(mk_report("monthly_audit", vec![ReportFormat::Csv]));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        // No `input` block → no Inputs field bloating the contract.
        assert!(!out.contains("Inputs: []report.Input{"));
    }

    #[test]
    fn two_reports_emit_one_init_with_both_registrations() {
        let mut feature = base_feature("customer");
        feature
            .reports
            .push(mk_report("monthly_audit", vec![ReportFormat::Csv]));
        feature
            .reports
            .push(mk_report("daily_summary", vec![ReportFormat::Xlsx]));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");

        // Only ONE init() block holds both registrations.
        assert_eq!(out.matches("func init() {").count(), 1);
        assert!(out.contains("report.Register(MonthlyAuditReport,"));
        assert!(out.contains("report.Register(DailySummaryReport,"));
        assert!(out.contains("MonthlyAuditRunner"));
        assert!(out.contains("DailySummaryRunner"));

        // Reports stay sorted by name (daily before monthly).
        let daily_pos = out.find("DailySummaryReport").expect("daily declared");
        let monthly_pos = out.find("MonthlyAuditReport").expect("monthly declared");
        assert!(daily_pos < monthly_pos, "expected sorted output");
    }

    // R.C.4 — Atoms emission from `policy_expr`.

    #[test]
    fn report_with_policy_expr_emits_atoms_slice() {
        let mut feature = base_feature("customer");
        let mut report = mk_report("monthly_audit", vec![ReportFormat::Csv]);
        report.policy_expr = Some(PolicyExpr::And(vec![
            PolicyExpr::Authenticated,
            PolicyExpr::HasPermission("reports:read".to_owned()),
        ]));
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"predicate\", Name: \"(\"}"));
        assert!(out.contains("{Namespace: \"predicate\", Name: \"authenticated\"}"));
        assert!(out.contains("{Namespace: \"predicate\", Name: \"and\"}"));
        assert!(out.contains("{Namespace: \"rbac.permission\", Name: \"reports:read\"}"));
        assert!(out.contains("{Namespace: \"predicate\", Name: \")\"}"));
    }

    #[test]
    fn report_with_policy_atom_emits_single_resolved_atom() {
        let mut feature = base_feature("customer");
        let mut report = mk_report("public_data", vec![ReportFormat::Csv]);
        report.policy = PolicyRef::Atom("scope.public".to_owned());
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"scope\", Name: \"public\"}"));
    }

    #[test]
    fn report_with_local_policy_ref_emits_feature_policy_atoms() {
        let mut feature = base_feature("customer");
        feature.policies.categories.push(PolicyCategory {
            name: "global_read".to_owned(),
            atoms: vec!["@role.admin".to_owned(), "@scope.same_org".to_owned()],
            conditional_atoms: Vec::new(),
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        });
        feature
            .reports
            .push(mk_report("monthly_audit", vec![ReportFormat::Csv]));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"role\", Name: \"admin\"}"));
        assert!(out.contains("{Namespace: \"scope\", Name: \"same_org\"}"));
        assert!(out.contains("Policy: \"@policy.global_read\""));
    }

    #[test]
    fn report_with_legacy_policy_atom_ref_emits_feature_policy_atoms() {
        let mut feature = base_feature("customer");
        feature.policies.categories.push(PolicyCategory {
            name: "global_read".to_owned(),
            atoms: vec!["@role.admin".to_owned()],
            conditional_atoms: Vec::new(),
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        });
        let mut report = mk_report("monthly_audit", vec![ReportFormat::Csv]);
        report.policy = PolicyRef::Atom("policy.global_read".to_owned());
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"role\", Name: \"admin\"}"));
        assert!(out.contains("Policy: \"@policy.global_read\""));
    }

    #[test]
    fn report_with_unresolved_legacy_policy_ref_emits_feature_policy_atoms() {
        let mut feature = base_feature("customer");
        feature.policies.categories.push(PolicyCategory {
            name: "global_read".to_owned(),
            atoms: vec!["@role.admin".to_owned()],
            conditional_atoms: Vec::new(),
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        });
        let mut report = mk_report("monthly_audit", vec![ReportFormat::Csv]);
        report.policy = PolicyRef::Unresolved("@policy.global_read".to_owned());
        feature.reports.push(report);
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("Atoms: []report.PolicyAtom{"));
        assert!(out.contains("{Namespace: \"role\", Name: \"admin\"}"));
        assert!(out.contains("Policy: \"@policy.global_read\""));
    }

    #[test]
    fn report_with_unresolved_policy_local_skips_atoms() {
        // PolicyRef::Local("global_read") with no matching feature
        // category should NOT synthesise atoms (would deny incorrectly).
        let mut feature = base_feature("customer");
        feature
            .reports
            .push(mk_report("monthly_audit", vec![ReportFormat::Csv]));
        let out = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert!(!out.contains("Atoms: []report.PolicyAtom{"));
        // Verbatim Policy: field stays for audit.
        assert!(out.contains("Policy: \"@policy.global_read\""));
    }

    #[test]
    fn deterministic_across_runs() {
        let mut feature = base_feature("customer");
        feature
            .reports
            .push(mk_report("zebra", vec![ReportFormat::Csv]));
        feature
            .reports
            .push(mk_report("alpha", vec![ReportFormat::Xlsx]));
        let a = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        let b = emit_reports_file("examples/x.lzi", &feature).expect("must emit");
        assert_eq!(a, b);
    }
