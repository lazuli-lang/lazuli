//! Shared `#[cfg(test)]` fixtures for the `SESSION-COOKIE-*` rule family.
//!
//! The five session-cookie rules each read the same IR slice
//! (`feature.auth.sessions.cookie`). Rather than re-spell the full
//! `Feature` literal in every unit-test module, the builders here seed a
//! minimal `customer_auth` feature with a `UserSession` resource and an
//! `auth.sessions` block whose `cookie` slot the caller supplies. Mirrors
//! the per-module `mk_feature` helper in
//! `auth_sessions_resource_unknown_001` but parameterised on the cookie.

#![cfg(test)]

use lazuli_ir::{
    Auth, AuthIdentity, AuthSessions, BuiltinType, Defaults, Feature, Field, FieldConstraints,
    FieldRef, Policies, QualifiedName, Resource, SessionCookie, TypeRef,
};

fn qn(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

fn mk_field(name: &str, type_ref: TypeRef) -> Field {
    Field {
        name: name.to_owned(),
        type_ref,
        required: true,
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

fn mk_session_resource() -> Resource {
    Resource {
        name: "UserSession".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![
            mk_field("id", TypeRef::Builtin(BuiltinType::Id)),
            mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
        ],
        constraints: vec![],
        validate: None,
        validates: vec![],
        retention: None,
        previous_names: vec![],
        span_ref: None,
        lifecycle: None,
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
        append_only: false,
        invariants: vec![],
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
    }
}

/// Build a `customer_auth` feature whose `auth.sessions` carries the
/// supplied `cookie`, plus the optional `rotation`/`refresh` knobs the
/// MISSING rule keys on.
fn build(cookie: Option<SessionCookie>, refresh: bool) -> Feature {
    Feature {
        name: "customer_auth".to_owned(),
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
        resources: vec![mk_session_resource()],
        events: vec![],
        rules: vec![],
        policies: Policies::default(),
        errors: None,
        commands: vec![],
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
        channels: vec![],
        caches: vec![],
        aggregates: vec![],
        mcp_servers: vec![],
        auth: Some(Auth {
            identity: AuthIdentity {
                field: FieldRef {
                    resource: qn("UserSession"),
                    field: "email".to_owned(),
                },
                public_contract: None,
            },
            password: None,
            sessions: Some(AuthSessions {
                resource: qn("UserSession"),
                ttl: "7 days".to_owned(),
                refresh,
                extra_columns: vec![],
                access_ttl: None,
                rotation: None,
                cookie,
            }),
            mfa: None,
            oauth: vec![],
            span_ref: None,
        }),
        surfaces: vec![],
        extensions: vec![],
        escape_routes: vec![],
        agents: vec![],
        reports: vec![],
        previous_names: vec![],
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: None,
    }
}

/// Feature with an `auth.sessions.cookie` carrying the supplied axes.
/// `refresh` is left `false` (the MISSING rule does not key on this
/// builder — it always has a cookie).
pub(super) fn feature_with_cookie(cookie: SessionCookie) -> Feature {
    build(Some(cookie), false)
}

/// Feature whose `auth.sessions` declares NO cookie block.
pub(super) fn feature_no_cookie() -> Feature {
    build(None, false)
}

/// Feature whose `auth.sessions` declares no cookie block but DOES enable
/// the legacy `refresh` flag — the MISSING rule's positive fixture.
pub(super) fn feature_refresh_no_cookie() -> Feature {
    build(None, true)
}
