//! Feature-emit tests for storage. Split out of `mod.rs` to keep
//! production under the 500 LOC budget.

use super::*;
use lazuli_ir::{
    AppManifest, CapabilityRef, Defaults, Feature, Field, FileCapability, FileSize,
    FileSizeLiteral, FileVisibility, MimeType, Module, Policies, Resource, TypeRef,
};

fn minimal_app() -> AppManifest {
    AppManifest {
        name: "test".to_owned(),
        title: None,
        version: None,
        lazuli_version: None,
        targets: Vec::new(),
        default_locale: None,
        default_timezone: None,
        auth_failed_redirect: None,
        not_found: None,
        error_pages: Vec::new(),
        uses: Vec::new(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
        headers: None,
        cookie: None,
        proxy: None,
        limits: None,
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        logging: None,
        tracing: None,
        observability: None,
        locale: None,
        encryption_bindings: Vec::new(),
        route_guard: None,
        actor_query: None,
        span_ref: None,
    }
}

fn base_feature(name: &str) -> Feature {
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

fn emit_entry_point(feature: &Feature) -> Option<String> {
    let module = Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app()),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: vec![feature.clone()],
    };
    let index = CrossFeatureIndex::build(&module);
    emit_storage_file(
        "features/documents/documents.lzi",
        feature,
        "lazuli/test",
        &index,
    )
}

#[test]
fn entry_point_emits_representative_resource_contract() {
    let mut feature = base_feature("documents");
    feature.resources.push(Resource {
        name: "Document".to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields: vec![Field {
            name: "attachment".to_owned(),
            type_ref: TypeRef::Capability(CapabilityRef::File(FileCapability {
                max_size: FileSize {
                    bytes: FileSizeLiteral::Mb(10).bytes(),
                    literal: FileSizeLiteral::Mb(10),
                },
                accept: vec![MimeType {
                    family: "application".to_owned(),
                    subtype: "pdf".to_owned(),
                }],
                visibility: Some(FileVisibility::Signed),
                signed_ttl: Some("30m".to_owned()),
                auto_photo_policy: None,
            })),
            required: true,
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
        }],
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
    });

    let out = emit_entry_point(&feature).expect("typed file field must emit storage.gen.go");

    assert!(!out.is_empty());
    assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
    assert!(out.contains("package documentsgen"));
    assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
    assert!(out.contains("var DocumentsAttachmentFile = storage.FileContract{"));
    assert!(out.contains("Resource:   \"Document\","));
    assert!(out.contains("Field:      \"attachment\","));
    assert!(
        out.contains(
            "Accept:     []storage.MimeType{{Family: \"application\", Subtype: \"pdf\"}},"
        )
    );
    assert!(out.contains("SignedTTL:  30 * time.Minute,"));
}
