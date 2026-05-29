//! Domain-model aggregator.
//!
//! Surfaces the four CL.C.4 closed-catalog rules over every feature's
//! aggregate / resource / invariant / slug shape:
//!
//! - `AGGREGATE-ROOT-UNKNOWN`        — error
//! - `AGGREGATE-CONTAINS-UNKNOWN`    — error
//! - `INVARIANT-PREDICATE-INVALID`   — error (resource-scoped OR aggregate-scoped)
//! - `SLUG-UNIQUENESS-IMPLICIT`      — warning
//!
//! Each rule lives in `lazuli_doctor::domain` and consumes a synthesized
//! `&Feature` (so the rule signature stays independent of doctor
//! scaffolding). This aggregator hands the rules a Feature view built
//! from the Tier 3 fact bundle via `make_synthetic_feature_for_reports`
//! (which carries the slots — `aggregates`, `resources`, `apis`,
//! `records`, `queries`, `agents`, `reports` — that the domain checks
//! actually walk).
//!
//! Optimization: when a fact row has zero aggregates AND no resource
//! invariants AND no `slug` fields anywhere, the rules can't fire, so
//! the loop body is skipped. This keeps the per-feature dispatch O(1)
//! for the common case where a feature is plain CRUD without domain
//! modeling.
//!
//! Line anchoring resolves through `aggregate_lines` for aggregate-scoped
//! findings; resource-scoped findings anchor at the feature header
//! because the fact bundle does not yet carry per-resource line spans.
//!
//! See `docs/proposals/correctness-tier3.md` §"Domain-model rules" for
//! the full closed catalog and per-rule rationale.

use crate::doctor::{
    DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts, domain, make_synthetic_feature_for_reports,
};

/// Aggregate every domain-model finding across all Tier 3 features into
/// the canonical `DoctorDiagnostic` envelope.
pub(crate) fn diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for fact in facts {
        let has_conditional_unique = fact.resources.iter().any(|r| {
            r.constraints
                .iter()
                .any(|c| matches!(c, lazuli_ir::Constraint::Unique(u) if u.when.is_some()))
        });
        // GAP-07 — `many_through` declarations also gate the domain rules.
        let has_many_through = fact.resources.iter().any(|r| !r.many_through.is_empty());
        // GAP-09 — any policy category carrying a predicate-gated atom puts
        // this feature in scope for POLICY-PREDICATE-001.
        let has_conditional_policy = fact
            .policies
            .categories
            .iter()
            .any(|c| !c.conditional_atoms.is_empty());
        if fact.aggregates.is_empty()
            && fact.resources.iter().all(|r| r.invariants.is_empty())
            && fact
                .resources
                .iter()
                .all(|r| r.fields.iter().all(|f| !f.slug))
            && !has_conditional_unique
            && !has_many_through
            && !has_conditional_policy
        {
            continue;
        }
        let mut feature = make_synthetic_feature_for_reports(fact);
        // GAP-09 — POLICY-PREDICATE-001 walks `feature.policies` and
        // cross-checks each predicate's `input.*` refs against the
        // commands that consume the category. The report-shaped synthetic
        // feature drops both slots, so re-attach them from the fact.
        feature.policies = fact.policies.clone();
        feature.commands = fact.commands.clone();

        // AGGREGATE-ROOT-UNKNOWN
        for finding in domain::aggregate_root_unknown::check(&feature, &fact.path) {
            let line = fact
                .aggregate_lines
                .get(&finding.aggregate)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::aggregate_root_unknown::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // AGGREGATE-CONTAINS-UNKNOWN
        for finding in domain::aggregate_contains_unknown::check(&feature, &fact.path) {
            let line = fact
                .aggregate_lines
                .get(&finding.aggregate)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::aggregate_contains_unknown::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // INVARIANT-PREDICATE-INVALID — covers both resource-scoped
        // and aggregate-scoped invariants.
        for finding in domain::invariant_predicate_invalid::check(&feature, &fact.path) {
            let line = match &finding.scope {
                domain::invariant_predicate_invalid::InvariantScope::Aggregate(a) => fact
                    .aggregate_lines
                    .get(a)
                    .copied()
                    .unwrap_or(fact.feature_line),
                domain::invariant_predicate_invalid::InvariantScope::Resource(_) => {
                    fact.feature_line
                }
            };
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::invariant_predicate_invalid::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // SLUG-UNIQUENESS-IMPLICIT — warning, not error.
        for finding in domain::slug_uniqueness_implicit::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: domain::slug_uniqueness_implicit::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // CONSTRAINT-UNIQUE-WHEN-001 — GAP-NEW-001 partial-index predicate
        // / column field-reference check. Error.
        for finding in domain::constraint_unique_when_invalid::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::constraint_unique_when_invalid::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // POLICY-PREDICATE-001 — GAP-09 input-value-predicate policy atom
        // `input.*` field-reference + atom-namespace check. Error.
        for finding in domain::policy_predicate_invalid::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::policy_predicate_invalid::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // COMPUTED-DATE-EXPR-001 — W3 GAP-03 `computed_date from <base>
        // offset <offset>` base/offset type-check. Error.
        for finding in domain::computed_date_expr_invalid::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::computed_date_expr_invalid::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // SCHEDULE-RULE-001 — W4 GAP-08 `schedule_rule from @fn.<name>(<arg>)
        // offset <offset>` binding-fn / rule-arg / offset type-check. Error.
        for finding in domain::schedule_rule_invalid::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::schedule_rule_invalid::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // RESOURCE-APPEND-ONLY-001 — W4 GAP-AUDIT-02 — a command updates or
        // deletes an `append_only` resource. Error.
        for finding in domain::resource_append_only_invalid::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::resource_append_only_invalid::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // REORDER-POSITION-FIELD-001 — W4 GAP-REORDER-01 — `reorder <Resource>
        // by <field>` position-field type-check. Error.
        for finding in domain::reorder_position_field_invalid::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::reorder_position_field_invalid::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        // MANY-THROUGH-ENDPOINT-001 — GAP-07 — `many_through <Junction> to
        // <Partner>` partner-endpoint resolution + payload-type legality. Error.
        for finding in domain::many_through_endpoint_001::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::many_through_endpoint_001::Finding::CODE.to_owned(),
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
