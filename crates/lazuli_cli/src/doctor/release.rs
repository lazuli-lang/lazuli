//! `lazuli doctor --release` entrypoint.
//!
//! Migration-recipe gate that fails the release pipeline when LZIR_SCHEMA
//! moves without a corresponding `migrations/recipes/<from>-to-<to>/` entry,
//! or when an existing recipe fails to smoke-test against its bundled
//! input/output fixtures.
//!
//! Extracted from `doctor/mod.rs` as part of the rails-style R3-C cut
//! (CLI-entrypoint bodies live in named files, not in mod.rs).

use std::path::Path;

use anyhow::{Result, bail};
use lazuli_ir::LZIR_SCHEMA;

use super::helpers::doctor_project_root;
use super::parsers::major_minor;
use super::scanners::{collect_recipe_dirs, extract_lzir_schema};
use super::{DoctorDiagnostic, DoctorSeverity};

pub(super) fn doctor_release_command(input: &Path) -> Result<()> {
    let project_root = doctor_project_root(input);
    let mut diagnostics = Vec::new();
    diagnostics.extend(check_migration_recipe_001(&project_root, LZIR_SCHEMA));
    diagnostics.extend(check_migration_recipe_002(&project_root));
    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DoctorSeverity::Error);

    for diagnostic in &diagnostics {
        diagnostic.print();
    }

    if has_error {
        bail!("{} failed Lazuli release checks", project_root.display());
    }

    println!("{} passed Lazuli release checks", project_root.display());
    Ok(())
}

pub(super) fn check_migration_recipe_001(
    project_root: &Path,
    lzir_schema: &str,
) -> Vec<DoctorDiagnostic> {
    let Ok(output) = std::process::Command::new("git")
        .args(["show", "HEAD~1:crates/lazuli_ir/src/lib.rs"])
        .current_dir(project_root)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let previous_source = String::from_utf8_lossy(&output.stdout);
    let Some(previous_schema) = extract_lzir_schema(&previous_source) else {
        return Vec::new();
    };
    if previous_schema == lzir_schema {
        return Vec::new();
    }

    let previous_major_minor = major_minor(&previous_schema);
    let current_major_minor = major_minor(lzir_schema);
    let transition_dir = project_root.join("migrations/recipes").join(format!(
        "{}-to-{}",
        previous_major_minor, current_major_minor
    ));
    let recipe_count = std::fs::read_dir(&transition_dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .count()
        })
        .unwrap_or(0);

    if recipe_count > 0 {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: transition_dir,
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "MIGRATION-RECIPE-001".to_owned(),
        message: format!(
            "LZIR_SCHEMA changed from {} to {}, but no recipe directory exists under migrations/recipes/{}-to-{}/.",
            previous_schema, lzir_schema, previous_major_minor, current_major_minor
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

pub(super) fn check_migration_recipe_002(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let recipe_root = project_root.join("migrations/recipes");
    let mut recipe_dirs = Vec::new();
    collect_recipe_dirs(&recipe_root, &mut recipe_dirs);

    let mut diagnostics = Vec::new();
    for recipe_dir in recipe_dirs {
        let input = recipe_dir.join("input.lzi");
        let output = recipe_dir.join("output.lzi");
        if !input.exists() && !output.exists() {
            continue;
        }
        if let Err(error) = crate::upgrade::smoke_recipe(&recipe_dir) {
            diagnostics.push(DoctorDiagnostic {
                path: recipe_dir,
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "MIGRATION-RECIPE-002".to_owned(),
                message: format!("migration recipe smoke failed: {error}"),
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
