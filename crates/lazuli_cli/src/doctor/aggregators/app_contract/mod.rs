//! App / profile / pack / service contract aggregator — emits the
//! family of `app.lzi` + `profiles.lzi` + `registry.lzi` cross-contract
//! diagnostics that ride on top of the lifted operational facts:
//!
//!   * app_binding_contract_diagnostics (APP-BIND-001..005)
//!   * profile_contract_diagnostics (PROFILE-001 / PROFILE-URL-001 /
//!     PROFILE-INT-001..002 / PROFILE-BIND-001..004)
//!   * app_pack_contract_diagnostics (APP-PACK-001..003)
//!   * adapter_provenance_diagnostics (APP-ADAPTER-001 / REG-ADAPTER-001
//!     / PROFILE-ADAPTER-001)
//!   * app_service_contract_diagnostics (APP-SVC-001..004)
//!
//! Plus the predicate/helper layer that the rest of the doctor consumes
//! when it needs to ask "does this app declare X?":
//! `operational_integrations`, `enabled_pack_provided_features`,
//! `enabled_pack_integration_requirements`, `integration_source_name`,
//! `pack_source_name`, `integration_environment_allowed`,
//! `app_has_target`, `profile_url_target_valid`, `app_has_url`,
//! `operational_env_names`, `collect_object_storage_caps`,
//! `app_has_any_capability`, `app_runtime_serves`, `app_runtime_runs`,
//! `runtime_item_matches`, `adapter_source_diagnostic`.
//!
//! These were `pub(super)` in `doctor/mod.rs` (Wave R4-C scaffolding);
//! the R6-2 extract promotes the cross-aggregator helpers to
//! `pub(crate)` so siblings (`app_manifest`, `dispatch`, the report
//! aggregators) can keep their existing call paths.
//!
//! Extracted from `doctor/mod.rs` in rails-style R6-2.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lazuli_ir::AppManifest;

use crate::doctor::{
    DoctorAppManifest, DoctorAppProfile, DoctorAppRegistry, DoctorDiagnostic, DoctorSeverity,
    OperationalFacts,
};

mod predicates;

pub(crate) use predicates::{
    app_has_any_capability, app_has_target, app_has_url, app_runtime_runs, app_runtime_serves,
    collect_object_storage_caps, integration_environment_allowed, integration_source_name,
    operational_env_names, operational_integrations, pack_source_name, profile_url_target_valid,
    runtime_item_matches,
};

