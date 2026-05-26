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

use crate::doctor::{
    DoctorDiagnostic, DoctorSeverity, ResourceFact, Tier3FeatureFacts,
};

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
                            "command `{}.{}` is write-effect but has no `audit default` declared — write actions without audit are invisible to compliance. Add `audit default` on the command or `audit_default` in feature defaults.",
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
                    "feature `{}` declares resource `{}` with no `policies` block — every write command implicitly gets the default policy. Add an explicit `policies` block to make access control auditable.",
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
        lazuli_ir::CommandEffect::Returns(_) | lazuli_ir::CommandEffect::None => None,
    }
}
