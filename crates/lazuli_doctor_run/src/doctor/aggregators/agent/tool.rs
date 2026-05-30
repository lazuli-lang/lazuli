//! `agent.tools` policy / write-guard / PII-safety diagnostics.
//!
//! Diagnostic ids:
//!   - `agent_tool_policy_diagnostics` (error) — agent's policy is
//!     weaker than a tool's declared policy.
//!   - `agent_tool_write_unguarded_diagnostics` (error) — a write
//!     tool is dispatched without either an agent `safety`
//!     validator or an `approval` block on the target command.
//!   - `agent_pii_unsafetied_warning` (warning) — a tool resolving
//!     to a `@pii.*` class is dispatched without any agent safety
//!     validator.
//!
//! `resolve_tool` produces the (label, effect, policy, pii) shape
//! every guard above consumes. `policy_atoms_more_restrictive` is
//! the conservative comparator that fires
//! `agent_tool_policy_diagnostics` on the obvious cases.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir as ir;

use crate::doctor::parsers::{format_agent_policy, tool_kind_word};
use crate::doctor::{
    AgentFacts, DoctorAppRegistry, DoctorDiagnostic, DoctorSeverity, FeatureSymbols,
    Tier3FeatureFacts,
};

pub(crate) fn agent_tool_diagnostics(
    agents: &[AgentFacts],
    feature_symbols: &BTreeMap<String, FeatureSymbols>,
    registry: Option<&DoctorAppRegistry>,
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let registry_tools: BTreeMap<String, &lazuli_ir::RegistryToolEntry> = registry
        .map(|r| {
            r.manifest
                .tools
                .iter()
                .map(|t| (t.name.clone(), t))
                .collect()
        })
        .unwrap_or_default();

    // Cut A.9: write-tool guard accepts `approval` on the target
    // command as an alternative to the agent's `safety` validator.
    // Build a quick lookup keyed by (feature, command) so the guard
    // check resolves per-tool in O(1). Sourced from IR
    // `Command.approval` populated by `lower_feature_skeleton`;
    // Phase L Tier 4b retired the `CommandApprovalFact` text-walker.
    let approval_index: BTreeSet<(String, String)> = tier3_facts
        .iter()
        .flat_map(|f| {
            let feature = f.feature.clone();
            f.commands
                .iter()
                .filter(|c| c.approval.is_some())
                .map(move |c| (feature.clone(), c.name.clone()))
        })
        .collect();

    for fact in agents {
        let agent = &fact.agent;
        let agent_safety_empty = agent.safety.is_empty();
        let mut has_unguarded_write_tool = false;
        let mut has_pii_tool = false;
        let agent_policy_text = format_agent_policy(agent);

        for binding in &agent.tools {
            let (tool_label, resolved) =
                resolve_tool(fact, &binding.reference, feature_symbols, &registry_tools);

            if resolved.effect == ResolvedToolEffect::Write {
                // Check whether the target command carries an
                // `approval` block — that satisfies the write-tool
                // guard for this binding, regardless of the agent's
                // own `safety` list.
                let approved = match &binding.reference {
                    lazuli_ir::QualifiedToolRef::Local {
                        kind: lazuli_ir::ToolKind::Command,
                        name,
                    } => approval_index.contains(&(fact.feature.clone(), name.clone())),
                    lazuli_ir::QualifiedToolRef::CrossFeature {
                        feature,
                        kind: lazuli_ir::ToolKind::Command,
                        name,
                    } => approval_index.contains(&(feature.clone(), name.clone())),
                    _ => false,
                };
                if !approved {
                    has_unguarded_write_tool = true;
                }
            }
            if !resolved.pii_classes.is_empty() {
                has_pii_tool = true;
            }

            // agent_tool_policy_diagnostics: when the tool's policy is
            // known and stricter than the agent's, emit. Cut A keeps the
            // comparison conservative: we report a gap only when both
            // sides resolve and the tool's policy is *more* restrictive
            // by surface (atom set is a strict superset). Full lattice
            // ranking lands when the policy lattice helper migrates here.
            if let Some(tool_policy) = &resolved.policy
                && policy_atoms_more_restrictive(tool_policy, &agent_policy_text)
            {
                diagnostics.push(DoctorDiagnostic {
                        path: fact.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_tool_policy_diagnostics".to_owned(),
                        message: format!(
                            "agent `{}` declares policy `{}`, but tool `{}` requires `{}` — agent policy must be at least as strict as every tool.",
                            agent.name,
                            agent_policy_text,
                            tool_label,
                            tool_policy,
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
            }
        }

        // agent_tool_write_unguarded_diagnostics: every write tool
        // must be guarded by either the agent's `safety` validator or
        // the target command's `approval` block (Cut A.9 extension).
        // `has_unguarded_write_tool` only stays true for write tools
        // whose command has no approval — agent.safety is the
        // fallback guard for those.
        if has_unguarded_write_tool && agent_safety_empty {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "agent_tool_write_unguarded_diagnostics".to_owned(),
                message: format!(
                    "agent `{}` dispatches a `write` tool with neither `safety @validator.<name>` on the agent nor `approval` on the target command; Cut A requires at least one guard for write-effect tools.",
                    agent.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // agent_pii_unsafetied_warning: any PII-bearing tool plus an
        // empty safety list emits a warning.
        if has_pii_tool && agent_safety_empty {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "agent_pii_unsafetied_warning".to_owned(),
                message: format!(
                    "agent `{}` invokes a tool that resolves to a `@pii.*` class but declares no `safety @validator.<name>`; consider adding a scrub validator.",
                    agent.name
                ),
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedToolEffect {
    Read,
    Write,
    Unknown,
}

#[derive(Debug, Clone)]
struct ResolvedTool {
    effect: ResolvedToolEffect,
    policy: Option<String>,
    pii_classes: Vec<String>,
}

pub(super) fn resolve_tool(
    fact: &AgentFacts,
    reference: &ir::QualifiedToolRef,
    feature_symbols: &BTreeMap<String, FeatureSymbols>,
    registry_tools: &BTreeMap<String, &lazuli_ir::RegistryToolEntry>,
) -> (String, ResolvedTool) {
    match reference {
        ir::QualifiedToolRef::Adapter { dotted } => {
            let key = dotted.join(".");
            let label = format!("@tool.{key}");
            let resolved = registry_tools
                .get(&key)
                .map(|entry| ResolvedTool {
                    effect: match entry.effect {
                        ir::ToolEffect::Read => ResolvedToolEffect::Read,
                        ir::ToolEffect::Write => ResolvedToolEffect::Write,
                    },
                    policy: None,
                    pii_classes: entry.pii_classes.iter().map(|q| q.name.clone()).collect(),
                })
                .unwrap_or(ResolvedTool {
                    effect: ResolvedToolEffect::Unknown,
                    policy: None,
                    pii_classes: Vec::new(),
                });
            (label, resolved)
        }
        ir::QualifiedToolRef::Local { kind, name }
        | ir::QualifiedToolRef::CrossFeature { kind, name, .. } => {
            let owning_feature = match reference {
                ir::QualifiedToolRef::CrossFeature { feature, .. } => feature.clone(),
                _ => fact.feature.clone(),
            };
            let kind_word = tool_kind_word(*kind);
            let label = format!("{}.{}.{}", owning_feature, kind_word, name);
            let symbols = feature_symbols.get(&owning_feature);
            let resolved = match (*kind, symbols) {
                (ir::ToolKind::Command, Some(syms)) => syms
                    .commands
                    .get(name)
                    .map(|cmd| ResolvedTool {
                        effect: ResolvedToolEffect::Write,
                        policy: cmd.policy.clone(),
                        pii_classes: Vec::new(),
                    })
                    .unwrap_or(ResolvedTool {
                        effect: ResolvedToolEffect::Write,
                        policy: None,
                        pii_classes: Vec::new(),
                    }),
                (
                    ir::ToolKind::QueryList
                    | ir::ToolKind::QueryLookup
                    | ir::ToolKind::QuerySql
                    | ir::ToolKind::QueryView
                    | ir::ToolKind::QueryUnspecified,
                    _,
                ) => {
                    // Phase L Tier 4b — the `FeatureSymbols.queries`
                    // text-walker retired. Queries are always read-effect
                    // for the tool resolver; query-level `policy`
                    // declarations are a future extension (today, queries
                    // inherit feature-level `policies` via the analyzer,
                    // which the tool resolver does not consume).
                    ResolvedTool {
                        effect: ResolvedToolEffect::Read,
                        policy: None,
                        pii_classes: Vec::new(),
                    }
                }
                (ir::ToolKind::Command, None) => ResolvedTool {
                    effect: ResolvedToolEffect::Write,
                    policy: None,
                    pii_classes: Vec::new(),
                },
                _ => ResolvedTool {
                    effect: ResolvedToolEffect::Read,
                    policy: None,
                    pii_classes: Vec::new(),
                },
            };
            (label, resolved)
        }
    }
}

/// Conservative `more restrictive than` check: a policy is considered
/// stricter than the agent's when both texts parse as `@policy.<x>` and
/// the names diverge in a documented hierarchy. For Cut A we surface a
/// gap whenever the tool policy text is a non-empty stricter category
/// (`delete`, `update`) and the agent's is a weaker one (`read`).
///
/// Plan §5.4 punts the full lattice migration to a later cut; this stub
/// keeps the diagnostic firing for the obvious cases without false
/// positives.
pub(super) fn policy_atoms_more_restrictive(tool_policy: &str, agent_policy: &str) -> bool {
    let order = |text: &str| match text {
        s if s.contains("delete") => 3,
        s if s.contains("update") => 2,
        s if s.contains("create") => 1,
        s if s.contains("read") => 0,
        _ => 0,
    };
    order(tool_policy) > order(agent_policy)
}
