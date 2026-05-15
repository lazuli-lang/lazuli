//! Cell G2b -- `Webhook` kind emission. Walks every `Webhook`
//! declared on a feature and emits the v0 Lazuli Go
//! `webhooks.WebhookContract` value into `<feature>/webhook.gen.go`.
//!
//! The Lazuli Go runtime carries the webhook spine plus expanded
//! webhook slots: `PayloadFrom`, `Replay`, `DLQ`, and `Retry`.
//! `Retry` reuses `jobs.RetryPolicy`, so the jobs runtime import is
//! included only when a feature declares webhook retry policy.
//!
//! Determinism: webhooks are sorted by name before emission. Imports
//! flow through `ImportSet`, and type strings for `handler returns`
//! reuse `types::go_type_for` so cross-feature names render the same
//! way as resource/command emitters.

use lazuli_ir::{
    BackoffStrategy, DlqSpec, Feature, Gate, PolicyRef, ReplayMode, RetryPolicy, TypeRef,
    VerifyScheme, Webhook,
};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::module::EmitContext;
use super::patterns::{PATTERN_WEBHOOK_RECEIVER, emit_pattern_header};
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

/// Emit `<feature>/webhook.gen.go` for a feature, or `None` when the
/// feature declares no webhooks.
pub fn emit_webhook_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    emit_ctx: &EmitContext<'_>,
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

    imports.add("context");
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/webhooks");
    if webhooks.iter().any(|webhook| webhook.retry.is_some()) {
        imports.add("lazuli.dev/runtime/lazuli/jobs");
    }
    // PG.C.2 — gated webhooks carry a `Prelude: []billing.GateRef{...}`
    // field on the WebhookContract value; the receiver runs it via
    // the runner the `billing` package registers on `webhooks` at
    // init. Import `billing` only when any webhook in the file
    // declares gates.
    let any_gated = webhooks
        .iter()
        .any(|w| !emit_ctx.gates_for("webhook", &w.name).is_empty());
    if any_gated {
        imports.add("lazuli.dev/runtime/lazuli/billing");
        imports.add(&format!("{module_name}/plan"));
    }

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for webhook in &webhooks {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_webhook(&mut p, feature, webhook, &type_ctx, emit_ctx);
    }

    Some(p.finish())
}

fn emit_webhook(
    p: &mut GoPrinter,
    feature: &Feature,
    webhook: &Webhook,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    let qualified_name = format!("{}.{}", feature.name, webhook.name);
    let var_name = format!("{}Webhook", lower_camel(&webhook.name));

    write_section_banner(
        p,
        &[
            format!("Webhook: {qualified_name}"),
            format!("  webhook {}", webhook.name),
        ],
    );

    emit_pattern_header(p, PATTERN_WEBHOOK_RECEIVER);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, webhook.span_ref);
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
    if let Some(payload_from) = &webhook.payload_from {
        kv_rows.push(("PayloadFrom:".to_owned(), format_payload_from(payload_from)));
    }
    if let Some(replay) = &webhook.replay {
        kv_rows.push(("Replay:".to_owned(), format_replay_spec(replay)));
    }
    if let Some(dlq) = &webhook.dlq {
        kv_rows.push(("DLQ:".to_owned(), format_dlq_spec(dlq)));
    }
    if let Some(retry) = &webhook.retry {
        kv_rows.push(("Retry:".to_owned(), format_retry_policy(retry)));
    }

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    emit_ctx.emit_with_source_field(p, "webhook", &webhook.name, webhook.span_ref);
    emit_gate_annotations(p, emit_ctx.gates_for("webhook", &webhook.name));

    emit_runtime_gaps(p, webhook);

    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
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

fn format_payload_from(payload_from: &lazuli_ir::WebhookEventRef) -> String {
    format!(
        "&webhooks.WebhookEventRef{{Name: \"{}\"}},",
        escape_string(&payload_from.name)
    )
}

fn format_replay_spec(replay: &lazuli_ir::ReplaySpec) -> String {
    let mut fields = vec![format!("Mode: {}", replay_mode_const(replay.mode))];
    if let Some(window) = &replay.within {
        fields.push(format!("Window: \"{}\"", escape_string(window)));
    }
    format!("&webhooks.ReplaySpec{{{}}},", fields.join(", "))
}

fn replay_mode_const(mode: ReplayMode) -> &'static str {
    match mode {
        ReplayMode::Allow => "webhooks.ReplayAllow",
        ReplayMode::Deny => "webhooks.ReplayDeny",
    }
}

fn format_dlq_spec(dlq: &DlqSpec) -> String {
    match dlq {
        DlqSpec::Emit { event } => format!(
            "&webhooks.DlqSpec{{Kind: webhooks.DlqEmit, Topic: \"{}\"}},",
            escape_string(event)
        ),
        DlqSpec::Handler { path } => format!(
            "&webhooks.DlqSpec{{Kind: webhooks.DlqHandler, Handler: \"{}\"}},",
            escape_string(&path.path)
        ),
        DlqSpec::Drop { .. } => "&webhooks.DlqSpec{Kind: webhooks.DlqDrop},".to_owned(),
    }
}

