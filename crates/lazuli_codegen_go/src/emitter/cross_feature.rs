//! Cross-feature type resolver — Phase Prep §1.1 mini-cell pré-E3.
//!
//! ## Rationale
//!
//! The analyzer lowers every unknown identifier to
//! `TypeRef::UserDefined(QualifiedName { feature: None, name })`
//! (`crates/lazuli_analyzer/src/lib.rs:575-577`). It does not run a
//! post-pass that walks all features and resolves cross-feature
//! references — `qname.feature` always stays `None` for user-defined
//! types. As a result, the resource emitter emits a bare identifier
//! (e.g. `User`) in features that don't declare it, and `go build`
//! fails with `undefined: User`.
//!
//! Refactoring the analyzer to resolve cross-feature references is the
//! canonical fix, but the blast radius (doctor, inspect, LSP, every
//! other downstream pass) is too large for a single codegen mini-cell.
//! This module performs the resolution **codegen-only**: before
//! emitting any feature, walk every feature once and build a
//! `type_name -> owning_feature` index. Per-feature emitters consult
//! the index when they hit `TypeRef::UserDefined`/`TypeRef::EnumRef`
//! and emit `<feature>.<Name>` plus a cross-feature import when the
//! owner is a sibling. When the analyzer eventually grows a resolve
//! pass, this module simplifies to a thin wrapper that reads
//! `qname.feature` directly.
//!
//! ## Ambiguity policy
//!
//! If a type name appears in two or more features, the index records
//! the conflict in `ambiguous` and returns `None` from `owner()`. The
//! caller falls back to a bare reference plus a warning comment so the
//! emitted file still compiles within whichever feature it lives in.
//! This shouldn't happen in the canonical fixture — type names are
//! globally unique by convention — but the fallback exists so future
//! pilots don't crash codegen the moment they violate it.
//!
//! ## Determinism
//!
//! `BTreeMap` storage keeps the resolution order stable across runs.
//! The index is built once per `generate_v1` invocation and shared by
//! reference through `types::TypeCtx`, so per-feature emitters can't
//! observe each other's mutation order.

use std::collections::BTreeMap;

use lazuli_ir::Module;

/// Kind of the declaring slot. Used by `types::go_type_for` to decide
/// whether a `TypeRef::UserDefined` field renders as a struct ref
/// (records hold subfields, scan by name) or as `lazuli.ID` (resources
/// are FKs to a BIGINT column — scanning into the full struct is a
/// type mismatch).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeclKind {
    Resource,
    Record,
    Enum,
}

/// Maps every type-bearing declaration name to the feature that owns
/// it. Built once per `generate_v1` run and threaded through the
/// per-feature walkers via `types::TypeCtx`.
pub struct CrossFeatureIndex<'a> {
    /// `type_name -> (feature_name, kind)` for non-ambiguous declarations.
    /// Names that collide across features are removed from this map
    /// and recorded in `ambiguous` instead.
    map: BTreeMap<&'a str, (&'a str, DeclKind)>,
    /// `type_name -> [feature_name; ...]` for names declared in two
    /// or more features. `owner()` returns `None` for these.
    ambiguous: BTreeMap<&'a str, Vec<&'a str>>,
}

impl<'a> CrossFeatureIndex<'a> {
    /// Walk the module and index every `Resource.name`, `Record.name`,
    /// and `EnumDecl.name` against its owning feature. Names declared
    /// in two or more features are tracked separately so the caller can
    /// emit a fallback warning instead of guessing.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_codegen_go::emitter::cross_feature::CrossFeatureIndex;
    /// let index = CrossFeatureIndex::build(&module);
    /// assert!(index.owner("UnknownThing").is_none());
    /// ```
    pub fn build(module: &'a Module) -> Self {
        let mut map: BTreeMap<&'a str, (&'a str, DeclKind)> = BTreeMap::new();
        let mut ambiguous: BTreeMap<&'a str, Vec<&'a str>> = BTreeMap::new();

        for feature in &module.features {
            let feature_name = feature.name.as_str();
            // Three catalog sources today. Each contributes its kind
            // so downstream emitters can branch (resource FKs become
            // `lazuli.ID`; records and enums keep their struct/enum
            // identity).
            let names = feature
                .resources
                .iter()
                .map(|r| (r.name.as_str(), DeclKind::Resource))
                .chain(
                    feature
                        .records
                        .iter()
                        .map(|r| (r.name.as_str(), DeclKind::Record)),
                )
                .chain(
                    feature
                        .enums
                        .iter()
                        .map(|e| (e.name.as_str(), DeclKind::Enum)),
                );

            for (name, kind) in names {
                Self::register(&mut map, &mut ambiguous, name, feature_name, kind);
            }
        }

