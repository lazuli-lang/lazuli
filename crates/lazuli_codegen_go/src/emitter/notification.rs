//! Cell G2c - `Notification` kind emission. Walks every notification
//! declared on a feature and emits `notifications.NotificationContract`
//! values into `<feature>/notification.gen.go`.
//!
//! Proposal references:
//! - Section 3.8 - notification contract shape.
//! - Section 4.4 - digest/throttle are intentionally held as TODOs in
//!   this v0 spine even though the IR already carries the typed shapes.
//!
//! Runtime note: the current Lazuli Go notifications package exposes
//! `TriggerKind` / `TriggerEvent` / `TriggerCron` fields and
//! `Idempotency *IdempotencyKeySpec`, not the proposal's
//! `notifications.Trigger` / `IdempotencyBy` surface. The emitter uses
//! the runtime fields that exist and leaves TODO comments in the value
//! literal so the mismatch stays visible without emitting uncompilable
//! Go.

use lazuli_ir::{
    BackoffStrategy, Feature, IdempotencyKey, JobTrigger, Notification, PolicyRef, QualifiedName,
    RetryPolicy,
};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;

/// Emit `<feature>/notification.gen.go` for a feature, or `None` when
/// the feature declares no notifications.
pub fn emit_notification_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    if feature.notifications.is_empty() {
        return None;
    }

    let _ = (module_name, cross_index);

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
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
        emit_notification(&mut p, feature, notification);
    }

    Some(p.finish())
}

fn emit_notification(p: &mut GoPrinter, feature: &Feature, notification: &Notification) {
    let qualified_name = format!("{}.{}", feature.name, notification.name);
    let var_name = format!("{}Notification", lower_camel(&notification.name));

    write_section_banner(
        p,
        &[
            format!("Notification: {qualified_name}"),
            format!("  notification {}", notification.name),
        ],
    );

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
    rows.push(LiteralRow::comment(
        "// TODO(runtime): notifications.Trigger struct is absent; using TriggerKind/TriggerEvent/TriggerCron fields.",
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
        rows.push(LiteralRow::comment(
            "// TODO(runtime): NotificationContract.IdempotencyBy is absent; using IdempotencyKeySpec.Path.",
        ));
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
    if notification.digest.is_some() {
        rows.push(LiteralRow::comment(
            "// TODO(runtime): Notification.digest is not emitted in the v0 spine (proposal section 4.4).",
        ));
    }
    if notification.throttle.is_some() {
        rows.push(LiteralRow::comment(
            "// TODO(runtime): Notification.throttle is not emitted in the v0 spine (proposal section 4.4).",
        ));
    }

    emit_literal_rows(p, &rows);

    p.dedent();
    p.line("}");
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
    Comment(String),
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

    fn comment(text: &str) -> Self {
        Self::Comment(text.to_owned())
    }
}

fn emit_literal_rows(p: &mut GoPrinter, rows: &[LiteralRow]) {
    let key_width = rows
        .iter()
        .filter_map(|row| match row {
            LiteralRow::Field { key, .. } | LiteralRow::Block { key, .. } => Some(key.len()),
            LiteralRow::Comment(_) => None,
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
            LiteralRow::Comment(text) => p.line(text),
        }
    }
}

fn channel_rows(channels: &[String]) -> Vec<LiteralRow> {
    if channels.is_empty() {
        return vec![
            LiteralRow::comment(
                "// TODO(runtime): notification has no channels; doctor should reject this before codegen.",
            ),
            LiteralRow::field("Channels:", "[]notifications.Channel{},".to_owned()),
        ];
    }

    let literals: Vec<ChannelLiteral> = channels.iter().map(|ch| channel_literal(ch)).collect();
    if literals.iter().all(|lit| lit.todo.is_none()) {
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
        if let Some(todo) = literal.todo {
            body.push(LiteralRow::Comment(todo));
        }
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
    todo: Option<String>,
}

fn channel_literal(channel: &str) -> ChannelLiteral {
    let normalized = channel.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "email" => supported_channel("notifications.ChannelEmail"),
        "in_app" => supported_channel("notifications.ChannelInApp"),
        "webhook" => supported_channel("notifications.ChannelWebhook"),
        "slack" => supported_channel("notifications.ChannelSlack"),
        "discord" => supported_channel("notifications.ChannelDiscord"),
        "push" => missing_channel_const(channel, "notifications.ChannelPush"),
        "sms" => missing_channel_const(channel, "notifications.ChannelSms"),
        _ => ChannelLiteral {
            expr: format!("notifications.Channel({})", go_string_literal(channel)),
            todo: Some(format!(
                "// TODO(runtime): unknown notification channel {}; preserving as typed literal.",
                go_string_literal(channel)
            )),
        },
    }
}

fn supported_channel(expr: &str) -> ChannelLiteral {
    ChannelLiteral {
        expr: expr.to_owned(),
        todo: None,
    }
}

fn missing_channel_const(channel: &str, const_name: &str) -> ChannelLiteral {
    ChannelLiteral {
        expr: format!("notifications.Channel({})", go_string_literal(channel)),
        todo: Some(format!(
            "// TODO(runtime): {const_name} constant is missing; preserving {} as typed literal.",
            go_string_literal(channel)
        )),
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
    let mut out = String::with_capacity(s.len());
    let mut first = true;
    for word in s.split(|c: char| c == '_' || c == '-') {
        if word.is_empty() {
            continue;
        }
        if first {
            out.push_str(&word.to_ascii_lowercase());
            first = false;
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(c) = chars.next() {
            for u in c.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl" | "uuid"
    )
}

#[cfg(test)]
mod tests {
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
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
            title: None,
            version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
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
            locale: None,
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
            features: vec![feature],
        }
    }

    fn emit(feature: &Feature) -> Option<String> {
        let module = module_with_feature(feature.clone());
        let index = CrossFeatureIndex::build(&module);
        emit_notification_file("examples/x.lzi", feature, "lazuli/test", &index)
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
    fn missing_push_and_sms_constants_emit_todo_and_typed_literals() {
        let mut feature = base_feature("customer_outreach");
        let mut notification = base_notification("mobile_alert");
        notification.channels = vec!["push".to_owned(), "sms".to_owned()];
        feature.notifications.push(notification);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// TODO(runtime): notifications.ChannelPush constant is missing"));
        assert!(out.contains("// TODO(runtime): notifications.ChannelSms constant is missing"));
        assert!(out.contains("notifications.Channel(\"push\"),"));
        assert!(out.contains("notifications.Channel(\"sms\"),"));
    }

    #[test]
    fn digest_throttle_are_todo_comments_and_notifications_sort_by_name() {
        let mut feature = base_feature("customer_outreach");
        let mut zebra = base_notification("zebra");
        let mut alpha = base_notification("alpha");
        alpha.digest = Some(NotificationDigest {
            every: "15 minutes".to_owned(),
            group_by: Some("customer_id".to_owned()),
            max_size: Some(50),
            template_strategy: Some(DigestStrategy::Merge),
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
        assert!(out.contains("// TODO(runtime): Notification.digest is not emitted"));
        assert!(out.contains("// TODO(runtime): Notification.throttle is not emitted"));

        zebra = base_notification("zebra");
        alpha = base_notification("alpha");
        feature.notifications.clear();
        feature.notifications.push(zebra);
        feature.notifications.push(alpha);
        let out_again = emit(&feature).expect("must emit");
        assert_eq!(out_again, emit(&feature).expect("must emit"));
    }
}
