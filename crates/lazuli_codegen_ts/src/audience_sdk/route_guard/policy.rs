//! Policy reference + atom resolution shared by every route-guard
//! emitter.
//!
//! Route guards reference policies in two shapes: `@policy.<name>` (a
//! reference into the feature's `policies` block) and raw atoms like
//! `@scope.workspace_admin` written directly on the route. This module
//! turns both into a uniform `ResolvedPolicy { name, atoms }` so the
//! TS-side `RouteGuardSpec` carries one shape regardless of where it
//! was authored.
//!
//! Policy atoms are emitted in a canonical order so the generated TS is
//! deterministic across runs: scope → role → actor → other, then
//! alphabetical within each band. Dedup is namespace+name based.

use std::collections::BTreeMap;

use lazuli_ir::Feature;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedPolicy {
    pub(super) name: Option<String>,
    pub(super) atoms: Vec<RoutePolicyAtom>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RoutePolicyAtom {
    pub(super) namespace: String,
    pub(super) name: String,
}

/// Trait-bound polymorphism over `policy: <single>` vs.
/// `policy: [<a>, <b>]` so the resolver consumes either authored
/// shape without a duplicated emit path.
pub(super) trait PolicyRefList {
    fn policy_refs(&self) -> Vec<&str>;
}

impl PolicyRefList for String {
    fn policy_refs(&self) -> Vec<&str> {
        vec![self.as_str()]
    }
}

impl PolicyRefList for Vec<String> {
    fn policy_refs(&self) -> Vec<&str> {
        self.iter().map(String::as_str).collect()
    }
}

pub(super) fn build_policy_lookup(
    features: &[Feature],
) -> BTreeMap<(String, String), Vec<RoutePolicyAtom>> {
    let mut out = BTreeMap::new();
    for feature in features {
        for category in &feature.policies.categories {
            let mut atoms: Vec<RoutePolicyAtom> = category
                .atoms
                .iter()
                .filter_map(|atom| parse_policy_atom(atom))
                .collect();
            sort_policy_atoms(&mut atoms);
            out.insert((feature.name.clone(), category.name.clone()), atoms);
        }
    }
    out
}

pub(super) fn resolve_policy(
    policy: &str,
    policies: &BTreeMap<(String, String), Vec<RoutePolicyAtom>>,
    default_feature: &str,
) -> ResolvedPolicy {
    if !policy.starts_with("@policy.") {
        if let Some(atom) = parse_policy_atom(policy) {
            return ResolvedPolicy {
                name: None,
                atoms: vec![atom],
            };
        }
    }

    if let Some((feature, category)) = parse_policy_ref(policy, default_feature) {
        let atoms = policies
            .get(&(feature, category))
            .cloned()
            .unwrap_or_default();
        return ResolvedPolicy {
            name: Some(policy.to_owned()),
            atoms,
        };
    }

    ResolvedPolicy {
        name: Some(policy.to_owned()),
        atoms: Vec::new(),
    }
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

fn parse_policy_atom(value: &str) -> Option<RoutePolicyAtom> {
    let raw = value.trim().trim_start_matches('@');
    let (namespace, name) = raw.split_once('.')?;
    if namespace.is_empty() || name.is_empty() || namespace == "policy" {
        return None;
    }
    Some(RoutePolicyAtom {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    })
}

fn sort_policy_atoms(atoms: &mut Vec<RoutePolicyAtom>) {
    atoms.sort_by(|a, b| {
        policy_namespace_rank(&a.namespace)
            .cmp(&policy_namespace_rank(&b.namespace))
            .then(a.namespace.cmp(&b.namespace))
            .then(a.name.cmp(&b.name))
    });
    atoms.dedup_by(|a, b| a.namespace == b.namespace && a.name == b.name);
}

fn policy_namespace_rank(namespace: &str) -> u8 {
    match namespace {
        "scope" => 0,
        "role" => 1,
        "actor" => 2,
        _ => 9,
    }
}
