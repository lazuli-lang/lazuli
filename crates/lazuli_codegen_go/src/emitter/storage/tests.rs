//! Tests for storage emission. Split out of `mod.rs` to keep
//! production under the 500 LOC budget.

use super::*;
use lazuli_ir::{
    Api, AppManifest, CapabilityRef, Defaults, Feature, Field, FileCapability, FileSize,
    FileSizeLiteral, FileVisibility, HttpMethod, MimeType, Module, PathRef, Policies, PolicyRef,
    Resource, TypeRef,
};

fn emit(feature: &Feature) -> Option<String> {
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
    emit_storage_file("examples/x.lzi", feature, "lazuli/test", &index)
}

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

fn resource(name: &str, fields: Vec<Field>) -> Resource {
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
        restrict_on_delete: Vec::new(),
        append_only: false,
    }
}

fn file_field(name: &str, capability: FileCapability) -> Field {
    Field {
        name: name.to_owned(),
        type_ref: TypeRef::Capability(CapabilityRef::File(capability)),
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
    }
}

fn make_file_capability(
    literal: FileSizeLiteral,
    accept: Vec<(&str, &str)>,
    visibility: Option<FileVisibility>,
    signed_ttl: Option<&str>,
) -> FileCapability {
    let max_size = FileSize {
        bytes: literal.bytes(),
        literal,
    };
    FileCapability {
        max_size,
        accept: accept
            .into_iter()
            .map(|(family, subtype)| MimeType {
                family: family.to_owned(),
                subtype: subtype.to_owned(),
            })
            .collect(),
        visibility,
        signed_ttl: signed_ttl.map(str::to_owned),
        auto_photo_policy: None,
    }
}

fn mb(n: u32) -> FileSizeLiteral {
    FileSizeLiteral::Mb(n)
}

fn api(name: &str, output: TypeRef) -> Api {
    Api {
        name: name.to_owned(),
        method: HttpMethod::Get,
        path: format!("/api/{name}"),
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        rate_limit: None,
        output,
        handler: PathRef::authored("./api/handler.go"),
        locale_negotiate: None,
        deprecated: None,
        span_ref: None,
    }
}

#[test]
fn empty_feature_returns_none() {
    let feature = base_feature("customer");
    assert!(emit(&feature).is_none());
}

#[test]
fn resource_field_emits_private_contract() {
    let mut feature = base_feature("customer_import");
    feature.resources.push(resource(
        "CustomerImportBatch",
        vec![file_field(
            "file",
            make_file_capability(
                mb(25),
                vec![("text", "csv")],
                Some(FileVisibility::Private),
                None,
            ),
        )],
    ));

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
    assert!(out.contains("package customer_importgen"));
    assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
    assert!(!out.contains("\"time\""));
    assert!(out.contains("var CustomerImportFileFile = storage.FileContract{"));
    assert!(out.contains("Resource:   \"CustomerImportBatch\","));
    assert!(out.contains("Field:      \"file\","));
    assert!(out.contains("MaxSize:    25 * 1024 * 1024,"));
    assert!(out.contains("Accept:     []storage.MimeType{{Family: \"text\", Subtype: \"csv\"}},"));
    assert!(out.contains("Visibility: storage.VisibilityPrivate,"));
}

#[test]
fn api_output_emits_signed_contract_and_time_import() {
    let mut feature = base_feature("customer");
    let cap = make_file_capability(
        mb(100),
        vec![("text", "csv")],
        Some(FileVisibility::Signed),
        Some("1h"),
    );
    feature.apis.push(api(
        "customer_export",
        TypeRef::Capability(CapabilityRef::File(cap)),
    ));

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("import (\n\t\"time\"\n\n\t\"lazuli.dev/runtime/lazuli/storage\"\n)"));
    assert!(out.contains("var CustomerCustomerExportFile = storage.FileContract{"));
    assert!(out.contains("API:        \"customer_export\","));
    assert!(out.contains("Visibility: storage.VisibilitySigned,"));
    assert!(out.contains("SignedTTL:  1 * time.Hour,"));
}

#[test]
fn public_wildcard_accept_renders_mime_literals() {
    let mut feature = base_feature("customer");
    feature.resources.push(resource(
        "Customer",
        vec![file_field(
            "profile_photo",
            make_file_capability(
                mb(5),
                vec![("image", "*")],
                Some(FileVisibility::Public),
                None,
            ),
        )],
    ));

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("var CustomerProfilePhotoFile = storage.FileContract{"));
    assert!(out.contains("Resource:   \"Customer\","));
    assert!(out.contains("Field:      \"profile_photo\","));
    assert!(out.contains("Accept:     []storage.MimeType{{Family: \"image\", Subtype: \"*\"}},"));
    assert!(out.contains("Visibility: storage.VisibilityPublic,"));
}

#[test]
fn deterministic_site_order_across_runs() {
    let mut feature = base_feature("docs");
    feature.resources.push(resource(
        "Zebra",
        vec![file_field(
            "z_file",
            make_file_capability(
                mb(1),
                vec![("text", "csv")],
                Some(FileVisibility::Private),
                None,
            ),
        )],
    ));
    feature.resources.push(resource(
        "Alpha",
        vec![file_field(
            "a_file",
            make_file_capability(
                mb(1),
                vec![("text", "csv")],
                Some(FileVisibility::Private),
                None,
            ),
        )],
    ));

    let a = emit(&feature).expect("must emit");
    let b = emit(&feature).expect("must emit");
    assert_eq!(a, b);
    let alpha_pos = a.find("Storage: Alpha.a_file").expect("alpha banner");
    let zebra_pos = a.find("Storage: Zebra.z_file").expect("zebra banner");
    assert!(alpha_pos < zebra_pos);
}
