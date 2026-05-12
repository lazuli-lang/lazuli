//! Cell G2b -- `Webhook` kind emission. Walks every `Webhook`
//! declared on a feature and emits the v0 Lazuli Go
//! `webhooks.WebhookContract` value into `<feature>/webhook.gen.go`.
//!
//! The Lazuli Go runtime currently carries the v0 spine only:
//! `Feature`, `Name`, `Route`, `Verify`, `TenantFrom`,
//! `IdempotencyBy`, `Policy`, `HandlerPath`, `ReturnsType`, and
//! `Emits`. Expanded webhook slots (`payload_from`, `replay`, `dlq`,
//! `retry`) are preserved as TODO comments inside the value literal so
//! the generated source keeps the user's captured intent visible
//! without inventing runtime fields that do not exist yet.
//!
//! Determinism: webhooks are sorted by name before emission. Imports
//! flow through `ImportSet`, and type strings for `handler returns`
//! reuse `types::go_type_for` so cross-feature names render the same
//! way as resource/command emitters.

use lazuli_ir::{Feature, PolicyRef, TypeRef, VerifyScheme, Webhook};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

/// Emit `<feature>/webhook.gen.go` for a feature, or `None` when the
/// feature declares no webhooks.
pub fn emit_webhook_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    if feature.webhooks.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();

    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    let mut webhooks: Vec<&Webhook> = feature.webhooks.iter().collect();
    webhooks.sort_by(|a, b| a.name.cmp(&b.name));

    imports.add("lazuli.dev/runtime/lazuli/webhooks");

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for webhook in &webhooks {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_webhook(&mut p, feature, webhook, &type_ctx);
    }

    Some(p.finish())
}

fn emit_webhook(p: &mut GoPrinter, feature: &Feature, webhook: &Webhook, ctx: &TypeCtx<'_>) {
    let qualified_name = format!("{}.{}", feature.name, webhook.name);
    let var_name = format!("{}Webhook", lower_camel(&webhook.name));

    write_section_banner(
        p,
        &[
            format!("Webhook: {qualified_name}"),
            format!("  webhook {}", webhook.name),
        ],
    );

    p.line(&format!("var {var_name} = webhooks.WebhookContract{{"));
    p.indent();

    let mut kv_rows: Vec<(String, String)> = vec![
        (
            "Feature:".to_owned(),
            format!("\"{}\",", escape_string(&feature.name)),
        ),
        (
            "Name:".to_owned(),
            format!("\"{}\",", escape_string(&webhook.name)),
        ),
        (
            "Route:".to_owned(),
            format!("\"{}\",", escape_string(&webhook.route)),
        ),
    ];

    if let Some(verify) = &webhook.structured_verify {
        kv_rows.push(("Verify:".to_owned(), format_verify_spec(verify)));
    }
    if let Some(tenant_from) = &webhook.tenant_from {
        kv_rows.push((
            "TenantFrom:".to_owned(),
            format!(
                "&webhooks.TenantFromSpec{{Path: \"{}\"}},",
                escape_string(&path_to_string(&tenant_from.path))
            ),
        ));
    }
    if let Some(idempotency) = &webhook.idempotency {
        kv_rows.push((
            "IdempotencyBy:".to_owned(),
            format!("\"{}\",", escape_string(&path_to_string(&idempotency.by))),
        ));
    }
    if let Some(policy) = effective_policy(feature, webhook).and_then(format_policy_string) {
        kv_rows.push((
            "Policy:".to_owned(),
            format!("\"{}\",", escape_string(&policy)),
        ));
    }
    kv_rows.push((
        "HandlerPath:".to_owned(),
        format!("\"{}\",", escape_string(&webhook.handler.path)),
    ));
    if let Some(return_type) = &webhook.returns {
        kv_rows.push((
            "ReturnsType:".to_owned(),
            format!(
                "\"{}\",",
                escape_string(&return_type_name(return_type, ctx))
            ),
        ));
    }
    if !webhook.emits.is_empty() {
        kv_rows.push(("Emits:".to_owned(), format_string_slice(&webhook.emits)));
    }

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }

    emit_runtime_gaps(p, webhook);

    p.dedent();
    p.line("}");
}

fn format_verify_spec(verify: &lazuli_ir::VerifySpec) -> String {
    let scheme = match verify.scheme {
        VerifyScheme::Hmac => "webhooks.VerifyHmac",
    };
    format!(
        "webhooks.VerifySpec{{Scheme: {scheme}, Algorithm: \"{}\", SecretEnv: \"{}\", Header: \"{}\"}},",
        escape_string(&verify.algorithm),
        escape_string(&verify.secret_env),
        escape_string(&verify.header),
    )
}

fn effective_policy<'a>(feature: &'a Feature, webhook: &'a Webhook) -> Option<&'a PolicyRef> {
    webhook.policy.as_ref().or(feature.defaults.policy.as_ref())
}

