//! CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001 — consumer pinned to a different
//! contract version than the origin currently publishes.
//!
//! Fires under `architecture mode microservices` when a consumer feature
//! references a cross-feature symbol AND has a consumer-side version pin
//! (`uses <feature> version v<N>`) that does not match the origin's current
//! `public contract <Symbol> as v<M>` version.
//!
//! Severity: `error`.
//! Reference: docs/proposals/cross-feature-contracts.md §5.4
//! Invariant: docs/proposals/cross-feature-contracts.md §4

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{
    AppManifest, CommandInput, Feature, Module, PublicContract, QualifiedName, Query, TypeRef,
};

// ── output ────────────────────────────────────────────────────────────────────

/// One CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub consumer_feature: String,
    pub origin_feature: String,
    pub symbol: String,
    pub consumer_version: u16,
    pub origin_version: u16,
    pub consumer_site: String,
}

impl Finding {
    pub const CODE: &'static str = "CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001";

    pub fn message(&self) -> String {
        format!(
            "feature `{}` pins `{}` at v{} but references `{}.{}` whose origin publishes v{} \
             (consumer site: {}). Either migrate the consumer to v{} (and update affected call \
             sites if the bump is breaking) or republish the origin at v{} for compatibility. \
             See docs/proposals/cross-feature-contracts.md §5.4.",
            self.consumer_feature,
            self.origin_feature,
            self.consumer_version,
            self.origin_feature,
            self.symbol,
            self.origin_version,
            self.consumer_site,
            self.origin_version,
            self.consumer_version,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001 across a module.
///
/// Gated on `architecture mode microservices`. Returns `Vec::new()` for
/// any other architecture mode (or when `app.architecture` is None).
pub fn check(module: &Module, app: Option<&AppManifest>) -> Vec<Finding> {
    if !is_microservices(app) {
        return Vec::new();
    }

    let contracts = build_contract_map(module);
    let symbols = build_symbol_map(module);
    let mut out = Vec::new();

    for feature in &module.features {
        let pins = build_pin_map(feature);
        if pins.is_empty() {
            continue;
        }
        walk_feature(feature, &pins, &contracts, &symbols, &mut out);
    }

    out
}

// ── internals ─────────────────────────────────────────────────────────────────

fn is_microservices(app: Option<&AppManifest>) -> bool {
    app.and_then(|app| app.architecture.as_ref())
        .and_then(|architecture| architecture.mode.as_deref())
        == Some("microservices")
}

/// Build map of `(origin_feature, symbol_name) → Some(version)` for every
/// declaration that carries `public contract <Symbol> as v<N>`. Symbols
/// without a contract are absent from the map (CONTRACT-MISSING-001
/// handles those).
fn build_contract_map(module: &Module) -> BTreeMap<(String, String), u16> {
    let mut contracts = BTreeMap::new();

    for feature in &module.features {
        for r#enum in &feature.enums {
            insert_contract(
                &mut contracts,
                feature,
                &r#enum.name,
                r#enum.public_contract.as_ref(),
            );
        }
        for resource in &feature.resources {
            insert_contract(
                &mut contracts,
                feature,
                &resource.name,
                resource.public_contract.as_ref(),
            );
        }
        for record in &feature.records {
            insert_contract(
                &mut contracts,
                feature,
                &record.name,
                record.public_contract.as_ref(),
            );
        }
        for command in &feature.commands {
            insert_contract(
                &mut contracts,
                feature,
                &command.name,
                command.public_contract.as_ref(),
            );
        }
        for query in &feature.queries {
            match query {
                Query::List(query) => insert_contract(
                    &mut contracts,
                    feature,
                    &query.name,
                    query.public_contract.as_ref(),
                ),
                Query::Lookup(query) => insert_contract(
                    &mut contracts,
                    feature,
                    &query.name,
                    query.public_contract.as_ref(),
                ),
                Query::Sql(query) => insert_contract(
                    &mut contracts,
                    feature,
                    &query.name,
                    query.public_contract.as_ref(),
                ),
            }
        }
    }

    contracts
}

fn insert_contract(
    contracts: &mut BTreeMap<(String, String), u16>,
    feature: &Feature,
    name: &str,
    contract: Option<&PublicContract>,
) {
    if let Some(contract) = contract {
        contracts.insert((feature.name.clone(), name.to_owned()), contract.version);
    }
}

fn build_symbol_map(module: &Module) -> BTreeMap<String, BTreeSet<String>> {
    let mut symbols: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for feature in &module.features {
        for name in feature
            .enums
            .iter()
            .map(|decl| decl.name.as_str())
            .chain(feature.resources.iter().map(|decl| decl.name.as_str()))
            .chain(feature.records.iter().map(|decl| decl.name.as_str()))
            .chain(feature.commands.iter().map(|decl| decl.name.as_str()))
            .chain(feature.events.iter().map(|decl| decl.name.as_str()))
        {
            symbols
                .entry(name.to_owned())
                .or_default()
                .insert(feature.name.clone());
        }

        for query in &feature.queries {
            symbols
                .entry(query.name().to_owned())
                .or_default()
                .insert(feature.name.clone());
        }
    }

    symbols
}

/// Build `origin_feature → pinned_version` for one consumer feature.
/// Reads `feature.uses` zipped with `feature.uses_versions`. Entries where
/// the version is `None` (unpinned) are excluded.
fn build_pin_map(feature: &Feature) -> BTreeMap<String, u16> {
    let mut pins = BTreeMap::new();
    for (i, used) in feature.uses.iter().enumerate() {
        if let Some(Some(version)) = feature.uses_versions.get(i) {
            pins.insert(used.clone(), *version);
        }
    }
    pins
}

fn walk_feature(
    feature: &Feature,
    pins: &BTreeMap<String, u16>,
    contracts: &BTreeMap<(String, String), u16>,
    symbols: &BTreeMap<String, BTreeSet<String>>,
    out: &mut Vec<Finding>,
) {
    for resource in &feature.resources {
        for field in &resource.fields {
            inspect_type_ref(
                feature,
                pins,
                &field.type_ref,
                format!("field `{}` of resource `{}`", field.name, resource.name),
                contracts,
                symbols,
                out,
            );
        }
    }

    for record in &feature.records {
        for field in &record.fields {
            inspect_type_ref(
                feature,
                pins,
                &field.type_ref,
                format!("field `{}` of record `{}`", field.name, record.name),
                contracts,
                symbols,
                out,
            );
        }
    }

    for command in &feature.commands {
        if let CommandInput::Typed(slots) = &command.input {
            for slot in slots {
                inspect_type_ref(
                    feature,
                    pins,
                    &slot.type_ref,
                    format!("input `{}` of command `{}`", slot.name, command.name),
                    contracts,
                    symbols,
                    out,
                );
            }
        }
    }

    for query in &feature.queries {
        match query {
            Query::List(query) => {
                for param in &query.params {
                    inspect_type_ref(
                        feature,
                        pins,
                        &param.type_ref,
                        format!("param `{}` of query.list `{}`", param.name, query.name),
                        contracts,
                        symbols,
                        out,
                    );
                }
            }
            Query::Lookup(query) => {
                for param in &query.params {
                    inspect_type_ref(
                        feature,
                        pins,
                        &param.type_ref,
                        format!("param `{}` of query.lookup `{}`", param.name, query.name),
                        contracts,
                        symbols,
                        out,
                    );
                }
            }
            Query::Sql(query) => {
                for param in &query.params {
                    inspect_type_ref(
                        feature,
                        pins,
                        &param.type_ref,
                        format!("param `{}` of query.sql `{}`", param.name, query.name),
                        contracts,
                        symbols,
                        out,
                    );
                }
                inspect_type_ref(
                    feature,
                    pins,
                    &query.returns,
                    format!("return type of query.sql `{}`", query.name),
                    contracts,
                    symbols,
                    out,
                );
            }
        }
    }
}

fn inspect_type_ref(
    consumer: &Feature,
    pins: &BTreeMap<String, u16>,
    type_ref: &TypeRef,
    consumer_site: String,
    contracts: &BTreeMap<(String, String), u16>,
    symbols: &BTreeMap<String, BTreeSet<String>>,
    out: &mut Vec<Finding>,
) {
    match type_ref {
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => {
            let Some(origin_feature) = resolve_origin_feature(consumer, qn, symbols) else {
                return;
            };
            if origin_feature == consumer.name {
                return;
            }
            let Some(consumer_version) = pins.get(&origin_feature).copied() else {
                return;
            };
            let Some(origin_version) = contracts
                .get(&(origin_feature.clone(), qn.name.clone()))
                .copied()
            else {
                return;
            };
            if consumer_version != origin_version {
                out.push(Finding {
                    consumer_feature: consumer.name.clone(),
                    origin_feature,
                    symbol: qn.name.clone(),
                    consumer_version,
                    origin_version,
                    consumer_site,
                });
            }
        }
        TypeRef::Many(inner) => {
            inspect_type_ref(consumer, pins, inner, consumer_site, contracts, symbols, out);
        }
        TypeRef::Builtin(_) | TypeRef::Unresolved(_) | TypeRef::Capability(_) => {}
    }
}

fn resolve_origin_feature(
    consumer: &Feature,
    qn: &QualifiedName,
    symbols: &BTreeMap<String, BTreeSet<String>>,
) -> Option<String> {
    if let Some(feature) = &qn.feature {
        return Some(feature.clone());
    }

    let candidates = symbols.get(&qn.name)?;

    if candidates.contains(&consumer.name) {
        return Some(consumer.name.clone());
    }

    let imported_matches: Vec<&String> = consumer
        .uses
        .iter()
        .filter(|feature| candidates.contains(*feature))
        .collect();
    if imported_matches.len() == 1 {
        return Some(imported_matches[0].clone());
    }

    if candidates.len() == 1 {
        return candidates.iter().next().cloned();
    }

    None
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppArchitecture, Command, CommandEffect, CommandKind, Defaults, Field, FieldConstraints,
        Module, Policies, PolicyRef, PublicContract as PC, Resource, TypedSlot,
    };

    fn qn(feature: Option<&str>, name: &str) -> QualifiedName {
        QualifiedName {
            feature: feature.map(str::to_owned),
            name: name.to_owned(),
        }
    }

    fn user_defined(feature: Option<&str>, name: &str) -> TypeRef {
        TypeRef::UserDefined(qn(feature, name))
    }

    fn field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn slot(name: &str, type_ref: TypeRef) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref,
            required: true,
            constraints: FieldConstraints::default(),
        }
    }

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
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

    fn resource_with_contract(name: &str, public_contract: Option<PC>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![field(
                "id",
                TypeRef::Builtin(lazuli_ir::BuiltinType::Id),
            )],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    fn account_with_version(symbol: &str, version: u16) -> Feature {
        let mut feature = empty_feature("account");
        feature
            .resources
            .push(resource_with_contract(symbol, Some(PC { version, span_ref: None })));
        feature
    }

    fn consumer_pinned(symbol: &str, pin: Option<u16>) -> Feature {
        let mut feature = empty_feature("billing");
        feature.uses = vec!["account".to_owned()];
        feature.uses_versions = vec![pin];
        feature.uses_spans = vec![lazuli_ir::SpanRef { start: 0, end: 0 }];
        feature.commands.push(Command {
            name: "ChargeAccount".to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Typed(vec![slot(
                "account",
                user_defined(Some("account"), symbol),
            )]),
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
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
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        });
        feature
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
            features,
        }
    }

    fn microservices_app() -> AppManifest {
        AppManifest {
            name: "TestApp".into(),
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
            architecture: Some(AppArchitecture {
                mode: Some("microservices".into()),
                service_ready: None,
                enforce_service_boundaries: None,
            }),
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
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
            cookie: None,
            proxy: None,
            limits: None,
            headers: None,
            route_guard: None,
            actor_query: None,
            span_ref: None,
        }
    }

    #[test]
    fn pinned_at_drifted_version_fires() {
        let module = module_with(vec![
            account_with_version("Customer", 2),
            consumer_pinned("Customer", Some(1)),
        ]);
        let app = microservices_app();

        let findings = check(&module, Some(&app));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.consumer_feature, "billing");
        assert_eq!(f.origin_feature, "account");
        assert_eq!(f.symbol, "Customer");
        assert_eq!(f.consumer_version, 1);
        assert_eq!(f.origin_version, 2);
        assert!(f.message().contains("v1"));
        assert!(f.message().contains("v2"));
    }

    #[test]
    fn pinned_at_matching_version_does_not_fire() {
        let module = module_with(vec![
            account_with_version("Customer", 1),
            consumer_pinned("Customer", Some(1)),
        ]);
        let app = microservices_app();

        assert!(check(&module, Some(&app)).is_empty());
    }

    #[test]
    fn unpinned_consumer_does_not_fire() {
        // No pin → consumer floats with origin; drift is impossible.
        let module = module_with(vec![
            account_with_version("Customer", 2),
            consumer_pinned("Customer", None),
        ]);
        let app = microservices_app();

        assert!(check(&module, Some(&app)).is_empty());
    }

    #[test]
    fn origin_without_contract_does_not_fire() {
        // Origin lacks `public contract` → CONTRACT-MISSING-001 handles it,
        // not this rule. Drift detection only runs against contracted
        // origins.
        let mut account = empty_feature("account");
        account
            .resources
            .push(resource_with_contract("Customer", None));
        let module = module_with(vec![account, consumer_pinned("Customer", Some(1))]);
        let app = microservices_app();

        assert!(check(&module, Some(&app)).is_empty());
    }

    #[test]
    fn non_microservices_mode_does_not_fire() {
        let module = module_with(vec![
            account_with_version("Customer", 2),
            consumer_pinned("Customer", Some(1)),
        ]);
        let mut app = microservices_app();
        app.architecture.as_mut().unwrap().mode = Some("modular_monolith".into());

        assert!(check(&module, Some(&app)).is_empty());
        assert_eq!(Finding::CODE, "CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001");
    }
}
