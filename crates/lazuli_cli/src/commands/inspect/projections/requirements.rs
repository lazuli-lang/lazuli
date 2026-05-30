//! `--expand=requirements` projection.
//!
//! Surfaces the `requires integration <slot>: <Capability>` declarations
//! authored on a feature. Supports the inline form (`requires
//! integration foo: Bar` directly under the feature header) and the
//! block form (`requires` header with one or more indent-4 child
//! declarations). Each row records the origin (`requires inline` vs
//! `requires block`) so downstream consumers can preserve authoring
//! provenance.

use super::super::InspectRequirement;
use super::super::expand::{is_identifier, is_type_name, leading_spaces};

pub(in crate::commands::inspect) fn inspect_requirements(
    lines: &[String],
) -> Vec<InspectRequirement> {
    let mut requirements = Vec::new();
    let mut in_requires_block = false;

    for line in lines {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 2 {
            in_requires_block = trimmed == "requires";
            if let Some(requirement) = trimmed.strip_prefix("requires ")
                && let Some(parsed) = parse_inspect_requirement(requirement, "requires inline")
            {
                requirements.push(parsed);
            }
            continue;
        }

        if leading <= 2 {
            in_requires_block = false;
        }

        if in_requires_block
            && leading == 4
            && let Some(parsed) = parse_inspect_requirement(trimmed, "requires block")
        {
            requirements.push(parsed);
        }
    }

    requirements
}

fn parse_inspect_requirement(source: &str, origin: &'static str) -> Option<InspectRequirement> {
    let rest = source.trim().strip_prefix("integration ")?;
    let (name, contract) = rest.split_once(':')?;
    let name = name.trim();
    let contract = contract.trim();

    if !is_identifier(name) || !is_type_name(contract) {
        return None;
    }

    Some(InspectRequirement {
        kind: "integration".to_owned(),
        name: name.to_owned(),
        contract: contract.to_owned(),
        origin,
    })
}
