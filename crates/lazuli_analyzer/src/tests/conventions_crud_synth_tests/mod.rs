//! CRUD-synthesis tests for the conventions pass — Rails-style R9 split.
//! Shared fixture builders live here; tests are partitioned across
//! [`basic`] (worked example + author-override + fx1 happy paths) and
//! [`edge_cases`] (fx1 catalog collision warns, lifecycle preservation,
//! missing-policy + signature-mismatch diagnostics, no-op resources).

mod basic;
mod edge_cases;

use crate::{CrudSynthDiagnostic, synthesize_conventions};
use lazuli_ir as ir;

/// Minimal `Feature` for testing — empty defaults, a single
/// `authenticated` policy unless the test overrides.
pub(super) fn empty_feature(name: &str, with_authenticated: bool) -> ir::Feature {
    let policies = if with_authenticated {
        ir::Policies {
            categories: vec![ir::PolicyCategory {
                name: "authenticated".to_owned(),
                atoms: vec!["@scope.authenticated".to_owned()],
                conditional_atoms: Vec::new(),
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        }
    } else {
        ir::Policies::default()
    };
    ir::Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        knowledge: None,
        defaults: ir::Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
            rate_limit: None,
            audit: None,
        },
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies,
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
        synth_origins: std::collections::BTreeMap::new(),
    }
}

pub(super) fn req_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
    ir::Field {
        name: name.to_owned(),
        type_ref,
        required: true,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: ir::FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

pub(super) fn req_unique_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
    ir::Field {
        unique: true,
        ..req_field(name, type_ref)
    }
}

pub(super) fn user_qn(name: &str) -> ir::TypeRef {
    ir::TypeRef::UserDefined(ir::QualifiedName {
        feature: None,
        name: name.to_owned(),
    })
}

pub(super) fn author_list_customers_query(policy: ir::PolicyRef) -> ir::Query {
    let mut query = crate::conventions::build_list_query("list_customers", "Customer");
    match &mut query {
        ir::Query::List(lq) => {
            lq.policy = policy;
        }
        other => panic!("expected list query helper to build List, got {other:?}"),
    }
    query
}

pub(super) fn customer_resource() -> ir::Resource {
    // §8 worked example: feature customer, resource Customer.
    ir::Resource {
        name: "Customer".to_owned(),
        public_contract: None,
        tenancy: Some(ir::Tenancy::Org),
        soft_delete: false,
        soft_delete_actor: false,
        timestamps: None,
        fields: vec![
            req_field("org", user_qn("Org")),
            req_unique_field(
                "email",
                ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
            ),
            req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            req_field("status", user_qn("CustomerStatus")),
            req_field(
                "created_at",
                ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
            ),
        ],
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        invariants: Vec::new(),
        lock: None,
        composite_key: None,
        conventions: vec![ir::ConventionRef::Crud],
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    }
}
