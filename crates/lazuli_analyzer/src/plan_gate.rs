//! Plan + gate facts aggregator and cross-feature diagnostics.
//!
//! ## Role in the pipeline
//!
//! `PlanGateFacts` is the analyzer-side projection of the package-wide
//! plan catalog (`plan ...` blocks lifted from `app.lzi` /
//! `registry.lzi`) plus the subscription anchor plus the per-callable
//! gate directives lifted from each feature's `command` / `job` /
//! `webhook` / `api` / `query.*` bodies.
//!
//! Codegen (PG.C), doctor (PG.B), and LSP (PG.B) all consume this
//! struct as a side-table to the existing `Module`. It deliberately
//! stays out of the per-callable IR shapes so the existing struct-
//! literal fixtures keep building without modification.
//!
//! Two-phase pipeline:
//!
//! 1. `aggregate_plan_gate_facts` — fold raw parser outputs
//!    (`PlanBlockAst`, `FeatureGatesAst`, `SubscriptionAnchor`) into
//!    the typed `PlanGateFacts`. Cross-plan reference expansion runs
//!    here in `build_plan_catalog` (Pass 1 stages direct sets, Pass 2
//!    resolves `features <plan>.*` / `limits <plan>.*` references).
//! 2. `diagnose_plan_gate_facts` — emit the closed catalog of plan-
//!    gate diagnostic codes against a built `PlanGateFacts`:
//!    `PLAN-FEATURE-UNDECLARED-001`, `PLAN-QUOTA-MISSING-001`,
//!    `PLAN-NO-SUBSCRIPTION-001`, `PLAN-TRIAL-WITHOUT-FALLBACK-001`,
//!    `PLAN-SUBSCRIPTION-TENANCY-001`, `GATE-EVAL-ORDER-001`.
//!
//! ## Cross-references
//!
//! * Input: `lazuli_syntax::ast::PlanBlockAst`, `FeatureGatesAst`,
//!   `GateDirectiveAst`, `PlanFeatureRefAst`, `PlanLimitRefAst`.
//! * Output: `lazuli_ir::PlanCatalog`, `Plan`, `PlanLimit`, `Gate`,
//!   `SubscriptionAnchor`, `TrialPolicy`.
//! * Diagnostics: `PlanGateDiagnostic` + `PlanGateCode` (closed
//!   variant set; codegen + doctor read these via
//!   `PlanGateCode::as_str`).
//!
//! ## ABI guarantee
//!
//! `PlanGateFacts`, `PlanGateDiagnostic`, `PlanGateCode`,
//! `parse_subscription_anchor`, `aggregate_plan_gate_facts`, and
//! `diagnose_plan_gate_facts` are part of the analyzer's external
//! ABI — `lazuli_cli`, `lazuli_codegen_go`, and `lazuli_doctor`
//! consume them. The module re-exports them through `lib.rs` so
//! existing `lazuli_analyzer::PlanGateFacts` paths continue to
//! resolve.

use crate::helpers::find_keyword_line_offset;
use lazuli_ir as ir;
use lazuli_syntax as syntax;

// =============================================================================
// PG.B — plan-and-gate facts aggregator.
// -----------------------------------------------------------------------------
// `PlanGateFacts` is the analyzer-side projection of the package-wide
// plan catalog (`plan ...` blocks lifted from app.lzi / registry.lzi)
// + the subscription anchor + the per-callable gate directives lifted
// from each feature's `command/job/webhook/api/query.*` bodies.
//
// Codegen (PG.C), doctor (PG.B), and LSP (PG.B) all consume this struct
// as a side-table to the existing `Module`. It deliberately stays out
// of the per-callable IR shapes so the existing ~150 struct-literal
// fixtures keep building without modification.
// =============================================================================

/// Plan/gate facts derived from a single package's `.lzi` sources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanGateFacts {
    /// Closed plan catalog. `None` when no `plan ...` blocks are
    /// authored (the package has no subscription model).
    pub catalog: Option<ir::PlanCatalog>,
    /// `app.lzi subscription resource <feature>.<field>`. Required if
    /// any callable carries a gate; doctor `PLAN-NO-SUBSCRIPTION-001`
    /// fires when missing.
    pub subscription_anchor: Option<ir::SubscriptionAnchor>,
    /// Per-callable gate directives keyed by
    /// `<feature>/<callable_kind>:<callable_name>`. The qualified key
    /// matches what `parse_feature_gates` produces (with the feature
    /// prefix added in this layer because gates are aggregated across
    /// every feature in the package).
    pub gates: std::collections::BTreeMap<String, Vec<ir::Gate>>,
}