pub(crate) fn app_binding_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut requirement_index = BTreeMap::new();

    for requirement in &operational.integration_requirements {
        requirement_index.insert(
            (requirement.feature.as_str(), requirement.slot.as_str()),
            requirement.contract.as_str(),
        );

        let matching_binding = app.manifest.bindings.iter().find(|binding| {
            binding.target_feature == requirement.feature && binding.target_slot == requirement.slot
        });

        if matching_binding.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: requirement.path.clone(),
                line: requirement.line,
                column: requirement.column,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-001".to_owned(),
                message: format!(
                    "feature `{}` requires integration slot `{}`: `{}`, but app manifest does not bind `{}.{}`.",
                    requirement.feature,
                    requirement.slot,
                    requirement.contract,
                    requirement.feature,
                    requirement.slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for (feature, slot, contract) in enabled_pack_integration_requirements(&app.manifest, registry)
    {
        requirement_index.insert((feature, slot), contract);

        let matching_binding = app
            .manifest
            .bindings
            .iter()
            .find(|binding| binding.target_feature == feature && binding.target_slot == slot);

        if matching_binding.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-001".to_owned(),
                message: format!(
                    "enabled pack `{feature}` requires integration slot `{slot}`: `{contract}`, but app manifest does not bind `{feature}.{slot}`.",
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    let integrations = operational_integrations(&app.manifest, registry);

    for binding in &app.manifest.bindings {
        let target = (
            binding.target_feature.as_str(),
            binding.target_slot.as_str(),
        );
        let Some(expected_contract) = requirement_index.get(&target).copied() else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-BIND-005".to_owned(),
                message: format!(
                    "app binding `{}.{}` has no matching feature requirement.",
                    binding.target_feature, binding.target_slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        let Some(integration_name) = integration_source_name(&binding.source) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-002".to_owned(),
                message: format!(
                    "app binding `{}.{}` points to `{}`, but bindings must use `integrations.<name>` or `registry.integrations.<name>`.",
                    binding.target_feature, binding.target_slot, binding.source
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        let Some(actual_contract) = integrations.get(integration_name) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-003".to_owned(),
                message: format!(
                    "app binding `{}.{}` references integration `{integration_name}`, but no app/registry integration with that name exists.",
                    binding.target_feature, binding.target_slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        if *actual_contract != expected_contract {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-004".to_owned(),
                message: format!(
                    "app binding `{}.{}` expects `{expected_contract}`, but integration `{integration_name}` is `{actual_contract}`.",
                    binding.target_feature, binding.target_slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

pub(crate) fn profile_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    profiles: &[DoctorAppProfile],
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let app_environments: BTreeSet<_> = app
        .manifest
        .environments
        .iter()
        .map(String::as_str)
        .collect();
    let integrations = operational_integrations(&app.manifest, registry);
    let mut requirement_index = BTreeMap::new();
    for requirement in &operational.integration_requirements {
        requirement_index.insert(
            (requirement.feature.as_str(), requirement.slot.as_str()),
            requirement.contract.as_str(),
        );
    }
    for (feature, slot, contract) in enabled_pack_integration_requirements(&app.manifest, registry)
    {
        requirement_index.insert((feature, slot), contract);
    }

    for profile in profiles {
        if !app_environments.contains(profile.profile.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: profile.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "PROFILE-001".to_owned(),
                message: format!(
                    "profile `{}` is not declared in app `environments`.",
                    profile.profile.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        for url in &profile.profile.urls {
            if !profile_url_target_valid(&app.manifest, &url.target) {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-URL-001".to_owned(),
                    message: format!(
                        "profile `{}` declares URL target `{}`, but app targets do not expose that target.",
                        profile.profile.name, url.target
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        for integration in &profile.profile.integrations {
            let Some(kind) = integrations.get(integration.name.as_str()).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-INT-001".to_owned(),
                    message: format!(
                        "profile `{}` overrides integration `{}`, but no app/registry integration with that name exists.",
                        profile.profile.name, integration.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };

            if let Some(environment) = &integration.environment
                && !integration_environment_allowed(
                    &app.manifest,
                    registry,
                    &integration.name,
                    environment,
                )
            {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-INT-002".to_owned(),
                    message: format!(
                        "profile `{}` selects `{}` environment `{environment}`, but `{}` does not list that environment.",
                        profile.profile.name, kind, integration.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        for binding in &profile.profile.bindings {
            let target = (
                binding.target_feature.as_str(),
                binding.target_slot.as_str(),
            );
            let Some(expected_contract) = requirement_index.get(&target).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-BIND-001".to_owned(),
                    message: format!(
                        "profile `{}` overrides binding `{}.{}`, but that feature slot has no requirement.",
                        profile.profile.name, binding.target_feature, binding.target_slot
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };

            let Some(integration_name) = integration_source_name(&binding.source) else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-002".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` points to `{}`, but profile bindings must use `integrations.<name>` or `registry.integrations.<name>`.",
                        profile.profile.name,
                        binding.target_feature,
                        binding.target_slot,
                        binding.source
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };

            let Some(actual_contract) = integrations.get(integration_name).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-003".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` references integration `{integration_name}`, but no app/registry integration with that name exists.",
                        profile.profile.name, binding.target_feature, binding.target_slot
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };

            if actual_contract != expected_contract {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-004".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` expects `{expected_contract}`, but integration `{integration_name}` is `{actual_contract}`.",
                        profile.profile.name, binding.target_feature, binding.target_slot
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

pub(crate) fn app_pack_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let integrations = operational_integrations(&app.manifest, registry);

    for pack_use in &app.manifest.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-PACK-001".to_owned(),
                message: format!(
                    "app pack `{}` points to `{}`, but packs must use `packs.<name>` or `registry.packs.<name>`.",
                    pack_use.name, pack_use.source
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        let Some(pack) = registry.and_then(|registry| {
            registry
                .manifest
                .packs
                .iter()
                .find(|pack| pack.name == pack_name)
        }) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-PACK-002".to_owned(),
                message: format!(
                    "app pack `{}` references registry pack `{pack_name}`, but no such pack exists in `registry.lzi`.",
                    pack_use.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        for requirement in &pack.requirements {
            if requirement.kind == "integration"
                && !integrations
                    .values()
                    .any(|contract| *contract == requirement.contract)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: app.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "APP-PACK-003".to_owned(),
                    message: format!(
                        "enabled pack `{}` requires integration `{}`: `{}`, but app/registry declares no integration with that contract.",
                        pack_use.name, requirement.name, requirement.contract
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

pub(crate) fn adapter_provenance_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    profiles: &[DoctorAppProfile],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for integration in &app.manifest.integrations {
        if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
            diagnostics.push(adapter_source_diagnostic(
                app.path.clone(),
                "APP-ADAPTER-001",
                &integration.name,
                integration.adapter.as_deref().unwrap_or_default(),
            ));
        }
    }

    if let Some(registry) = registry {
        for integration in &registry.manifest.integrations {
            if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
                diagnostics.push(adapter_source_diagnostic(
                    registry.path.clone(),
                    "REG-ADAPTER-001",
                    &integration.name,
                    integration.adapter.as_deref().unwrap_or_default(),
                ));
            }
        }
    }

    for profile in profiles {
        for integration in &profile.profile.integrations {
            if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
                diagnostics.push(adapter_source_diagnostic(
                    profile.path.clone(),
                    "PROFILE-ADAPTER-001",
                    &integration.name,
                    integration.adapter.as_deref().unwrap_or_default(),
                ));
            }
        }
    }

    diagnostics
}

pub(crate) fn adapter_source_diagnostic(
    path: PathBuf,
    code: &str,
    integration_name: &str,
    adapter: &str,
) -> DoctorDiagnostic {
    DoctorDiagnostic {
        path,
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: code.to_owned(),
        message: format!(
            "integration `{integration_name}` uses adapter `{adapter}`, but adapter sources must declare provenance with `@runtime/...`, `@lazuli/plugin-<name>` (or `@lazuli/plugin-<publisher>/<name>`), `@adapter.<local>`, or a local path."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }
}

pub(crate) fn enabled_pack_provided_features<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeSet<&'a str> {
    let mut features = BTreeSet::new();
    let Some(registry) = registry else {
        return features;
    };

    for pack_use in &app.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            continue;
        };
        let Some(pack) = registry
            .manifest
            .packs
            .iter()
            .find(|pack| pack.name == pack_name)
        else {
            continue;
        };

        for provide in &pack.provides {
            if provide.kind == "feature" {
                features.insert(provide.name.as_str());
            }
        }
    }

    features
}

pub(crate) fn enabled_pack_integration_requirements<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> Vec<(&'a str, &'a str, &'a str)> {
    let mut requirements = Vec::new();
    let Some(registry) = registry else {
        return requirements;
    };

    for pack_use in &app.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            continue;
        };
        let Some(pack) = registry
            .manifest
            .packs
            .iter()
            .find(|pack| pack.name == pack_name)
        else {
            continue;
        };

        for requirement in &pack.requirements {
            if requirement.kind == "integration" {
                requirements.push((
                    pack_use.name.as_str(),
                    requirement.name.as_str(),
                    requirement.contract.as_str(),
                ));
            }
        }
    }

    requirements
}

pub(crate) fn app_service_contract_diagnostics(
    app: &DoctorAppManifest,
    operational: &OperationalFacts,
    pack_features: &BTreeSet<&str>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

    for service in &app.manifest.services {
        for owned in &service.owns {
            owners
                .entry(owned.as_str())
                .or_default()
                .push(service.name.as_str());
        }

        for exposure in &service.exposes {
            if let Some(feature_name) = exposure.target.split('.').next()
                && !service.owns.iter().any(|owned| owned == feature_name)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: app.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "APP-SVC-003".to_owned(),
                    message: format!(
                        "service `{}` exposes `{}` from feature `{feature_name}`, but does not own that feature.",
                        service.name, exposure.target
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    for feature in operational.features.values() {
        match owners.get(feature.name.as_str()) {
            Some(service_names) if service_names.len() == 1 => {}
            Some(service_names) => diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-SVC-001".to_owned(),
                message: format!(
                    "feature `{}` is owned by multiple app services: {}.",
                    feature.name,
                    service_names.join(", ")
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
            None => diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-SVC-002".to_owned(),
                message: format!(
                    "feature `{}` is not assigned to any app service boundary.",
                    feature.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
        }
    }

    for owned in owners.keys() {
        if !operational.features.contains_key(*owned) && !pack_features.contains(*owned) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-SVC-004".to_owned(),
                message: format!(
                    "app service owns `{owned}`, but no local feature with that name was found in this package."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

