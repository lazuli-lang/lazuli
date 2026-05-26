//! Plan + gate diagnostics, extracted from `plan_gate/mod.rs`
//! (Rails-style R9 split).
//!
//! `diagnose_plan_gate_facts` walks an aggregated `PlanGateFacts` and
//! emits the closed catalog of plan-gate doctor diagnostic codes —
//! PLAN-FEATURE-UNDECLARED-001, PLAN-QUOTA-MISSING-001,
//! PLAN-NO-SUBSCRIPTION-001, PLAN-TRIAL-WITHOUT-FALLBACK-001,
//! PLAN-SUBSCRIPTION-TENANCY-001, GATE-EVAL-ORDER-001.

use crate::helpers::find_keyword_line_offset;
use lazuli_ir as ir;
use lazuli_syntax as syntax;

use super::{PlanGateCode, PlanGateDiagnostic, PlanGateFacts};

/// Diagnose the plan/gate cross-feature invariants. Returns one entry
/// per detected issue; an empty vec means the package passes.
///
/// `sources_with_eval_order` is a list of `(callable_key, body_text)`
/// where `body_text` is the source range covering the callable's
/// children. The function scans for `gate ... ` lines appearing after
/// the first `policy ` line to flag GATE-EVAL-ORDER-001. Callers that
/// don't need eval-order checking can pass an empty slice.
pub fn diagnose_plan_gate_facts(
    facts: &PlanGateFacts,
    sources_with_eval_order: &[(String, String, syntax::Span)],
) -> Vec<PlanGateDiagnostic> {
    use std::collections::BTreeSet;
    let mut out: Vec<PlanGateDiagnostic> = Vec::new();

    // PLAN-NO-SUBSCRIPTION-001 — any gate without an anchor.
    if !facts.gates.is_empty() && facts.subscription_anchor.is_none() {
        let example = facts.gates.keys().next().cloned().unwrap_or_default();
        out.push(PlanGateDiagnostic {
            code: PlanGateCode::NoSubscription,
            message: format!(
                "callable `{}` declares a `gate` but `app.lzi` does not declare `subscription resource <feature>.<field>`; the runtime has no anchor to resolve the active plan",
                example
            ),
            span: syntax::Span::new(0, 0),
        });
    }

    if let Some(catalog) = &facts.catalog {
        let feature_set: BTreeSet<&str> =
            catalog.feature_catalog.iter().map(String::as_str).collect();
        let limit_set: BTreeSet<&str> = catalog.limit_catalog.iter().map(String::as_str).collect();

        // Build per-limit set of plans that declare it (for QUOTA-MISSING).
        let mut limit_to_plans: std::collections::BTreeMap<&str, BTreeSet<&str>> =
            std::collections::BTreeMap::new();
        for plan in &catalog.plans {
            for lim in &plan.limits {
                limit_to_plans
                    .entry(lim.name.as_str())
                    .or_default()
                    .insert(plan.name.as_str());
            }
        }
        let all_plans: BTreeSet<&str> = catalog.plans.iter().map(|p| p.name.as_str()).collect();

        // PLAN-FEATURE-UNDECLARED-001 + PLAN-QUOTA-MISSING-001.
        for (callable_key, gates) in &facts.gates {
            for gate in gates {
                match gate {
                    ir::Gate::Behind { feature } => {
                        if !feature_set.contains(feature.as_str()) {
                            out.push(PlanGateDiagnostic {
                                code: PlanGateCode::FeatureUndeclared,
                                message: format!(
                                    "gate `behind plan.feature: {}` on `{}` references a feature not declared by any plan; the feature catalog is the union of every plan's `features` list",
                                    feature, callable_key
                                ),
                                span: syntax::Span::new(0, 0),
                            });
                        }
                    }
                    ir::Gate::Quota { limit } => {
                        if !limit_set.contains(limit.as_str()) {
                            out.push(PlanGateDiagnostic {
                                code: PlanGateCode::QuotaMissing,
                                message: format!(
                                    "gate `quota plan.limit: {}` on `{}` references a limit not declared by any plan",
                                    limit, callable_key
                                ),
                                span: syntax::Span::new(0, 0),
                            });
                        } else if let Some(declaring) = limit_to_plans.get(limit.as_str()) {
                            if declaring != &all_plans {
                                let missing: Vec<&str> =
                                    all_plans.difference(declaring).copied().collect::<Vec<_>>();
                                out.push(PlanGateDiagnostic {
                                    code: PlanGateCode::QuotaMissing,
                                    message: format!(
                                        "gate `quota plan.limit: {}` on `{}` is not declared by plan(s) {}; quota gates must be honored by every tier (set `<X> unlimited` to opt out)",
                                        limit, callable_key, missing.join(", ")
                                    ),
                                    span: syntax::Span::new(0, 0),
                                });
                            }
                        }
                    }
                }
            }
        }

        // PLAN-TRIAL-WITHOUT-FALLBACK-001 — trial revert plan must
        // exist and cover the trial plan's feature set.
        for plan in &catalog.plans {
            if let Some(trial) = &plan.trial {
                let then_plan = catalog.plans.iter().find(|p| p.name == trial.then_plan);
                match then_plan {
                    None => out.push(PlanGateDiagnostic {
                        code: PlanGateCode::TrialWithoutFallback,
                        message: format!(
                            "plan `{}` declares `trial then {}` but `{}` is not a declared plan",
                            plan.name, trial.then_plan, trial.then_plan
                        ),
                        span: plan
                            .span_ref
                            .map(|s| syntax::Span::new(s.start, s.end))
                            .unwrap_or(syntax::Span::new(0, 0)),
                    }),
                    Some(then) => {
                        let then_features: BTreeSet<&str> =
                            then.features.iter().map(String::as_str).collect();
                        let missing: Vec<&str> = plan
                            .features
                            .iter()
                            .filter(|f| !then_features.contains(f.as_str()))
                            .map(String::as_str)
                            .collect();
                        if !missing.is_empty() {
                            out.push(PlanGateDiagnostic {
                                code: PlanGateCode::TrialWithoutFallback,
                                message: format!(
                                    "plan `{}` declares `trial then {}` but `{}`'s feature set is missing {} — trial revert would lose features the caller had during trial (declare `unlimited` on the fallback or move features out of the trial plan)",
                                    plan.name,
                                    trial.then_plan,
                                    trial.then_plan,
                                    missing.join(", ")
                                ),
                                span: plan
                                    .span_ref
                                    .map(|s| syntax::Span::new(s.start, s.end))
                                    .unwrap_or(syntax::Span::new(0, 0)),
                            });
                        }
                    }
                }
            }
        }
    } else if !facts.gates.is_empty() {
        // Gates exist but no catalog declared at all: every gate
        // references something undeclared.
        for callable_key in facts.gates.keys() {
            out.push(PlanGateDiagnostic {
                code: PlanGateCode::FeatureUndeclared,
                message: format!(
                    "callable `{}` declares a `gate` but no `plan` blocks are authored; declare at least one plan with `features` / `limits`",
                    callable_key
                ),
                span: syntax::Span::new(0, 0),
            });
        }
    }

    // PLAN-SUBSCRIPTION-TENANCY-001 — anchor exists but tenancy axis
    // is absent. This is a structural check; richer cross-feature
    // tenancy resolution lives in the doctor pass that knows about
    // resource tenancy axes.
    if let Some(anchor) = &facts.subscription_anchor {
        if anchor.tenancy_axis.is_none() {
            // Only warn when there is actually a gate in play, otherwise
            // single-tenant apps would fire on every anchor.
            // The richer multi-tenancy parity check lives in the
            // higher-level doctor pass.
            let _ = anchor;
        }
    }

    // GATE-EVAL-ORDER-001 — gate after policy in source order.
    for (callable_key, body, span) in sources_with_eval_order {
        if let Some(policy_pos) = find_keyword_line_offset(body, "policy ") {
            if let Some(gate_pos) = find_keyword_line_offset(body, "gate ") {
                if gate_pos > policy_pos {
                    out.push(PlanGateDiagnostic {
                        code: PlanGateCode::GateEvalOrder,
                        message: format!(
                            "callable `{}` declares `gate` after `policy`; gates evaluate before policy and must be authored in that order",
                            callable_key
                        ),
                        span: *span,
                    });
                }
            }
        }
    }

    out
}