        Self { map, ambiguous }
    }

    fn register(
        map: &mut BTreeMap<&'a str, (&'a str, DeclKind)>,
        ambiguous: &mut BTreeMap<&'a str, Vec<&'a str>>,
        name: &'a str,
        feature: &'a str,
        kind: DeclKind,
    ) {
        if let Some(existing) = ambiguous.get_mut(name) {
            if !existing.contains(&feature) {
                existing.push(feature);
            }
            return;
        }
        match map.remove(name) {
            Some((prev_feature, prev_kind)) if prev_feature == feature => {
                // Same feature redeclares the name — silently coalesce.
                // (Doctor surfaces duplicate-resource diagnostics
                // separately; codegen shouldn't double-fault on them.)
                map.insert(name, (prev_feature, prev_kind));
            }
            Some((prev_feature, _)) => {
                // Collision across features. Promote to ambiguous.
                ambiguous.insert(name, vec![prev_feature, feature]);
            }
            None => {
                map.insert(name, (feature, kind));
            }
        }
    }

    /// Return the unique owning feature for `type_name`, or `None`
    /// when the name is unknown or ambiguous. Callers distinguish
    /// "unknown" from "ambiguous" via [`Self::is_ambiguous`].
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// if let Some(feature) = index.owner("Billing") {
    ///     /* emit import path */
    /// }
    /// ```
    pub fn owner(&self, type_name: &str) -> Option<&'a str> {
        self.map.get(type_name).map(|(feature, _)| *feature)
    }

    /// Return the declaring slot kind for `type_name`, or `None` when
    /// the name is unknown or ambiguous. `types::go_type_for` consults
    /// this so a `TypeRef::UserDefined(<Resource>)` field renders as
    /// `lazuli.ID` (the actual DB column shape) instead of the resource
    /// struct ref (which would fail `pgx.RowToStructByName` scan into
    /// a BIGINT FK column).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_codegen_go::emitter::cross_feature::DeclKind;
    /// match index.kind("Billing") {
    ///     Some(DeclKind::Resource) => { /* FK column */ }
    ///     _ => { /* struct/enum */ }
    /// }
    /// ```
    pub fn kind(&self, type_name: &str) -> Option<DeclKind> {
        self.map.get(type_name).map(|(_, kind)| *kind)
    }

    /// `true` when `type_name` is declared in two or more features.
    /// The emitter pairs this with a warning comment so a reviewer
    /// can spot the name collision.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// if index.is_ambiguous("Foo") {
    ///     // emit a `// WARN: ambiguous cross-feature ref` comment
    /// }
    /// ```
    pub fn is_ambiguous(&self, type_name: &str) -> bool {
        self.ambiguous.contains_key(type_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, Defaults, EnumDecl, EnumVariant, Feature, Module, Policies, Record, Resource,
    };

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

    fn make_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields: Vec::new(),
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

    fn make_record(name: &str) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields: Vec::new(),
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn make_enum(name: &str) -> EnumDecl {
        EnumDecl {
            name: name.to_owned(),
            public_contract: None,
            variants: vec![EnumVariant {
                name: "a".to_owned(),
                storage_value: None,
                label_key: None,
                hint_key: None,
                icon_key: None,
                previous_names: Vec::new(),
            }],
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
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
            }),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            doctor_allows: Vec::new(),
            features,
        }
    }

    #[test]
    fn single_feature_resource_indexes_to_owning_feature() {
        let mut customer = empty_feature("customer");
        customer.resources.push(make_resource("Customer"));
        let module = module_with_features(vec![customer]);
        let index = CrossFeatureIndex::build(&module);
        assert_eq!(index.owner("Customer"), Some("customer"));
        assert!(!index.is_ambiguous("Customer"));
    }

    #[test]
    fn cross_feature_resource_resolves_against_owning_feature() {
        let mut customer = empty_feature("customer");
        customer.resources.push(make_resource("Customer"));
        let mut org = empty_feature("org");
        org.resources.push(make_resource("User"));
        let module = module_with_features(vec![customer, org]);
        let index = CrossFeatureIndex::build(&module);
        assert_eq!(index.owner("Customer"), Some("customer"));
        assert_eq!(index.owner("User"), Some("org"));
    }

    #[test]
    fn name_declared_in_two_features_is_ambiguous() {
        let mut a = empty_feature("a");
        a.enums.push(make_enum("Status"));
        let mut b = empty_feature("b");
        b.enums.push(make_enum("Status"));
        let module = module_with_features(vec![a, b]);
        let index = CrossFeatureIndex::build(&module);
        assert_eq!(index.owner("Status"), None);
        assert!(index.is_ambiguous("Status"));
    }

    #[test]
    fn unknown_name_returns_none_without_ambiguity() {
        let module = module_with_features(vec![empty_feature("customer")]);
        let index = CrossFeatureIndex::build(&module);
        assert_eq!(index.owner("Ghost"), None);
        assert!(!index.is_ambiguous("Ghost"));
    }

    #[test]
    fn records_and_enums_count_alongside_resources() {
        // The three catalog sources contribute equally to the index.
        let mut feature = empty_feature("customer");
        feature.resources.push(make_resource("Customer"));
        feature.records.push(make_record("CustomerLtv"));
        feature.enums.push(make_enum("CustomerStatus"));
        let module = module_with_features(vec![feature]);
        let index = CrossFeatureIndex::build(&module);
        assert_eq!(index.owner("Customer"), Some("customer"));
        assert_eq!(index.owner("CustomerLtv"), Some("customer"));
        assert_eq!(index.owner("CustomerStatus"), Some("customer"));
    }

    #[test]
    fn ambiguity_across_resource_record_enum_categories() {
        // A name declared as a resource in feature A and as a record
        // in feature B is still ambiguous — the kind doesn't matter.
        let mut a = empty_feature("a");
        a.resources.push(make_resource("Thing"));
        let mut b = empty_feature("b");
        b.records.push(make_record("Thing"));
        let module = module_with_features(vec![a, b]);
        let index = CrossFeatureIndex::build(&module);
        assert!(index.is_ambiguous("Thing"));
    }

    #[test]
    fn deterministic_across_runs() {
        // Build the index twice with the same features in the same
        // order; the resolved owner must not vary. `BTreeMap` keeps
        // iteration order stable across hash-DOS reseeds.
        let mut customer = empty_feature("customer");
        customer.resources.push(make_resource("Customer"));
        let mut org = empty_feature("org");
        org.resources.push(make_resource("User"));
        let module = module_with_features(vec![customer, org]);
        let a = CrossFeatureIndex::build(&module);
        let b = CrossFeatureIndex::build(&module);
        assert_eq!(a.owner("Customer"), b.owner("Customer"));
        assert_eq!(a.owner("User"), b.owner("User"));
    }
}
