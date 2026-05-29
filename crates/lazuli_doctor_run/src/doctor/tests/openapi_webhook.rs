    // Doctor openapi text-pattern + webhook payload/replay/dlq/event tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn openapi_text_pattern_api_block_fires() {
        // The diagnostic fires when the source contains an `api` token
        // that the typed lifter did not promote into `feature.apis`.
        // Authoring an `api` block without the required `method`/`path`/
        // `output` fails the feature skeleton parse, so the fixture
        // routes through a hand-built `tier3_facts` entry that mirrors
        // a real-world mixed package (some features typed, one feature
        // legacy text-pattern). The shape is regression-style: when the
        // fixture changes the diagnostic shape, this test catches it.
        let mut package = package_from_sources(vec![]);
        package.tier3_facts.push(Tier3FeatureFacts {
            feature: "legacy".to_owned(),
            path: PathBuf::from("legacy.lzi"),
            feature_line: 1,
            tenancy_axis: None,
            defaults_policy: None,
            defaults_timestamps: false,
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            resource_previous_names: Vec::new(),
            field_previous_names: Vec::new(),
            all_resource_names_in_feature: BTreeSet::new(),
            all_field_names_in_feature: BTreeMap::new(),
            job_lines: BTreeMap::new(),
            webhook_lines: BTreeMap::new(),
            notification_lines: BTreeMap::new(),
            tenant_migration_lines: BTreeMap::new(),
            event_group_lines: BTreeMap::new(),
            commands: Vec::new(),
            command_lines: BTreeMap::new(),
            queries: Vec::new(),
            query_lines: BTreeMap::new(),
            caches: Vec::new(),
            cache_lines: BTreeMap::new(),
            api_names_text_pattern: vec!["customer_legacy".to_owned()],
            apis: Vec::new(),
            api_lines: BTreeMap::new(),
            agents: Vec::new(),
            translation: None,
            translation_line: 1,
            records: Vec::new(),
            enums: Vec::new(),
            events: Vec::new(),
            policies_declared: false,
            policies: lazuli_ir::Policies::default(),
            extensions: Vec::new(),
            reports: Vec::new(),
            report_lines: BTreeMap::new(),
            resources: Vec::new(),
            report_decls: Vec::new(),
            aggregates: Vec::new(),
            aggregate_lines: BTreeMap::new(),
            errors: None,
            uses: Vec::new(),
            channels: Vec::new(),
        });
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("openapi_text_pattern_api_block"),
            "expected openapi_text_pattern_api_block in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // Webhooks expanded cycle — eight new doctor diagnostics.
    // =========================================================================

    /// `WEBHOOK-PAYLOAD-001` fires when `payload from
    /// webhook_events.<X>` cannot be resolved against the registry
    /// catalog.
    #[test]
    fn webhook_payload_001_unresolved_envelope() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
registry
  webhook_events
    crm_customer_upsert
      external_id: Text required

feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    payload from webhook_events.unknown_envelope
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.org_id
    handler "./integrations/upsert_customer_from_crm.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-PAYLOAD-001"),
            "expected WEBHOOK-PAYLOAD-001, got {codes:?}"
        );
    }

    /// `WEBHOOK-PAYLOAD-002` fires when `tenant_from payload.<axis>`
    /// references a field the envelope does not declare.
    #[test]
    fn webhook_payload_002_tenant_field_missing_in_envelope() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
registry
  webhook_events
    crm_customer_upsert
      external_id: Text required

feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    payload from webhook_events.crm_customer_upsert
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    handler "./integrations/upsert.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-PAYLOAD-002"),
            "expected WEBHOOK-PAYLOAD-002, got {codes:?}"
        );
    }

    /// `WEBHOOK-REPLAY-001` fires when `replay allow` is declared
    /// without `within "<duration>"`.
    #[test]
    fn webhook_replay_001_allow_without_window() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    replay
      allow
    handler "./integrations/upsert.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-REPLAY-001"),
            "expected WEBHOOK-REPLAY-001, got {codes:?}"
        );
    }

    /// `WEBHOOK-DLQ-001` fires when `dlq emit <event>` references an
    /// event the feature does not declare anywhere.
    #[test]
    fn webhook_dlq_001_unresolved_emit_event() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    retry 3 backoff exponential
    dlq emit not_declared_anywhere
    handler "./integrations/upsert.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-DLQ-001"),
            "expected WEBHOOK-DLQ-001, got {codes:?}"
        );
    }

    /// `WEBHOOK-DLQ-003` fires when `retry` is declared without `dlq`.
    #[test]
    fn webhook_dlq_003_retry_without_dlq() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    retry 3 backoff exponential
    handler "./integrations/upsert.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-DLQ-003"),
            "expected WEBHOOK-DLQ-003, got {codes:?}"
        );
    }

    /// `WEBHOOK-EVENT-001` fires when a `webhook_events.<X>` envelope
    /// is declared in registry but no webhook references it.
    #[test]
    fn webhook_event_001_dead_envelope_in_registry() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
registry
  webhook_events
    orphan_envelope
      external_id: Text required
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-EVENT-001"),
            "expected WEBHOOK-EVENT-001, got {codes:?}"
        );
    }

    #[test]
    fn webhook_event_version_decreasing_previous_exceeds_current() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  webhook_event customer.archived
    payload
      customer_id: ID
    version 1
    previous_version 2
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"webhook-event-version-decreasing"),
            "expected webhook-event-version-decreasing, got {codes:?}"
        );
    }

    #[test]
    fn webhook_event_payload_empty_rejects_empty_schema() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  webhook_event customer.created
    payload
    version 1
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"webhook-event-payload-empty"),
            "expected webhook-event-payload-empty, got {codes:?}"
        );
    }

    #[test]
    fn webhook_event_deprecated_no_replacement_requires_trail() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  webhook_event customer.deleted
    payload
      customer_id: ID
    version 3
    deprecated true
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"webhook-event-deprecated-no-replacement"),
            "expected webhook-event-deprecated-no-replacement, got {codes:?}"
        );
    }

    // =========================================================================
    // Notifications expanded bucket cycle — six new doctor diagnostics on
    // `notification.digest` and `notification.throttle`.
