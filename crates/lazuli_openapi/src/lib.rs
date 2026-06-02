//! OpenAPI 3.1.0 emission from typed Lazuli IR.
//!
//! Pure consumer of `lazuli_ir::Module`. Walks features for commands,
//! agent `expose http` mounts, `api <name>` blocks, and webhooks; emits
//! an OpenAPI 3.1 document via a small purpose-built YAML printer (we
//! don't depend on `serde_yaml` to keep the build graph tight).
//!
//! Boundary discipline: this crate emits a spec artifact. It never
//! generates Go server stubs, language SDKs, or any transport binding —
//! those live in adapters that consume the emitted spec.
//!
//! ## Module layout
//!
//! - `paths.rs` — one path-object emitter per IR operation flavour
//!   (`emit_command`, `emit_api`, `emit_agent_expose`, `emit_webhook`)
//!   plus path/method derivation, policy rendering, and deprecation
//!   replacement helpers.
//! - `schemas.rs` — `components.schemas` entries and inline schema
//!   fragments for request bodies, path parameters, and slot types.
//! - `extensions.rs` — `x-lazuli-*` blocks (approval, verify, retry,
//!   replay, dlq) and their literal-table helpers.
//! - `yaml.rs` — purpose-built indent-prefixed YAML emitter and the
//!   `quote_key` / `quote_value` helpers.
//!
//! The single public entry point is [`emit`]; everything else stays
//! `pub(crate)` so consumers cannot couple to the internal layout.

// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
mod extensions;
mod paths;
mod schemas;
mod yaml;

use lazuli_ir as ir;

use crate::paths::{emit_agent_expose, emit_api, emit_command, emit_webhook};
use crate::schemas::{emit_enum_schema, emit_record_schema, emit_resource_schema};
use crate::yaml::YamlEmitter;

/// Knobs the OpenAPI emitter exposes to its caller.
///
/// Kept deliberately small. Each new knob is a CLI flag commitment, so
/// the bar is "the doctor / build pipeline cannot produce a correct
/// spec without it". Anything stylistic belongs in a downstream
/// formatter, not here.
#[derive(Default)]
pub struct EmitOptions {
    /// API version reported in `info.version`. Defaults to "0.0.0".
    pub api_version: Option<String>,
    /// When true, omit operations whose IR shape is text-pattern only.
    pub strict_typed_only: bool,
}

