//! Feature-emit tests for notification. Split out of `mod.rs` to
//! keep production under the 500 LOC budget.

use super::*;
use lazuli_ir::{
    AppManifest, Defaults, DigestStrategy, Module, NotificationDigest, NotificationThrottle, Path,
    Policies, TenantFromSpec,
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

fn minimal_app() -> AppManifest {
    AppManifest {
        name: "test".to_owned(),
        title: None,
        version: None,
        lazuli_version: None,
        targets: Vec::new(),
        default_locale: None,
        default_timezone: None,
        auth_failed_redirect: None,
        not_found: None,
        error_pages: Vec::new(),
        uses: Vec::new(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
        headers: None,
        cookie: None,
        proxy: None,
        limits: None,
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        logging: None,
        tracing: None,
        observability: None,
        locale: None,
        encryption_bindings: Vec::new(),
        route_guard: None,
        actor_query: None,
        span_ref: None,
    }
}

fn module_with_feature(feature: Feature) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app()),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: vec![feature],
    }
}

fn emit(feature: &Feature) -> Option<String> {
    let module = module_with_feature(feature.clone());
    let index = CrossFeatureIndex::build(&module);
    let emit_ctx = EmitContext::no_source("customer/notification.gen.go");
    emit_notification_file("examples/x.lzi", feature, "lazuli/test", &index, &emit_ctx)
}

fn local_event(name: &str) -> JobTrigger {
    JobTrigger::Event {
        event: QualifiedName {
            feature: Some("customer".to_owned()),
            name: name.to_owned(),
        },
    }
}

fn base_notification(name: &str) -> Notification {
    Notification {
        name: name.to_owned(),
        trigger: local_event("customer_activated"),
        channels: vec!["email".to_owned()],
        recipient: "target.email".to_owned(),
        template: "./outreach/welcome_email.mjml".to_owned(),
        policy: None,
        policy_expr: None,
        tenant_from: None,
        idempotency: None,
        retry: None,
        emits: Vec::new(),
        digest: None,
        throttle: None,
        previous_names: Vec::new(),
        span_ref: None,
    }
}

#[test]
fn empty_feature_returns_none() {
    let feature = base_feature("customer_outreach");
    assert!(emit(&feature).is_none());
}

#[test]
fn feature_emit_entry_point_emits_representative_notification_file() {
    let mut feature = base_feature("customer_outreach");
    feature
        .notifications
        .push(base_notification("welcome_email"));

    let out = emit_notification_file(
        "features/customer_outreach/customer_outreach.lzi",
        &feature,
        "lazuli/test",
        &CrossFeatureIndex::build(&module_with_feature(feature.clone())),
        &EmitContext::no_source("customer_outreach/notification.gen.go"),
    )
    .expect("feature-level notification emitter must emit non-empty output");

    assert!(!out.is_empty());
    assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
    assert!(out.contains("package customer_outreachgen"));
    assert!(out.contains("\"lazuli.dev/runtime/lazuli/notifications\""));
    assert!(out.contains("var welcomeEmailNotification = notifications.NotificationContract{"));
    assert!(out.contains("Recipient:    \"target.email\","));
    assert!(out.contains("Template:     \"./outreach/welcome_email.mjml\","));
}

#[test]
fn event_notification_emits_runtime_contract_fields() {
    let mut feature = base_feature("customer_outreach");
    let mut notification = base_notification("archive_survey");
    notification.trigger = JobTrigger::Event {
        event: QualifiedName {
            feature: Some("customer".to_owned()),
            name: "customer_archived".to_owned(),
        },
    };
    notification.channels = vec!["email".to_owned(), "in_app".to_owned()];
    notification.policy = Some(PolicyRef::Atom("policy.notify".to_owned()));
    notification.tenant_from = Some(TenantFromSpec {
        path: Path::from_segments(["payload", "org_id"]),
    });
    notification.idempotency = Some(IdempotencyKey {
        by: Path::from_segments(["envelope", "id"]),
    });
    notification.retry = Some(RetryPolicy {
        count: 3,
        backoff: BackoffStrategy::Exponential,
    });
    notification.emits = vec!["archive_survey_sent".to_owned()];
    feature.notifications.push(notification);

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
    assert!(out.contains("package customer_outreachgen"));
    assert!(out.contains("\"lazuli.dev/runtime/lazuli/notifications\""));
    assert!(out.contains("var archiveSurveyNotification = notifications.NotificationContract{"));
    assert!(out.contains("Channels:     []notifications.Channel{notifications.ChannelEmail, notifications.ChannelInApp},"));
    assert!(out.contains("TriggerKind:  \"event\","));
    assert!(out.contains("TriggerEvent: \"customer.customer_archived\","));
    assert!(out.contains("Policy:       \"@policy.notify\","));
    assert!(out.contains("TenantFrom:   &notifications.TenantFromSpec{Path: \"payload.org_id\"},"));
    assert!(
        out.contains("Idempotency:  &notifications.IdempotencyKeySpec{Path: \"envelope.id\"},")
    );
    assert!(
        out.contains(
            "Retry:        &notifications.RetryPolicy{Count: 3, Backoff: \"exponential\"},"
        )
    );
    assert!(out.contains("Emits:        []string{\"archive_survey_sent\"},"));
}

