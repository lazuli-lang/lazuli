//! Per-rule correctness dispatchers — one helper per closed-catalog
//! rule from `lazuli_doctor::correctness::*`.
//!
//! Each helper maps the rule's `Finding` shape onto the canonical
//! `DoctorDiagnostic` envelope and resolves the line anchor through
//! the per-feature `command_lines` / `query_lines` tables. The
//! orchestrator (`correctness::diagnostics`) invokes each in turn and
//! concatenates the result.
//!
//! Lifted out of the `correctness` god-file in the rails-style R9
//! split.

use super::make_synthetic_feature_for_correctness;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts, correctness};

pub(crate) fn missing_policy_on_query_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use std::collections::BTreeSet;

    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    for fact in facts {
        for finding in correctness::missing_policy_on_query_001::check_queries(
            &fact.feature,
            fact.defaults_policy.as_ref(),
            &fact.queries,
            &fact.path,
        ) {
            let line = fact
                .query_lines
                .get(&finding.query_name)
                .copied()
                .unwrap_or(fact.feature_line);
            if !seen.insert((
                finding.path.clone(),
                finding.feature.clone(),
                finding.query_kind,
                finding.query_name.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: correctness::missing_policy_on_query_001::Finding::CODE.to_owned(),
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

/// POLICY-REF-UNRESOLVED-001 — SECURITY. A command / query / api policy
/// reference that resolves to no declared `policies` category in any feature
/// (cross-feature `PolicyRef::External` to a missing category, or a
/// feature-local `@policy.<name>` with no match). Pre-fix these lowered to a
/// Name-only empty-atoms policy (fail-open / bypass); codegen now fails closed
/// (deny atom) and this rule surfaces the broken reference at build time.
///
/// Resolution uses the GLOBAL category catalog (every feature's `policies`
/// block) so legitimately cross-feature references do not false-positive —
/// matching the route-guard analyzer's `category_atoms` cross-feature walk.
pub(crate) fn policy_ref_unresolved_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use correctness::policy_ref_unresolved_001::{Finding, PolicyCatalog, unresolved_reference};

    // Build the catalog once from every feature's policy categories.
    let catalog =
        PolicyCatalog::from_feature_policies(facts.iter().map(|f| (f.feature.as_str(), &f.policies)));

    let mut diagnostics = Vec::new();

    for fact in facts {
        // Commands.
        for command in &fact.commands {
            // A structured `policy <expr>` carries its own atoms — never a
            // category reference, so skip category resolution for it.
            if command.policy_expr.is_some() {
                continue;
            }
            if let Some(reference) =
                unresolved_reference(&command.policy, &fact.feature, &catalog)
            {
                let line = fact
                    .command_lines
                    .get(&command.name)
                    .copied()
                    .unwrap_or(fact.feature_line);
                diagnostics.push(make_diag(
                    Finding {
                        feature: fact.feature.clone(),
                        kind: "command",
                        name: command.name.clone(),
                        reference,
                    },
                    fact.path.clone(),
                    line,
                ));
            }
        }
        // Queries.
        for query in &fact.queries {
            let (name, policy, has_expr) = query_policy_view(query);
            if has_expr {
                continue;
            }
            if let Some(reference) = unresolved_reference(policy, &fact.feature, &catalog) {
                let line = fact
                    .query_lines
                    .get(name)
                    .copied()
                    .unwrap_or(fact.feature_line);
                diagnostics.push(make_diag(
                    Finding {
                        feature: fact.feature.clone(),
                        kind: "query",
                        name: name.to_owned(),
                        reference,
                    },
                    fact.path.clone(),
                    line,
                ));
            }
        }
        // Api endpoints.
        for api in &fact.apis {
            if let Some(reference) = unresolved_reference(&api.policy, &fact.feature, &catalog) {
                let line = fact
                    .api_lines
                    .get(&api.name)
                    .copied()
                    .unwrap_or(fact.feature_line);
                diagnostics.push(make_diag(
                    Finding {
                        feature: fact.feature.clone(),
                        kind: "api",
                        name: api.name.clone(),
                        reference,
                    },
                    fact.path.clone(),
                    line,
                ));
            }
        }
    }

    diagnostics
}

/// Project a `Query` into (name, &policy, has_structured_expr) for the
/// POLICY-REF-UNRESOLVED scan.
fn query_policy_view(query: &lazuli_ir::Query) -> (&str, &lazuli_ir::PolicyRef, bool) {
    match query {
        lazuli_ir::Query::List(q) => (q.name.as_str(), &q.policy, q.policy_expr.is_some()),
        lazuli_ir::Query::Lookup(q) => (q.name.as_str(), &q.policy, q.policy_expr.is_some()),
        lazuli_ir::Query::Sql(q) => (q.name.as_str(), &q.policy, q.policy_expr.is_some()),
    }
}

fn make_diag(
    finding: correctness::policy_ref_unresolved_001::Finding,
    path: std::path::PathBuf,
    line: usize,
) -> DoctorDiagnostic {
    DoctorDiagnostic {
        message: finding.message(),
        path,
        line,
        column: 1,
        severity: DoctorSeverity::Error,
        code: correctness::policy_ref_unresolved_001::Finding::CODE.to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }
}

pub(crate) fn duplicate_query_name_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for fact in facts {
        for finding in correctness::duplicate_query_name::check_queries(
            &fact.feature,
            &fact.queries,
            &fact.path,
        ) {
            let line = fact
                .query_lines
                .get(&finding.query_name)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::duplicate_query_name::Finding::CODE.to_owned(),
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

/// ROUTE-ID-UNUSED-IN-EFFECT-001 — pair to the LAZ-route-id-codegen-go
/// guard. Walks each feature's lifted commands and fires when a
/// `route <name>: <Type>` slot on an `updates` / `deletes` effect has
/// no matching input slot to back the codegen's `FromInput(...)`
/// binding. Anchored at the command header via `command_lines`.
pub(crate) fn route_id_effect_consistency_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use std::collections::BTreeSet;

    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    for fact in facts {
        for finding in correctness::route_id_effect_consistency::check_commands(
            &fact.feature,
            &fact.commands,
            &fact.path,
        ) {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or(fact.feature_line);
            if !seen.insert((
                finding.path.clone(),
                finding.feature.clone(),
                finding.command.clone(),
                finding.param_name.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::route_id_effect_consistency::Finding::CODE.to_owned(),
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

/// MUTATION-WITHOUT-READBACK-001 dispatch — codegen-correctness cycle
/// 2026-05-21 cell A6. Pairs each mutating command (`creates` / `updates`
/// / `deletes`) with the set of `query.lookup` / `query.list` shapes
/// across ALL features (cross-feature read queries count, per cycle
/// decision). Anchored at the command header line; falls back to the
/// feature header when the command line is unknown.
pub(crate) fn mutation_without_readback_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use std::collections::BTreeSet;

    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    // Pre-build the cross-feature read-query index once. Each fact's
    // own queries get re-included via `check_from_facts`, which dedupes
    // against the current feature name to avoid double-counting.
    let neighbor_queries: Vec<(String, &[lazuli_ir::Query])> = facts
        .iter()
        .map(|fact| (fact.feature.clone(), fact.queries.as_slice()))
        .collect();

    for fact in facts {
        for finding in correctness::mutation_without_readback::check_from_facts(
            &fact.feature,
            &fact.commands,
            &fact.queries,
            &neighbor_queries,
            &fact.path,
        ) {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or(fact.feature_line);
            if !seen.insert((
                finding.path.clone(),
                finding.feature.clone(),
                finding.command.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: correctness::mutation_without_readback::Finding::CODE.to_owned(),
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

/// UPDATES-MISSING-UPDATED-AT-001 dispatch — any local resource touched
/// by an `updates` effect must either declare `updated_at: DateTime` or
/// have effective timestamps enabled. Anchored at the feature header
/// because the finding is resource-scoped and the current fact row does
/// not carry resource-header line anchors.
pub(crate) fn updates_missing_updated_at_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use std::collections::BTreeSet;

    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    for fact in facts {
        let mut feature = make_synthetic_feature_for_correctness(fact);
        feature.defaults.timestamps = fact.defaults_timestamps;

        for finding in correctness::updates_missing_updated_at::check(&feature, &fact.path) {
            if !seen.insert((
                finding.path.clone(),
                finding.feature.clone(),
                finding.resource.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: correctness::updates_missing_updated_at::Finding::CODE.to_owned(),
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
