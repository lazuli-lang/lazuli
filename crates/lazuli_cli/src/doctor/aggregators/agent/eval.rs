//! `agent.evals` diagnostics — determinism + ordered-operator type
//! checks.
//!
//! Diagnostic ids:
//!   - `eval_nondeterministic_warning` — `evals` declared but
//!     `temperature 0` or `seed <int>` is missing; eval cases run as
//!     informational until both are pinned.
//!   - `eval_ordered_op_invalid_diagnostics` (error) — a `<` / `<=`
//!     / `>` / `>=` assertion uses operands neither of which
//!     resolves to a numeric type (integer literal or one of the
//!     canonical numeric paths `.length` / `.count` / `.size`).
//!
//! `operand_resolves_numeric` is the best-effort numeric check used
//! by the ordered-op rule; closed-predicate IR doesn't carry
//! resolved types yet, so the helper accepts only the canonical
//! numeric paths plus integer literals.

use lazuli_ir as ir;

use crate::doctor::{AgentFacts, DoctorDiagnostic, DoctorSeverity};

pub(crate) fn agent_eval_diagnostics(agents: &[AgentFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for fact in agents {
        let agent = &fact.agent;

        if !agent.evals.is_empty() && (agent.temperature != Some(0.0) || agent.seed.is_none()) {
            let reason = if agent.temperature != Some(0.0) {
                "missing `temperature 0`"
            } else {
                "missing `seed <int>`"
            };
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "eval_nondeterministic_warning".to_owned(),
                message: format!(
                    "agent `{}` declares `evals` but the agent is non-deterministic ({}); cases run as informational results until both `temperature 0` and `seed <int>` are pinned.",
                    agent.name, reason,
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        for case in &agent.evals {
            for assertion in &case.assertions {
                if let ir::EvalPredicate::Closed(ir::Predicate::Comparison { left, op, right }) =
                    &assertion.predicate
                {
                    if matches!(
                        op,
                        ir::CompareOp::Lt
                            | ir::CompareOp::Le
                            | ir::CompareOp::Gt
                            | ir::CompareOp::Ge
                    ) && !operand_resolves_numeric(left)
                        && !operand_resolves_numeric(right)
                    {
                        diagnostics.push(DoctorDiagnostic {
                            path: fact.path.clone(),
                            line: fact.line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "eval_ordered_op_invalid_diagnostics".to_owned(),
                            message: format!(
                                "agent `{}` eval case `{}` uses an ordered operator but neither operand resolves to a numeric type; ordered comparisons require numeric refs (`<ref>.length`, `<ref>.count`, integer fields).",
                                agent.name, case.name,
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
    }

    diagnostics
}

/// Best-effort numeric-operand check. The closed-predicate IR doesn't
/// carry resolved types yet, so we accept the canonical numeric paths
/// (`<x>.length`, `<x>.count`) and integer literals. Everything else is
/// rejected as non-numeric — authors who hit a false positive can split
/// the case until type resolution arrives.
pub(crate) fn operand_resolves_numeric(expr: &ir::Expr) -> bool {
    match expr {
        ir::Expr::Integer(_) => true,
        ir::Expr::Path(path) => {
            let last = path.segments.last().map(String::as_str);
            matches!(last, Some("length") | Some("count") | Some("size"))
        }
        _ => false,
    }
}
