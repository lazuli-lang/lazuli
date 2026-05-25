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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PlanFeatureRefAst {
    Ident(String),
    CrossPlan(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PlanLimitRefAst {
    Integer { name: String, value: u64 },
    Unlimited { name: String },
    CrossPlan(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanTrialAst {
    pub duration: String,
    pub then_plan: String,
    pub span: Span,
}

/// `gate` directive on a callable. Two closed forms in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum GateDirectiveAst {
    Behind { feature: String, span: Span },
    Quota { limit: String, span: Span },
}

impl GateDirectiveAst {
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
