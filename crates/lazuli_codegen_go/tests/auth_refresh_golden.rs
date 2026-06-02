//! LAZ-77 / CODEGEN-1 golden tests for refresh-token rotation Go emission.
//!
//! These fixtures intentionally drive the Go emitter from hand-rolled IR:
//! no parser or analyzer changes are in scope for this cell. The generated
//! Go references runtime symbols introduced by RUNTIME-1; this test compares
//! strings only.

use lazuli_codegen_go::{GeneratedFile, GoEmitOptions, generate_v1};
use lazuli_ir::{
    Auth, AuthIdentity, AuthSessions, Defaults, Feature, FieldRef, Module, Policies, QualifiedName,
    RotationConfig,
};

const ROTATION_AUTH_GEN: &str = include_str!("golden/auth-refresh/rotation/account/auth.gen.go");
const ROTATION_AUTH_REFRESH_GEN: &str =
    include_str!("golden/auth-refresh/rotation/account/auth.refresh.gen.go");
const NO_ROTATION_AUTH_GEN: &str =
    include_str!("golden/auth-refresh/no-rotation/account/auth.gen.go");

#[test]
fn rotation_enabled_emits_contract_registration_and_refresh_wire() {
    let files = generate_v1(
        &module_with(vec![feature_with_sessions("account", true)]),
        &GoEmitOptions::default(),
    );

    assert_eq!(
        file_contents(&files, "account/auth.gen.go").expect("account/auth.gen.go"),
        ROTATION_AUTH_GEN
    );
    assert_eq!(
        file_contents(&files, "account/auth.refresh.gen.go").expect("account/auth.refresh.gen.go"),
        ROTATION_AUTH_REFRESH_GEN
    );
}

#[test]
fn rotation_absent_emits_only_existing_auth_file() {
    let files = generate_v1(
        &module_with(vec![feature_with_sessions("account", false)]),
        &GoEmitOptions::default(),
    );

    assert_eq!(
        file_contents(&files, "account/auth.gen.go").expect("account/auth.gen.go"),
        NO_ROTATION_AUTH_GEN
    );
    assert!(
        file_contents(&files, "account/auth.refresh.gen.go").is_none(),
        "rotation-off fixture must not emit account/auth.refresh.gen.go"
    );
}

fn file_contents<'a>(files: &'a [GeneratedFile], path: &str) -> Option<&'a str> {
    files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.contents.as_str())
}

fn module_with(features: Vec<Feature>) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        doctor_allows: Vec::new(),
        features,
    }
}

fn feature_with_sessions(name: &str, rotation: bool) -> Feature {
    let mut feature = empty_feature(name);
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
            rotation: rotation.then_some(RotationConfig {
                refresh_ttl: None,
                grace: None,
                theft_detection_action: None,
                span_ref: None,
            }),
            cookie: None,
        }),
        mfa: None,
        oauth: Vec::new(),
        span_ref: None,
    });
    feature
}

fn empty_feature(name: &str) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        knowledge: None,
        defaults: Defaults {
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
        synth_origins: std::collections::BTreeMap::new(),
    }
}

fn qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}
