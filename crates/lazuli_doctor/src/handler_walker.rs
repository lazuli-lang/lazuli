//! Iterator over every `HandlerRef`-style site in an IR `Feature`.
//!
//! Centralizes the walk so both doctor rules (HANDLER-MISSING-001 /
//! TEST-HANDLER-MISSING-001) and downstream consumers see the same
//! catalog of sites. Adding a new IR slot that carries a handler
//! reference is a one-line change here — every twin-rule, generator,
//! or coverage report picks it up automatically.
//!
//! Site catalog (per docs/proposals/tdd-bdd-first-2026-05-23.md §5.1):
//!
//! - `Command.handler` — `@fn.<name>` driving the command body
//! - `Resource.validate` — resource-level inline validator
//! - `Resource.validates[*]` — per-field inline validators
//! - `Resource.lifecycle.invariant_handlers[*]` — lifecycle escape hatches
//! - `Job.body = Handler(JobHandler)` — when the job uses a handler body
//! - `Webhook.handler` — inbound webhook handler
//!
//! Path references that go through `PathRef` (validators, jobs, webhooks)
//! are normalized into the same `HandlerSite` shape so consumers don't
//! need to special-case the file-vs-name discriminator. The "handler
//! name" surfaced for a `PathRef` is the file stem (so
//! `"./handlers/verify_hmac.go"` yields handler name `verify_hmac`).
//!
//! Cross-feature handler resolution is **out of scope** — the doctor rule
//! only checks same-feature handlers (mirroring the existing HOOK-TARGET-001
//! convention). A handler living under a different feature's directory
//! would be filed under that feature anyway.

use std::path::Path;

use lazuli_ir::{Feature, JobBody, PathRef};

/// What kind of construct holds the handler reference. Used by the rule
/// to render a "X references @fn.Y but ..." message that pinpoints the
/// construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerSiteKind {
    /// `command X { handler @fn.Y }`
    CommandHandler,
    /// `resource X { validates resource "./domain/Y.go" }`
    ResourceValidate,
    /// `resource X { validates field <f> "./hooks/Y.go" }`
    ResourceFieldValidate,
    /// `resource X { lifecycle { invariant_handler @fn.Y } }`
    LifecycleInvariantHandler,
    /// `job X { handler "./jobs/Y.go" returns ... }`
    JobHandler,
    /// `webhook X { handler "./webhooks/Y.go" }`
    WebhookHandler,
}

impl HandlerSiteKind {
    pub fn describe(self) -> &'static str {
        match self {
            Self::CommandHandler => "command handler",
            Self::ResourceValidate => "resource validator",
            Self::ResourceFieldValidate => "field validator",
            Self::LifecycleInvariantHandler => "lifecycle invariant handler",
            Self::JobHandler => "job handler",
            Self::WebhookHandler => "webhook handler",
        }
    }
}

/// One handler reference site, flattened from any of the IR carriers
/// listed in the module-level doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerSite {
    pub kind: HandlerSiteKind,
    /// Feature owning the construct.
    pub feature_name: String,
    /// Name of the command / resource / job / webhook that holds the ref.
    /// `<unknown>` is never used — every IR carrier has a name.
    pub construct_name: String,
    /// `fn`, `validator`, `hook`, or the synthetic namespaces this walker
    /// assigns to `PathRef` carriers (`validator`, `job`, `webhook`).
    pub handler_namespace: String,
    /// Bare handler stem — `verify_password` for `@fn.verify_password`
    /// or for `"./handlers/verify_password.go"`.
    pub handler_name: String,
}

/// Walk every handler-reference site in `feature` and return them in
/// declaration order. The caller filters by [`HandlerSite::handler_namespace`]
/// or [`HandlerSiteKind`] as needed.
pub fn iter_handler_sites(feature: &Feature) -> Vec<HandlerSite> {
    let mut sites = Vec::new();
    let feature_name = feature.name.clone();

    // Command.handler — @fn.<name>
    for cmd in &feature.commands {
        if let Some(href) = &cmd.handler {
            sites.push(HandlerSite {
                kind: HandlerSiteKind::CommandHandler,
                feature_name: feature_name.clone(),
                construct_name: cmd.name.clone(),
                handler_namespace: href.namespace.clone(),
                handler_name: href.name.clone(),
            });
        }
    }

    // Resource.validate / Resource.validates[*] / Resource.lifecycle.invariant_handlers
    for res in &feature.resources {
        if let Some(pref) = &res.validate {
            if let Some(name) = path_ref_stem(pref) {
                sites.push(HandlerSite {
                    kind: HandlerSiteKind::ResourceValidate,
                    feature_name: feature_name.clone(),
                    construct_name: res.name.clone(),
                    handler_namespace: "validator".to_owned(),
                    handler_name: name,
                });
            }
        }
        for fv in &res.validates {
            if let Some(name) = path_ref_stem(&fv.path) {
                sites.push(HandlerSite {
                    kind: HandlerSiteKind::ResourceFieldValidate,
                    feature_name: feature_name.clone(),
                    construct_name: format!("{}.{}", res.name, fv.field),
                    handler_namespace: "validator".to_owned(),
                    handler_name: name,
                });
            }
        }
        if let Some(lifecycle) = &res.lifecycle {
            for href in &lifecycle.invariant_handlers {
                sites.push(HandlerSite {
                    kind: HandlerSiteKind::LifecycleInvariantHandler,
                    feature_name: feature_name.clone(),
                    construct_name: res.name.clone(),
                    handler_namespace: href.namespace.clone(),
                    handler_name: href.name.clone(),
                });
            }
        }
    }

    // Job.body = Handler(JobHandler { path: PathRef, ... })
    for job in &feature.jobs {
        if let JobBody::Handler(jh) = &job.body {
            if let Some(name) = path_ref_stem(&jh.path) {
                sites.push(HandlerSite {
                    kind: HandlerSiteKind::JobHandler,
                    feature_name: feature_name.clone(),
                    construct_name: job.name.clone(),
                    handler_namespace: "job".to_owned(),
                    handler_name: name,
                });
            }
        }
    }

    // Webhook.handler — PathRef
    for webhook in &feature.webhooks {
        if let Some(name) = path_ref_stem(&webhook.handler) {
            sites.push(HandlerSite {
                kind: HandlerSiteKind::WebhookHandler,
                feature_name: feature_name.clone(),
                construct_name: webhook.name.clone(),
                handler_namespace: "webhook".to_owned(),
                handler_name: name,
            });
        }
    }

    sites
}

