//! Cell G2c - `Notification` kind emission. Walks every notification
//! declared on a feature and emits `notifications.NotificationContract`
//! values into `<feature>/notification.gen.go`.
//!
//! Proposal references:
//! - Section 3.8 - notification contract shape.
//! - Section 4.4 - digest/throttle typed runtime contract fields.
//!
//! Runtime note: the current Lazuli Go notifications package exposes
//! `TriggerKind` / `TriggerEvent` / `TriggerCron` fields and
//! `Idempotency *IdempotencyKeySpec`, not the proposal's
//! `notifications.Trigger` / `IdempotencyBy` surface. The emitter uses
//! the runtime fields that exist.

use lazuli_ir::{
    BackoffStrategy, Feature, IdempotencyKey, JobTrigger, Notification, PolicyRef, QualifiedName,
    RetryPolicy,
};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::module::EmitContext;
use super::patterns::{PATTERN_NOTIFICATION_DISPATCH, emit_pattern_header};
use super::printer::GoPrinter;

/// Emit `<feature>/notification.gen.go` for a feature, or `None` when
/// the feature declares no notifications.
pub fn emit_notification_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    emit_ctx: &EmitContext<'_>,
) -> Option<String> {
    if feature.notifications.is_empty() {
        return None;
    }

    let _ = (module_name, cross_index);

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("context");
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/notifications");

    let mut notifications: Vec<&Notification> = feature.notifications.iter().collect();
    notifications.sort_by(|a, b| a.name.cmp(&b.name));

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for notification in &notifications {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_notification(&mut p, feature, notification, emit_ctx);
    }

    Some(p.finish())
}

fn emit_notification(
    p: &mut GoPrinter,
    feature: &Feature,
    notification: &Notification,
    emit_ctx: &EmitContext<'_>,
) {
    let qualified_name = format!("{}.{}", feature.name, notification.name);
    let var_name = format!("{}Notification", lower_camel(&notification.name));

    write_section_banner(
        p,
        &[
            format!("Notification: {qualified_name}"),
            format!("  notification {}", notification.name),
        ],
    );

    emit_pattern_header(p, PATTERN_NOTIFICATION_DISPATCH);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, notification.span_ref);
    p.line(&format!(
        "var {var_name} = notifications.NotificationContract{{"
    ));
    p.indent();

    let mut rows = Vec::new();
    rows.push(LiteralRow::field("Feature:", go_string(&feature.name)));
    rows.push(LiteralRow::field("Name:", go_string(&notification.name)));
    rows.extend(channel_rows(&notification.channels));
    rows.push(LiteralRow::field(
        "Recipient:",
        go_string(&notification.recipient),
    ));
    rows.extend(trigger_rows(&notification.trigger));
    rows.push(LiteralRow::field(
        "Template:",
        go_string(&notification.template),
    ));
    if let Some(policy) = notification
        .policy
        .as_ref()
        .and_then(policy_ref_surface_text)
    {
        rows.push(LiteralRow::field("Policy:", go_string(&policy)));
    }
    if let Some(tenant_from) = &notification.tenant_from {
        rows.push(LiteralRow::field(
            "TenantFrom:",
            format!(
                "&notifications.TenantFromSpec{{Path: {}}},",
                go_string_literal(&path_to_string(&tenant_from.path))
            ),
        ));
    }
    if let Some(idempotency) = &notification.idempotency {
        rows.push(LiteralRow::field(
            "Idempotency:",
            format!(
                "&notifications.IdempotencyKeySpec{{Path: {}}},",
                go_string_literal(&idempotency_path(idempotency))
            ),
        ));
    }
    if let Some(retry) = &notification.retry {
        rows.push(LiteralRow::field("Retry:", format_retry(retry)));
    }
    if !notification.emits.is_empty() {
        rows.push(LiteralRow::field(
            "Emits:",
            string_slice(&notification.emits),
        ));
    }
    if let Some(digest) = &notification.digest {
        rows.push(LiteralRow::field("Digest:", format_digest(digest)));
    }
    if let Some(throttle) = &notification.throttle {
        rows.push(LiteralRow::field("Throttle:", format_throttle(throttle)));
    }

    emit_literal_rows(p, &rows);
    emit_ctx.emit_with_source_field(p, "notification", &notification.name, notification.span_ref);

    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
}

