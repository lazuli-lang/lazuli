//! Migrations bucket cycle Route C — eight IR-driven cross-checks.
//!
//! Two surfaces:
//!
//! - **Feature-scoped** `previously_diagnostics` (resources + fields)
//!   and `tenant_migration_diagnostics` (`target query`/`target command`
//!   resolution, axis match against `defaults.tenancy`, mandatory
//!   `idempotency by`).
//! - **App-scoped** `deploy_strategy_diagnostics` (closed catalog
//!   `{rolling, blue_green, canary}`) and `deploy_checkpoint_diagnostics`
//!   (checkpoint path resolves relative to `app.lzi`; snapshot
//!   `lazuli_version` warning when it lags the analyzer).
//!
//! The eight doctor codes the aggregator owns:
//!
//! - `PREVIOUSLY-FWD-001`   — `Resource.previous_names` /
//!   `Field.previous_names` reference a name that exists nowhere in the
//!   package. Warning.
//! - `PREVIOUSLY-CYCLE-001` — `A previously B`, `B previously A` cycle.
//!   Error (silent-data-loss footgun).
//! - `PREVIOUSLY-DUP-001`   — two current names claim the same
//!   `previously` source. Error.
//! - `tenant-migration-target-unknown` — `tenant_migration` targets an
//!   unknown query / command. Error.
//! - `tenant-migration-handler-missing` — handler file does not exist.
//!   Warning.
//! - `tenant-migration-axis-mismatch` — axis disagrees with feature
//!   `defaults.tenancy`. Error.
//! - `tenant-migration-idempotency-required` — `idempotency by` missing.
//!   Error.
//! - `DEPLOY-STRATEGY-001`  — strategy outside the closed catalog. Error.
//! - `DEPLOY-CHECKPOINT-001` — checkpoint path does not resolve. Error.
//! - `DEPLOY-CHECKPOINT-002` — checkpoint snapshot `lazuli_version`
//!   lags the analyzer. Warning.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

pub(crate) fn diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
    app: Option<&DoctorAppManifest>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut queries_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut commands_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for feature in tier3_facts {
        queries_by_feature.insert(
            feature.feature.as_str(),
            feature.queries.iter().map(query_name).collect(),
        );
        commands_by_feature.insert(
            feature.feature.as_str(),
            feature.commands.iter().map(|c| c.name.as_str()).collect(),
        );
    }

    for feature in tier3_facts {
        previously_diagnostics(feature, &mut diagnostics);
        tenant_migration_diagnostics(
            feature,
            &queries_by_feature,
            &commands_by_feature,
            &mut diagnostics,
        );
    }

    if let Some(app) = app {
        deploy_strategy_diagnostics(app, &mut diagnostics);
        deploy_checkpoint_diagnostics(app, &mut diagnostics);
    }

    diagnostics
}

fn query_name(query: &lazuli_ir::Query) -> &str {
    match query {
        lazuli_ir::Query::List(q) => &q.name,
        lazuli_ir::Query::Lookup(q) => &q.name,
        lazuli_ir::Query::Sql(q) => &q.name,
    }
}

