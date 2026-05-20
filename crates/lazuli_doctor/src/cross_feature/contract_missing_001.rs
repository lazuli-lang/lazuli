//! CROSS-FEATURE-CONTRACT-MISSING-001 — cross-feature reference without an
//! origin `public contract`.
//!
//! Fires under `architecture mode microservices` when a typed reference in one
//! feature resolves to a symbol declared in another feature and the origin
//! symbol does not carry `public contract <Symbol> as v<N>`.
//!
//! Severity: `error`.
//! Reference: docs/proposals/cross-feature-contracts.md §7 row 1
//! Invariant: docs/proposals/cross-feature-contracts.md §4

use std::collections::{BTreeMap, BTreeSet};
#[allow(unused_imports)]
use std::path::PathBuf;

use lazuli_ir::{
    AppManifest, CommandInput, Feature, Module, PublicContract, QualifiedName, Query, TypeRef,
};

// ── output ────────────────────────────────────────────────────────────────────

/// One CROSS-FEATURE-CONTRACT-MISSING-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub consumer_feature: String,
    pub origin_feature: String,
    pub symbol: String,
    pub consumer_site: String,
}

impl Finding {
    pub const CODE: &'static str = "CROSS-FEATURE-CONTRACT-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "feature `{}` references `{}.{}` from `{}` but the origin lacks a `public contract` annotation \
             — add `public contract {} as v1` adjacent to the declaration in feature `{}`. \
             Required under `architecture mode microservices`; under `monolith`/`modular_monolith` \
             this would compile but cross-service deploy would silently couple the binaries. \
             See docs/proposals/cross-feature-contracts.md §5.1.",
            self.consumer_feature,
            self.origin_feature,
            self.symbol,
            self.consumer_site,
            self.symbol,
            self.origin_feature,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run CROSS-FEATURE-CONTRACT-MISSING-001 across a module.
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
        walk_feature(feature, &contracts, &symbols, &mut out);
    }

    out
}

// ── internals ─────────────────────────────────────────────────────────────────

fn is_microservices(app: Option<&AppManifest>) -> bool {
    app.and_then(|app| app.architecture.as_ref())
        .and_then(|architecture| architecture.mode.as_deref())
        == Some("microservices")
}

fn build_contract_map(module: &Module) -> BTreeMap<(String, String), bool> {
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
        for event in &feature.events {
            contracts.insert((feature.name.clone(), event.name.clone()), false);
        }
    }

    contracts
}

fn insert_contract(
    contracts: &mut BTreeMap<(String, String), bool>,
    feature: &Feature,
    name: &str,
    contract: Option<&PublicContract>,
) {
    contracts.insert((feature.name.clone(), name.to_owned()), contract.is_some());
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

fn walk_feature(
    feature: &Feature,
    contracts: &BTreeMap<(String, String), bool>,
    symbols: &BTreeMap<String, BTreeSet<String>>,
    out: &mut Vec<Finding>,
) {
    for resource in &feature.resources {
        for field in &resource.fields {
            inspect_type_ref(
                feature,
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
                        &param.type_ref,
                        format!("param `{}` of query.sql `{}`", param.name, query.name),
                        contracts,
                        symbols,
                        out,
                    );
                }
                inspect_type_ref(
                    feature,
                    &query.returns,
                    format!("return type of query.sql `{}`", query.name),
                    contracts,
                    symbols,
                    out,
                );
            }
        }
    }

    for event in &feature.events {
        for payload in &event.payload {
            inspect_type_ref(
                feature,
                &payload.type_ref,
                format!("payload `{}` of event `{}`", payload.name, event.name),
                contracts,
                symbols,
                out,
            );
        }
    }
}

