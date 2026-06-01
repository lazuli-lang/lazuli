//! Owner-scope synthesis tests — Rails-style R9 split. Shared fixture
//! builders live here; tests partition across [`emit`] (chain-WHERE +
//! CTE + composition emission) and [`diagnostics`] (owner-axis error
//! catalog + builder round-trip).

mod diagnostics;
mod emit;

use crate::{
    ConventionSynthDiagnostic, build_owner_scope_cte_prefix_for_test,
    build_owner_scope_where_for_test, synthesize_conventions,
};
use lazuli_ir as ir;

pub(super) fn empty_feature(name: &str) -> ir::Feature {
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
        },
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: ir::Policies {
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

/// Build an FK field annotated with `@owner_axis(through: <col>)`.
pub(super) fn fk_field_with_axis(name: &str, target: &str, through: &str) -> ir::Field {
    let mut f = req_field(
        name,
        ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: target.to_owned(),
        }),
    );
    f.owner_axis = Some(ir::OwnerAxis {
        through_column: through.to_owned(),
    });
    f
}

pub(super) fn user_qn(name: &str) -> ir::TypeRef {
    ir::TypeRef::UserDefined(ir::QualifiedName {
        feature: None,
        name: name.to_owned(),
    })
}

/// Build the the canonical pilot's `Host` resource (the FK target with
/// the `user: User required unique` actor key). Used to back the
/// owner-chain in fixtures.
pub(super) fn host_resource() -> ir::Resource {
    ir::Resource {
        name: "Host".to_owned(),
        public_contract: None,
        tenancy: Some(ir::Tenancy::Org),
        soft_delete: false,
        timestamps: None,
        fields: vec![
            req_field("org", user_qn("Org")),
            req_unique_field("user", user_qn("User")),
            req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
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
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
    }
}

/// Build the trigger pilot's `Property` resource — owner-scoped via
/// `host: Host required @owner_axis(through: user)`.
pub(super) fn property_resource_with_axis() -> ir::Resource {
    ir::Resource {
        name: "Property".to_owned(),
        public_contract: None,
        tenancy: Some(ir::Tenancy::Org),
        soft_delete: false,
        timestamps: None,
        fields: vec![
            req_field("org", user_qn("Org")),
            fk_field_with_axis("host", "Host", "user"),
            req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
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
