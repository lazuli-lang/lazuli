//! Wave 4 (TDD/BDD-first proposal) — integration coverage for the new
//! `test_discipline::*` rules. Bypasses the inline `mod tests` because
//! the doctor crate's lib-test build currently fails to compile due to
//! pre-existing stale fixtures in unrelated modules (`Resource` missing
//! `lifecycle_routes`, `TypedSlot` missing `validate_skip`). Those rots
//! are out of Wave 4 scope.
//!
//! The integration test uses only the public IR surface, so it survives
//! the stale-fixture rot in the lib-test target.

use std::path::Path;

use lazuli_doctor::test_discipline::{
    test_command_assertion_drift_001 as cmd_drift, test_view_drift_001 as view_drift,
    test_view_extensibility_001 as view_ext,
};
use lazuli_ir::{
    BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CompareOp, Defaults,
    EvalPredicate, Experience, ExperienceModule, ExperienceView, Expr, Feature, Field,
    FieldConstraints, Invariant, Lifecycle, LifecycleState, LifecycleStateKind, Path as IrPath,
    Policies, PolicyRef, Predicate, QualifiedName, Resource, TestAssertion, TestBlock, TypeRef,
    UpdateEffect, ViewExtension, ViewTestAssertion,
};

// ── view extensibility -------------------------------------------------------

fn view_with_tests(
    name: &str,
    extensible_by: Vec<&str>,
    anchor: Option<&str>,
    tests: Vec<ViewTestAssertion>,
) -> ExperienceView {
    ExperienceView {
        name: name.into(),
        anchor: anchor.map(Into::into),
        routes: vec![],
        extensible_by: extensible_by.into_iter().map(Into::into).collect(),
        source: None,
        submit: None,
        blocks: vec![],
        actions: vec![],
        opens: vec![],
        tests,
        guard: None,
        resolved_guard_policy: None,
        resolved_lifecycle_gate: None,
        span_ref: None,
    }
}

fn module_with_experience(experience: Experience) -> ExperienceModule {
    ExperienceModule {
        app: None,
        routes: vec![],
        experiences: vec![experience],
        surfaces: vec![],
    }
}

fn experience(name: &str, views: Vec<ExperienceView>, extension_anchors: Vec<&str>) -> Experience {
    Experience {
        name: name.into(),
        imports: vec![],
        views,
        resume_routers: vec![],
        extensions: extension_anchors
            .into_iter()
            .map(|a| ViewExtension {
                anchor: a.into(),
                blocks: vec![],
                slots: vec![],
                span_ref: None,
            })
            .collect(),
        span_ref: None,
    }
}

#[test]
fn extensibility_fires_when_extensible_view_has_no_assertions() {
    let view = view_with_tests("detail", vec!["tags", "imports"], Some("@anchor.X"), vec![]);
    let module = module_with_experience(experience("customer", vec![view], vec![]));
    let findings = view_ext::check(&module, Path::new("c.lzx"));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].view, "detail");
    assert_eq!(view_ext::Finding::CODE, "TEST-VIEW-EXTENSIBILITY-001");
}

#[test]
fn extensibility_quiet_when_one_assertion_present() {
    let view = view_with_tests(
        "detail",
        vec!["tags"],
        Some("@anchor.X"),
        vec![ViewTestAssertion::AllowsExtension {
            feature: "tags".into(),
            span_ref: None,
        }],
    );
    let module = module_with_experience(experience("customer", vec![view], vec![]));
    assert!(view_ext::check(&module, Path::new("c.lzx")).is_empty());
}

// ── view drift ----------------------------------------------------------------

#[test]
fn drift_quiet_when_target_extends_anchor() {
    let host_view = view_with_tests(
        "detail",
        vec!["tags"],
        Some("@anchor.customer_detail"),
        vec![ViewTestAssertion::AllowsExtension {
            feature: "tags".into(),
            span_ref: None,
        }],
    );
    let host = experience("customer", vec![host_view], vec![]);
    let target = experience("tags", vec![], vec!["@anchor.customer_detail"]);
    let module = ExperienceModule {
        app: None,
        routes: vec![],
        experiences: vec![host, target],
        surfaces: vec![],
    };
    assert!(view_drift::check(&module, Path::new("c.lzx")).is_empty());
}

#[test]
fn drift_fires_when_target_extends_wrong_anchor() {
    let host_view = view_with_tests(
        "detail",
        vec!["tags"],
        Some("@anchor.customer_detail"),
        vec![ViewTestAssertion::AllowsExtension {
            feature: "tags".into(),
            span_ref: None,
        }],
    );
    let host = experience("customer", vec![host_view], vec![]);
    let target = experience("tags", vec![], vec!["@anchor.OTHER"]);
    let module = ExperienceModule {
        app: None,
        routes: vec![],
        experiences: vec![host, target],
        surfaces: vec![],
    };
    let findings = view_drift::check(&module, Path::new("c.lzx"));
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].kind,
        view_drift::FindingKind::MissingAnchorExtension
    );
    assert_eq!(view_drift::Finding::CODE, "TEST-VIEW-DRIFT-001");
}

