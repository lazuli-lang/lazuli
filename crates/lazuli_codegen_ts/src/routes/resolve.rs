//! IR -> emit-payload resolvers. The top-level driver hands each
//! `AppRoute` through here to decompose its guard / lifecycle / policy
//! references into the camelCase exports and atom tuples that
//! `emit.rs` then writes verbatim.

use lazuli_ir::{Feature, RequiresLifecycle, ViewGuard};

use super::spec::{ForbidEmit, GuardEmit, LifecycleEmit};
use super::{lower_camel_export, snake_case};

pub(super) fn resolve_guard_emit(guard: &ViewGuard, features: &[Feature]) -> Option<GuardEmit> {
    let has_policy = !guard.policy.is_empty();
    let has_forbid = !guard.forbid_when.is_empty();
    if !has_policy && !has_forbid {
        return None;
    }
    let (policy_name, policy_atoms) = if let Some(policy_ref) = guard.policy.first() {
        let policy_ref = policy_ref.trim();
        (
            policy_ref.to_owned(),
            resolve_policy_atoms(policy_ref, features),
        )
    } else {
        // forbid_when alone with no main policy: emit a trivial
        // always-authorized policy so the verdict logic falls through.
        (
            "@scope.authenticated".to_owned(),
            vec![("scope".to_owned(), "authenticated".to_owned())],
        )
    };
    Some(GuardEmit {
        policy_name,
        policy_atoms,
        on_unauthenticated: guard.on_unauthenticated.clone(),
        on_unauthorized: guard.on_unauthorized.clone(),
        forbid_when: guard
            .forbid_when
            .iter()
            .map(|fw| ForbidEmit {
                atom_namespace: fw.atom.namespace.clone(),
                atom_name: fw.atom.name.clone(),
                dispatch_to: fw.dispatch_to.clone(),
            })
            .collect(),
    })
}

/// router-w4 — resolve `requires_lifecycle <Resource> = <state>` into
/// a `LifecycleEmit` by locating the owning feature (the one with a
/// `lookup_my_<snake_resource>` query and a `lifecycle_routes` table
/// on the resource).
pub(super) fn resolve_lifecycle_emit(
    rl: &RequiresLifecycle,
    features: &[Feature],
) -> Option<LifecycleEmit> {
    let snake = snake_case(&rl.resource);
    let lookup_name = format!("lookup_my_{snake}");
    for feature in features {
        // The resource must live in this feature.
        let Some(resource) = feature.resources.iter().find(|r| r.name == rl.resource) else {
            continue;
        };
        if resource.lifecycle_routes.is_none() {
            return None;
        }
        // The lookup query must exist on the same feature.
        let has_lookup = feature
            .queries
            .iter()
            .any(|q| q.name() == lookup_name.as_str());
        if !has_lookup {
            return None;
        }
        return Some(LifecycleEmit {
            feature: feature.name.clone(),
            lookup_export: lower_camel_export(&lookup_name),
            helper_export: lower_camel_export(&format!("{snake}_lifecycle_route")),
            required_state: rl.state.clone(),
        });
    }
    None
}

/// Decompose a policy reference into its atoms. Handles two shapes:
/// 1. Atom-form: `@<ns>.<name>` (e.g. `@role.host`) — single atom.
/// 2. Named-form: `@policy.<name>` — look up the policy by name across
///    every feature's catalog; flatten its atom strings.
fn resolve_policy_atoms(policy_ref: &str, features: &[Feature]) -> Vec<(String, String)> {
    let bare = policy_ref.strip_prefix('@').unwrap_or(policy_ref);
    let (ns, name) = match bare.split_once('.') {
        Some(parts) => parts,
        None => return Vec::new(),
    };
    if ns == "policy" {
        // Named policy — look it up in every feature's catalog.
        for feature in features {
            for category in &feature.policies.categories {
                if category.name == name {
                    return category
                        .atoms
                        .iter()
                        .filter_map(|atom| parse_atom_string(atom))
                        .collect();
                }
            }
        }
        Vec::new()
    } else {
        // Inline atom shape: `@role.host` etc.
        vec![(ns.to_owned(), name.to_owned())]
    }
}

fn parse_atom_string(s: &str) -> Option<(String, String)> {
    let bare = s.strip_prefix('@').unwrap_or(s);
    let (ns, rest) = bare.split_once('.')?;
    // Strip any trailing `(args)` parameterisation; codegen doesn't
    // forward those today (only `@mfa.required(within:15m)` carries
    // args, and route guards don't use it).
    let name = rest.split('(').next().unwrap_or(rest);
    Some((ns.to_owned(), name.to_owned()))
}
