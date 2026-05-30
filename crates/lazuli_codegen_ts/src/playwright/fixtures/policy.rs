//! Policy-atom resolution for Playwright fixture role inference.
//! Pulled out of `mod.rs` so the writer keeps its emission concerns
//! distinct from the policy-graph traversal.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::Feature;

use super::FIXTURE_ROLES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PolicyAtom {
    pub(super) namespace: String,
    pub(super) name: String,
}

pub(super) fn roles_from_policy_refs<'a>(
    policies: impl Iterator<Item = &'a str>,
    policy_lookup: &BTreeMap<(String, String), Vec<PolicyAtom>>,
    default_feature: &str,
) -> BTreeSet<String> {
    let atoms =
        policies.flat_map(|policy| resolve_policy_atoms(policy, policy_lookup, default_feature));
    roles_from_atoms(atoms)
}

pub(super) fn roles_from_atoms(atoms: impl Iterator<Item = PolicyAtom>) -> BTreeSet<String> {
    atoms
        .filter(|atom| atom.namespace == "role")
        .filter(|atom| FIXTURE_ROLES.contains(&atom.name.as_str()))
        .map(|atom| atom.name)
        .collect()
}

pub(super) fn build_policy_lookup(
    features: &[Feature],
) -> BTreeMap<(String, String), Vec<PolicyAtom>> {
    let mut out = BTreeMap::new();
    for feature in features {
        for category in &feature.policies.categories {
            let atoms = category
                .atoms
                .iter()
                .filter_map(|atom| parse_policy_atom(atom))
                .collect();
            out.insert((feature.name.clone(), category.name.clone()), atoms);
        }
    }
    out
}

pub(super) fn resolve_policy_atoms(
    policy: &str,
    policy_lookup: &BTreeMap<(String, String), Vec<PolicyAtom>>,
    default_feature: &str,
) -> Vec<PolicyAtom> {
    if let Some(atom) = parse_policy_atom(policy) {
        return vec![atom];
    }
    if let Some(role) = policy.strip_prefix("@policy.role.")
        && FIXTURE_ROLES.contains(&role)
    {
        return vec![PolicyAtom {
            namespace: "role".to_owned(),
            name: role.to_owned(),
        }];
    }
    let Some((feature, category)) = parse_policy_ref(policy, default_feature) else {
        return Vec::new();
    };
    policy_lookup
        .get(&(feature, category))
        .cloned()
        .unwrap_or_default()
}

fn parse_policy_ref(policy: &str, default_feature: &str) -> Option<(String, String)> {
    let tail = policy.strip_prefix("@policy.")?;
    let mut parts = tail.split('.');
    let first = parts.next()?.trim();
    let second = parts.next();
    if let Some(second) = second {
        let rest = std::iter::once(second)
            .chain(parts)
            .collect::<Vec<_>>()
            .join(".");
        Some((first.to_owned(), rest))
    } else {
        Some((default_feature.to_owned(), first.to_owned()))
    }
}

fn parse_policy_atom(value: &str) -> Option<PolicyAtom> {
    let raw = value.trim().trim_start_matches('@');
    let (namespace, name) = raw.split_once('.')?;
    if namespace.is_empty() || name.is_empty() || namespace == "policy" {
        return None;
    }
    Some(PolicyAtom {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    })
}