/// Emit OpenAPI 3.1.0 YAML from a Lazuli `Module`.
///
/// Single entry point. Walks every feature's commands, APIs, exposed
/// agents, and webhooks, then materialises `components.schemas` from the
/// feature's resources / records / enums. The output is text, not a
/// typed `OpenApi` value, so downstream tooling (linting, bundling) sees
/// the same bytes the pilot will publish.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_ir::Module;
/// use lazuli_openapi::{emit, EmitOptions};
///
/// let module: Module = /* obtain via lazuli_analyzer */ unimplemented!();
/// let yaml = emit(&module, EmitOptions::default());
/// assert!(yaml.starts_with("openapi: 3.1.0"));
/// ```
pub fn emit(module: &ir::Module, opts: EmitOptions) -> String {
    let mut out = YamlEmitter::new();
    out.line("openapi: 3.1.0");
    out.line("info:");
    out.indent();
    out.kv(
        "title",
        module
            .app
            .as_ref()
            .map(|a| a.name.as_str())
            .unwrap_or("lazuli"),
    );
    out.kv(
        "version",
        &opts
            .api_version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_owned()),
    );
    out.line("x-lazuli-generator: lazuli_openapi");
    out.dedent();

    out.line("paths:");
    out.indent();
    for feature in &module.features {
        for cmd in &feature.commands {
            emit_command(&mut out, &feature.name, cmd);
        }
        for api in &feature.apis {
            emit_api(&mut out, &feature.name, api, opts.strict_typed_only);
        }
        for agent in &feature.agents {
            if let Some(expose) = &agent.expose_http {
                emit_agent_expose(&mut out, &feature.name, &agent.name, expose);
            }
        }
        for webhook in &feature.webhooks {
            emit_webhook(&mut out, &feature.name, webhook);
        }
    }
    out.dedent();

    out.line("components:");
    out.indent();
    out.line("schemas:");
    out.indent();
    let mut schemas_emitted = false;
    for feature in &module.features {
        for resource in &feature.resources {
            emit_resource_schema(&mut out, resource);
            schemas_emitted = true;
        }
        for record in &feature.records {
            emit_record_schema(&mut out, record);
            schemas_emitted = true;
        }
        for enum_decl in &feature.enums {
            emit_enum_schema(&mut out, enum_decl);
            schemas_emitted = true;
        }
    }
    if !schemas_emitted {
        out.line("{}");
    }
    out.dedent();
    out.line("responses:");
    out.indent();
    out.line("Problem:");
    out.indent();
    out.line("description: RFC 7807 problem-details envelope.");
    out.line("content:");
    out.indent();
    out.line("application/problem+json:");
    out.indent();
    out.line("schema:");
    out.indent();
    out.line("type: object");
    out.line("required: [type, title, status]");
    out.line("properties:");
    out.indent();
    out.kv("type", "{ type: string, format: uri }");
    out.kv("title", "{ type: string }");
    out.kv("status", "{ type: integer }");
    out.kv("detail", "{ type: string }");
    out.kv("instance", "{ type: string }");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();

    out.into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module_with_feature(feature: ir::Feature) -> ir::Module {
        ir::Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            doctor_allows: Vec::new(),
            features: vec![feature],
        }
    }

    fn base_feature() -> ir::Feature {
        ir::Feature {
            name: "billing".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            knowledge: None,
            defaults: ir::Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: ir::Policies::default(),
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

    fn base_command() -> ir::Command {
        ir::Command {
            name: "reassign".to_owned(),
            public_contract: None,
            kind: ir::CommandKind::Update,
            route: Vec::new(),
            input: ir::CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::None,
            policy: ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            previous_names: Vec::new(),
            span_ref: None,
            derived_from: None,
        }
    }

    fn base_webhook() -> ir::Webhook {
        ir::Webhook {
            name: "stripe_invoice_paid".to_owned(),
            route: "/webhooks/stripe/invoice-paid".to_owned(),
            verify: ir::PathRef::authored("./webhooks/verify_stripe.go"),
            structured_verify: None,
            tenant_from: None,
            scope_global: None,
            idempotency: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            handler: ir::PathRef::authored("./webhooks/stripe_invoice_paid.go"),
            returns: None,
            emits: Vec::new(),
            emit_predicates: Vec::new(),
            payload_from: None,
            replay: None,
            dlq: None,
            retry: None,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn command_tier4_fields_emit_as_openapi_extensions() {
        let mut command = base_command();
        command.rate_limit = Some(ir::RateLimitSpec::from_default(
            "10 per minute per user".to_owned(),
        ));
        command.audit = Some(ir::AuditSpec {
            subjects: vec!["actor".to_owned(), "target.id".to_owned()],
            emit_to: Some("audit_log".to_owned()),
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
            materialize: None,
        });
        command.approval = Some(ir::ApprovalSpec {
            required_when: Some("target.tier = enterprise".to_owned()),
            by: "@role.admin".to_owned(),
            chain: vec!["@role.admin".to_owned()],
            sequential: false,
            timeout: Some("24h".to_owned()),
            then: ir::ApprovalThen::Deny,
        });
        command.deprecated = Some(ir::Deprecation {
            since: Some("2026.04".to_owned()),
            replacement: Some(ir::DeprecationReplacement::LocalCommand(
                "reassign_v2".to_owned(),
            )),
            sunset: Some("2026-12-31".to_owned()),
        });

        let mut feature = base_feature();
        feature.commands.push(command);
        let yaml = emit(&module_with_feature(feature), EmitOptions::default());

        assert!(yaml.contains("x-lazuli-rate-limit: \"10 per minute per user\""));
        assert!(yaml.contains("x-lazuli-audit: [\"actor\", \"target.id\"]"));
        assert!(yaml.contains("x-lazuli-audit-emit-to: \"audit_log\""));
        assert!(yaml.contains(
            "x-lazuli-approval:\n        then: deny\n        by: \"@role.admin\"\n        reason: \"target.tier = enterprise\""
        ));
        assert!(yaml.contains("deprecated: true"));
        assert!(yaml.contains("x-lazuli-deprecated-since: \"2026.04\""));
        assert!(yaml.contains("x-lazuli-deprecated-replacement: \"billing.command.reassign_v2\""));
        assert!(yaml.contains("x-lazuli-deprecated-sunset: \"2026-12-31\""));
        assert!(!yaml.contains("x-lazuli-replacement:"));
        assert!(!yaml.contains("x-lazuli-sunset:"));
    }

    #[test]
    fn webhook_replay_retry_and_dlq_emit_as_openapi_extensions() {
        let mut webhook = base_webhook();
        webhook.structured_verify = Some(ir::VerifySpec {
            scheme: ir::VerifyScheme::Hmac,
            algorithm: "sha256".to_owned(),
            secret_env: "STRIPE_SECRET".to_owned(),
            header: "Stripe-Signature".to_owned(),
        });
        webhook.tenant_from = Some(ir::TenantFromSpec {
            path: ir::Path::from_segments(["payload", "org_id"]),
        });
        webhook.idempotency = Some(ir::IdempotencyKey {
            by: ir::Path::from_segments(["payload", "event_id"]),
        });
        webhook.payload_from = Some(ir::WebhookEventRef {
            name: "stripe_invoice_paid".to_owned(),
        });
        webhook.retry = Some(ir::RetryPolicy {
            count: 5,
            backoff: ir::BackoffStrategy::Exponential,
        });
        webhook.replay = Some(ir::ReplaySpec {
            mode: ir::ReplayMode::Allow,
            within: Some("24h".to_owned()),
            dedupe_by: Some(ir::Path::from_segments(["payload", "event_id"])),
        });
        webhook.dlq = Some(ir::DlqSpec::Emit {
            event: "stripe_invoice_paid_dead_lettered".to_owned(),
        });

        let mut feature = base_feature();
        feature.webhooks.push(webhook);
        let yaml = emit(&module_with_feature(feature), EmitOptions::default());

        assert!(yaml.contains("/webhooks/stripe/invoice-paid:\n    post:"));
        assert!(yaml.contains("x-lazuli-kind: webhook"));
        assert!(yaml.contains(
            "x-lazuli-verify:\n        scheme: hmac\n        algorithm: \"sha256\"\n        secret_env: \"STRIPE_SECRET\"\n        header: \"Stripe-Signature\""
        ));
        assert!(yaml.contains("x-lazuli-tenant-from: \"payload.org_id\""));
        assert!(yaml.contains("x-lazuli-idempotency-by: \"payload.event_id\""));
        assert!(yaml.contains("x-lazuli-payload-from: \"webhook_events.stripe_invoice_paid\""));
        assert!(yaml.contains("x-lazuli-retry:\n        count: 5\n        backoff: exponential"));
        assert!(yaml.contains(
            "x-lazuli-replay:\n        mode: allow\n        within: \"24h\"\n        dedupe_by: \"payload.event_id\""
        ));
        assert!(yaml.contains(
            "x-lazuli-dlq:\n        kind: emit\n        event: \"stripe_invoice_paid_dead_lettered\""
        ));
    }
}