#[test]
fn drift_fires_when_target_feature_missing() {
    let host_view = view_with_tests(
        "detail",
        vec!["tags"],
        Some("@anchor.customer_detail"),
        vec![ViewTestAssertion::AllowsExtension {
            feature: "missing_feat".into(),
            span_ref: None,
        }],
    );
    let host = experience("customer", vec![host_view], vec![]);
    let module = module_with_experience(host);
    let findings = view_drift::check(&module, Path::new("c.lzx"));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, view_drift::FindingKind::MissingFeature);
}

// ── command assertion drift --------------------------------------------------

fn mk_field(name: &str) -> Field {
    Field {
        name: name.into(),
        type_ref: TypeRef::Builtin(BuiltinType::Text),
        required: false,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: FieldConstraints::default(),
        full_text: false,
        previous_names: vec![],
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

fn mk_resource(name: &str, lifecycle: Option<Lifecycle>, invariants: Vec<Invariant>) -> Resource {
    Resource {
        name: name.into(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![mk_field("status"), mk_field("host_reply")],
        constraints: vec![],
        validate: None,
        validates: vec![],
        retention: None,
        previous_names: vec![],
        span_ref: None,
        lifecycle,
        invariants,
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    }
}

fn mk_qn(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.into(),
    }
}

fn mk_cmd(name: &str, effect: CommandEffect, tests: Option<TestBlock>) -> Command {
    Command {
        name: name.into(),
        public_contract: None,
        kind: CommandKind::Returns,
        route: vec![],
        input: CommandInput::Empty,
        target: None,
        lets: vec![],
        effect,
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
        handler: None,
        tests,
        triggers: vec![],
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
        previous_names: vec![],
        span_ref: None,
        derived_from: None,
    }
}

fn denies_when_target_eq(field: &str, value: &str) -> TestBlock {
    TestBlock {
        assertions: vec![TestAssertion::DeniesWhen {
            predicate: Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["target", field])),
                op: CompareOp::Eq,
                right: Expr::String(value.to_string()),
            },
        }],
        span_ref: None,
    }
}

fn mk_feature(resources: Vec<Resource>, commands: Vec<Command>) -> Feature {
    Feature {
        name: "trust".into(),
        purpose: None,
        non_goals: vec![],
        context_path: None,
        knowledge: None,
        defaults: Defaults::default(),
        uses: vec![],
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: vec![],
        enums: vec![],
        resources,
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
fn cmd_drift_fires_for_leave_host_reply_pattern() {
    let cmd = mk_cmd(
        "leave_host_reply",
        CommandEffect::Updates(UpdateEffect {
            resource: mk_qn("Review"),
            assignments: vec![],
        }),
        Some(denies_when_target_eq("status", "removed")),
    );
    let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
    let findings = cmd_drift::check(&feature, Path::new("trust.lzi"));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].field, "status");
    assert_eq!(cmd_drift::Finding::CODE, "TEST-COMMAND-ASSERTION-DRIFT-001");
}

#[test]
fn cmd_drift_quiet_when_lifecycle_discriminator_matches() {
    let lifecycle = Lifecycle {
        discriminator_field: "status".into(),
        generated_enum: "ReviewStatus".into(),
        states: vec![LifecycleState {
            name: "active".into(),
            kind: LifecycleStateKind::Initial,
            span_ref: None,
        }],
        transitions: vec![],
        invariants: vec![],
        invariant_handlers: vec![],
        previous_names: vec![],
        span_ref: None,
    };
    let cmd = mk_cmd(
        "leave_host_reply",
        CommandEffect::Updates(UpdateEffect {
            resource: mk_qn("Review"),
            assignments: vec![],
        }),
        Some(denies_when_target_eq("status", "removed")),
    );
    let feature = mk_feature(
        vec![mk_resource("Review", Some(lifecycle), vec![])],
        vec![cmd],
    );
    assert!(cmd_drift::check(&feature, Path::new("trust.lzi")).is_empty());
}

#[test]
fn cmd_drift_quiet_for_returns_commands() {
    let cmd = mk_cmd(
        "summarize",
        CommandEffect::None,
        Some(denies_when_target_eq("status", "removed")),
    );
    let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
    assert!(cmd_drift::check(&feature, Path::new("trust.lzi")).is_empty());
}

#[test]
fn cmd_drift_quiet_when_invariant_mentions_field() {
    let inv = Invariant {
        name: "host_reply_only_when_active".into(),
        when: EvalPredicate::Closed(Predicate::Comparison {
            left: Expr::Path(IrPath::from_segments(["target", "status"])),
            op: CompareOp::Eq,
            right: Expr::String("active".to_string()),
        }),
        message: "".into(),
        span_ref: None,
    };
    let cmd = mk_cmd(
        "leave_host_reply",
        CommandEffect::Updates(UpdateEffect {
            resource: mk_qn("Review"),
            assignments: vec![],
        }),
        Some(denies_when_target_eq("status", "removed")),
    );
    let feature = mk_feature(vec![mk_resource("Review", None, vec![inv])], vec![cmd]);
    assert!(cmd_drift::check(&feature, Path::new("trust.lzi")).is_empty());
}