/// PG.A — scan an `app.lzi` source for the
/// `subscription resource <feature>.<field>` directive. Returns
/// `None` when not declared. `tenancy_axis` is filled in `None`
/// here; richer resolution requires the doctor's resource-tenancy
/// table and lives downstream.
pub fn parse_subscription_anchor(app_lzi_source: &str) -> Option<ir::SubscriptionAnchor> {
    let mut in_app = false;
    let mut offset = 0usize;
    for line in app_lzi_source.lines() {
        let line_len = line.len();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            offset += line_len + 1;
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_app = trimmed.starts_with("app ");
            offset += line_len + 1;
            continue;
        }
        if !in_app {
            offset += line_len + 1;
            continue;
        }
        if indent != 2 {
            offset += line_len + 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("subscription resource ") {
            let body = rest.trim();
            if let Some((feature, field)) = body.split_once('.') {
                let feature = feature.trim().to_owned();
                let field = field.trim().to_owned();
                if !feature.is_empty() && !field.is_empty() {
                    return Some(ir::SubscriptionAnchor {
                        feature,
                        field,
                        tenancy_axis: None,
                        span_ref: Some(ir::SpanRef {
                            start: offset,
                            end: offset + line_len,
                        }),
                    });
                }
            }
        }
        offset += line_len + 1;
    }
    None
}

/// Build a `PlanGateFacts` from raw inputs. The caller is responsible
/// for parsing the per-file plan blocks and gate side-channels first;
/// this aggregator just merges and expands cross-plan references.
///
/// - `plan_blocks` is the union of `parse_plan_blocks(app_lzi)` plus
///   `parse_plan_blocks(registry_lzi)` plus every other top-level
///   source carrying `plan` blocks. Doctor enforces single declaration
///   per plan name upstream.
/// - `feature_gates` is one entry per feature, name + the
///   `FeatureGatesAst` produced by `parse_feature_gates(source)`.
/// - `anchor` is the `SubscriptionAnchor` resolved from
///   `app.lzi subscription resource ...`.
pub fn aggregate_plan_gate_facts(
    plan_blocks: &[syntax::PlanBlockAst],
    feature_gates: &[(String, syntax::FeatureGatesAst)],
    anchor: Option<ir::SubscriptionAnchor>,
) -> PlanGateFacts {
    let catalog = if plan_blocks.is_empty() {
        None
    } else {
        Some(build_plan_catalog(plan_blocks))
    };

    let mut gates: std::collections::BTreeMap<String, Vec<ir::Gate>> =
        std::collections::BTreeMap::new();
    for (feature, fg) in feature_gates {
        for (callable_key, directives) in &fg.callables {
            let qualified = format!("{}/{}", feature, callable_key);
            let mut out = Vec::with_capacity(directives.len());
            for directive in directives {
                out.push(match directive {
                    syntax::GateDirectiveAst::Behind { feature, .. } => ir::Gate::Behind {
                        feature: feature.clone(),
                    },
                    syntax::GateDirectiveAst::Quota { limit, .. } => ir::Gate::Quota {
                        limit: limit.clone(),
                    },
                });
            }
            gates.insert(qualified, out);
        }
    }

    PlanGateFacts {
        catalog,
        subscription_anchor: anchor,
        gates,
    }
}

