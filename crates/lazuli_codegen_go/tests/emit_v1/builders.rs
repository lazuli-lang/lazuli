//! Boilerplate IR builders shared by the emit_v1 integration tests.
//!
//! All helpers here are intentionally hand-rolled — the test suite
//! must stay decoupled from the analyzer/grammar so emitter drift is
//! the only thing the assertions can ever flag.

use std::collections::BTreeMap;

use lazuli_codegen_go::{
    GoSourceContext, LazuriteDev, LazuriteGenerateGo, LazuriteManifest, LazuritePlugin,
};
use lazuli_ir::{
    AppManifest, Command, CommandEffect, CommandInput, CommandKind, Defaults, Feature, Module,
    Policies, PolicyRef, SourceMap, SpanRef,
};

pub fn empty_feature(name: &str) -> Feature {
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

pub fn minimal_app_manifest(name: &str) -> AppManifest {
    // `AppManifest` has no `Default` impl (the bucket-observability
    // cycle dropped `Eq` derive so `f64` sample-rate fields can
    // coexist). Build the whole shape explicitly.
    AppManifest {
        name: name.to_owned(),
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

pub fn minimal_module(app_name: &str, feature_name: &str) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: Some(minimal_app_manifest(app_name)),
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        doctor_allows: Vec::new(),
        features: vec![empty_feature(feature_name)],
    }
}

pub fn empty_command(name: &str, span_ref: Option<SpanRef>) -> Command {
    Command {
        name: name.to_owned(),
        public_contract: None,
        kind: CommandKind::Create,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: CommandEffect::None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
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
        triggers: vec![],
        synthesized_from_cap_file: None,
        previous_names: Vec::new(),
        span_ref,
        owner_scope_sql: None,
        derived_from: None,
    }
}

#[allow(dead_code)]
pub fn source_map_context<'a>(
    source_map: &'a SourceMap,
    feature_file_ids: &'a BTreeMap<String, lazuli_ir::FileId>,
) -> GoSourceContext<'a> {
    GoSourceContext {
        source_map,
        feature_file_ids,
    }
}

pub fn command_gen_contents(files: &[lazuli_codegen_go::GeneratedFile], feature: &str) -> String {
    files
        .iter()
        .find(|file| file.path == format!("{feature}/command.gen.go"))
        .expect("expected command.gen.go")
        .contents
        .clone()
}

pub fn lazurite_manifest(
    plugins: Vec<(&str, &str)>,
    generate_go: Option<LazuriteGenerateGo>,
    plugin_paths: Vec<(&str, &str)>,
) -> LazuriteManifest {
    LazuriteManifest {
        project_module: "github.com/acme/test-app".to_owned(),
        plugins: plugins
            .into_iter()
            .map(|(plugin_ref, module)| {
                (
                    plugin_ref.to_owned(),
                    LazuritePlugin {
                        module: Some(module.to_owned()),
                        version: Some("v0.1.0".to_owned()),
                        path: None,
                        go_module: Some(module.to_owned()),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
        generate_go,
        dev: Some(LazuriteDev {
            plugin_paths: plugin_paths
                .into_iter()
                .map(|(plugin_ref, path)| (plugin_ref.to_owned(), path.to_owned()))
                .collect::<BTreeMap<_, _>>(),
        }),
    }
}
