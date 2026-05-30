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
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_notification_file("billing.lzi", &feature, "demo", &cross_index, &emit_ctx);
/// ```
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

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
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
mod feature_emit_tests;