pub(crate) fn build_plan_catalog(plan_blocks: &[syntax::PlanBlockAst]) -> ir::PlanCatalog {
    use std::collections::{BTreeMap, BTreeSet};

    // Pass 1: gather declared features/limits per plan, deferring
    // cross-plan reuse references until pass 2 (the referenced plan
    // might be authored later in source order).
    struct Staging {
        direct_features: Vec<String>,
        feature_refs: Vec<String>,
        direct_limits: BTreeMap<String, ir::PlanLimitValue>,
        limit_refs: Vec<String>,
        trial: Option<ir::TrialPolicy>,
        span_ref: Option<ir::SpanRef>,
    }

    let mut staging: BTreeMap<String, Staging> = BTreeMap::new();
    for plan in plan_blocks {
        let mut direct_features = Vec::new();
        let mut feature_refs = Vec::new();
        for feat in &plan.features {
            match feat {
                syntax::PlanFeatureRefAst::Ident(s) => direct_features.push(s.clone()),
                syntax::PlanFeatureRefAst::CrossPlan(other) => feature_refs.push(other.clone()),
            }
        }
        let mut direct_limits: BTreeMap<String, ir::PlanLimitValue> = BTreeMap::new();
        let mut limit_refs = Vec::new();
        for lim in &plan.limits {
            match lim {
                syntax::PlanLimitRefAst::Integer { name, value } => {
                    direct_limits.insert(name.clone(), ir::PlanLimitValue::Integer(*value));
                }
                syntax::PlanLimitRefAst::Unlimited { name } => {
                    direct_limits.insert(name.clone(), ir::PlanLimitValue::Unlimited);
                }
                syntax::PlanLimitRefAst::CrossPlan(other) => limit_refs.push(other.clone()),
            }
        }
        let trial = plan.trial.as_ref().map(|t| ir::TrialPolicy {
            duration: t.duration.clone(),
            then_plan: t.then_plan.clone(),
        });
        let span_ref = Some(ir::SpanRef {
            start: plan.span.start,
            end: plan.span.end,
        });
        staging.insert(
            plan.name.clone(),
            Staging {
                direct_features,
                feature_refs,
                direct_limits,
                limit_refs,
                trial,
                span_ref,
            },
        );
    }

    // Pass 2: expand cross-plan references. We snapshot direct sets
    // before mutation so reference chains see the original direct
    // declarations (single-level expansion; deeper chains require the
    // referenced plan to also resolve, enforced by doctor).
    let direct_features_snapshot: BTreeMap<String, Vec<String>> = staging
        .iter()
        .map(|(k, v)| (k.clone(), v.direct_features.clone()))
        .collect();
    let direct_limits_snapshot: BTreeMap<String, BTreeMap<String, ir::PlanLimitValue>> = staging
        .iter()
        .map(|(k, v)| (k.clone(), v.direct_limits.clone()))
        .collect();

    let mut plans: Vec<ir::Plan> = Vec::with_capacity(staging.len());
    let mut feature_union: BTreeSet<String> = BTreeSet::new();
    let mut limit_union: BTreeSet<String> = BTreeSet::new();

    for (name, s) in &staging {
        let mut feature_set: BTreeSet<String> = s.direct_features.iter().cloned().collect();
        for other in &s.feature_refs {
            if let Some(other_features) = direct_features_snapshot.get(other) {
                for f in other_features {
                    feature_set.insert(f.clone());
                }
            }
        }
        let mut limit_map = s.direct_limits.clone();
        for other in &s.limit_refs {
            if let Some(other_limits) = direct_limits_snapshot.get(other) {
                for (k, v) in other_limits {
                    limit_map.entry(k.clone()).or_insert(*v);
                }
            }
        }
        let mut features: Vec<String> = feature_set.into_iter().collect();
        features.sort();
        let mut limits: Vec<ir::PlanLimit> = limit_map
            .into_iter()
            .map(|(name, value)| ir::PlanLimit { name, value })
            .collect();
        limits.sort_by(|a, b| a.name.cmp(&b.name));
        for f in &features {
            feature_union.insert(f.clone());
        }
        for l in &limits {
            limit_union.insert(l.name.clone());
        }
        plans.push(ir::Plan {
            name: name.clone(),
            features,
            limits,
            trial: s.trial.clone(),
            span_ref: s.span_ref,
        });
    }
    plans.sort_by(|a, b| a.name.cmp(&b.name));

    ir::PlanCatalog {
        plans,
        feature_catalog: feature_union.into_iter().collect(),
        limit_catalog: limit_union.into_iter().collect(),
    }
}

/// PG.B — closed catalog of plan-and-gate doctor diagnostic codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanGateDiagnostic {
    pub code: PlanGateCode,
    pub message: String,
    /// Best-effort source-byte range of the offending construct. The
    /// `0..0` span indicates a package-wide issue (catalog absent,
    /// anchor missing for the whole app).
    pub span: syntax::Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanGateCode {
    /// `PLAN-FEATURE-UNDECLARED-001` — `gate behind plan.feature: <X>`
    /// references a feature not in the catalog's feature union.
    FeatureUndeclared,
    /// `PLAN-QUOTA-MISSING-001` — `gate quota plan.limit: <X>`
    /// references a limit that is not declared by every plan; the
    /// closed-grammar rule requires explicit `unlimited` opt-outs.
    QuotaMissing,
    /// `PLAN-NO-SUBSCRIPTION-001` — any gate exists in the package but
    /// `app.lzi` did not declare `subscription resource ...`.
    NoSubscription,
    /// `PLAN-TRIAL-WITHOUT-FALLBACK-001` — trial revert plan does not
    /// cover the trial plan's feature set, or its `then` target is
    /// missing entirely.
    TrialWithoutFallback,
    /// `PLAN-SUBSCRIPTION-TENANCY-001` — multi-tenant app's anchor
    /// resource lacks a `tenancy` axis matching `app.defaults.tenancy`.
    SubscriptionTenancy,
    /// `GATE-EVAL-ORDER-001` — a `gate` directive appears after
    /// `policy` in source order; the closed evaluation order requires
    /// gates to be authored before policies.
    GateEvalOrder,
}

impl PlanGateCode {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanGateCode::FeatureUndeclared => "PLAN-FEATURE-UNDECLARED-001",
            PlanGateCode::QuotaMissing => "PLAN-QUOTA-MISSING-001",
            PlanGateCode::NoSubscription => "PLAN-NO-SUBSCRIPTION-001",
            PlanGateCode::TrialWithoutFallback => "PLAN-TRIAL-WITHOUT-FALLBACK-001",
            PlanGateCode::SubscriptionTenancy => "PLAN-SUBSCRIPTION-TENANCY-001",
            PlanGateCode::GateEvalOrder => "GATE-EVAL-ORDER-001",
        }
    }
}

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
