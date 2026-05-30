//! Initial code-generation plan for a parsed Lazuli module.
//!
//! Given a [`Module`] (the post-parse / post-lower IR root), the planner
//! produces a [`Plan`] enumerating the ordered backend + frontend codegen
//! steps that `lazuli build` will execute. The planner does not run the
//! steps — it only records what would run and a coarse [`Risk`] tag per
//! step so the CLI can surface high-risk operations to the user before
//! commit (cf. `lazuli build --dry-run`).
//!
//! Today the planner is intentionally minimal: every module produces the
//! same two-step plan (Go backend → React frontend), both at `Low` risk.
//! The structure exists so future codegen targets (RN mobile, OpenAPI
//! emit, SDK gen) can plug in without breaking the IR consumer surface.

// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
use lazuli_ir::Module;
use serde::{Deserialize, Serialize};

/// The full codegen plan for one parsed Lazuli module.
///
/// Carries the feature names that will be processed and the ordered list
/// of [`PlanStep`]s. Serializable so `lazuli inspect --plan` can emit the
/// same shape the CLI consumes internally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Names of every feature in the module, in declaration order.
    pub feature_names: Vec<String>,
    /// Ordered steps `lazuli build` will execute.
    pub steps: Vec<PlanStep>,
}

/// One discrete operation in a [`Plan`].
///
/// Steps are intentionally coarse — one per generator target, not one
/// per emitted file — so the user-facing plan stays a short bullet list
/// even for large modules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    /// Human-readable description (e.g. `"Generate Go backend target"`).
    pub label: String,
    /// Coarse risk classification surfaced to the user.
    pub risk: Risk,
}

/// Coarse risk classification for a [`PlanStep`].
///
/// Used by the CLI to decide whether to confirm a step interactively
/// (`Medium`/`High`) or run silently (`Low`). Not used for scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risk {
    /// Pure file emission to `dist/`. No external side effects.
    Low,
    /// Touches schema / migrations / runtime config. Reversible.
    Medium,
    /// Destructive or hard-to-reverse (drops a table, rotates a key).
    High,
}

/// Build the default [`Plan`] for a freshly-parsed [`Module`].
///
/// The current default is a fixed two-step plan: Go backend then React
/// frontend. Future iterations may branch on `module.features[].targets`
/// to add per-feature mobile / OpenAPI / SDK steps.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_ir::Module;
/// use lazuli_planner::{plan_initial_generation, Risk};
///
/// let module: Module = /* obtain from analyzer or parser */ unimplemented!();
/// let plan = plan_initial_generation(&module);
/// assert_eq!(plan.steps.len(), 2);
/// assert!(plan.steps.iter().all(|s| s.risk == Risk::Low));
/// ```
pub fn plan_initial_generation(module: &Module) -> Plan {
    Plan {
        feature_names: module
            .features
            .iter()
            .map(|feature| feature.name.clone())
            .collect(),
        steps: vec![
            PlanStep {
                label: "Generate Go backend target".to_owned(),
                risk: Risk::Low,
            },
            PlanStep {
                label: "Generate React frontend target".to_owned(),
                risk: Risk::Low,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_module() -> Module {
        serde_json::from_str(r#"{"features": []}"#)
            .expect("empty Module deserializes with empty features list")
    }

    #[test]
    fn empty_module_yields_two_step_low_risk_plan() {
        let plan = plan_initial_generation(&empty_module());
        assert!(plan.feature_names.is_empty());
        assert_eq!(plan.steps.len(), 2);
        assert!(plan.steps.iter().all(|s| s.risk == Risk::Low));
    }

    #[test]
    fn step_labels_name_backend_then_frontend() {
        let plan = plan_initial_generation(&empty_module());
        assert!(plan.steps[0].label.contains("Go"));
        assert!(plan.steps[1].label.contains("React"));
    }
}