enum LiteralRow {
    Field {
        key: String,
        value: String,
    },
    Block {
        key: String,
        opener: String,
        body: Vec<LiteralRow>,
        closer: String,
    },
}

impl LiteralRow {
    fn field(key: &str, value: String) -> Self {
        Self::Field {
            key: key.to_owned(),
            value,
        }
    }

    fn block(key: &str, opener: String, body: Vec<LiteralRow>, closer: &str) -> Self {
        Self::Block {
            key: key.to_owned(),
            opener,
            body,
            closer: closer.to_owned(),
        }
    }
}

fn emit_literal_rows(p: &mut GoPrinter, rows: &[LiteralRow]) {
    let key_width = rows
        .iter()
        .filter_map(|row| match row {
            LiteralRow::Field { key, .. } | LiteralRow::Block { key, .. } => Some(key.len()),
        })
        .max()
        .unwrap_or(0);

    for row in rows {
        match row {
            LiteralRow::Field { key, value } => {
                if key.is_empty() {
                    p.line(value);
                } else {
                    let pad = key_width.saturating_sub(key.len());
                    p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
                }
            }
            LiteralRow::Block {
                key,
                opener,
                body,
                closer,
            } => {
                let pad = key_width.saturating_sub(key.len());
                p.line(&format!("{}{} {}", key, " ".repeat(pad), opener));
                p.indent();
                emit_literal_rows(p, body);
                p.dedent();
                p.line(closer);
            }
        }
    }
}

fn channel_rows(channels: &[String]) -> Vec<LiteralRow> {
    if channels.is_empty() {
        return vec![LiteralRow::field(
            "Channels:",
            "[]notifications.Channel{},".to_owned(),
        )];
    }

    let literals: Vec<ChannelLiteral> = channels.iter().map(|ch| channel_literal(ch)).collect();
    if literals.iter().all(|lit| lit.inline) {
        let joined = literals
            .iter()
            .map(|lit| lit.expr.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return vec![LiteralRow::field(
            "Channels:",
            format!("[]notifications.Channel{{{joined}}},"),
        )];
    }

    let mut body = Vec::new();
    for literal in literals {
        body.push(LiteralRow::Field {
            key: String::new(),
            value: format!("{},", literal.expr),
        });
    }
    vec![LiteralRow::block(
        "Channels:",
        "[]notifications.Channel{".to_owned(),
        body,
        "},",
    )]
}

struct ChannelLiteral {
    expr: String,
    inline: bool,
}

fn channel_literal(channel: &str) -> ChannelLiteral {
    let normalized = channel.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "email" => supported_channel("notifications.ChannelEmail"),
        "in_app" => supported_channel("notifications.ChannelInApp"),
        "webhook" => supported_channel("notifications.ChannelWebhook"),
        "slack" => supported_channel("notifications.ChannelSlack"),
        "discord" => supported_channel("notifications.ChannelDiscord"),
        "push" => typed_channel_literal(channel),
        "sms" => typed_channel_literal(channel),
        _ => ChannelLiteral {
            expr: format!("notifications.Channel({})", go_string_literal(channel)),
            inline: false,
        },
    }
}

fn supported_channel(expr: &str) -> ChannelLiteral {
    ChannelLiteral {
        expr: expr.to_owned(),
        inline: true,
    }
}

fn typed_channel_literal(channel: &str) -> ChannelLiteral {
    ChannelLiteral {
        expr: format!("notifications.Channel({})", go_string_literal(channel)),
        inline: false,
    }
}

fn trigger_rows(trigger: &JobTrigger) -> Vec<LiteralRow> {
    match trigger {
        JobTrigger::Event { event } => vec![
            LiteralRow::field("TriggerKind:", "\"event\",".to_owned()),
            LiteralRow::field("TriggerEvent:", go_string(&qualified_name(event))),
        ],
        JobTrigger::Schedule { cron } => vec![
            LiteralRow::field("TriggerKind:", "\"schedule\",".to_owned()),
            LiteralRow::field("TriggerCron:", go_string(cron)),
        ],
    }
}

fn format_retry(retry: &RetryPolicy) -> String {
    format!(
        "&notifications.RetryPolicy{{Count: {}, Backoff: {}}},",
        retry.count,
        go_string_literal(backoff_strategy(retry.backoff))
    )
}

