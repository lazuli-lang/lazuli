//! `resource_without_policy_hint` and `command_without_audit_hint`
//! — cross-checks that flag write commands without an `audit
//! default` block and resources whose features lack a `policies`
//! block.
//!
//! Plus two tiny shape predicates (`is_write_effect_command`,
//! `write_effect_resource`) that other audit rules consume.
//!
//! Lifted out of the `audit` god-file in the rails-style R9 split.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::doctor::{DoctorDiagnostic, DoctorSeverity, ResourceFact, Tier3FeatureFacts};

pub(crate) fn resource_policy_and_command_audit_hints(
    facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen_commands: BTreeSet<(PathBuf, String, String)> = BTreeSet::new();
    let mut seen_resources: BTreeSet<(PathBuf, String, String)> = BTreeSet::new();

    for feature in facts {
        let mut referenced_write_resources = BTreeSet::new();
        for command in &feature.commands {
            if is_write_effect_command(command) {
                if command.audit.is_none()
                    && seen_commands.insert((
                        feature.path.clone(),
                        feature.feature.clone(),
                        command.name.clone(),
                    ))
                {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: feature
                            .command_lines
                            .get(&command.name)
                            .copied()
                            .unwrap_or(feature.feature_line),
                        column: 1,
                        severity: DoctorSeverity::Hint,
                        code: "command_without_audit_hint".to_owned(),
                        message: format!(
                            "command `{}.{}` is write-effect but has no `audit default` declared ÔÇö write actions without audit are invisible to compliance. Add `audit default` on the command or `audit_default` in feature defaults.",
                            feature.feature, command.name
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }

                if let Some(resource) = write_effect_resource(command) {
                    let is_local_resource = match resource.feature.as_deref() {
                        Some(owner) => owner == feature.feature,
                        None => true,
                    };
                    if is_local_resource {
                        referenced_write_resources.insert(resource.name.clone());
                    }
                }
            }
        }

        if feature.policies_declared || referenced_write_resources.is_empty() {
            continue;
        }

        let Some(resources) = feature_resources.get(&feature.feature) else {
            continue;
        };
        for resource in referenced_write_resources {
            let Some(resource_fact) = resources.get(&resource) else {
                continue;
            };
            if !seen_resources.insert((
                resource_fact.path.clone(),
                feature.feature.clone(),
                resource.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                path: resource_fact.path.clone(),
                line: if resource_fact.line == 0 {
                    feature.feature_line
                } else {
                    resource_fact.line
                },
                column: 1,
                severity: DoctorSeverity::Hint,
                code: "resource_without_policy_hint".to_owned(),
                message: format!(
                    "feature `{}` declares resource `{}` with no `policies` block ÔÇö every write command implicitly gets the default policy. Add an explicit `policies` block to make access control auditable.",
                    feature.feature, resource
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

/// 0004 — `defaults_hoist_rate_limit_hint` / `defaults_hoist_audit_hint`.
///
/// Fires when a feature spells an identical `rate_limit` (or `audit`) on
/// **≥3** commands while NOT already hoisting it into the `defaults`
/// block — the signal that the value should be declared once via
/// `defaults rate_limit "<spec>"` / `defaults audit default` and inherited
/// (spec 0004, deep-link `docs/lazuli_way/feature-defaults.md`).
///
/// Why ≥3 and why the `defaults_*` guard: the inheritance pass bakes a
/// hoisted default onto every command at lowering, so a migrated feature's
/// IR commands all carry the value even though source has zero repeats.
/// Guarding on `feature.defaults_rate_limit` / `defaults_audit` makes the
/// hint fire ONLY on the un-hoisted, literally-repeated source — never on a
/// feature that already did the hoist. Two identical commands are common
/// and benign; ≥3 is where the hoist clearly pays for itself.
pub(crate) fn defaults_hoist_hints(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    const THRESHOLD: usize = 3;
    let mut diagnostics = Vec::new();

    for feature in facts {
        // rate_limit hoist hint — only when the feature has NOT already
        // hoisted it. `RateLimitSpec.default` is the string spec; group
        // commands by identical spec.
        if !feature.defaults_rate_limit {
            let mut by_spec: BTreeMap<String, usize> = BTreeMap::new();
            for command in &feature.commands {
                if let Some(spec) = command.rate_limit.as_ref() {
                    // Only the simple single-string form is hoistable; an
                    // env-qualified `by_env` spec stays per-command.
                    if spec.by_env.is_empty() && !spec.default.is_empty() {
                        *by_spec.entry(spec.default.clone()).or_default() += 1;
                    }
                }
            }
            if let Some((spec, count)) = by_spec.into_iter().max_by_key(|(_, c)| *c)
                && count >= THRESHOLD
            {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Hint,
                    code: "defaults_hoist_rate_limit_hint".to_owned(),
                    message: format!(
                        "feature `{}` repeats the same `rate_limit \"{}\"` on {} commands. Hoist it into the feature `defaults` block (`defaults rate_limit \"{}\"`) and let each command inherit; override only where a command differs. See docs/lazuli_way/feature-defaults.md.",
                        feature.feature, spec, count, spec
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // audit hoist hint — only when the feature has NOT already hoisted
        // it. Count commands carrying the canonical `audit default`
        // (subjects == ["default"]); that is the only hoistable shape.
        if !feature.defaults_audit {
            let count = feature
                .commands
                .iter()
                .filter(|command| is_default_audit_command(command))
                .count();
            if count >= THRESHOLD {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Hint,
                    code: "defaults_hoist_audit_hint".to_owned(),
                    message: format!(
                        "feature `{}` repeats `audit default` on {} commands. Hoist it into the feature `defaults` block (`defaults audit default`) and let each command inherit; use `audit none` to opt a command out. See docs/lazuli_way/feature-defaults.md.",
                        feature.feature, count
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

    diagnostics
}

/// True when a command's audit is the canonical `audit default` shape
/// (subjects == `["default"]`, no extra subjects). Matches the codegen
/// `is_default_audit` predicate so the hint counts exactly the commands a
/// `defaults audit default` hoist would absorb.
fn is_default_audit_command(command: &lazuli_ir::Command) -> bool {
    command.audit.as_ref().is_some_and(|audit| {
        let subjects: Vec<&str> = audit
            .subjects
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        subjects.len() == 1 && subjects[0].eq_ignore_ascii_case("default")
    })
}

pub(crate) fn is_write_effect_command(command: &lazuli_ir::Command) -> bool {
    matches!(
        command.kind,
        lazuli_ir::CommandKind::Create
            | lazuli_ir::CommandKind::Update
            | lazuli_ir::CommandKind::Delete
    )
}

pub(crate) fn write_effect_resource(
    command: &lazuli_ir::Command,
) -> Option<&lazuli_ir::QualifiedName> {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => Some(&effect.resource),
        lazuli_ir::CommandEffect::Updates(effect) => Some(&effect.resource),
        lazuli_ir::CommandEffect::Deletes(effect) => Some(&effect.resource),
        lazuli_ir::CommandEffect::Reorders(effect) => Some(&effect.resource),
        lazuli_ir::CommandEffect::Returns(_) | lazuli_ir::CommandEffect::None => None,
    }
}
