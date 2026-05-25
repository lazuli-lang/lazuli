//! Plan-and-gate vocabulary — packaged feature/limit catalogs + the gate
//! directives that consume them.
//!
//! Two halves, both lowered from `app.lzi` and consumed by codegen + doctor
//! + LSP through one IR shape:
//!
//! 1. **Plan catalog** ([`PlanCatalog`], [`Plan`], [`PlanLimit`],
//!    [`PlanLimitValue`], [`TrialPolicy`]) — the package's `plan <name>`
//!    blocks closed-set, sorted, cross-reference-expanded. Producing this
//!    catalog is the analyzer's job; the IR is the wire shape.
//! 2. **Gate directives** ([`Gate`]) — what a command/api/agent declares
//!    when it wants the runtime to check the catalog at call time
//!    (`gate behind plan.feature: ...` / `gate quota plan.limit: ...`).
//!
//! The aggregation context (`PlanGateFacts` in `lazuli_analyzer`) is **not**
//! a slot on `Module` / `Feature` — it's a one-pass projection over `.lzi`
//! source. That keeps the IR's struct-literal call sites stable while still
//! letting consumers share a single derived view.
//!
//! ## Subscription anchor
//!
//! [`SubscriptionAnchor`] tells the plan-and-gate runtime which feature
//! edge points at the subscription resource. Lifted from
//! `subscription resource <feature>.<field>` in `app.lzi`. The optional
//! `tenancy_axis` parity hint is empty for single-tenant apps.
//!
//! ## Auto-photo command marker
//!
//! [`SynthesizedFromCapFile`] + [`AutoPhotoCommandRole`] are a sub-vocab
//! the analyzer attaches to commands it **auto-derived** from a
//! `@cap.File(...)` resource field (proposal FR-3a). The marker records
//! the source field's coordinates so codegen can wire the runtime
//! auto-photo helper without re-walking the IR.
//!
//! ## See also
//!
//! - `docs/proposals/plan-and-gate-vocab.md` (PG.A + PG.B) — full design
//! - `lazuli_analyzer::plan_gate_facts` — the aggregation pass
//! - [`crate::SpanRef`] — source-map back-link present on every node here

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// Closed plan catalog lifted from the package's `plan <name>` blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanCatalog {
    /// Plans declared in the package, sorted by name for deterministic
    /// JSON output.
    pub plans: Vec<Plan>,
    /// Union of every plan's feature set (sorted).
    pub feature_catalog: Vec<String>,
    /// Union of every plan's limit names (sorted).
    pub limit_catalog: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub name: String,
    /// Closed feature set (sorted) after cross-plan reference expansion.
    pub features: Vec<String>,
    /// Closed limit map (sorted by name) after cross-plan reference
    /// expansion.
    pub limits: Vec<PlanLimit>,
    /// Optional trial revert policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial: Option<TrialPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanLimit {
    pub name: String,
    pub value: PlanLimitValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PlanLimitValue {
    Integer(u64),
    Unlimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialPolicy {
    /// Raw duration literal (e.g. `"14d"`).
    pub duration: String,
    /// The plan to revert to after the trial elapses.
    pub then_plan: String,
}

/// PG.A/B — subscription anchor lifted from `app.lzi`
/// `subscription resource <feature>.<field>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionAnchor {
    /// The feature that owns the subscription edge (e.g. `users`).
    pub feature: String,
    /// The field/edge on the parent resource that points to the
    /// subscription resource (e.g. `subscription`).
    pub field: String,
    /// Optional `tenancy <axis>` parity hint. Empty for single-tenant
    /// apps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenancy_axis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Gate directive lifted onto a callable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Gate {
    /// `gate behind plan.feature: <name>` — boolean check.
    Behind { feature: String },
    /// `gate quota plan.limit: <name>` — counter check.
    Quota { limit: String },
}

/// FR-3a — marker carried on commands the analyzer auto-derived from
/// a `@cap.File(...)` resource field. Records the source field's
/// coordinates so codegen can wire the runtime auto-photo helper
/// without re-walking the IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SynthesizedFromCapFile {
    /// Resource name (PascalCase) carrying the `@cap.File` field.
    pub resource: String,
    /// Field name (snake_case) on the resource.
    pub field: String,
    /// Which of the 4 canonical command roles this is.
    pub role: AutoPhotoCommandRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoPhotoCommandRole {
    Request,
    Confirm,
    Clear,
    GetUrl,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_limit_value_integer_serializes_with_kind_value_envelope() {
        let json = serde_json::to_string(&PlanLimitValue::Integer(42)).unwrap();
        assert!(json.contains("\"kind\":\"Integer\""), "got: {json}");
        assert!(json.contains("\"value\":42"));
    }

    #[test]
    fn plan_limit_value_unlimited_round_trip() {
        let v = PlanLimitValue::Unlimited;
        let json = serde_json::to_string(&v).unwrap();
        let back: PlanLimitValue = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
        assert!(json.contains("\"kind\":\"Unlimited\""), "got: {json}");
    }

    #[test]
    fn gate_behind_round_trip() {
        let g = Gate::Behind {
            feature: "premium_export".into(),
        };
        let json = serde_json::to_string(&g).unwrap();
        let back: Gate = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
        assert!(json.contains("\"kind\":\"Behind\""), "got: {json}");
    }

    #[test]
    fn gate_quota_round_trip() {
        let g = Gate::Quota {
            limit: "monthly_reports".into(),
        };
        let json = serde_json::to_string(&g).unwrap();
        let back: Gate = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
        assert!(json.contains("\"kind\":\"Quota\""), "got: {json}");
    }

    #[test]
    fn auto_photo_role_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&AutoPhotoCommandRole::GetUrl).unwrap(),
            "\"get_url\""
        );
    }

    #[test]
    fn plan_catalog_default_is_empty() {
        let catalog = PlanCatalog::default();
        assert!(catalog.plans.is_empty());
        assert!(catalog.feature_catalog.is_empty());
        assert!(catalog.limit_catalog.is_empty());
    }
}