fn format_policy_string(policy: &PolicyRef) -> Option<String> {
    match policy {
        PolicyRef::Local(name) => Some(format!("@policy.{}", name)),
        PolicyRef::Atom(atom) => {
            if atom.starts_with('@') {
                Some(atom.clone())
            } else {
                Some(format!("@{}", atom))
            }
        }
        PolicyRef::External { feature, name } => Some(format!("{}.policy.{}", feature, name)),
        PolicyRef::Unresolved(raw) => Some(raw.clone()),
        PolicyRef::None => None,
    }
}

fn return_type_name(return_type: &TypeRef, ctx: &TypeCtx<'_>) -> String {
    let (go_type, _import) = types::go_type_for(return_type, ctx);
    go_type
}

fn format_string_slice(values: &[String]) -> String {
    let entries: Vec<String> = values
        .iter()
        .map(|value| format!("\"{}\"", escape_string(value)))
        .collect();
    format!("[]string{{{}}},", entries.join(", "))
}

fn emit_runtime_gaps(p: &mut GoPrinter, webhook: &Webhook) {
    if webhook.structured_verify.is_none() {
        p.line(&format!(
            "// TODO(runtime): legacy verifier path \"{}\" is not represented by WebhookContract v0.",
            escape_string(&webhook.verify.path)
        ));
    }
    if let Some(payload) = &webhook.payload_from {
        p.line(&format!(
            "// TODO(runtime): payload_from webhook_events.{} not yet in WebhookContract v0 spine.",
            comment_text(&payload.name)
        ));
    }
    if let Some(replay) = &webhook.replay {
        p.line(&format!(
            "// TODO(runtime): replay {}{} not yet in WebhookContract v0 spine.",
            replay_mode_name(replay.mode),
            replay_suffix(replay)
        ));
    }
    if let Some(dlq) = &webhook.dlq {
        p.line(&format!(
            "// TODO(runtime): dlq {} not yet in WebhookContract v0 spine.",
            describe_dlq(dlq)
        ));
    }
    if let Some(retry) = &webhook.retry {
        p.line(&format!(
            "// TODO(runtime): retry {} backoff {} not yet in WebhookContract v0 spine.",
            retry.count,
            backoff_strategy_name(retry.backoff)
        ));
    }
}

fn replay_mode_name(mode: lazuli_ir::ReplayMode) -> &'static str {
    match mode {
        lazuli_ir::ReplayMode::Allow => "allow",
        lazuli_ir::ReplayMode::Deny => "deny",
    }
}

fn replay_suffix(replay: &lazuli_ir::ReplaySpec) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(within) = &replay.within {
        parts.push(format!("within \"{}\"", comment_text(within)));
    }
    if let Some(dedupe_by) = &replay.dedupe_by {
        parts.push(format!("dedupe by {}", path_to_string(dedupe_by)));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
    }
}

fn describe_dlq(dlq: &lazuli_ir::DlqSpec) -> String {
    match dlq {
        lazuli_ir::DlqSpec::Emit { event } => format!("emit {}", comment_text(event)),
        lazuli_ir::DlqSpec::Handler { path } => {
            format!("handler \"{}\"", comment_text(&path.path))
        }
        lazuli_ir::DlqSpec::Drop { reason } => {
            format!("drop reason \"{}\"", comment_text(reason))
        }
    }
}

fn backoff_strategy_name(backoff: lazuli_ir::BackoffStrategy) -> &'static str {
    match backoff {
        lazuli_ir::BackoffStrategy::Fixed => "fixed",
        lazuli_ir::BackoffStrategy::Exponential => "exponential",
    }
}

