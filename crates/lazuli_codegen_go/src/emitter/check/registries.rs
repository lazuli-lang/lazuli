//! Closed catalogs that the Go codegen check pass consults to
//! validate the `@`-prefixed references it sees in the IR.
//!
//! Three flavors live here:
//! - Plugins: the `app.lzi` / `registry.lzi` integrations roster.
//!   Authors declare `@lazuli/plugin-X` references via the
//!   `integrations:` block; this module computes the set of declared
//!   plugin names (with hyphen / underscore / case variants so we
//!   accept the same author-intent reference in either form).
//! - Runtime / semantic / capability: known-good tail catalogs for
//!   `@runtime/X`, `@semantic.X`, `@cap.X` references that the Go
//!   emitter can actually lower. Anything outside these sets is a
//!   hard codegen error (CODE_UNRESOLVED / CODE_SEMANTIC / CODE_CAP).

use std::collections::BTreeSet;

use lazuli_ir::{AppIntegration, Module};

pub(super) fn declared_plugin_names(module: &Module) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(app) = &module.app {
        collect_declared_plugins_from_integrations(&app.integrations, &mut names);
    }
    if let Some(registry) = &module.registry {
        collect_declared_plugins_from_integrations(&registry.integrations, &mut names);
    }
    names
}

fn collect_declared_plugins_from_integrations(
    integrations: &[AppIntegration],
    names: &mut BTreeSet<String>,
) {
    for integration in integrations {
        insert_plugin_name_variants(names, &integration.name);
        if let Some(tail) = integration
            .adapter
            .as_deref()
            .and_then(|adapter| adapter.strip_prefix("@lazuli/plugin-"))
        {
            insert_plugin_name_variants(names, tail);
        }
    }
}

pub(super) fn plugin_declared(declared_plugins: &BTreeSet<String>, tail: &str) -> bool {
    plugin_name_variants(tail)
        .into_iter()
        .any(|name| declared_plugins.contains(&name))
}

fn insert_plugin_name_variants(names: &mut BTreeSet<String>, name: &str) {
    for variant in plugin_name_variants(name) {
        names.insert(variant);
    }
}

fn plugin_name_variants(name: &str) -> Vec<String> {
    let trimmed = name.trim_matches('/');
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    [trimmed, last]
        .into_iter()
        .flat_map(|value| {
            [
                value.to_owned(),
                value.replace('-', "_"),
                value.to_ascii_lowercase(),
                value.replace('-', "_").to_ascii_lowercase(),
            ]
        })
        .collect()
}

pub(super) fn known_runtime_ref(name: &str) -> bool {
    matches!(
        name,
        "postgres"
            | "s3"
            | "google_oauth"
            | "mercadopago"
            | "payments"
            | "customer-import"
            | "anthropic"
            | "serper"
            | "google_calendar"
    )
}

pub(super) fn known_semantic_ref(name: &str) -> bool {
    matches!(
        name,
        "Email" | "Phone" | "Url" | "Uuid" | "Money" | "GeoPoint" | "Currency"
    )
}

pub(super) fn known_cap_ref(name: &str) -> bool {
    matches!(name, "Hashed" | "Encrypted" | "Token" | "File")
}