fn format_retry_policy(retry: &RetryPolicy) -> String {
    format!(
        "&jobs.RetryPolicy{{Count: {}, Backoff: {}}},",
        retry.count,
        backoff_const(retry.backoff)
    )
}

fn backoff_const(backoff: BackoffStrategy) -> &'static str {
    match backoff {
        BackoffStrategy::Fixed => "jobs.BackoffFixed",
        BackoffStrategy::Exponential => "jobs.BackoffExponential",
    }
}

fn emit_runtime_gaps(p: &mut GoPrinter, webhook: &Webhook) {
    if webhook.structured_verify.is_none() {
        p.line(&format!(
            "// TODO(runtime): legacy verifier path \"{}\" is not represented by WebhookContract v0.",
            escape_string(&webhook.verify.path)
        ));
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

fn lower_camel(s: &str) -> String {
    super::casing::lower_camel(s)
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

/// PG.C.2 — emit the `Prelude: []billing.GateRef{...}` field on a
/// `webhooks.WebhookContract` value. The receiver (`webhooks.Mount`
/// → `handleOne`) consults the slice via the package-level runner
/// the `billing` package registers at init. Empty slice → no field
/// emitted.
fn emit_gate_annotations(p: &mut GoPrinter, gates: &[Gate]) {
    if gates.is_empty() {
        return;
    }
    p.line("Prelude: []billing.GateRef{");
    p.indent();
    for gate in gates {
        match gate {
            Gate::Behind { feature } => {
                p.line(&format!(
                    "{{Kind: billing.GateBehind, Name: {:?}}},",
                    feature
                ));
            }
            Gate::Quota { limit } => {
                p.line(&format!(
                    "{{Kind: billing.GateQuota, Name: {:?}}},",
                    limit
                ));
            }
        }
    }
    p.dedent();
    p.line("},");
}

#[cfg(test)]
mod feature_emit_tests {
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

    fn module_with_feature(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
                name: "test".to_owned(),
                title: None,
                version: None,
                lazuli_version: None,
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
                observability: None,
                locale: None,
                encryption_bindings: Vec::new(),
                span_ref: None,
            }),
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
        let emit_ctx = EmitContext::no_source("customer/webhook.gen.go");
        emit_webhook_file("examples/x.lzi", feature, "lazuli/test", &index, &emit_ctx)
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
            policy_expr: None,
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
        assert!(!out.contains("\"lazuli.dev/runtime/lazuli/jobs\""));
        assert!(!out.contains("PayloadFrom:"));
        assert!(!out.contains("Replay:"));
        assert!(!out.contains("DLQ:"));
        assert!(!out.contains("Retry:"));
    }

    #[test]
    fn payload_from_emits_webhook_event_ref() {
        let mut feature = base_feature("customer_import");
        let mut webhook = base_webhook("crm_customer_upsert");
        webhook.structured_verify = Some(hmac_verify());
        webhook.payload_from = Some(WebhookEventRef {
            name: "crm_customer_upsert".to_owned(),
        });
        feature.webhooks.push(webhook);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("PayloadFrom:"));
        assert!(out.contains("&webhooks.WebhookEventRef{Name: \"crm_customer_upsert\"},"));
        assert!(!out.contains("// TODO(runtime): payload_from"));
    }

    #[test]
    fn replay_emits_mode_and_window() {
        let mut feature = base_feature("customer_import");
        let mut webhook = base_webhook("crm_customer_upsert");
        webhook.structured_verify = Some(hmac_verify());
        webhook.replay = Some(ReplaySpec {
            mode: ReplayMode::Allow,
            within: Some("24h".to_owned()),
            dedupe_by: Some(path(&["payload", "external_id"])),
        });
        feature.webhooks.push(webhook);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("Replay:"));
        assert!(out.contains("&webhooks.ReplaySpec{Mode: webhooks.ReplayAllow, Window: \"24h\"},"));
        assert!(!out.contains("// TODO(runtime): replay"));
        assert!(!out.contains("dedupe by"));
    }

    #[test]
    fn dlq_emits_runtime_variants() {
        let mut feature = base_feature("customer_import");

        let mut emit_dlq = base_webhook("crm_emit_dlq");
        emit_dlq.structured_verify = Some(hmac_verify());
        emit_dlq.dlq = Some(DlqSpec::Emit {
            event: "customer_webhook_dead_lettered".to_owned(),
        });
        feature.webhooks.push(emit_dlq);

        let mut handler_dlq = base_webhook("crm_handler_dlq");
        handler_dlq.structured_verify = Some(hmac_verify());
        handler_dlq.dlq = Some(DlqSpec::Handler {
            path: PathRef::authored("./webhooks/customer_dlq.go"),
        });
        feature.webhooks.push(handler_dlq);

        let mut drop_dlq = base_webhook("crm_drop_dlq");
        drop_dlq.structured_verify = Some(hmac_verify());
        drop_dlq.dlq = Some(DlqSpec::Drop {
            reason: "provider sends transient noise".to_owned(),
        });
        feature.webhooks.push(drop_dlq);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("DLQ:"));
        assert!(out.contains(
            "&webhooks.DlqSpec{Kind: webhooks.DlqEmit, Topic: \"customer_webhook_dead_lettered\"},"
        ));
        assert!(out.contains(
            "&webhooks.DlqSpec{Kind: webhooks.DlqHandler, Handler: \"./webhooks/customer_dlq.go\"},"
        ));
        assert!(out.contains("&webhooks.DlqSpec{Kind: webhooks.DlqDrop},"));
        assert!(!out.contains("// TODO(runtime): dlq"));
        assert!(!out.contains("provider sends transient noise"));
    }

    #[test]
    fn retry_emits_jobs_policy_and_import() {
        let mut feature = base_feature("customer_import");
        let mut webhook = base_webhook("crm_customer_upsert");
        webhook.structured_verify = Some(hmac_verify());
        webhook.retry = Some(RetryPolicy {
            count: 5,
            backoff: BackoffStrategy::Exponential,
        });
        feature.webhooks.push(webhook);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/jobs\""));
        assert!(out.contains("Retry:"));
        assert!(out.contains("&jobs.RetryPolicy{Count: 5, Backoff: jobs.BackoffExponential},"));
        assert!(!out.contains("// TODO(runtime): retry"));
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

    #[test]
    fn feature_emit_entry_point_emits_representative_webhook_file() {
        let mut feature = base_feature("payments");
        let mut webhook = base_webhook("mercadopago_callback");
        webhook.route = "/webhooks/mercadopago".to_owned();
        webhook.structured_verify = Some(hmac_verify());
        webhook.retry = Some(RetryPolicy {
            count: 3,
            backoff: BackoffStrategy::Fixed,
        });
        feature.webhooks.push(webhook);

        let out = emit(&feature).expect("feature with webhook must emit webhook.gen.go");
        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package payments"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/webhooks\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/jobs\""));
        assert!(out.contains("var mercadopagoCallbackWebhook = webhooks.WebhookContract{"));
        assert!(out.contains("Route:       \"/webhooks/mercadopago\","));
        assert!(out.contains("HandlerPath: \"./webhooks/mercadopago_callback.go\","));
        assert!(out.contains("Retry:       &jobs.RetryPolicy{Count: 3, Backoff: jobs.BackoffFixed},"));
    }

    #[test]
    fn gated_webhook_emits_real_prelude_field_and_billing_imports() {
        // PG.C.2 — gated webhooks lift the wave-4 comment
        // annotation into a real `Prelude: []billing.GateRef{...}`
        // field on the WebhookContract; the receiver consults it
        // via the runner billing registers at init.
        let mut feature = base_feature("billing");
        let mut webhook = base_webhook("payment_callback");
        webhook.route = "/webhooks/payment".to_owned();
        webhook.structured_verify = Some(hmac_verify());
        feature.webhooks.push(webhook);

        let mut gates: std::collections::BTreeMap<String, Vec<lazuli_ir::Gate>> =
            std::collections::BTreeMap::new();
        gates.insert(
            "billing/webhook:payment_callback".to_owned(),
            vec![
                lazuli_ir::Gate::Behind {
                    feature: "payment_webhooks".to_owned(),
                },
                lazuli_ir::Gate::Quota {
                    limit: "webhooks_per_month".to_owned(),
                },
            ],
        );
        let module = module_with_feature(feature);
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx =
            EmitContext::for_feature(None, "billing-app", "billing", "billing/webhook.gen.go")
                .with_gates(Some(&gates));
        let out = emit_webhook_file(
            "examples/billing.lzi",
            &module.features[0],
            "billing-app",
            &index,
            &emit_ctx,
        )
        .expect("must emit");

        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "billing import missing:\n{out}"
        );
        assert!(
            out.contains("\"billing-app/plan\""),
            "plan import missing:\n{out}"
        );
        assert!(
            out.contains("Prelude: []billing.GateRef{"),
            "Prelude field missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateBehind, Name: \"payment_webhooks\"},"),
            "behind-gate row missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateQuota, Name: \"webhooks_per_month\"},"),
            "quota-gate row missing:\n{out}"
        );
    }

    #[test]
    fn ungated_webhook_emits_no_prelude_or_billing_import() {
        let mut feature = base_feature("payments");
        let mut webhook = base_webhook("callback");
        webhook.structured_verify = Some(hmac_verify());
        feature.webhooks.push(webhook);
        let out = emit(&feature).expect("must emit");
        assert!(!out.contains("Prelude:"), "no Prelude when no gates");
        assert!(
            !out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "no billing import when no gates"
        );
    }
}
