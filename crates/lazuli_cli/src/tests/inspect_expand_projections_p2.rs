    #[test]
    fn inspect_expand_tools_flag_parses() {
        let expansions = parse_expand_set("tools").unwrap();
        assert!(expansions.tools);
        assert!(!expansions.summary);
    }

    // -------------------------------------------------------------------------
    // CUT 2 — `--expand=context` (alias `ctx`) composite section catalog.
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_context_token_and_ctx_alias_both_set_flag() {
        let full = parse_expand_set("context").unwrap();
        assert!(full.context, "`context` token must set .context");
        let alias = parse_expand_set("ctx").unwrap();
        assert!(alias.context, "`ctx` alias must set .context");
        // Isolation: setting context alone leaves the other axes off.
        assert!(!full.summary);
        assert!(!alias.security);
    }

    #[test]
    fn inspect_expand_all_includes_context_axis() {
        let all = parse_expand_set("all").unwrap();
        assert!(all.context, "`all` must turn the context axis on");
        // The label list (echoed under report.expand) carries it too.
        let report =
            inspect_canonical_source("feature billing\n", Path::new("billing.lzi"), all);
        let value: serde_json::Value = serde_json::to_value(&report).unwrap();
        let labels = value["expand"].as_array().expect("expand label array");
        assert!(
            labels.iter().any(|l| l == "context"),
            "report.expand should list the context axis: {labels:?}"
        );
    }

    #[test]
    fn inspect_expand_context_projects_section_catalog_with_status_tags() {
        let expansions = parse_expand_set("context").unwrap();
        assert!(expansions.context);

        // Mirrors the canonical command-test feature shape (domain ->
        // event_group -> resource -> command) so the skeleton lowering
        // lifts resources + commands cleanly. The resource declares
        // `soft_delete` so the `invariants` section is `derived`; the
        // command's `emits order_paid` + `policy` lines feed the events
        // and authorization text-walkers.
        let source = r#"
feature billing
  domain
    event_group audit_stream on Order

  resource Order
    total: Integer required
    soft_delete

  command pay
    route id: ID
    input
      amount: Integer required
    policy @policy.create
    audit actor, target.id, input.amount
      emit_to audit_stream
    creates Order
      total = input.amount
    emits order_paid
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();

        // The composite section block is present.
        assert!(
            json.contains("\"context\":{"),
            "expected context composite in JSON: {json}"
        );
        // The full section catalog is enumerated.
        for section in [
            "purpose",
            "non_goals",
            "data_model",
            "operations",
            "contracts",
            "errors",
            "authorization",
            "events",
            "security",
            "invariants",
            "code_pointers",
            "test_matrix",
            "boundaries",
            "performance",
            "examples",
            "decisions",
        ] {
            assert!(
                json.contains(&format!("\"{section}\":{{")),
                "section `{section}` missing from catalog: {json}"
            );
        }

        // CRITICAL: the three text-walk sections carry
        // `derived-via-textwalk`, NOT `derived`.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let ctx = &value["features"][0]["context"];
        assert_eq!(ctx["authorization"]["status"], "derived-via-textwalk");
        assert_eq!(ctx["events"]["status"], "derived-via-textwalk");
        assert_eq!(ctx["security"]["status"], "derived-via-textwalk");

        // Clean IR-derived sections carry `derived` (even when the
        // feature declared none — `derived` + null payload means "the
        // compiler can derive this, the feature simply has none").
        assert_eq!(ctx["purpose"]["status"], "derived");
        assert_eq!(ctx["non_goals"]["status"], "derived");
        assert_eq!(ctx["data_model"]["status"], "derived");
        assert_eq!(ctx["operations"]["status"], "derived");
        assert_eq!(ctx["contracts"]["status"], "derived");
        assert_eq!(ctx["errors"]["status"], "derived");

        // invariants is `derived` because Order declares soft_delete.
        assert_eq!(ctx["invariants"]["status"], "derived");
        assert_eq!(ctx["invariants"]["payload"][0]["resource"], "Order");
        assert_eq!(ctx["invariants"]["payload"][0]["soft_delete"], true);

        // The derived data_model / operations sections box the lifted
        // resource + command verbatim.
        assert_eq!(ctx["data_model"]["payload"]["resources"][0]["name"], "Order");
        assert_eq!(ctx["operations"]["payload"]["commands"][0]["name"], "pay");

        // prose / vault / absent sections carry their tag with no payload.
        assert_eq!(ctx["boundaries"]["status"], "prose");
        assert_eq!(ctx["performance"]["status"], "prose");
        assert_eq!(ctx["examples"]["status"], "prose");
        assert_eq!(ctx["decisions"]["status"], "vault");
        assert_eq!(ctx["code_pointers"]["status"], "absent");
        assert_eq!(ctx["test_matrix"]["status"], "absent");
        assert!(
            ctx["boundaries"]["payload"].is_null(),
            "prose payload must be empty/null: {json}"
        );

        // The events text-walker boxes the emitted event.
        assert!(
            json.contains("\"order_paid\""),
            "events section should box the order_paid event: {json}"
        );
    }

    #[test]
    fn inspect_expand_context_boxes_purpose_payload_from_ir() {
        let expansions = parse_expand_set("context").unwrap();
        // Minimal purpose + resource feature: confirms the `purpose`
        // section boxes the verbatim IR string when the feature declares
        // one (kept separate from the catalog test so a single skeleton
        // quirk can't mask the payload assertion).
        let source = r#"
feature billing
  purpose "Billing lifecycle."

  resource Order
    total: Integer required
    soft_delete
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let value: serde_json::Value = serde_json::to_value(&report).unwrap();
        let ctx = &value["features"][0]["context"];
        assert_eq!(ctx["purpose"]["status"], "derived");
        assert_eq!(ctx["purpose"]["payload"], "Billing lifecycle.");
        // soft_delete-only resource => invariants derived.
        assert_eq!(ctx["invariants"]["status"], "derived");
    }

    #[test]
    fn inspect_expand_context_marks_invariants_absent_when_no_decorator() {
        let expansions = parse_expand_set("ctx").unwrap();
        let source = r#"
feature billing
  resource Order
    total: Integer required
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let value: serde_json::Value = serde_json::to_value(&report).unwrap();
        let ctx = &value["features"][0]["context"];
        assert_eq!(
            ctx["invariants"]["status"], "absent",
            "no soft_delete/append_only resource => invariants absent"
        );
        assert!(ctx["invariants"]["payload"].is_null());
    }

    #[test]
    fn inspect_expand_tenant_migrations_alias_projects_ir() {
        let source = r#"
feature customer
  defaults
    tenancy org

  domain
    query.lookup by_id by id: ID

  tenant_migration backfill_lifecycle_stage
    target query.by_id
    axis org
    idempotency envelope.tenant_id
    handler "./migrations/backfill_lifecycle_stage.go"
"#;
        let expansions = parse_expand_set("tenant_migrations").unwrap();
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let projected = report.features[0]
            .tenant_migrations
            .as_ref()
            .expect("tenant migrations projection");
        assert_eq!(projected[0].name, "backfill_lifecycle_stage");
        assert_eq!(projected[0].target.axis, "org");
        assert!(projected[0].target.operation.is_some());
    }
