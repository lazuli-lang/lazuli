//! IR -> emit-payload resolvers. The top-level driver hands each
//! `AppRoute` through here to decompose its guard / lifecycle / policy
//! references into the camelCase exports and atom tuples that
//! `emit.rs` then writes verbatim.

use lazuli_ir::{
    DefaultValue, Feature, RequiresField, RequiresLifecycle, RequiresLifecycleIn, ViewGuard,
};

use super::spec::{FieldGateEmit, ForbidEmit, GuardEmit, LifecycleEmit, OnlyWhenLifecycleEmit};
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
                only_when_lifecycle: fw
                    .only_when_lifecycle
                    .as_ref()
                    .and_then(|rl| resolve_only_when_lifecycle(rl, features)),
            })
            .collect(),
    })
}

/// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — resolve a
/// `forbid_when ... only_when lifecycle <R> = <state>` sub-slot into
/// the lookup query references the conditional branch needs at emit
/// time. Same shape as [`resolve_lifecycle_emit`] minus the helper
/// export (the conditional dispatch reuses the authored
/// `dispatch_to` URL — no lifecycle_routes helper involved).
fn resolve_only_when_lifecycle(
    rl: &RequiresLifecycle,
    features: &[Feature],
) -> Option<OnlyWhenLifecycleEmit> {
    let snake = snake_case(&rl.resource);
    let lookup_name = format!("lookup_my_{snake}");
    for feature in features {
        if !feature.resources.iter().any(|r| r.name == rl.resource) {
            continue;
        }
        let has_lookup = feature
            .queries
            .iter()
            .any(|q| q.name() == lookup_name.as_str());
        if !has_lookup {
            return None;
        }
        return Some(OnlyWhenLifecycleEmit {
            feature: feature.name.clone(),
            lookup_export: lower_camel_export(&lookup_name),
            required_state: rl.state.clone(),
        });
    }
    None
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
            allowed_states: None,
        });
    }
    None
}

/// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — resolve a
/// `requires_lifecycle_in <Resource> [<state>, ...]` allow-list slot
/// into the same `LifecycleEmit` shape as the exact-match variant.
/// Mirrors [`resolve_lifecycle_emit`] feature lookup; populates
/// `allowed_states` with the authored order and leaves `required_state`
/// empty (the two are mutually exclusive at IR layer, enforced by
/// doctor `ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001`).
pub(super) fn resolve_lifecycle_in_emit(
    rli: &RequiresLifecycleIn,
    features: &[Feature],
) -> Option<LifecycleEmit> {
    let snake = snake_case(&rli.resource);
    let lookup_name = format!("lookup_my_{snake}");
    for feature in features {
        let Some(resource) = feature.resources.iter().find(|r| r.name == rli.resource) else {
            continue;
        };
        if resource.lifecycle_routes.is_none() {
            return None;
        }
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
            required_state: String::new(),
            allowed_states: Some(rli.allowed_states.clone()),
        });
    }
    None
}

/// `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1 — resolve a
/// `requires <feature>.lookup_my.<field> = <literal> on_unmet redirect
/// <path>` slot into a [`FieldGateEmit`]. Locates the named feature
/// (snake_case match), confirms a `lookup_my_*` query exists on it
/// (the analyzer's `ROUTE-GUARD-FIELD-UNKNOWN-FEATURE-004` is the
/// authoritative gate, but codegen silently drops unresolved slots to
/// avoid emitting broken TS), and renders the expected literal as TS.
pub(super) fn resolve_field_gate_emit(
    rf: &RequiresField,
    features: &[Feature],
) -> Option<FieldGateEmit> {
    let feature = features.iter().find(|f| f.name == rf.feature)?;
    // Pick the first `lookup_my_*` query on the feature; the analyzer
    // is responsible for validating the resource implied by `lookup_my`
    // matches the field shape — codegen's contract is only "emit
    // against the same lookup the lifecycle gate would fetch".
    let lookup_query = feature
        .queries
        .iter()
        .find(|q| q.name().starts_with("lookup_my_"))?;
    let expected_literal_ts = render_default_value_ts(&rf.expected)?;
    // SDK row shape is camelCase per `lower_camel` policy in TS codegen
    // (e.g. `is_phone_verified` field becomes `isPhoneVerified` in the
    // generated row type — see `account.gen.ts`'s `User` interface).
    // Emitting the snake_case name in the runtime check reads `undefined`
    // and inverts the gate (every user appears unverified → loop to OTP).
    // Convert the IR's snake_case `field` to camelCase before emit.
    Some(FieldGateEmit {
        feature: feature.name.clone(),
        lookup_export: lower_camel_export(lookup_query.name()),
        field: snake_to_lower_camel(&rf.field),
        expected_literal_ts,
        on_unmet_redirect: rf.on_unmet_redirect.clone(),
    })
}

/// snake_case → lowerCamelCase converter for SDK-row field accessors.
/// Used by [`resolve_field_gate_emit`] so the emitted runtime check
/// reads the correctly-cased property name from the SDK row shape.
fn snake_to_lower_camel(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut capitalize_next = false;
    for ch in snake.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            out.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod snake_to_lower_camel_tests {
    use super::snake_to_lower_camel;

    #[test]
    fn single_word_passthrough() {
        assert_eq!(snake_to_lower_camel("phone"), "phone");
    }

    #[test]
    fn boolean_flag_three_words() {
        assert_eq!(snake_to_lower_camel("is_phone_verified"), "isPhoneVerified");
    }

    #[test]
    fn already_lower_camel_passthrough() {
        // Non-snake input stays as-is — the converter only acts on `_`.
        assert_eq!(snake_to_lower_camel("isPhoneVerified"), "isPhoneVerified");
    }

    #[test]
    fn trailing_underscore_ignored() {
        assert_eq!(snake_to_lower_camel("verified_"), "verified");
    }
}

/// Render a [`DefaultValue`] as a TS literal string for inline use
/// inside a `!==` comparison. Returns `None` for shapes the field
/// gate doesn't support (enum literals + nil currently — doctor
/// `ROUTE-GUARD-FIELD-TYPE-MISMATCH-006` rejects these at lint
/// time, but codegen stays defensive to avoid broken emit).
fn render_default_value_ts(value: &DefaultValue) -> Option<String> {
    match value {
        DefaultValue::Boolean(b) => Some(if *b {
            "true".to_owned()
        } else {
            "false".to_owned()
        }),
        DefaultValue::Integer(n) => Some(n.to_string()),
        DefaultValue::String(s) => Some(format!("{:?}", s)),
        DefaultValue::Nil => Some("null".to_owned()),
        DefaultValue::EnumLiteral(_) => None,
    }
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