fn path_to_string(path: &lazuli_ir::Path) -> String {
    path.segments.join(".")
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

fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(|c: char| c == '_' || c == '-') {
        if word.is_empty() {
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for u in first.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

fn lower_camel(s: &str) -> String {
    super::casing::lower_camel(s)
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl" | "uuid"
    )
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

fn comment_text(raw: &str) -> String {
    raw.chars()
        .map(|ch| if ch == '\r' || ch == '\n' { ' ' } else { ch })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, BackoffStrategy, Defaults, DlqSpec, Feature, IdempotencyKey, Module, Path,
        PathRef, Policies, QualifiedName, ReplayMode, ReplaySpec, RetryPolicy, TenantFromSpec,
        TypeRef, VerifySpec, WebhookEventRef,
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

    fn module_with_feature(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
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
            }),
            registry: None,
            profiles: Vec::new(),
            features: vec![feature],
        }
    }

    fn emit(feature: &Feature) -> Option<String> {
        let module = module_with_feature(feature.clone());
        let index = CrossFeatureIndex::build(&module);
        emit_webhook_file("examples/x.lzi", feature, "lazuli/test", &index)
    }

    fn path(segments: &[&str]) -> Path {
        Path::from_segments(segments.iter().copied())
    }

    fn base_webhook(name: &str) -> Webhook {
        Webhook {
            name: name.to_owned(),
            route: format!("/webhooks/{name}"),
            verify: PathRef::convention(format!("./webhooks/{name}_verify.go")),
            structured_verify: None,
            tenant_from: None,
            idempotency: None,
            policy: None,
            handler: PathRef::authored(format!("./webhooks/{name}.go")),
            returns: None,
            emits: Vec::new(),
            payload_from: None,
            replay: None,
            dlq: None,
            retry: None,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn hmac_verify() -> VerifySpec {
        VerifySpec {
            scheme: VerifyScheme::Hmac,
            algorithm: "sha256".to_owned(),
            secret_env: "MERCADOPAGO_HMAC_SECRET".to_owned(),
            header: "X-Signature".to_owned(),
        }
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("payments");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn hmac_webhook_emits_v0_contract_fields() {
        let mut feature = base_feature("payments");
        feature.defaults.policy = Some(PolicyRef::Atom("actor.system".to_owned()));
        let mut webhook = base_webhook("mercadopago_callback");
        webhook.route = "/webhooks/mercadopago".to_owned();
        webhook.structured_verify = Some(hmac_verify());
        webhook.tenant_from = Some(TenantFromSpec {
            path: path(&["payload", "external_reference"]),
        });
        webhook.idempotency = Some(IdempotencyKey {
            by: path(&["payload", "id"]),
        });
        webhook.handler = PathRef::authored("./webhooks/mercadopago_callback.go");
        webhook.returns = Some(TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "Payment".to_owned(),
        }));
        webhook.emits = vec!["payment_received".to_owned()];
        feature.webhooks.push(webhook);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package payments"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/webhooks\""));
        assert!(out.contains("var mercadopagoCallbackWebhook = webhooks.WebhookContract{"));
        assert!(out.contains("Feature:"));
        assert!(out.contains("\"payments\","));
        assert!(out.contains("Verify:"));
        assert!(out.contains("webhooks.VerifySpec{Scheme: webhooks.VerifyHmac"));
        assert!(out.contains("SecretEnv: \"MERCADOPAGO_HMAC_SECRET\""));
        assert!(out.contains("TenantFrom:"));
        assert!(out.contains("&webhooks.TenantFromSpec{Path: \"payload.external_reference\"}"));
        assert!(out.contains("IdempotencyBy:"));
        assert!(out.contains("\"payload.id\","));
        assert!(out.contains("Policy:"));
        assert!(out.contains("\"@actor.system\","));
        assert!(out.contains("HandlerPath:"));
        assert!(out.contains("\"./webhooks/mercadopago_callback.go\","));
        assert!(out.contains("ReturnsType:"));
        assert!(out.contains("\"Payment\","));
        assert!(out.contains("Emits:"));
        assert!(out.contains("[]string{\"payment_received\"},"));
    }

    #[test]
    fn expanded_slots_emit_runtime_todos_inside_literal() {
        let mut feature = base_feature("customer_import");
        let mut webhook = base_webhook("crm_customer_upsert");
        webhook.structured_verify = Some(hmac_verify());
        webhook.payload_from = Some(WebhookEventRef {
            name: "crm_customer_upsert".to_owned(),
        });
        webhook.replay = Some(ReplaySpec {
            mode: ReplayMode::Allow,
            within: Some("24h".to_owned()),
            dedupe_by: Some(path(&["payload", "external_id"])),
        });
        webhook.dlq = Some(DlqSpec::Emit {
            event: "customer_webhook_dead_lettered".to_owned(),
        });
        webhook.retry = Some(RetryPolicy {
            count: 5,
            backoff: BackoffStrategy::Exponential,
        });
        feature.webhooks.push(webhook);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// TODO(runtime): payload_from webhook_events.crm_customer_upsert"));
        assert!(out.contains(
            "// TODO(runtime): replay allow within \"24h\" dedupe by payload.external_id"
        ));
        assert!(out.contains("// TODO(runtime): dlq emit customer_webhook_dead_lettered"));
        assert!(out.contains("// TODO(runtime): retry 5 backoff exponential"));
        assert!(!out.contains("PayloadType:"));
        assert!(!out.contains("Replay:"));
        assert!(!out.contains("DLQ:"));
        assert!(!out.contains("Retry:"));
    }

    #[test]
    fn legacy_verify_path_surfaces_todo() {
        let mut feature = base_feature("legacy");
        feature.webhooks.push(base_webhook("github_ping"));
        let out = emit(&feature).expect("must emit");
        assert!(out.contains(
            "// TODO(runtime): legacy verifier path \"./webhooks/github_ping_verify.go\""
        ));
        assert!(!out.contains("Verify: webhooks.VerifySpec"));
    }

    #[test]
    fn deterministic_across_runs_and_sorts_by_name() {
        let mut feature = base_feature("payments");
        let mut zebra = base_webhook("zebra");
        zebra.structured_verify = Some(hmac_verify());
        let mut alpha = base_webhook("alpha");
        alpha.structured_verify = Some(hmac_verify());
        feature.webhooks.push(zebra);
        feature.webhooks.push(alpha);

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);

        let alpha_pos = a.find("Webhook: payments.alpha").expect("alpha banner");
        let zebra_pos = a.find("Webhook: payments.zebra").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }
}
