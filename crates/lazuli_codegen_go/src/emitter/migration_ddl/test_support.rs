//! Shared test fixtures for the `migration_ddl` siblings. Pulled out
//! of `mod.rs` so the sibling `*_tests.rs` files can build their
//! `Module` / `Feature` / `Resource` / `Field` skeletons without each
//! one redeclaring the IR boilerplate.
//!
//! The fixtures stay strictly construction-only — no asserts here — so
//! every sibling can pick whichever helper it needs without dragging
//! unused test bodies into its compilation unit.

#![cfg(test)]
#![allow(dead_code)]

use lazuli_ir::{
    Auth, AuthIdentity, AuthSessions, BuiltinType, CapabilityRef, Defaults, Feature, Field,
    FieldRef, HashAlgorithm, HashedCapability, Module, Policies, QualifiedName, Resource,
    RotationConfig, TypeRef,
};

pub(super) fn base_module(features: Vec<Feature>) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features,
    }
}

pub(super) fn parsed_module(source: &str) -> Module {
    let features = lazuli_syntax::parse_feature_skeletons(source)
        .expect("feature source should parse")
        .into_iter()
        .map(|feature| {
            lazuli_analyzer::lower_feature_skeleton(&feature)
                .expect("feature source should lower")
        })
        .collect();
    base_module(features)
}

pub(super) fn base_feature(name: &str) -> Feature {
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

pub(super) fn resource(name: &str, fields: Vec<Field>) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields,
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        invariants: vec![],

        lock: None,

        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
    }
}

pub(super) fn field(name: &str, type_ref: TypeRef, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref,
        required,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: lazuli_ir::FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

pub(super) fn builtin(name: &str, builtin: BuiltinType, required: bool) -> Field {
    field(name, TypeRef::Builtin(builtin), required)
}

pub(super) fn qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

pub(super) fn auth_session_module(rotation: Option<RotationConfig>) -> Module {
    let mut feature = base_feature("account");
    feature.resources.push(resource(
        "User",
        vec![builtin("email", BuiltinType::SemanticEmail, true)],
    ));
    feature.resources.push(resource(
        "UserSession",
        vec![
            field("user", TypeRef::UserDefined(qname("User")), true),
            field(
                "token_hash",
                TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                    algorithm: HashAlgorithm::Argon2id,
                })),
                true,
            ),
            builtin("expires_at", BuiltinType::DateTime, true),
        ],
    ));
    feature.auth = Some(Auth {
        identity: AuthIdentity {
            field: FieldRef {
                resource: qname("User"),
                field: "email".to_owned(),
            },
            public_contract: None,
        },
        password: None,
        sessions: Some(AuthSessions {
            resource: qname("UserSession"),
            ttl: "7 days".to_owned(),
            refresh: false,
            extra_columns: Vec::new(),
            access_ttl: None,
            rotation,
        }),
        mfa: None,
        oauth: Vec::new(),
        span_ref: None,
    });
    base_module(vec![feature])
}
