//! `query.compose` aggregator — wires the W3 composite-read doctor codes
//! into the runtime `lazuli doctor` dispatch.
//!
//! W3 shipped the nine `query.compose` rules (`lazuli_doctor::compose::*`
//! plus the complementary `vocab::vocab_sql_composable_001`) with logic +
//! passing unit tests, but left them **declared-but-not-dispatched** — the
//! dormant-rule anti-pattern (`docs/proposals/primitive-end-to-end-gaps-2026-05-29.md`
//! §3). This aggregator is W7: it maps each rule's typed `Finding` onto the
//! canonical `DoctorDiagnostic` envelope so the codes actually fire in the
//! real CLI command, mirroring the
//! [`super::correctness::rule_dispatchers`] pattern
//! (`missing_policy_on_query_diagnostics`).
//!
//! ## What each dispatch does
//!
//! * Synthesizes a per-feature `&Feature` view via the correctness
//!   aggregator's [`super::correctness::make_synthetic_feature_for_correctness`]
//!   adapter — it populates the `resources` / `records` / `queries` /
//!   `policies` / `defaults` (tenancy) slices the compose rules read.
//! * Resolves the diagnostic line through the per-feature `query_lines`
//!   table (the `query.compose <name>` header), falling back to the feature
//!   header when the line is unknown — the same line-resolution the query
//!   dispatchers in `correctness::rule_dispatchers` use.
//! * Stamps severity per proposal §7: the six structural / security codes
//!   are `error` in both strict and production profiles (they make a
//!   malformed composite read a build-time failure — the hard bar); the two
//!   hygiene/determinism codes (`-NULLABILITY-MISMATCH`, `-DEMOTABLE-TO-LIST`)
//!   and the complementary `VOCAB-SQL-COMPOSABLE-001` nudge are `warning`.
//!   `category` is `Correctness` for every compose finding (the
//!   `COMPOSE-*` prefix routes there via `RuleCategory::from_code_prefix`).
//!
//! ## The Module-graph path for `COMPOSE-JOIN-PATH-001`
//!
//! W2's analyzer resolves FK-path joins against the IN-feature relation
//! graph and TRUSTS cross-feature hops. `COMPOSE-JOIN-PATH-001` is the
//! deferred Module-context resolution, so this aggregator threads the union
//! of EVERY feature's resources (`check_in_module`) — a join hop that
//! resolves nowhere module-wide fires.
//!
//! ## Loading SQL bodies for `VOCAB-SQL-COMPOSABLE-001`
//!
//! The heuristic reads the `query.sql` body off `sql_path` (resolved against
//! `project_root`, identical to the `query.view` SQL-file pass). A query
//! whose `.sql` file is absent / unreadable is skipped — the rule is a
//! best-effort nudge, never an error, and missing-file reporting belongs to
//! a different code.

use std::fs;
use std::path::{Path, PathBuf};

use lazuli_doctor::compose;
use lazuli_doctor::vocab::vocab_sql_composable_001;

