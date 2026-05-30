//! Backend (command / query / api) policy resolution and atom comparison.
//!
//! Builds the `BackendPolicy` list a view's guard must dominate and exposes
//! the atom-translation primitives shared with `resolve_guard`.

use std::collections::BTreeSet;

use lazuli_ir::{ExperienceView, Feature, PlatformView, PolicyAtom, PolicyRef, Query};

use super::{parse_command_ref, parse_query_ref};

pub(super) struct BackendPolicy {
    pub(super) label: String,
    pub(super) atoms: Vec<PolicyAtom>,
    pub(super) public: bool,
}

pub(super) fn backend_policies(
    default_feature: &str,
    view: &ExperienceView,
    platform: &PlatformView,
    features: &[Feature],
) -> Vec<BackendPolicy> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some((feature, name)) = view
        .source
        .as_deref()
        .and_then(|s| parse_query_ref(s, default_feature))
    {
        push_backend_query(&feature, &name, features, &mut seen, &mut out);
    }
    for text in view
        .submit
        .iter()
        .chain(view.actions.iter().map(|a| &a.target))
        .chain(platform.submit.iter())
        .chain(platform.actions.iter())
    {
        if let Some((feature, name)) = parse_command_ref(text, default_feature) {
            push_backend_command(&feature, &name, features, &mut seen, &mut out);
        }
    }
    out
}

fn push_backend_command(
    feature: &str,
    name: &str,
    features: &[Feature],
    seen: &mut BTreeSet<String>,
    out: &mut Vec<BackendPolicy>,
) {
    let key = format!("command:{feature}.{name}");
    if !seen.insert(key) {
        return;
    }
    let Some(f) = features.iter().find(|f| f.name == feature) else {
        return;
    };
    let Some(cmd) = f.commands.iter().find(|c| c.name == name) else {
        return;
    };
    let atoms = policy_ref_atoms(
        effective_policy(&cmd.policy, &f.defaults.policy),
        f,
        features,
    );
    out.push(BackendPolicy {
        label: format!("{feature}.command.{name}"),
        public: policy_is_public(&atoms, effective_policy(&cmd.policy, &f.defaults.policy)),
        atoms,
    });
}

fn push_backend_query(
    feature: &str,
    name: &str,
    features: &[Feature],
    seen: &mut BTreeSet<String>,
    out: &mut Vec<BackendPolicy>,
) {
    let key = format!("query:{feature}.{name}");
    if !seen.insert(key) {
        return;
    }
    let Some(f) = features.iter().find(|f| f.name == feature) else {
        return;
    };
    let Some(query) = f.queries.iter().find(|q| q.name() == name) else {
        return;
    };
    let policy = query_policy(query);
    let atoms = policy_ref_atoms(effective_policy(policy, &f.defaults.policy), f, features);
    out.push(BackendPolicy {
        label: format!("{feature}.query.{name}"),
        public: policy_is_public(&atoms, effective_policy(policy, &f.defaults.policy)),
        atoms,
    });
}

pub(super) fn query_policy(query: &Query) -> &PolicyRef {
    match query {
        Query::List(q) => &q.policy,
        Query::Lookup(q) => &q.policy,
        Query::Sql(q) => &q.policy,
    }
}

pub(super) fn effective_policy<'a>(
    policy: &'a PolicyRef,
    default: &'a Option<PolicyRef>,
) -> Option<&'a PolicyRef> {
    if policy.is_none() {
        default.as_ref()
    } else {
        Some(policy)
    }
}

pub(super) fn policy_ref_atoms(
    policy: Option<&PolicyRef>,
    feature: &Feature,
    features: &[Feature],
) -> Vec<PolicyAtom> {
    match policy {
        None | Some(PolicyRef::None) => Vec::new(),
        Some(PolicyRef::Atom(a)) => {
            if let Some(local) = a.strip_prefix("policy.") {
                category_atoms(&feature.name, local, features)
            } else {
                parse_atom(a).into_iter().collect()
            }
        }
        Some(PolicyRef::Local(n)) => category_atoms(&feature.name, n, features),
        Some(PolicyRef::External { feature, name }) => category_atoms(feature, name, features),
        Some(PolicyRef::Unresolved(s)) => policy_text_atoms(s, &feature.name, features),
    }
}

pub(super) fn policy_text_atoms(
    text: &str,
    default_feature: &str,
    features: &[Feature],
) -> Vec<PolicyAtom> {
    let raw = text.trim().trim_start_matches('@');
    if let Some(tail) = raw.strip_prefix("policy.") {
        let mut parts = tail.splitn(2, '.');
        let first = parts.next().unwrap_or_default();
        if let Some(second) = parts.next()
            && features.iter().any(|f| f.name == first)
        {
            return category_atoms(first, second, features);
        }
        return category_atoms(default_feature, tail, features);
    }
    parse_atom(raw).into_iter().collect()
}

fn category_atoms(feature: &str, name: &str, features: &[Feature]) -> Vec<PolicyAtom> {
    let mut atoms: Vec<_> = features
        .iter()
        .find(|f| f.name == feature)
        .into_iter()
        .flat_map(|f| &f.policies.categories)
        .find(|c| c.name == name)
        .into_iter()
        .flat_map(|c| c.atoms.iter())
        .filter_map(|a| parse_atom(a))
        .collect();
    sort_atoms(&mut atoms);
    atoms
}

fn parse_atom(text: &str) -> Option<PolicyAtom> {
    let raw = text.trim().trim_start_matches('@');
    let (namespace, name) = raw.split_once('.')?;
    (namespace != "policy" && !namespace.is_empty() && !name.is_empty())
        .then(|| atom(namespace, name))
}

pub(super) fn atom(namespace: &str, name: &str) -> PolicyAtom {
    PolicyAtom {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        args: None,
    }
}

fn policy_is_public(atoms: &[PolicyAtom], policy: Option<&PolicyRef>) -> bool {
    policy.is_none()
        || atoms
            .iter()
            .any(|a| a.namespace == "scope" && a.name == "public")
}

pub(super) fn missing_atoms(guard: &[PolicyAtom], backend: &[PolicyAtom]) -> Vec<String> {
    backend
        .iter()
        .filter(|atom| !guard.iter().any(|g| g == *atom))
        .map(|a| format!("@{}.{}", a.namespace, a.name))
        .collect()
}

fn sort_atoms(atoms: &mut Vec<PolicyAtom>) {
    atoms.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    atoms.dedup();
}
