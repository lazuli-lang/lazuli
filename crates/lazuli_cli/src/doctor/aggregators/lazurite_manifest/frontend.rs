//! Frontend / audience contracts between `.lzx` and
//! `[frontends.*]`.
//!
//! - `FRONTEND-AUDIENCE-UNKNOWN-001` — `[frontends.<name>].audiences
//!   = [...]` lists an audience no `.lzx` surface declares.
//! - `AUDIENCE-NO-FRONTEND-001` — `.lzx` declares an audience that
//!   no `[frontends.*]` block ships. Warning, not error — pre-launch
//!   surfaces may declare ahead of the frontend wiring.
//! - `FRONTEND-OUT-COLLISION-001` — two `[frontends.<name>].out`
//!   slots point to the same directory.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::aggregators::cross_feature::collect_known_audiences;
use crate::doctor::{DoctorDiagnostic, DoctorPackage, DoctorSeverity};
use crate::lazurite_manifest::Manifest;

pub(super) fn check_frontend_audience_unknown(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let known = collect_known_audiences(&package.files);
    let mut diagnostics = Vec::new();
    for (frontend_name, frontend) in &manifest.frontends {
        for audience in &frontend.audiences {
            if !known.contains(audience) {
                diagnostics.push(DoctorDiagnostic {
                    path: package.project_root.join("Lazurite.toml"),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "FRONTEND-AUDIENCE-UNKNOWN-001".to_owned(),
                    message: format!(
                        "`[frontends.{frontend_name}]` ships audience `{audience}`, but no `.lzx` surface declares it."
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

pub(super) fn check_audience_no_frontend(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let shipped: BTreeSet<&str> = manifest
        .frontends
        .values()
        .flat_map(|frontend| frontend.audiences.iter().map(|audience| audience.as_str()))
        .collect();

    collect_known_audiences(&package.files)
        .into_iter()
        .filter(|audience| !shipped.contains(audience.as_str()))
        .map(|audience| DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "AUDIENCE-NO-FRONTEND-001".to_owned(),
            message: format!(
                "`.lzx` declares audience `{audience}`, but no `[frontends.*]` block ships it."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

pub(super) fn check_frontend_out_collision(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut first_by_out: BTreeMap<&str, &str> = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (name, frontend) in &manifest.frontends {
        if let Some(first) = first_by_out.insert(frontend.out.as_str(), name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: package.project_root.join("Lazurite.toml"),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "FRONTEND-OUT-COLLISION-001".to_owned(),
                message: format!(
                    "`[frontends.{name}]` and `[frontends.{first}]` both declare output path `{}`.",
                    frontend.out
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
