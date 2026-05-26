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
    /// Feature whose declaration references the foreign symbol.
    pub consumer_feature: String,
    /// Feature that declares `symbol` (and is missing the `public contract`).
    pub origin_feature: String,
    /// Bare symbol name (no feature prefix) that triggered the finding.
    pub symbol: String,
    /// Human-readable site such as `field \`status\` of resource \`Post\``.
    pub consumer_site: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "CROSS-FEATURE-CONTRACT-MISSING-001";

    /// Render the user-facing diagnostic body. The remediation prefix
    /// (`add \`public contract X as v1\``) is embedded in the message so
    /// CLI and LSP show identical phrasing.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::cross_feature::contract_missing_001::Finding;
    ///
    /// let f = Finding {
    ///     consumer_feature: "billing".into(),
    ///     origin_feature: "auth".into(),
    ///     symbol: "User".into(),
    ///     consumer_site: "field `user` of resource `Invoice`".into(),
    /// };
    /// assert!(f.message().contains("billing"));
    /// assert!(f.message().contains("User"));
    /// ```
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
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::cross_feature::contract_missing_001::check;
///
/// // `module` and `app` come from the IR pipeline; the call is gated
/// // internally on `app.architecture.mode == "microservices"`.
/// let findings = check(&module, Some(&app));
/// for f in findings {
///     eprintln!("{}: {}", f.consumer_feature, f.message());
/// }
/// ```
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
    include!("contract_missing_001_tests.rs");
}