fn inspect_type_ref(
    consumer: &Feature,
    type_ref: &TypeRef,
    consumer_site: String,
    contracts: &BTreeMap<(String, String), bool>,
    symbols: &BTreeMap<String, BTreeSet<String>>,
    out: &mut Vec<Finding>,
) {
    match type_ref {
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => {
            if let Some(origin_feature) = resolve_origin_feature(consumer, qn, symbols) {
                if origin_feature != consumer.name
                    && !contracts
                        .get(&(origin_feature.clone(), qn.name.clone()))
                        .copied()
                        .unwrap_or(false)
                {
                    out.push(Finding {
                        consumer_feature: consumer.name.clone(),
                        origin_feature,
                        symbol: qn.name.clone(),
                        consumer_site,
                    });
                }
            }
        }
        TypeRef::Many(inner) => {
            inspect_type_ref(consumer, inner, consumer_site, contracts, symbols, out);
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
        Command, CommandEffect, CommandKind, Defaults, EnumDecl, Event, EventKind, Field,
        FieldConstraints, Policies, PolicyRef, Resource, TypedSlot,
    };

    fn qn(feature: Option<&str>, name: &str) -> QualifiedName {
        QualifiedName {
            feature: feature.map(str::to_owned),
            name: name.to_owned(),
        }
    }

    fn enum_ref(feature: Option<&str>, name: &str) -> TypeRef {
        TypeRef::EnumRef(qn(feature, name))
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
            span_ref: None,
        }
    }

    fn module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features,
        }
    }

    fn app(mode: &str) -> AppManifest {
        serde_json::from_value(serde_json::json!({
            "name": "TestApp",
            "defaults": {},
            "architecture": { "mode": mode }
        }))
        .expect("minimal AppManifest fixture")
    }

    fn public_contract() -> Option<PublicContract> {
        Some(PublicContract {
            version: 1,
            span_ref: None,
        })
    }

    fn enum_decl(name: &str, public_contract: Option<PublicContract>) -> EnumDecl {
        EnumDecl {
            name: name.to_owned(),
            public_contract,
            variants: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn resource(
        name: &str,
        fields: Vec<Field>,
        public_contract: Option<PublicContract>,
    ) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
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
        }
    }

    fn command(name: &str, input: CommandInput) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input,
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
        }
    }

    fn event(name: &str, payload: Vec<lazuli_ir::EventField>) -> Event {
        Event {
            name: name.to_owned(),
            kind: EventKind::Domain,
            payload,
            payload_none: false,
            level: None,
            outbox: lazuli_ir::OutboxMode::None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn non_microservices_mode_does_not_fire() {
        let mut account = empty_feature("account");
        account.enums.push(enum_decl("Gender", None));
        let mut host = empty_feature("host");
        host.uses.push("account".to_owned());
        host.resources.push(resource(
            "User",
            vec![field("gender", enum_ref(Some("account"), "Gender"))],
            None,
        ));

        let findings = check(&module(vec![account, host]), Some(&app("modular_monolith")));

        assert!(findings.is_empty());
    }

    #[test]
    fn cross_feature_type_ref_without_contract_fires() {
        let mut account = empty_feature("account");
        account.enums.push(enum_decl("Gender", None));
        let mut host = empty_feature("host");
        host.resources.push(resource(
            "User",
            vec![field("gender", enum_ref(Some("account"), "Gender"))],
            None,
        ));

        let findings = check(&module(vec![account, host]), Some(&app("microservices")));

        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "CROSS-FEATURE-CONTRACT-MISSING-001");
        assert_eq!(findings[0].consumer_feature, "host");
        assert_eq!(findings[0].origin_feature, "account");
        assert_eq!(findings[0].symbol, "Gender");
        assert_eq!(
            findings[0].consumer_site,
            "field `gender` of resource `User`"
        );
        assert!(findings[0]
            .message()
            .contains("public contract Gender as v1"));
    }

    #[test]
    fn cross_feature_type_ref_with_contract_does_not_fire() {
        let mut account = empty_feature("account");
        account.enums.push(enum_decl("Gender", public_contract()));
        let mut host = empty_feature("host");
        host.resources.push(resource(
            "User",
            vec![field("gender", enum_ref(Some("account"), "Gender"))],
            None,
        ));

        let findings = check(&module(vec![account, host]), Some(&app("microservices")));

        assert!(findings.is_empty());
    }

    #[test]
    fn intra_feature_type_ref_does_not_fire() {
        let mut host = empty_feature("host");
        host.resources.push(resource("Listing", vec![], None));
        host.resources.push(resource(
            "Booking",
            vec![field("listing", user_defined(None, "Listing"))],
            None,
        ));

        let findings = check(&module(vec![host]), Some(&app("microservices")));

        assert!(findings.is_empty());
    }

    #[test]
    fn multiple_consumers_emit_one_finding_each() {
        let mut account = empty_feature("account");
        account.resources.push(resource("User", vec![], None));
        let mut host = empty_feature("host");
        host.uses.push("account".to_owned());
        host.commands.push(command(
            "book",
            CommandInput::Typed(vec![slot("user", user_defined(None, "User"))]),
        ));
        let mut customer_outreach = empty_feature("customer_outreach");
        customer_outreach.uses.push("account".to_owned());
        customer_outreach.events.push(event(
            "campaign_sent",
            vec![lazuli_ir::EventField {
                name: "user".to_owned(),
                type_ref: user_defined(None, "User"),
                optional: false,
            }],
        ));

        let findings = check(
            &module(vec![account, host, customer_outreach]),
            Some(&app("microservices")),
        );

        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|finding| {
            finding.consumer_feature == "host"
                && finding.consumer_site == "input `user` of command `book`"
        }));
        assert!(findings.iter().any(|finding| {
            finding.consumer_feature == "customer_outreach"
                && finding.consumer_site == "payload `user` of event `campaign_sent`"
        }));
    }

    #[test]
    fn sql_query_return_shape_without_contract_fires() {
        let mut account = empty_feature("account");
        account.resources.push(resource("User", vec![], None));
        let mut host = empty_feature("host");
        host.queries.push(Query::Sql(lazuli_ir::SqlQuery {
            name: "recent_users".to_owned(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            returns: TypeRef::Many(Box::new(user_defined(Some("account"), "User"))),
            sql_path: "queries/recent_users.sql".to_owned(),
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
        }));

        let findings = check(&module(vec![account, host]), Some(&app("microservices")));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].consumer_site,
            "return type of query.sql `recent_users`"
        );
    }
}