fn format_digest(digest: &lazuli_ir::NotificationDigest) -> String {
    format!(
        "&notifications.NotificationDigest{{Every: {}, GroupBy: {}, MaxSize: {}, TemplateStrategy: {}}},",
        go_string_literal(&digest.every),
        go_string_literal(digest.group_by.as_deref().unwrap_or("")),
        digest.max_size.unwrap_or(0),
        digest_strategy(
            digest
                .template_strategy
                .unwrap_or(lazuli_ir::DigestStrategy::Merge)
        )
    )
}

fn digest_strategy(strategy: lazuli_ir::DigestStrategy) -> &'static str {
    match strategy {
        lazuli_ir::DigestStrategy::Merge => "notifications.DigestStrategyMerge",
        lazuli_ir::DigestStrategy::Append => "notifications.DigestStrategyAppend",
    }
}

fn format_throttle(throttle: &lazuli_ir::NotificationThrottle) -> String {
    format!(
        "&notifications.NotificationThrottle{{MaxPer: {}, PerRecipient: {}, PerChannel: {}, Burst: {}}},",
        go_string_literal(&throttle.max_per),
        throttle.per_recipient,
        throttle.per_channel,
        throttle.burst.unwrap_or(0)
    )
}

fn backoff_strategy(strategy: BackoffStrategy) -> &'static str {
    match strategy {
        BackoffStrategy::Fixed => "fixed",
        BackoffStrategy::Exponential => "exponential",
    }
}

fn policy_ref_surface_text(policy: &PolicyRef) -> Option<String> {
    match policy {
        PolicyRef::Local(name) => Some(format!("@policy.{}", name)),
        PolicyRef::Atom(atom) => {
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            Some(format!("@{stripped}"))
        }
        PolicyRef::External { feature, name } => Some(format!("{feature}.policy.{name}")),
        PolicyRef::Unresolved(raw) => Some(raw.clone()),
        PolicyRef::None => None,
    }
}

fn idempotency_path(idempotency: &IdempotencyKey) -> String {
    path_to_string(&idempotency.by)
}

fn path_to_string(path: &lazuli_ir::Path) -> String {
    path.segments.join(".")
}

fn qualified_name(qname: &QualifiedName) -> String {
    match qname.feature.as_deref() {
        Some(feature) => format!("{}.{}", feature, qname.name),
        None => qname.name.clone(),
    }
}

fn string_slice(values: &[String]) -> String {
    let joined = values
        .iter()
        .map(|value| go_string_literal(value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[]string{{{joined}}},")
}

fn go_string(raw: &str) -> String {
    format!("{},", go_string_literal(raw))
}

fn go_string_literal(raw: &str) -> String {
    format!("\"{}\"", escape_string(raw))
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

fn lower_camel(s: &str) -> String {
    super::casing::lower_camel(s)
}

#[cfg(test)]
mod feature_emit_tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, Defaults, DigestStrategy, Module, NotificationDigest, NotificationThrottle,
        Path, Policies, TenantFromSpec,
    };

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
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
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
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
            previous_names: Vec::new(),
            span_ref: None,
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
        feature.notifications.push(base_notification("welcome_email"));

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
        assert!(out.contains("package customer_outreach"));
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
        assert!(out.contains("package customer_outreach"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/notifications\""));
        assert!(out.contains("var archiveSurveyNotification = notifications.NotificationContract{"));
        assert!(out.contains("Channels:     []notifications.Channel{notifications.ChannelEmail, notifications.ChannelInApp},"));
        assert!(out.contains("TriggerKind:  \"event\","));
        assert!(out.contains("TriggerEvent: \"customer.customer_archived\","));
        assert!(out.contains("Policy:       \"@policy.notify\","));
        assert!(
            out.contains("TenantFrom:   &notifications.TenantFromSpec{Path: \"payload.org_id\"},")
        );
        assert!(
            out.contains("Idempotency:  &notifications.IdempotencyKeySpec{Path: \"envelope.id\"},")
        );
        assert!(out.contains(
            "Retry:        &notifications.RetryPolicy{Count: 3, Backoff: \"exponential\"},"
        ));
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
}
