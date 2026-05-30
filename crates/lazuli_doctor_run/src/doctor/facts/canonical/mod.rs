//! Canonical `.lzi` fact collectors.
//!
//! Walks each `DoctorFile` whose path is `.lzi` and projects:
//!
//! * `OperationalFacts` — features, apis, webhooks, jobs, env
//!   references, file-capability uses, integration requirements,
//!   external calls.
//! * `commands: BTreeMap<CommandKey, CommandPolicy>` — the typed
//!   command-policy map that powers
//!   `policy_reachability_diagnostics` and
//!   `command_route_binding_diagnostics`.
//!
//! Two layers cooperate:
//!
//! * The legacy text walker (`collect_operational_lzi_facts`) handles
//!   the indent-driven `.lzi` surface (apis / webhooks / jobs /
//!   `@cap.File` / `env.*` references) for items not yet projected
//!   through the IR.
//! * The IR-driven walkers (`populate_*_from_ir`) replaced the
//!   retired `collect_external_calls_in_block` and
//!   `collect_feature_commands` text-walkers. They consume the typed
//!   `lazuli_ir::Feature` (timeouts, retries, idempotency, route
//!   slots, policy refs) so the resulting facts carry typed shape
//!   instead of substring evidence.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

use std::collections::BTreeMap;

use lazuli_syntax::Span;

use crate::doctor::parsers::is_lzi_path;
use crate::doctor::scanners::{derive_feature_name, leading_spaces};
use crate::doctor::{
    CommandKey, CommandPolicy, CommandRouteSlot, DoctorFile, ExternalCallFact,
    IntegrationRequirementFact, OperationalFacts, SourceFact, collect_construct_lines,
    collect_file_capability_facts, line_col_for_offset, parse_integration_requirement,
    path_references,
};

pub(crate) fn collect_callable_bodies_for_eval_order(
    files: &[DoctorFile],
) -> Vec<(String, String, Span)> {
    let mut out = Vec::new();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let feature = derive_feature_name(&file.source).unwrap_or_else(|| {
            file.path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_owned()
        });
        // Walk source lines tracking callable header + indent.
        let mut current: Option<(String, usize, usize, String)> = None; // (key, header_indent, body_offset_start, body)
        let mut offset = 0usize;
        for line in file.source.lines() {
            let line_len = line.len();
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if let Some((_, header_indent, _, _)) = &current
                && !trimmed.is_empty()
                && indent <= *header_indent
            {
                // Close current.
                if let Some((key, _, body_start, body)) = current.take() {
                    out.push((
                        format!("{}/{}", feature, key),
                        body,
                        Span::new(body_start, offset),
                    ));
                }
            }
            if let Some(key) = callable_header_key_from_trimmed(trimmed)
                && indent == 2
            {
                current = Some((key, indent, offset + line_len + 1, String::new()));
                offset += line_len + 1;
                continue;
            }
            if let Some((_, _, _, body)) = &mut current {
                body.push_str(line);
                body.push('\n');
            }
            offset += line_len + 1;
        }
        if let Some((key, _, body_start, body)) = current.take() {
            out.push((
                format!("{}/{}", feature, key),
                body,
                Span::new(body_start, offset),
            ));
        }
    }
    out
}

pub(crate) fn callable_header_key_from_trimmed(trimmed: &str) -> Option<String> {
    let prefixes: &[(&str, &str)] = &[
        ("command ", "command"),
        ("job ", "job"),
        ("webhook ", "webhook"),
        ("api ", "api"),
        ("query.list ", "query.list"),
        ("query.lookup ", "query.lookup"),
        ("query.sql ", "query.sql"),
        ("query.view ", "query.view"),
    ];
    for (prefix, kind) in prefixes {
        if let Some(rest) = trimmed.strip_prefix(*prefix) {
            let name = rest.split_whitespace().next().unwrap_or_default();
            if !name.is_empty() {
                return Some(format!("{}:{}", kind, name));
            }
        }
    }
    None
}

pub(crate) fn collect_canonical_facts(file: &DoctorFile, operational: &mut OperationalFacts) {
    let lines: Vec<_> = file.source.lines().collect();
    collect_operational_lzi_facts(file, &lines, operational);
    collect_file_capability_facts(file, &lines, operational);

    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if leading_spaces(lines[index]) != 0 || !trimmed.starts_with("feature ") {
            index += 1;
            continue;
        }

        let feature = trimmed
            .split_whitespace()
            .nth(1)
            .unwrap_or("<anonymous>")
            .to_owned();
        let start = index;
        index += 1;
        while index < lines.len()
            && !(leading_spaces(lines[index]) == 0
                && lines[index].trim_start().starts_with("feature "))
        {
            index += 1;
        }

        collect_feature_integration_requirements(
            file,
            &feature,
            start,
            &lines[start..index],
            operational,
        );
        // Phase L Tier 4 follow-up — the `job` branch of
        // `collect_external_calls_in_block` is retired alongside the
        // legacy `command` branch. Jobs now flow through
        // `populate_job_external_calls_from_ir` which reads
        // `Job.external_calls[*].span_ref` plus the typed
        // `Job.timeout` / `Job.retry` / `Job.idempotency` axes.
    }
}

