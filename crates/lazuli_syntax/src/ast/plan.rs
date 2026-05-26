//! Plan & Gate vocabulary AST (PG.A,
//! `docs/proposals/plan-and-gate-vocab.md`).
//!
//! `plan <name>` is a top-level package-wide block (sibling of
//! `feature`). It declares which features the plan unlocks and the
//! numeric limits the plan caps.
//!
//! `gate behind plan.feature: ...` / `gate quota plan.limit: ...` are
//! the per-callable directives. They are **not** stored on the callable
//! AST nodes; instead the parser lifts them into a side-channel
//! `FeatureGatesAst` map (one entry per `feature.callable`), returned by
//! `parse_feature_gates(source)`. The analyzer (PG.B) reads that map in
//! a sibling pass so the existing IR struct literals stay unchanged.
//!
//! `subscription resource <feature>.<field>` is a child of `app.lzi` and
//! lives in `crates/lazuli_cli/src/app_manifest.rs` — it is not part of
//! this AST surface.

use serde::{Deserialize, Serialize};

use super::Span;

/// Top-level `plan <name>` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanBlockAst {
    pub name: String,
    pub features: Vec<PlanFeatureRefAst>,
    pub limits: Vec<PlanLimitRefAst>,
    pub trial: Option<PlanTrialAst>,
    pub span: Span,
}

/// One `features <ref>` entry inside a [`PlanBlockAst`]. `Ident` is the
/// local form (`feature.foo`); `CrossPlan` references `<other_plan>.<feature>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PlanFeatureRefAst {
    /// `features foo` — local feature reference.
    Ident(String),
    /// `features other_plan.foo` — cross-plan reference (verbatim text).
    CrossPlan(String),
}

/// One `limits <name> = <value>` entry inside a [`PlanBlockAst`].
/// Three closed shapes covering integer caps, the `unlimited` keyword,
/// and cross-plan references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PlanLimitRefAst {
    /// `limits seats = 5` — closed integer cap.
    Integer { name: String, value: u64 },
    /// `limits seats = unlimited` — uncapped sentinel.
    Unlimited { name: String },
    /// `limits other_plan.seats` — cross-plan reference verbatim.
    CrossPlan(String),
}

/// `trial <duration> then <plan>` clause on a [`PlanBlockAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanTrialAst {
    /// `<duration>` verbatim (e.g. `14d`).
    pub duration: String,
    /// `then <plan>` — target plan once the trial expires.
    pub then_plan: String,
    pub span: Span,
}

/// `gate` directive on a callable. Two closed forms in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum GateDirectiveAst {
    /// `gate behind plan.<feature>` — opt-in gating by plan feature flag.
    Behind { feature: String, span: Span },
    /// `gate quota plan.<limit>` — quota cap referencing a plan limit.
    Quota { limit: String, span: Span },
}

impl GateDirectiveAst {
    /// Source span of the gate line — used by doctor + LSP diagnostics.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::{GateDirectiveAst, Span};
    ///
    /// let g = GateDirectiveAst::Behind {
    ///     feature: "premium".into(),
    ///     span: Span::new(120, 145),
    /// };
    /// assert_eq!(g.span().start, 120);
    /// ```
    pub fn span(&self) -> Span {
        match self {
            GateDirectiveAst::Behind { span, .. } => *span,
            GateDirectiveAst::Quota { span, .. } => *span,
        }
    }
}

/// PG.A — out-of-band map keyed by callable name, holding gate
/// directives lifted from each `command` / `job` / `webhook` / `api` /
/// `query.list` / `query.lookup` / `query.sql` block. Returned by
/// `parse_feature_gates(source)` so analyzers and codegen can read
/// gates without churning the existing surface AST struct literals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureGatesAst {
    /// Per-feature, per-callable gates. Outer key is the feature name;
    /// inner key is `command:<name>` / `job:<name>` / `webhook:<name>` /
    /// `api:<name>` / `query.list:<name>` / `query.lookup:<name>` /
    /// `query.sql:<name>`. The qualified-callable key is what doctor
    /// and codegen consume.
    pub callables: std::collections::BTreeMap<String, Vec<GateDirectiveAst>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_feature_ref_ident_serde_tagged() {
        let r = PlanFeatureRefAst::Ident("foo".into());
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["kind"], "Ident");
        assert_eq!(v["value"], "foo");
    }

    #[test]
    fn gate_directive_span_dispatches_per_variant() {
        let g = GateDirectiveAst::Quota {
            limit: "seats".into(),
            span: Span::new(10, 20),
        };
        assert_eq!(g.span(), Span::new(10, 20));
    }
}
