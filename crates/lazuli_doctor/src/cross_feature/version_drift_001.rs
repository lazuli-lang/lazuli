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
    /// Feature whose declaration references the foreign symbol.
    pub consumer_feature: String,
    /// Feature that declares `symbol` and publishes the contract.
    pub origin_feature: String,
    /// Bare symbol name (no feature prefix) that triggered the finding.
    pub symbol: String,
    /// Version the consumer pinned via `uses <feature> version v<N>`.
    pub consumer_version: u16,
    /// Current `public contract <symbol> as v<M>` version at the origin.
    pub origin_version: u16,
    /// Human-readable site such as `field \`status\` of resource \`Post\``.
    pub consumer_site: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001";

    /// Render the user-facing diagnostic body. The remediation prefix
    /// suggests both migrating the consumer or republishing the origin
    /// for compatibility.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::cross_feature::version_drift_001::Finding;
    ///
    /// let f = Finding {
    ///     consumer_feature: "billing".into(),
    ///     origin_feature: "auth".into(),
    ///     symbol: "User".into(),
    ///     consumer_version: 1,
    ///     origin_version: 2,
    ///     consumer_site: "field `user` of resource `Invoice`".into(),
    /// };
    /// assert!(f.message().contains("v1"));
    /// assert!(f.message().contains("v2"));
    /// ```
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
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::cross_feature::version_drift_001::check;
///
/// let findings = check(&module, Some(&app));
/// for f in findings {
///     eprintln!("v{} pinned vs v{} published — {}",
///         f.consumer_version, f.origin_version, f.symbol);
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
            inspect_type_ref(
                consumer,
                pins,
                inner,
                consumer_site,
                contracts,
                symbols,
                out,
            );
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
    include!("version_drift_001_tests.rs");
}
