//! `app_service_contract_diagnostics` — APP-SVC-001..004.
//!
//! Reconciles the `services` block in `app.lzi` with the lifted
//! operational feature set + the feature names contributed by enabled
//! registry packs. Emits four diagnostic codes:
//!
//!  - APP-SVC-001: feature owned by multiple services (error).
//!  - APP-SVC-002: feature not assigned to any service (warning).
//!  - APP-SVC-003: service exposes a target from a feature it does not
//!    own (warning).
//!  - APP-SVC-004: service owns a name that is neither a local feature
//!    nor a pack-provided feature (warning).

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity, OperationalFacts};

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

#[cfg(test)]
mod tests {
    //! W3-3: app-service contract aggregator had no inline test. These pin
    //! APP-SVC-001 (feature owned by >1 service — a boundary correctness
    //! violation), APP-SVC-002 (feature with no service), APP-SVC-003
    //! (service exposes a target from a feature it does not own — a
    //! cross-boundary leak), and APP-SVC-004 (service owns a non-existent
    //! feature). Each was previously unasserted, so a bad multi-app service
    //! boundary would merge clean.
    use super::*;
    use lazuli_ir::{AppManifest, AppService, AppServiceExposure};
    use std::path::PathBuf;

    fn doctor_app(services: Vec<AppService>) -> DoctorAppManifest {
        let mut m: AppManifest =
            serde_json::from_str(r#"{"name":"testapp"}"#).expect("minimal app manifest");
        m.services = services;
        DoctorAppManifest {
            path: PathBuf::from("app.lzi"),
            source: String::new(),
            manifest: m,
        }
    }

    fn service(name: &str, owns: &[&str], exposes: Vec<AppServiceExposure>) -> AppService {
        AppService {
            name: name.to_owned(),
            owns: owns.iter().map(|s| s.to_string()).collect(),
            exposes,
            publishes: vec![],
            consumes: vec![],
        }
    }

    fn expose(kind: &str, target: &str) -> AppServiceExposure {
        AppServiceExposure {
            kind: kind.to_owned(),
            target: target.to_owned(),
        }
    }

    fn ops_with_features(names: &[&str]) -> OperationalFacts {
        let mut features = BTreeMap::new();
        for name in names {
            features.insert(
                (*name).to_owned(),
                crate::doctor::SourceFact {
                    path: PathBuf::from(format!("{name}.lzi")),
                    line: 1,
                    column: 1,
                    name: (*name).to_owned(),
                },
            );
        }
        OperationalFacts {
            features,
            ..OperationalFacts::default()
        }
    }

    fn codes(diags: &[DoctorDiagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn clean_service_boundary_emits_nothing() {
        let app = doctor_app(vec![
            service("svc_a", &["alpha"], vec![]),
            service("svc_b", &["beta"], vec![expose("command", "beta.publish")]),
        ]);
        let ops = ops_with_features(&["alpha", "beta"]);
        let diags = app_service_contract_diagnostics(&app, &ops, &BTreeSet::new());
        assert!(diags.is_empty(), "clean boundary, got {:?}", codes(&diags));
    }

    #[test]
    fn app_svc_001_fires_when_feature_owned_by_two_services() {
        let app = doctor_app(vec![
            service("svc_a", &["alpha"], vec![]),
            service("svc_b", &["alpha"], vec![]),
        ]);
        let ops = ops_with_features(&["alpha"]);
        let diags = app_service_contract_diagnostics(&app, &ops, &BTreeSet::new());
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "APP-SVC-001").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one APP-SVC-001, got {:?}",
            codes(&diags)
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
        assert!(hits[0].message.contains("svc_a") && hits[0].message.contains("svc_b"));
    }

    #[test]
    fn app_svc_002_fires_when_feature_unassigned() {
        let app = doctor_app(vec![service("svc_a", &["alpha"], vec![])]);
        // `beta` exists operationally but no service owns it.
        let ops = ops_with_features(&["alpha", "beta"]);
        let diags = app_service_contract_diagnostics(&app, &ops, &BTreeSet::new());
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "APP-SVC-002").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one APP-SVC-002, got {:?}",
            codes(&diags)
        );
        assert!(hits[0].message.contains("beta"));
    }

    #[test]
    fn app_svc_003_fires_when_exposing_unowned_feature() {
        // svc_a exposes a target rooted at feature `beta` it does not own.
        let app = doctor_app(vec![service(
            "svc_a",
            &["alpha"],
            vec![expose("command", "beta.leak")],
        )]);
        let ops = ops_with_features(&["alpha", "beta"]);
        let diags = app_service_contract_diagnostics(&app, &ops, &BTreeSet::new());
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "APP-SVC-003").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one APP-SVC-003, got {:?}",
            codes(&diags)
        );
        assert!(hits[0].message.contains("beta"));
    }

    #[test]
    fn app_svc_004_fires_when_owning_unknown_feature() {
        // svc_a owns `ghost` which is neither a local feature nor pack-provided.
        let app = doctor_app(vec![service("svc_a", &["ghost"], vec![])]);
        let ops = ops_with_features(&["alpha"]);
        let diags = app_service_contract_diagnostics(&app, &ops, &BTreeSet::new());
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "APP-SVC-004").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one APP-SVC-004, got {:?}",
            codes(&diags)
        );
        assert!(hits[0].message.contains("ghost"));
    }
}
