//! End-to-end round-trip of a Feature populated with every error-vocab
//! field set (the main `lazuli_ir::Feature` shape).

use lazuli_ir::{
    Agent, AgentOutputKind, Api, BuiltinType, Channel, Command, CommandEffect, CommandInput,
    CommandKind, ErrorExposureDefault, Feature, FeatureErrorMessage, FeatureErrors, FieldRef,
    HttpMethod, Job, JobBody, JobDeclarative, JobTrigger, ListQuery, LookupQuery, Path, PathRef,
    PathSource, PolicyCategory, PolicyRef, QualifiedName, Query, SpanRef, SqlQuery, TenantFromSpec,
    TypeRef, Webhook, Workflow,
};

use super::{empty_feature, key_ref};

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
        audience_exposure: Vec::new(),
        redact_patterns: Vec::new(),
        span_ref: Some(SpanRef {
            start: 80,
            end: 200,
        }),
    });

    // PolicyCategory.when_denied — per-policy default.
    feature.policies.categories.push(PolicyCategory {
        name: "authenticated".to_owned(),
        atoms: vec!["@scope.authenticated".to_owned()],
        conditional_atoms: Vec::new(),
        previous_names: Vec::new(),
        when_denied: Some(key_ref("must_be_signed_in", 300)),
        when_denied_route: None,
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
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
        previous_names: Vec::new(),
        span_ref: None,
        derived_from: None,
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
        owner_scope_sql: None,
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
        owner_scope_sql: None,
    }));
    feature.queries.push(Query::Sql(SqlQuery {
        name: "monthly_audit".to_owned(),
        sql_kind: lazuli_ir::SqlQueryKind::Sql,
        public_contract: None,
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        returns: TypeRef::Builtin(BuiltinType::Boolean),
        sql_path: "./queries/monthly_audit.sql".to_owned(),
        sql_text: None,
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