#[test]
fn schedule_trigger_emits_cron_axis() {
    let mut feature = base_feature("billing");
    let mut notification = base_notification("daily_digest");
    notification.trigger = JobTrigger::Schedule {
        cron: "0 9 * * *".to_owned(),
    };
    feature.notifications.push(notification);

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("TriggerKind: \"schedule\","));
    assert!(out.contains("TriggerCron: \"0 9 * * *\","));
    assert!(!out.contains("TriggerEvent:"));
}

#[test]
fn push_and_sms_channels_emit_typed_literals() {
    let mut feature = base_feature("customer_outreach");
    let mut notification = base_notification("mobile_alert");
    notification.channels = vec!["push".to_owned(), "sms".to_owned()];
    feature.notifications.push(notification);

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("notifications.Channel(\"push\"),"));
    assert!(out.contains("notifications.Channel(\"sms\"),"));
}

#[test]
fn digest_throttle_emit_runtime_contract_fields_and_notifications_sort_by_name() {
    let mut feature = base_feature("customer_outreach");
    let mut zebra = base_notification("zebra");
    let mut alpha = base_notification("alpha");
    alpha.digest = Some(NotificationDigest {
        every: "15 minutes".to_owned(),
        group_by: Some("customer_id".to_owned()),
        max_size: Some(50),
        template_strategy: Some(DigestStrategy::Merge),
        invalid_template_strategy: None,
    });
    alpha.throttle = Some(NotificationThrottle {
        max_per: "1 hour".to_owned(),
        per_recipient: true,
        per_channel: true,
        burst: Some(3),
    });
    feature.notifications.push(zebra);
    feature.notifications.push(alpha);

    let out = emit(&feature).expect("must emit");
    let alpha_pos = out.find("Notification: customer_outreach.alpha").unwrap();
    let zebra_pos = out.find("Notification: customer_outreach.zebra").unwrap();
    assert!(alpha_pos < zebra_pos);
    assert!(out.contains("Digest:       &notifications.NotificationDigest{Every: \"15 minutes\", GroupBy: \"customer_id\", MaxSize: 50, TemplateStrategy: notifications.DigestStrategyMerge},"));
    assert!(out.contains("Throttle:     &notifications.NotificationThrottle{MaxPer: \"1 hour\", PerRecipient: true, PerChannel: true, Burst: 3},"));
    assert!(!out.contains("Notification.digest"));
    assert!(!out.contains("Notification.throttle"));

    zebra = base_notification("zebra");
    alpha = base_notification("alpha");
    feature.notifications.clear();
    feature.notifications.push(zebra);
    feature.notifications.push(alpha);
    let out_again = emit(&feature).expect("must emit");
    assert!(!out_again.contains("Digest:"));
    assert!(!out_again.contains("Throttle:"));
    assert_eq!(out_again, emit(&feature).expect("must emit"));
}

#[test]
fn digest_defaults_zero_values_and_preserves_append_strategy() {
    let mut feature = base_feature("customer_outreach");
    let mut notification = base_notification("append_digest");
    notification.digest = Some(NotificationDigest {
        every: "1 day".to_owned(),
        group_by: None,
        max_size: None,
        template_strategy: Some(DigestStrategy::Append),
        invalid_template_strategy: None,
    });
    notification.throttle = Some(NotificationThrottle {
        max_per: "1 day".to_owned(),
        per_recipient: false,
        per_channel: true,
        burst: None,
    });
    feature.notifications.push(notification);

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("Digest:       &notifications.NotificationDigest{Every: \"1 day\", GroupBy: \"\", MaxSize: 0, TemplateStrategy: notifications.DigestStrategyAppend},"));
    assert!(out.contains("Throttle:     &notifications.NotificationThrottle{MaxPer: \"1 day\", PerRecipient: false, PerChannel: true, Burst: 0},"));
}
