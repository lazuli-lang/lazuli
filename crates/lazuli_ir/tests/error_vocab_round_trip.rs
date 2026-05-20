//! Round-trip test for the IR Error-Vocab cell (Cell IR-1).
//!
//! Constructs a `Feature` with every new error-vocab field populated
//! (`Feature.errors`, `PolicyCategory.when_denied`, `Command.policy_when_denied`,
//! `Api.policy_when_denied`, `Webhook.policy_when_denied`,
//! `Job.policy_when_denied`, `Channel.policy_when_denied`,
//! `Workflow.policy_when_denied`, `Agent.policy_when_denied`, plus every
//! `Query` variant). Serializes via serde to JSON, deserializes back, and
//! asserts equality.
//!
//! See `docs/proposals/ir-error-messages-vocab.md` §11 Cell IR-1.

use lazuli_ir::{
    Agent, AgentOutputKind, Api, BuiltinType, Channel, Command, CommandEffect, CommandInput,
    CommandKind, Defaults, ErrorExposureDefault, Feature, FeatureErrorMessage, FeatureErrors,
    FeatureFieldError, FieldRef, HttpMethod, Job, JobBody, JobDeclarative, JobTrigger, ListQuery,
    LookupQuery, Path, PathRef, PathSource, Policies, PolicyCategory, PolicyRef, QualifiedName,
    Query, SpanRef, SqlQuery, TenantFromSpec, TranslationKeyRef, TypeRef, Webhook, Workflow,
};

fn key_ref(name: &str, span_offset: usize) -> TranslationKeyRef {
    TranslationKeyRef {
        key: name.to_owned(),
        span_ref: Some(SpanRef {
            start: span_offset,
            end: span_offset + 16,
        }),
    }
}

fn empty_feature() -> Feature {
    Feature {
        name: "account".to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
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
        pollers: Vec::new(),
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: Vec::new(),
        mcp_servers: Vec::new(),
        previous_names: Vec::new(),
        span_ref: None,
    }
}

#[test]
fn translation_key_ref_round_trips_through_json() {
    let key = key_ref("must_be_signed_in", 0);
    let json = serde_json::to_string(&key).expect("serialize TranslationKeyRef");
    let back: TranslationKeyRef =
        serde_json::from_str(&json).expect("deserialize TranslationKeyRef");
    assert_eq!(key, back);
    assert!(
        json.contains("\"key\":\"must_be_signed_in\""),
        "key field must serialize verbatim: {json}"
    );
}

#[test]
fn translation_key_ref_omits_span_when_none() {
    let key = TranslationKeyRef {
        key: "compact".to_owned(),
        span_ref: None,
    };
    let json = serde_json::to_string(&key).expect("serialize compact TranslationKeyRef");
    assert!(
        !json.contains("span_ref"),
        "skip_serializing_if = Option::is_none should drop span_ref: {json}"
    );
}

#[test]
fn error_exposure_default_round_trips_snake_case() {
    let hide_json = serde_json::to_string(&ErrorExposureDefault::Hide).expect("serialize");
    assert_eq!(hide_json, "\"hide\"");
    let expose_json = serde_json::to_string(&ErrorExposureDefault::Expose).expect("serialize");
    assert_eq!(expose_json, "\"expose\"");

    let back: ErrorExposureDefault =
        serde_json::from_str(&hide_json).expect("deserialize hide");
    assert_eq!(back, ErrorExposureDefault::Hide);
}

#[test]
fn feature_errors_round_trips_with_all_subshapes() {
    let errors = FeatureErrors {
        default: Some(ErrorExposureDefault::Hide),
        exposure_4xx: vec!["message".to_owned(), "code".to_owned(), "message_key".to_owned()],
        exposure_5xx: vec!["code".to_owned()],
        messages: vec![
            FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: key_ref("account_signin_required", 100),
                span_ref: Some(SpanRef {
                    start: 100,
                    end: 132,
                }),
            },
            FeatureErrorMessage {
                code: "validation_failed".to_owned(),
                message: key_ref("account_invalid_input", 140),
                span_ref: None,
            },
        ],
        field_messages: vec![FeatureFieldError {
            resource: "Customer".to_owned(),
            field: "email".to_owned(),
            code: "format_invalid".to_owned(),
            message: key_ref("customer_email_format", 200),
            span_ref: Some(SpanRef {
                start: 200,
                end: 240,
            }),
        }],
        span_ref: Some(SpanRef { start: 80, end: 260 }),
    };
    let json = serde_json::to_string(&errors).expect("serialize FeatureErrors");
    let back: FeatureErrors =
        serde_json::from_str(&json).expect("deserialize FeatureErrors");
    assert_eq!(errors, back);
}