pub(crate) fn collect_feature_integration_requirements(
    file: &DoctorFile,
    feature: &str,
    feature_start: usize,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    let mut in_requires_block = false;

    for (offset, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 2 {
            in_requires_block = trimmed == "requires";
            if let Some(requirement) = trimmed.strip_prefix("requires ")
                && let Some((slot, contract)) = parse_integration_requirement(requirement)
            {
                operational
                    .integration_requirements
                    .push(IntegrationRequirementFact {
                        path: file.path.clone(),
                        line: feature_start + offset + 1,
                        column: leading + 1,
                        feature: feature.to_owned(),
                        slot: slot.to_owned(),
                        contract: contract.to_owned(),
                    });
            }
            continue;
        }

        if leading <= 2 {
            in_requires_block = false;
        }

        if in_requires_block
            && leading == 4
            && let Some((slot, contract)) = parse_integration_requirement(trimmed)
        {
            operational
                .integration_requirements
                .push(IntegrationRequirementFact {
                    path: file.path.clone(),
                    line: feature_start + offset + 1,
                    column: leading + 1,
                    feature: feature.to_owned(),
                    slot: slot.to_owned(),
                    contract: contract.to_owned(),
                });
        }
    }
}

mod external_calls;

pub(crate) use external_calls::{
    populate_command_external_calls_from_ir, populate_job_external_calls_from_ir,
};

pub(crate) fn collect_operational_lzi_facts(
    file: &DoctorFile,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let line_number = index + 1;
        let column = leading_spaces(line) + 1;

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0
            && trimmed.starts_with("feature ")
            && let Some(feature) = trimmed.split_whitespace().nth(1)
        {
            operational.features.insert(
                feature.to_owned(),
                SourceFact {
                    path: file.path.clone(),
                    line: line_number,
                    column,
                    name: feature.to_owned(),
                },
            );
        }

        for reference in path_references(trimmed, "env.") {
            operational.env_references.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: reference.to_owned(),
            });
        }

        if trimmed.contains("@cap.File") {
            operational.file_capabilities.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: "@cap.File".to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("api ") {
            operational.apis.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: named_block_name(trimmed, "api")
                    .unwrap_or("unknown")
                    .to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("webhook ") {
            operational.webhooks.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: named_block_name(trimmed, "webhook")
                    .unwrap_or("unknown")
                    .to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("job ") {
            let job_name = named_block_name(trimmed, "job").unwrap_or("unknown");
            let fact = SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: job_name.to_owned(),
            };
            if job_block_has_schedule(lines, index) {
                operational.schedules.push(fact.clone());
            }
            operational.jobs.push(fact);
        }
    }
}

pub(crate) fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    rest.split_whitespace().next()
}

pub(crate) fn job_block_has_schedule(lines: &[&str], start: usize) -> bool {
    let mut index = start + 1;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if !trimmed.is_empty() && leading_spaces(line) <= 2 {
            break;
        }
        if leading_spaces(line) == 4 && trimmed.starts_with("trigger schedule ") {
            return true;
        }
        index += 1;
    }
    false
}

/// Phase L Tier 4 follow-up — IR-driven replacement for the retired
/// `collect_feature_commands` text-walker. Reads `feature.commands`
/// (typed) and populates the `commands: BTreeMap<CommandKey, CommandPolicy>`
/// map that `policy_reachability_diagnostics` +
/// `command_route_binding_diagnostics` consume.
///
/// Commands without a `policy` clause are skipped (mirroring the old
/// walker, which only inserted entries when it saw `policy ...`).
///
/// `@policy.<name>` atoms expand against the typed
/// `feature.policies.categories` slot, populated by
/// `lower_feature_skeleton` from the canonical-indent `policies` block.
/// The retired `collect_policy_atoms` text-walker that previously
/// resolved these names is gone. Route slot binding-from-context reads
/// typed `RouteSlot.from`.
pub(crate) fn populate_commands_from_ir(
    feature: &lazuli_ir::Feature,
    commands: &mut BTreeMap<CommandKey, CommandPolicy>,
) {
    use lazuli_ir::PolicyRef;

    let local_policies: BTreeMap<&str, &Vec<String>> = feature
        .policies
        .categories
        .iter()
        .map(|c| (c.name.as_str(), &c.atoms))
        .collect();

    for command in &feature.commands {
        let (reference, atoms) = match &command.policy {
            PolicyRef::None | PolicyRef::Unresolved(_) => continue,
            PolicyRef::Atom(atom) => {
                let reference = format!("@{atom}");
                let atoms = if let Some(local) = atom.strip_prefix("policy.") {
                    local_policies
                        .get(local)
                        .map(|atoms| (*atoms).clone())
                        .unwrap_or_default()
                } else {
                    vec![reference.clone()]
                };
                (reference, atoms)
            }
            PolicyRef::Local(name) => (name.clone(), Vec::new()),
            PolicyRef::External { feature, name } => {
                let reference = format!("{feature}.policy.{name}");
                (reference, Vec::new())
            }
        };

        let routes = command
            .route
            .iter()
            .map(|slot| {
                (
                    slot.name.clone(),
                    CommandRouteSlot {
                        bound_from_context: slot.from.is_some(),
                    },
                )
            })
            .collect();

        commands.insert(
            CommandKey {
                feature: feature.name.clone(),
                command: command.name.clone(),
            },
            CommandPolicy {
                reference,
                atoms,
                routes,
            },
        );
    }
}