fn previously_diagnostics(feature: &Tier3FeatureFacts, diagnostics: &mut Vec<DoctorDiagnostic>) {
    let all_resource_names: &BTreeSet<String> = &feature.all_resource_names_in_feature;
    let all_field_names: &BTreeMap<String, BTreeSet<String>> = &feature.all_field_names_in_feature;
    // PREVIOUSLY-DUP-001 — two current names claim the same previous
    // source. Build a `previous -> Vec<current>` map per feature.
    let mut resource_previous_claims: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for fact in &feature.resource_previous_names {
        for prev in &fact.previous_names {
            resource_previous_claims
                .entry(prev.as_str())
                .or_default()
                .push(fact.current_name.as_str());
        }
    }
    for (prev, currents) in &resource_previous_claims {
        if currents.len() > 1 {
            // Anchor on the first claiming resource line.
            let first_current = currents[0];
            let line = feature
                .resource_previous_names
                .iter()
                .find(|f| f.current_name == first_current)
                .map(|f| f.line)
                .unwrap_or(feature.feature_line);
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "PREVIOUSLY-DUP-001".to_owned(),
                message: format!(
                    "resource rename target `{}` is claimed by multiple current resources ({}) in feature `{}` — only one current name may inherit a previous identity.",
                    prev,
                    currents.join(", "),
                    feature.feature
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // PREVIOUSLY-FWD-001 + PREVIOUSLY-CYCLE-001 on resources.
    for fact in &feature.resource_previous_names {
        for prev in &fact.previous_names {
            // FWD-001 — the previous name must NOT exist as a current
            // resource name (it has been renamed away). If it does, the
            // author may have copy-pasted a stale identifier.
            if all_resource_names.contains(prev.as_str()) {
                // Check for a rename cycle: does the resource `prev`
                // claim `fact.current_name` as one of its previous names?
                let cycle = tier3_iter_resource_previously_pairs(feature, prev.as_str())
                    .any(|other_prev| other_prev == fact.current_name);
                if cycle {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "PREVIOUSLY-CYCLE-001".to_owned(),
                        message: format!(
                            "resource rename cycle between `{}` and `{}` in feature `{}` — only one direction may carry `previously migrated`.",
                            fact.current_name, prev, feature.feature
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                } else {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Warning,
                        code: "PREVIOUSLY-FWD-001".to_owned(),
                        message: format!(
                            "resource `{}` declares `previously migrated {}` but `{}` is also a current resource — the rename hint will be ignored or misrouted.",
                            fact.current_name, prev, prev
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
    }

    // PREVIOUSLY-FWD-001 on fields — the previous field name must NOT
    // shadow a current field on the same resource.
    for fact in &feature.field_previous_names {
        let current_fields = all_field_names
            .get(fact.resource_name.as_str())
            .cloned()
            .unwrap_or_default();
        for prev in &fact.previous_names {
            if current_fields.contains(prev.as_str()) && prev != &fact.current_name {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: fact.line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PREVIOUSLY-FWD-001".to_owned(),
                    message: format!(
                        "field `{}.{}` declares `previously migrated {}` but `{}` is also a current field on the same resource.",
                        fact.resource_name, fact.current_name, prev, prev
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
}

/// Helper for cycle detection: yield every `previous_name` declared by
/// any resource whose current name matches `current`. Iterator avoids
/// cloning the entire fact list.
fn tier3_iter_resource_previously_pairs<'a>(
    feature: &'a Tier3FeatureFacts,
    current: &'a str,
) -> impl Iterator<Item = &'a str> {
    feature
        .resource_previous_names
        .iter()
        .filter(move |f| f.current_name == current)
        .flat_map(|f| f.previous_names.iter().map(String::as_str))
}

fn tenant_migration_diagnostics(
    feature: &Tier3FeatureFacts,
    queries_by_feature: &BTreeMap<&str, BTreeSet<&str>>,
    commands_by_feature: &BTreeMap<&str, BTreeSet<&str>>,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    for tm in &feature.tenant_migrations {
        let line = feature
            .tenant_migration_lines
            .get(&tm.name)
            .copied()
            .unwrap_or(feature.feature_line);

        if let Some(operation) = &tm.target.operation {
            let (kind, target_feature, name, index) = match operation {
                lazuli_ir::TenantMigrationTargetOperation::Query {
                    feature: target_feature,
                    name,
                } => (
                    "query",
                    target_feature
                        .as_deref()
                        .unwrap_or(feature.feature.as_str()),
                    name.as_str(),
                    queries_by_feature,
                ),
                lazuli_ir::TenantMigrationTargetOperation::Command {
                    feature: target_feature,
                    name,
                } => (
                    "command",
                    target_feature
                        .as_deref()
                        .unwrap_or(feature.feature.as_str()),
                    name.as_str(),
                    commands_by_feature,
                ),
            };
            if !index
                .get(target_feature)
                .map(|names| names.contains(name))
                .unwrap_or(false)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "tenant-migration-target-unknown".to_owned(),
                    message: format!(
                        "tenant_migration `{}` targets unknown {} `{}.{}`.",
                        tm.name, kind, target_feature, name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        let handler_path = feature
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&tm.handler.path);
        if !handler_path.is_file() {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "tenant-migration-handler-missing".to_owned(),
                message: format!(
                    "tenant_migration `{}` handler `{}` does not exist on disk.",
                    tm.name, tm.handler.path
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // Target axis must match the feature's tenancy axis.
        if let Some(axis) = &feature.tenancy_axis {
            if &tm.target.axis != axis {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "tenant-migration-axis-mismatch".to_owned(),
                    message: format!(
                        "tenant_migration `{}` declares `axis {}` but feature `{}` uses tenancy axis `{}`.",
                        tm.name, tm.target.axis, feature.feature, axis
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        } else {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "tenant-migration-axis-mismatch".to_owned(),
                message: format!(
                    "tenant_migration `{}` declares `axis {}` but feature `{}` did not declare a `defaults.tenancy` axis.",
                    tm.name, tm.target.axis, feature.feature
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // `idempotency <path>` is mandatory; absence
        // surfaces as an empty `IdempotencyKey.by` Path.
        if tm.idempotency.by.segments.is_empty() {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "tenant-migration-idempotency-required".to_owned(),
                message: format!(
                    "tenant_migration `{}` does not declare `idempotency <path>` — tenant migrations are not safely re-runnable without an idempotency key.",
                    tm.name
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

const DEPLOY_STRATEGY_CATALOG: &[&str] = &["rolling", "blue_green", "canary"];

fn deploy_strategy_diagnostics(app: &DoctorAppManifest, diagnostics: &mut Vec<DoctorDiagnostic>) {
    let Some(deploy) = app.manifest.deploy.as_ref() else {
        return;
    };
    let Some(strategy) = deploy.strategy.as_ref() else {
        return;
    };
    if !DEPLOY_STRATEGY_CATALOG.contains(&strategy.as_str()) {
        diagnostics.push(DoctorDiagnostic {
            path: app.path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "DEPLOY-STRATEGY-001".to_owned(),
            message: format!(
                "app `deploy.strategy {}` is not in the closed catalog ({}).",
                strategy,
                DEPLOY_STRATEGY_CATALOG.join(", ")
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

fn deploy_checkpoint_diagnostics(app: &DoctorAppManifest, diagnostics: &mut Vec<DoctorDiagnostic>) {
    let Some(deploy) = app.manifest.deploy.as_ref() else {
        return;
    };
    let Some(checkpoint) = deploy.checkpoint.as_ref() else {
        return;
    };
    // DEPLOY-CHECKPOINT-001 — path must resolve to a file relative
    // to the directory containing `app.lzi`.
    let app_dir = app
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let candidate = app_dir.join(&checkpoint.path);
    if !candidate.exists() {
        diagnostics.push(DoctorDiagnostic {
            path: app.path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "DEPLOY-CHECKPOINT-001".to_owned(),
            message: format!(
                "deploy checkpoint `{}` references path `{}` that does not exist relative to app.lzi.",
                checkpoint.name, checkpoint.path
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
        return;
    }

    // DEPLOY-CHECKPOINT-002 — load snapshot and verify `lazuli_version`
    // (a top-level JSON field). Stale = warning, not error: the snapshot
    // file existed but is older than the analyzer's expected schema.
    if let Ok(text) = std::fs::read_to_string(&candidate) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            let snapshot_version = value
                .get("lazuli_version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let expected = env!("CARGO_PKG_VERSION");
            if !snapshot_version.is_empty() && snapshot_version != expected {
                diagnostics.push(DoctorDiagnostic {
                    path: app.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "DEPLOY-CHECKPOINT-002".to_owned(),
                    message: format!(
                        "deploy checkpoint `{}` snapshot `lazuli_version` ({}) lags analyzer ({}); regenerate to silence this warning.",
                        checkpoint.name, snapshot_version, expected
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
}