#[test]
fn feature_errors_default_constructor_is_empty() {
    let empty = FeatureErrors::default();
    assert!(empty.default.is_none());
    assert!(empty.exposure_4xx.is_empty());
    assert!(empty.exposure_5xx.is_empty());
    assert!(empty.messages.is_empty());
    assert!(empty.field_messages.is_empty());
    assert!(empty.span_ref.is_none());
}

#[test]
fn feature_with_all_error_vocab_fields_round_trips() {
    let mut feature = empty_feature();

    // Feature.errors — the new lowering surface.
    feature.errors = Some(FeatureErrors {
        default: Some(ErrorExposureDefault::Hide),
        exposure_4xx: vec!["message".to_owned(), "code".to_owned()],
        exposure_5xx: vec!["code".to_owned()],
        messages: vec![FeatureErrorMessage {
            code: "policy_denied".to_owned(),
            message: key_ref("account_signin_required", 100),
            span_ref: None,
        }],
        field_messages: Vec::new(),
        span_ref: Some(SpanRef { start: 80, end: 200 }),
    });

    // PolicyCategory.when_denied — per-policy default.
    feature.policies.categories.push(PolicyCategory {
        name: "authenticated".to_owned(),
        atoms: vec!["@scope.authenticated".to_owned()],
        previous_names: Vec::new(),
        when_denied: Some(key_ref("must_be_signed_in", 300)),
    });

    // Command.policy_when_denied — per-command override.
    feature.commands.push(Command {
        name: "choose_role".to_owned(),
        public_contract: None,
        kind: CommandKind::Returns,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: CommandEffect::None,
        policy: PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: Some(key_ref("choose_role_signin_required", 400)),
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
        previous_names: Vec::new(),
        span_ref: None,
    });

    // Api.policy_when_denied
    feature.apis.push(Api {
        name: "ping".to_owned(),
        method: HttpMethod::Get,
        path: "/ping".to_owned(),
        policy: PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: Some(key_ref("api_signin_required", 500)),
        rate_limit: None,
        output: TypeRef::Builtin(BuiltinType::Boolean),
        handler: PathRef {
            path: "./api/ping.go".to_owned(),
            source: PathSource::Authored,
        },
        locale_negotiate: None,
        deprecated: None,
        span_ref: None,
    });

    // Webhook.policy_when_denied
    feature.webhooks.push(Webhook {
        name: "stripe_invoice_paid".to_owned(),
        route: "/webhooks/stripe/invoice-paid".to_owned(),
        verify: PathRef {
            path: "./hooks/verify_stripe.go".to_owned(),
            source: PathSource::Authored,
        },
        structured_verify: None,
        tenant_from: None,
        scope_global: None,
        idempotency: None,
        policy: Some(PolicyRef::Local("authenticated".to_owned())),
        policy_expr: None,
        policy_when_denied: Some(key_ref("webhook_signin_required", 600)),
        handler: PathRef {
            path: "./hooks/stripe_invoice_paid.go".to_owned(),
            source: PathSource::Authored,
        },
        returns: None,
        emits: Vec::new(),
        emit_predicates: Vec::new(),
        payload_from: None,
        replay: None,
        dlq: None,
        retry: None,
        previous_names: Vec::new(),
        span_ref: None,
    });

    // Job.policy_when_denied (reserved slot)
    feature.jobs.push(Job {
        name: "sweep".to_owned(),
        trigger: JobTrigger::Schedule {
            cron: "0 2 * * *".to_owned(),
        },
        queue: None,
        idempotency: None,
        retry: None,
        policy: None,
        policy_expr: None,
        policy_when_denied: Some(key_ref("job_signin_required", 700)),
        tenant_from: None,
        fanout: None,
        timeout: None,
        external_calls: Vec::new(),
        body: JobBody::Declarative(JobDeclarative {
            target: None,
            lets: Vec::new(),
            effect: CommandEffect::None,
        }),
        emits: Vec::new(),
        previous_names: Vec::new(),
        span_ref: None,
    });

    // Channel.policy_when_denied (reserved slot)
    feature.channels.push(Channel {
        name: "live_feed".to_owned(),
        tenant_from: TenantFromSpec {
            path: Path::from_segments(["payload", "org_id"]),
        },
        policy: PolicyRef::Local("authenticated".to_owned()),
        policy_when_denied: Some(key_ref("channel_signin_required", 800)),
        payload: "FeedEvent".to_owned(),
        span_ref: None,
    });

    // Workflow.policy_when_denied (reserved slot)
    feature.workflows.push(Workflow {
        name: "review".to_owned(),
        on: FieldRef {
            resource: QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            },
            field: "status".to_owned(),
        },
        default_policy: None,
        default_emits: Vec::new(),
        transitions: Vec::new(),
        policy_when_denied: Some(key_ref("workflow_signin_required", 900)),
        previous_names: Vec::new(),
        span_ref: None,
    });

    // Agent.policy_when_denied (reserved slot)
    feature.agents.push(Agent {
        name: "summarizer".to_owned(),
        feature: "account".to_owned(),
        input: Vec::new(),
        context: None,
        policy: Some(PolicyRef::Local("authenticated".to_owned())),
        policy_when_denied: Some(key_ref("agent_signin_required", 1000)),
        rate_limit: None,
        output_kind: AgentOutputKind::Text,
        output_type: None,
        output_discriminator: None,
        model: None,
        temperature: None,
        max_tokens: None,
        top_p: None,
        seed: None,
        prompt_path: None,
        safety: Vec::new(),
        tools: Vec::new(),
        evals: Vec::new(),
        expose_http: None,
        span_ref: None,
    });

    // Query variants — all three.
    feature.queries.push(Query::List(ListQuery {
        name: "list_active".to_owned(),
        public_contract: None,
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        paginate: None,
        modifier: None,
        cache: None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: Some(key_ref("list_signin_required", 1100)),
        previous_names: Vec::new(),
        span_ref: None,
    }));
    feature.queries.push(Query::Lookup(LookupQuery {
        name: "by_id".to_owned(),
        public_contract: None,
        params: Vec::new(),
        keys: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: Some(key_ref("lookup_signin_required", 1200)),
        previous_names: Vec::new(),
        span_ref: None,
    }));
    feature.queries.push(Query::Sql(SqlQuery {
        name: "monthly_audit".to_owned(),
        public_contract: None,
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        returns: TypeRef::Builtin(BuiltinType::Boolean),
        sql_path: "./queries/monthly_audit.sql".to_owned(),
        cache: None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: Some(key_ref("sql_signin_required", 1300)),
        previous_names: Vec::new(),
        span_ref: None,
    }));

    // Round-trip.
    let json = serde_json::to_string(&feature).expect("serialize Feature");
    let back: Feature = serde_json::from_str(&json).expect("deserialize Feature");
    assert_eq!(feature, back);

    // Sanity-check key strings landed on the wire so a downstream
    // consumer can grep without parsing.
    assert!(
        json.contains("\"choose_role_signin_required\""),
        "command.policy_when_denied key missing from JSON: {json}"
    );
    assert!(
        json.contains("\"must_be_signed_in\""),
        "policy_category.when_denied key missing from JSON: {json}"
    );
    assert!(
        json.contains("\"account_signin_required\""),
        "feature.errors.messages key missing from JSON: {json}"
    );
    assert!(
        json.contains("\"exposure_4xx\""),
        "feature.errors.exposure_4xx field missing from JSON: {json}"
    );
}

