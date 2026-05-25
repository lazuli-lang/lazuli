//! Codegen-adjacent project hygiene checks.
//!
//! - `SUBMODULE-DRIFT-001` — when `[generate.go].submodule = true`,
//!   the root `go.mod` and `dist/go/go.mod` must agree on the
//!   `lazuli.dev/runtime` version. Drift means `go build` of the
//!   generated submodule picks a different runtime than the host
//!   workspace.
//! - `MIGRATION-STRATEGY-CONFLICT-001` — when
//!   `[migrations].strategy = "manual"` conflicts with `app.lzi
//!   deploy migrations before_deploy`. The runtime would attempt
//!   auto-migrations the manifest forbid.

use std::fs;

use crate::doctor::{DoctorDiagnostic, DoctorPackage, DoctorSeverity, go_mod_lazuli_runtime_version};
use crate::lazurite_manifest::{Manifest, MigrationStrategy};

pub(super) fn check_submodule_drift(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    if !manifest
        .generate
        .go
        .as_ref()
        .map(|go| go.submodule)
        .unwrap_or(false)
    {
        return Vec::new();
    }

    let root_go_mod = package.project_root.join("go.mod");
    let dist_go_mod = package.project_root.join("dist/go/go.mod");
    if !dist_go_mod.is_file() {
        return Vec::new();
    }

    let Ok(root_source) = fs::read_to_string(&root_go_mod) else {
        return Vec::new();
    };
    let Ok(dist_source) = fs::read_to_string(&dist_go_mod) else {
        return Vec::new();
    };
    let Some(root_version) = go_mod_lazuli_runtime_version(&root_source) else {
        return Vec::new();
    };
    let Some(dist_version) = go_mod_lazuli_runtime_version(&dist_source) else {
        return Vec::new();
    };

    if root_version == dist_version {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: dist_go_mod,
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "SUBMODULE-DRIFT-001".to_owned(),
        message: format!(
            "`dist/go/go.mod` requires lazuli.dev/runtime {dist_version}, but root go.mod requires {root_version}."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

pub(super) fn check_migration_strategy_conflict(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    if !matches!(
        manifest
            .migrations
            .as_ref()
            .map(|migrations| &migrations.strategy),
        Some(MigrationStrategy::Manual)
    ) {
        return Vec::new();
    }

    let Some(app) = package.app.as_ref() else {
        return Vec::new();
    };
    if app
        .manifest
        .deploy
        .as_ref()
        .and_then(|deploy| deploy.migrations.as_deref())
        != Some("before_deploy")
    {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: app.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "MIGRATION-STRATEGY-CONFLICT-001".to_owned(),
        message: "`[migrations].strategy = \"manual\"` conflicts with `app.lzi deploy migrations before_deploy`."
            .to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}
