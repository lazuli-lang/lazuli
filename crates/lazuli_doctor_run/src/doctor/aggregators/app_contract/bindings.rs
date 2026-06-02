//! `app_binding_contract_diagnostics` — APP-BIND-001..005.
//!
//! Cross-references the lifted operational integration requirements (and
//! the enabled-pack requirements pulled from the registry) with the
//! app's `bindings` block. Emits five distinct diagnostic codes:
//!
//!  - APP-BIND-001: feature requires slot X but no binding present.
//!  - APP-BIND-002: binding `source` is not `integrations.<name>` /
//!    `registry.integrations.<name>`.
//!  - APP-BIND-003: binding source name does not resolve to a declared
//!    integration.
//!  - APP-BIND-004: contract mismatch between requirement and resolved
//!    integration.
//!  - APP-BIND-005: binding has no matching feature requirement.

use std::collections::BTreeMap;

use super::predicates::{
    enabled_pack_integration_requirements, integration_source_name, operational_integrations,
};
use crate::doctor::{
    DoctorAppManifest, DoctorAppRegistry, DoctorDiagnostic, DoctorSeverity, OperationalFacts,
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

#[cfg(test)]
mod tests {
    //! W3-3 (overnight-2026-06-02/07-test-coverage §3): the app-contract
    //! binding aggregator had NO inline test, so APP-BIND-002/003/004/005
    //! could regress and merge clean. These build malformed in-memory
    //! manifests and assert each exact code fires on a real violation, plus
    //! a clean manifest stays quiet. Security/correctness: a bad cross-app
    //! integration binding (wrong source form, dangling integration, or a
    //! contract mismatch) is a wiring escape hatch that ships to prod.
    use super::*;
    use lazuli_ir::{AppBinding, AppIntegration, AppManifest};
    use std::path::PathBuf;

    fn manifest_with(bindings: Vec<AppBinding>, integrations: Vec<AppIntegration>) -> AppManifest {
        // Start from the minimal serde envelope so we only set what matters
        // (the struct has ~40 fields, almost all `#[serde(default)]`).
        let mut m: AppManifest =
            serde_json::from_str(r#"{"name":"testapp"}"#).expect("minimal app manifest");
        m.bindings = bindings;
        m.integrations = integrations;
        m
    }

    fn doctor_app(manifest: AppManifest) -> DoctorAppManifest {
        DoctorAppManifest {
            path: PathBuf::from("app.lzi"),
            source: String::new(),
            manifest,
        }
    }

    fn integration(name: &str, kind: &str) -> AppIntegration {
        AppIntegration {
            name: name.to_owned(),
            kind: kind.to_owned(),
            adapter: None,
            adapter_provenance: None,
            environments: vec![],
            credentials: None,
            data_classification: None,
            ..serde_json::from_str(r#"{"name":"x","kind":"y"}"#).expect("integration envelope")
        }
    }

    fn binding(feature: &str, slot: &str, source: &str) -> AppBinding {
        AppBinding {
            target_feature: feature.to_owned(),
            target_slot: slot.to_owned(),
            source: source.to_owned(),
        }
    }

    fn requirement(feature: &str, slot: &str, contract: &str) -> OperationalFacts {
        OperationalFacts {
            integration_requirements: vec![crate::doctor::IntegrationRequirementFact {
                path: PathBuf::from("billing.lzi"),
                line: 1,
                column: 1,
                feature: feature.to_owned(),
                slot: slot.to_owned(),
                contract: contract.to_owned(),
            }],
            ..OperationalFacts::default()
        }
    }

    fn codes(diags: &[DoctorDiagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn clean_binding_emits_nothing() {
        // requirement billing.email: smtp, satisfied by integrations.mailer (kind smtp).
        let app = doctor_app(manifest_with(
            vec![binding("billing", "email", "integrations.mailer")],
            vec![integration("mailer", "smtp")],
        ));
        let ops = requirement("billing", "email", "smtp");
        let diags = app_binding_contract_diagnostics(&app, None, &ops);
        assert!(
            diags.is_empty(),
            "clean binding should emit no diagnostics, got: {:?}",
            codes(&diags)
        );
    }

    #[test]
    fn app_bind_002_fires_on_non_integrations_source() {
        // source not `integrations.<name>` / `registry.integrations.<name>`.
        let app = doctor_app(manifest_with(
            vec![binding("billing", "email", "mailer")], // missing prefix
            vec![integration("mailer", "smtp")],
        ));
        let ops = requirement("billing", "email", "smtp");
        let diags = app_binding_contract_diagnostics(&app, None, &ops);
        let bind002: Vec<_> = diags.iter().filter(|d| d.code == "APP-BIND-002").collect();
        assert_eq!(
            bind002.len(),
            1,
            "want exactly one APP-BIND-002, got codes {:?}",
            codes(&diags)
        );
        assert_eq!(bind002[0].severity, DoctorSeverity::Error);
        assert!(bind002[0].message.contains("integrations.<name>"));
    }

    #[test]
    fn app_bind_003_fires_on_dangling_integration() {
        // source resolves to integration name `ghost`, which is not declared.
        let app = doctor_app(manifest_with(
            vec![binding("billing", "email", "integrations.ghost")],
            vec![integration("mailer", "smtp")],
        ));
        let ops = requirement("billing", "email", "smtp");
        let diags = app_binding_contract_diagnostics(&app, None, &ops);
        let bind003: Vec<_> = diags.iter().filter(|d| d.code == "APP-BIND-003").collect();
        assert_eq!(
            bind003.len(),
            1,
            "want exactly one APP-BIND-003, got codes {:?}",
            codes(&diags)
        );
        assert_eq!(bind003[0].severity, DoctorSeverity::Error);
        assert!(bind003[0].message.contains("ghost"));
    }

    #[test]
    fn app_bind_004_fires_on_contract_mismatch() {
        // requirement wants `smtp`; integration `mailer` is actually `sendgrid_api`.
        let app = doctor_app(manifest_with(
            vec![binding("billing", "email", "integrations.mailer")],
            vec![integration("mailer", "sendgrid_api")],
        ));
        let ops = requirement("billing", "email", "smtp");
        let diags = app_binding_contract_diagnostics(&app, None, &ops);
        let bind004: Vec<_> = diags.iter().filter(|d| d.code == "APP-BIND-004").collect();
        assert_eq!(
            bind004.len(),
            1,
            "want exactly one APP-BIND-004, got codes {:?}",
            codes(&diags)
        );
        assert_eq!(bind004[0].severity, DoctorSeverity::Error);
        assert!(bind004[0].message.contains("smtp") && bind004[0].message.contains("sendgrid_api"));
    }

    #[test]
    fn app_bind_005_fires_on_orphan_binding() {
        // a binding with no matching feature requirement at all.
        let app = doctor_app(manifest_with(
            vec![binding("ghostfeature", "slot", "integrations.mailer")],
            vec![integration("mailer", "smtp")],
        ));
        // No requirement for ghostfeature.slot.
        let ops = OperationalFacts::default();
        let diags = app_binding_contract_diagnostics(&app, None, &ops);
        let bind005: Vec<_> = diags.iter().filter(|d| d.code == "APP-BIND-005").collect();
        assert_eq!(
            bind005.len(),
            1,
            "want exactly one APP-BIND-005, got codes {:?}",
            codes(&diags)
        );
        assert_eq!(bind005[0].severity, DoctorSeverity::Warning);
    }
}