use super::correctness::make_synthetic_feature_for_correctness;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// Aggregate every `query.compose` finding across the package's Tier 3
/// facts into the canonical `DoctorDiagnostic` envelope.
///
/// `project_root` is needed only to resolve `query.sql` `sql_path`s for the
/// complementary `VOCAB-SQL-COMPOSABLE-001` nudge.
pub(crate) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // The Module graph for COMPOSE-JOIN-PATH-001: the union of every
    // feature's resources, synthesized once. Cross-feature join hops W2
    // trusted are resolved here.
    let all_features: Vec<lazuli_ir::Feature> = facts
        .iter()
        .map(make_synthetic_feature_for_correctness)
        .collect();

    for fact in facts {
        let feature = make_synthetic_feature_for_correctness(fact);
        let path = &fact.path;

        // COMPOSE-JOIN-PATH-001 — error. Module-graph resolution (the
        // cross-feature hops W2's in-feature resolver trusts/defers).
        for finding in compose::join_path_001::check_in_module(&feature, &all_features, path) {
            diagnostics.push(compose_diag(
                fact,
                &finding.query_name,
                finding.message(),
                finding.path,
                compose::join_path_001::Finding::CODE,
                DoctorSeverity::Error,
            ));
        }

        // COMPOSE-PROJECTION-SOURCE-001 — error.
        for finding in compose::projection_source_001::check(&feature, path) {
            diagnostics.push(compose_diag(
                fact,
                &finding.query_name,
                finding.message(),
                finding.path,
                compose::projection_source_001::Finding::CODE,
                DoctorSeverity::Error,
            ));
        }

        // COMPOSE-SUBSELECT-RELATION-001 — error.
        for finding in compose::subselect_relation_001::check(&feature, path) {
            diagnostics.push(compose_diag(
                fact,
                &finding.query_name,
                finding.message(),
                finding.path,
                compose::subselect_relation_001::Finding::CODE,
                DoctorSeverity::Error,
            ));
        }

        // COMPOSE-SUBSELECT-PREDICATE-FIELD-001 — error.
        for finding in compose::subselect_predicate_field_001::check(&feature, path) {
            diagnostics.push(compose_diag(
                fact,
                &finding.query_name,
                finding.message(),
                finding.path,
                compose::subselect_predicate_field_001::Finding::CODE,
                DoctorSeverity::Error,
            ));
        }

        // COMPOSE-SCOPE-UNGROUNDED-001 — error. THE load-bearing security
        // code: a tenant-bearing root whose scope is overridden with no
        // policy is a cross-tenant leak.
        for finding in compose::scope_ungrounded_001::check(&feature, path) {
            diagnostics.push(compose_diag(
                fact,
                &finding.query_name,
                finding.message(),
                finding.path,
                compose::scope_ungrounded_001::Finding::CODE,
                DoctorSeverity::Error,
            ));
        }

        // COMPOSE-SUBSELECT-CATALOG-001 — error.
        for finding in compose::subselect_catalog_001::check(&feature, path) {
            diagnostics.push(compose_diag(
                fact,
                &finding.query_name,
                finding.message(),
                finding.path,
                compose::subselect_catalog_001::Finding::CODE,
                DoctorSeverity::Error,
            ));
        }

        // COMPOSE-NULLABILITY-MISMATCH-001 — warning.
        for finding in compose::nullability_mismatch_001::check(&feature, path) {
            diagnostics.push(compose_diag(
                fact,
                &finding.query_name,
                finding.message(),
                finding.path,
                compose::nullability_mismatch_001::Finding::CODE,
                DoctorSeverity::Warning,
            ));
        }

        // COMPOSE-DEMOTABLE-TO-LIST-001 — warning.
        for finding in compose::demotable_to_list_001::check(&feature, path) {
            diagnostics.push(compose_diag(
                fact,
                &finding.query_name,
                finding.message(),
                finding.path,
                compose::demotable_to_list_001::Finding::CODE,
                DoctorSeverity::Warning,
            ));
        }

        // VOCAB-SQL-COMPOSABLE-001 — warning. The symmetric Prisma-trap
        // nudge from the `query.sql` side: a pure FK-JOIN + scalar
        // sub-select read hidden in opaque SQL could be a checked
        // `query.compose`. Reads the `.sql` body off `sql_path`.
        for query in &fact.queries {
            let lazuli_ir::Query::Sql(sql_query) = query else {
                continue;
            };
            let sql_file = resolve_sql_path(project_root, &sql_query.sql_path);
            let Ok(sql_body) = fs::read_to_string(&sql_file) else {
                continue;
            };
            if let Some(finding) =
                vocab_sql_composable_001::check_query(&fact.feature, sql_query, &sql_body, path)
            {
                diagnostics.push(compose_diag(
                    fact,
                    &finding.query_name,
                    finding.message(),
                    finding.path,
                    vocab_sql_composable_001::Finding::CODE,
                    DoctorSeverity::Warning,
                ));
            }
        }
    }

    diagnostics
}

/// Build one compose `DoctorDiagnostic`, resolving the line from the
/// per-feature `query_lines` table (the `query.compose <name>` /
/// `query.sql <name>` header) and stamping `category = Correctness`.
fn compose_diag(
    fact: &Tier3FeatureFacts,
    query_name: &str,
    message: String,
    path: PathBuf,
    code: &str,
    severity: DoctorSeverity,
) -> DoctorDiagnostic {
    let line = fact
        .query_lines
        .get(query_name)
        .copied()
        .unwrap_or(fact.feature_line);
    DoctorDiagnostic {
        message,
        path,
        line,
        column: 1,
        severity,
        code: code.to_owned(),
        category: Some(lazuli_doctor::RuleCategory::Correctness),
        feature_name: Some(fact.feature.clone()),
        construct: None,
        fix: None,
        group: None,
    }
}

/// Resolve a `query.sql` `sql_path` against the project root. Absolute paths
/// pass through; relative paths (`./queries/foo.sql`) resolve under
/// `project_root` — identical to the `query.view` SQL-file pass.
fn resolve_sql_path(project_root: &Path, sql_path: &str) -> PathBuf {
    let path = Path::new(sql_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}