/// Strip the file stem out of a `PathRef`. Returns `None` for paths that
/// don't carry a Go-extension filename (e.g. directory paths,
/// `@validator.<name>` symbolic refs, or anything that doesn't end in
/// `.go`).
///
/// For `"./domain/validate_row.go"` returns `Some("validate_row")`.
/// For `"@validator.tier_check"` returns `None` — the analyzer carries
/// these as symbolic references to `Extension` entries; their real
/// filesystem path is materialized later from the matching
/// `validator <name> ... at "..."` declaration. The walker treats them
/// as out-of-scope for v0.1; a follow-up cell can resolve through the
/// extensions index.
fn path_ref_stem(pref: &PathRef) -> Option<String> {
    let path_str = pref.path.trim();
    if !path_str.ends_with(".go") {
        return None;
    }
    let path = Path::new(path_str);
    let stem = path.file_stem()?.to_str()?;
    if stem.is_empty() || stem.starts_with('@') {
        return None;
    }
    Some(stem.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, FieldValidation,
        HandlerRef, Job, JobBody, JobHandler, JobTrigger, PathRef, Policies, PolicyRef,
        QualifiedName, ReturnsEffect, TypeRef, Webhook,
    };

    fn mk_cmd(name: &str, handler: Option<HandlerRef>) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(BuiltinType::Boolean),
            }),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        }
    }

    fn mk_feature(commands: Vec<Command>) -> Feature {
        Feature {
            name: "post".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands,
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn command_handler_walked() {
        let feature = mk_feature(vec![mk_cmd(
            "create_post",
            Some(HandlerRef {
                namespace: "fn".into(),
                name: "validate_title".into(),
                span_ref: None,
            }),
        )]);

        let sites = iter_handler_sites(&feature);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].kind, HandlerSiteKind::CommandHandler);
        assert_eq!(sites[0].handler_namespace, "fn");
        assert_eq!(sites[0].handler_name, "validate_title");
        assert_eq!(sites[0].construct_name, "create_post");
    }

    #[test]
    fn empty_command_handler_skipped() {
        let feature = mk_feature(vec![mk_cmd("create_post", None)]);
        let sites = iter_handler_sites(&feature);
        assert!(sites.is_empty());
    }

    #[test]
    fn field_validator_walked() {
        let mut feature = mk_feature(vec![]);
        feature.resources.push(lazuli_ir::Resource {
            name: "Post".into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![],
            constraints: vec![],
            validate: Some(PathRef::authored("./domain/validate_row.go".to_owned())),
            validates: vec![FieldValidation {
                field: "tier".into(),
                path: PathRef::authored("./hooks/validate_tier.go".to_owned()),
            }],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
        });

        let sites = iter_handler_sites(&feature);
        assert_eq!(sites.len(), 2);
        assert!(
            sites
                .iter()
                .any(|s| s.handler_name == "validate_row"
                    && s.kind == HandlerSiteKind::ResourceValidate)
        );
        assert!(
            sites
                .iter()
                .any(|s| s.handler_name == "validate_tier"
                    && s.kind == HandlerSiteKind::ResourceFieldValidate
                    && s.construct_name == "Post.tier")
        );
    }

    #[test]
    fn job_handler_walked() {
        let mut feature = mk_feature(vec![]);
        feature.jobs.push(Job {
            name: "send_welcome".into(),
            trigger: JobTrigger::Event {
                event: QualifiedName {
                    feature: None,
                    name: "user_signed_up".into(),
                },
            },
            queue: None,
            idempotency: None,
            retry: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            tenant_from: None,
            fanout: None,
            timeout: None,
            external_calls: vec![],
            body: JobBody::Handler(JobHandler {
                path: PathRef::authored("./jobs/dispatch_welcome.go".to_owned()),
                returns: None,
            }),
            emits: vec![],
            previous_names: vec![],
            span_ref: None,
        });

        let sites = iter_handler_sites(&feature);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].kind, HandlerSiteKind::JobHandler);
        assert_eq!(sites[0].handler_name, "dispatch_welcome");
        assert_eq!(sites[0].handler_namespace, "job");
    }

    #[test]
    fn webhook_handler_walked() {
        let mut feature = mk_feature(vec![]);
        feature.webhooks.push(Webhook {
            name: "stripe_invoice_paid".into(),
            route: "/webhooks/stripe/invoice-paid".into(),
            verify: PathRef::authored("./webhooks/verify_stripe.go".to_owned()),
            structured_verify: None,
            tenant_from: None,
            scope_global: None,
            idempotency: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            handler: PathRef::authored("./webhooks/handle_invoice_paid.go".to_owned()),
            returns: None,
            emits: vec![],
            emit_predicates: vec![],
            payload_from: None,
            replay: None,
            dlq: None,
            retry: None,
            previous_names: vec![],
            span_ref: None,
        });

        let sites = iter_handler_sites(&feature);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].kind, HandlerSiteKind::WebhookHandler);
        assert_eq!(sites[0].handler_name, "handle_invoice_paid");
    }
}