#[test]
fn feature_without_error_vocab_fields_omits_them_from_json() {
    let feature = empty_feature();
    let json = serde_json::to_string(&feature).expect("serialize bare Feature");
    assert!(
        !json.contains("\"errors\""),
        "Feature.errors should skip when None: {json}"
    );
    // Round-trip the bare shape too.
    let back: Feature = serde_json::from_str(&json).expect("deserialize bare Feature");
    assert_eq!(feature, back);
}

#[test]
fn pre_vocab_feature_json_deserializes_with_none_errors() {
    // A pre-vocab JSON fixture has no `errors` field; deserialization
    // must populate `Feature.errors = None` via serde defaults.
    let json = r#"{
        "name": "legacy",
        "purpose": null,
        "defaults": { "tenancy": null, "timestamps": false, "policy": null },
        "uses": [],
        "enums": [],
        "resources": [],
        "events": [],
        "rules": [],
        "policies": { "categories": [], "fields": [] },
        "commands": [],
        "queries": [],
        "workflows": [],
        "jobs": [],
        "webhooks": [],
        "surfaces": [],
        "extensions": [],
        "escape_routes": []
    }"#;
    let feature: Feature = serde_json::from_str(json).expect("deserialize pre-vocab Feature");
    assert!(feature.errors.is_none());
}

#[test]
fn pre_vocab_policy_category_json_deserializes_with_none_when_denied() {
    let json = r#"{ "name": "create", "atoms": ["@role.admin"] }"#;
    let cat: PolicyCategory = serde_json::from_str(json).expect("deserialize pre-vocab category");
    assert!(cat.when_denied.is_none());
    assert_eq!(cat.atoms, vec!["@role.admin".to_owned()]);
}

#[test]
fn pre_vocab_command_json_deserializes_with_none_policy_when_denied() {
    let json = r#"{
        "name": "noop",
        "kind": "Returns",
        "input": { "kind": "Empty" },
        "effect": { "kind": "None" },
        "policy": { "kind": "None" }
    }"#;
    let cmd: Command = serde_json::from_str(json).expect("deserialize pre-vocab command");
    assert!(cmd.policy_when_denied.is_none());
}
