//! Pure predicates and lookup helpers used by every contract aggregator
//! (and a handful of out-of-aggregator callers — `report_storage`,
//! `lazurite_manifest`, etc.). No diagnostics emitted; each helper is a
//! small, side-effect-free read on the lifted manifest / registry /
//! operational facts.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::AppManifest;

use crate::doctor::{DoctorAppProfile, DoctorAppRegistry};

pub(crate) fn operational_integrations<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeMap<&'a str, &'a str> {
    let mut integrations = BTreeMap::new();
    for integration in &app.integrations {
        integrations.insert(integration.name.as_str(), integration.kind.as_str());
    }
    if let Some(registry) = registry {
        for integration in &registry.manifest.integrations {
            integrations.insert(integration.name.as_str(), integration.kind.as_str());
        }
    }
    integrations
}

pub(crate) fn integration_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))
}

pub(crate) fn pack_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("packs.")
        .or_else(|| source.strip_prefix("registry.packs."))
}

pub(crate) fn integration_environment_allowed(
    app: &AppManifest,
    registry: Option<&DoctorAppRegistry>,
    name: &str,
    environment: &str,
) -> bool {
    app.integrations
        .iter()
        .chain(
            registry
                .into_iter()
                .flat_map(|registry| registry.manifest.integrations.iter()),
        )
        .find(|integration| integration.name == name)
        .is_some_and(|integration| {
            integration.environments.is_empty()
                || integration
                    .environments
                    .iter()
                    .any(|allowed| allowed == environment)
        })
}

pub(crate) fn app_has_target(app: &AppManifest, target: &str) -> bool {
    app.targets
        .iter()
        .any(|entry| entry.split_whitespace().next() == Some(target))
}

pub(crate) fn profile_url_target_valid(app: &AppManifest, target: &str) -> bool {
    target == "api" && app_has_target(app, "backend") || app_has_target(app, target)
}

pub(crate) fn app_has_url(app: &AppManifest, profiles: &[DoctorAppProfile], target: &str) -> bool {
    app.urls.iter().any(|url| url.target == target)
        || profiles
            .iter()
            .flat_map(|profile| profile.profile.urls.iter())
            .any(|url| url.target == target)
}

pub(crate) fn operational_env_names<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeSet<&'a str> {
    let mut names: BTreeSet<_> = app.env.iter().map(|env| env.name.as_str()).collect();
    if let Some(registry) = registry {
        names.extend(registry.manifest.env.iter().map(|env| env.name.as_str()));
    }
    names
}

/// Collect every `object_storage` capability concrete-name declared by
/// the app manifest or registry. Capability lines parse as
/// `<kind> <name>` where the parser stores kind in `AppCapability.name`
/// and the concrete name in `AppCapability.value`. This helper returns
/// the list of concrete names (e.g. `files`) for every entry whose kind
/// is `object_storage` or `storage`. Used by report-vocab doctor rules
/// (`REPORT-SIGNED-NO-STORAGE-001` / `REPORT-STORAGE-AMBIGUOUS-001`)
/// to resolve implicit `storage` bindings and reject signed reports
/// in packages without any object-storage capability.
pub(crate) fn collect_object_storage_caps(
    app: Option<&AppManifest>,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Some(app) = app {
        for cap in &app.capabilities {
            if cap.name == "object_storage" || cap.name == "storage" {
                names.push(cap.value.clone());
            }
        }
    }
    if let Some(registry) = registry {
        for cap in &registry.manifest.capabilities {
            if cap.name == "object_storage" || cap.name == "storage" {
                names.push(cap.value.clone());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

pub(crate) fn app_has_any_capability(
    app: &AppManifest,
    registry: Option<&DoctorAppRegistry>,
    names: &[&str],
) -> bool {
    app.capabilities
        .iter()
        .any(|capability| names.contains(&capability.name.as_str()))
        || registry.is_some_and(|registry| {
            registry
                .manifest
                .capabilities
                .iter()
                .any(|capability| names.contains(&capability.name.as_str()))
        })
}

pub(crate) fn app_runtime_serves(app: &AppManifest, service: &str) -> bool {
    app.runtime
        .iter()
        .flat_map(|unit| unit.serves.iter())
        .any(|item| runtime_item_matches(item, service))
}

pub(crate) fn app_runtime_runs(app: &AppManifest, service: &str) -> bool {
    app.runtime
        .iter()
        .flat_map(|unit| unit.runs.iter())
        .any(|item| runtime_item_matches(item, service))
}

pub(crate) fn runtime_item_matches(item: &str, service: &str) -> bool {
    item == "*"
        || item == service
        || item
            .split_whitespace()
            .next()
            .is_some_and(|first| first == service)
}
