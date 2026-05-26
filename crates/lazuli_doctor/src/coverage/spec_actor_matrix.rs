//! `spec_actor_matrix` coverage layer.
//!
//! Walks every `policy @policy.X` reference on commands and
//! transitions and projects out a `(construct, actor)` matrix using
//! the closed `@role.<name>` namespace declared at `auth.roles`. A
//! pair is **covered** when the construct's `tests` block carries
//! either a `PolicyAllow`/`PolicyDeny` row mentioning the actor OR an
//! authored `AllowsAs`/`DeniesAs` row naming the actor.
//!
//! Total denominator design: when the construct's policy is local
//! (`PolicyRef::Local`) and the corresponding `PolicyCategory.atoms`
//! reference `@role.<name>` actors, that role enters the actor set
//! for the matrix. Atoms outside the `@role.*` namespace (scope,
//! actor pseudonyms, custom) are intentionally excluded from this
//! layer because they are not modelled as discrete principals.

use std::collections::BTreeSet;

use lazuli_ir::{Command, Feature, PolicyCategory, PolicyRef, TestAssertion, TestBlock};

use super::LayerCoverage;

/// Compute the `spec_actor_matrix` layer for `features`. Counts each
/// `(construct, actor)` pair as one denominator unit and as covered
/// when the construct's `tests` block names the actor in any allow/deny
/// assertion. Always emits `source = "ir-walk"`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::coverage::spec_actor_matrix::compute;
///
/// let layer = compute(&[]);
/// assert_eq!(layer.total, 0);
/// ```
pub fn compute(features: &[Feature]) -> LayerCoverage {
    let mut total = 0usize;
    let mut covered = 0usize;
    for feature in features {
        let role_atoms = collect_feature_role_atoms(feature);
        if role_atoms.is_empty() {
            continue;
        }
        for cmd in &feature.commands {
            count_for_command(cmd, &feature.policies.categories, &role_atoms, &mut total, &mut covered);
        }
        for workflow in &feature.workflows {
            for t in &workflow.transitions {
                let tests = t.tests.as_ref();
                let actors = actors_for_policy_ref_str(
                    t.requires.as_deref(),
                    &feature.policies.categories,
                    &role_atoms,
                );
                for actor in &actors {
                    total += 1;
                    if covers_actor(tests, actor) {
                        covered += 1;
                    }
                }
            }
        }
    }
    LayerCoverage::new(covered, total).with_source("ir-walk")
}

fn count_for_command(
    cmd: &Command,
    categories: &[PolicyCategory],
    role_atoms: &BTreeSet<String>,
    total: &mut usize,
    covered: &mut usize,
) {
    let actors = actors_for_policy(&cmd.policy, categories, role_atoms);
    let tests = cmd.tests.as_ref();
    for actor in &actors {
        *total += 1;
        if covers_actor(tests, actor) {
            *covered += 1;
        }
    }
}

fn covers_actor(tests: Option<&TestBlock>, actor: &str) -> bool {
    let Some(tb) = tests else { return false };
    for a in &tb.assertions {
        match a {
            TestAssertion::PolicyAllow { actors } | TestAssertion::PolicyDeny { actors } => {
                if actors.iter().any(|x| x == actor) {
                    return true;
                }
            }
            TestAssertion::AllowsAs { actor: a2 }
            | TestAssertion::DeniesAs { actor: a2 }
            | TestAssertion::AllowsFromAs { actor: a2, .. }
            | TestAssertion::DeniesFromAs { actor: a2, .. } => {
                if a2 == actor {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn collect_feature_role_atoms(feature: &Feature) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for cat in &feature.policies.categories {
        for atom in &cat.atoms {
            if atom.starts_with("@role.") {
                out.insert(atom.clone());
            }
        }
    }
    out
}

fn actors_for_policy(
    policy: &PolicyRef,
    categories: &[PolicyCategory],
    role_atoms: &BTreeSet<String>,
) -> Vec<String> {
    match policy {
        // `PolicyRef::Atom` stores the body without the `@` sigil
        // (lower_policy_atom strips it). So `@role.admin` becomes
        // `Atom("role.admin")` and `@policy.create` becomes
        // `Atom("policy.create")`.
        PolicyRef::Atom(name) if name.starts_with("role.") => {
            vec![format!("@{name}")]
        }
        PolicyRef::Atom(name) if name.starts_with("policy.") => {
            let category_name = name.trim_start_matches("policy.");
            atoms_for_category(category_name, categories)
        }
        PolicyRef::Local(category_name) => atoms_for_category(category_name, categories),
        PolicyRef::Unresolved(text) => actors_from_unresolved(text, role_atoms),
        _ => Vec::new(),
    }
}

fn atoms_for_category(name: &str, categories: &[PolicyCategory]) -> Vec<String> {
    for cat in categories {
        if cat.name == name {
            return cat
                .atoms
                .iter()
                .filter(|a| a.starts_with("@role."))
                .cloned()
                .collect();
        }
    }
    Vec::new()
}

fn actors_for_policy_ref_str(
    requires: Option<&str>,
    categories: &[PolicyCategory],
    role_atoms: &BTreeSet<String>,
) -> Vec<String> {
    // Workflow `Transition.requires: Option<String>` carries the raw
    // `@policy.<name>` text. We resolve it through the same closed
    // catalog as the policy ref above; unresolved forms fall back to
    // any `@role.*` substring we can spot.
    let Some(text) = requires else { return Vec::new() };
    if let Some(category_name) = text.strip_prefix("@policy.") {
        for cat in categories {
            if cat.name == category_name {
                return cat
                    .atoms
                    .iter()
                    .filter(|a| a.starts_with("@role."))
                    .cloned()
                    .collect();
            }
        }
    }
    actors_from_unresolved(text, role_atoms)
}

fn actors_from_unresolved(text: &str, role_atoms: &BTreeSet<String>) -> Vec<String> {
    // Conservative fallback: if the unresolved policy text mentions a
    // known `@role.<name>` atom verbatim, the calculator counts it.
    // This keeps coverage honest when an authored policy is not yet
    // lowered to a `PolicyRef` variant.
    let mut out = Vec::new();
    for atom in role_atoms {
        if text.contains(atom) {
            out.push(atom.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::test_support::empty_feature;
    use lazuli_ir::{Command, PolicyCategory, TestAssertion, TestBlock};

    fn cmd_with_policy(name: &str, policy: PolicyRef, tests: Option<TestBlock>) -> Command {
        crate::coverage::test_support::cmd_with_policy(name, policy, tests)
    }

    #[test]
    fn local_policy_with_two_roles_yields_two_pairs() {
        let mut f = empty_feature("f");
        f.policies.categories.push(PolicyCategory {
            name: "update".to_string(),
            atoms: vec!["@role.admin".to_string(), "@role.editor".to_string()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        });
        let tests = TestBlock {
            assertions: vec![TestAssertion::PolicyAllow {
                actors: vec!["@role.admin".to_string()],
            }],
            span_ref: None,
        };
        f.commands
            .push(cmd_with_policy("c", PolicyRef::Local("update".to_string()), Some(tests)));
        let layer = compute(&[f]);
        assert_eq!(layer.total, 2);
        assert_eq!(layer.covered, 1);
    }

    #[test]
    fn atom_policy_counts_directly() {
        let mut f = empty_feature("f");
        f.policies.categories.push(PolicyCategory {
            name: "_dummy".to_string(),
            atoms: vec!["@role.viewer".to_string()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        });
        let tests = TestBlock {
            assertions: vec![TestAssertion::DeniesAs {
                actor: "@role.viewer".to_string(),
            }],
            span_ref: None,
        };
        // `lower_policy_atom` strips the leading `@`, so `@role.viewer`
        // becomes `Atom("role.viewer")`.
        f.commands.push(cmd_with_policy(
            "c",
            PolicyRef::Atom("role.viewer".to_string()),
            Some(tests),
        ));
        let layer = compute(&[f]);
        assert_eq!(layer.total, 1);
        assert_eq!(layer.covered, 1);
    }

    #[test]
    fn policy_atom_resolves_to_local_category() {
        let mut f = empty_feature("f");
        f.policies.categories.push(PolicyCategory {
            name: "create".to_string(),
            atoms: vec!["@role.admin".to_string(), "@role.editor".to_string()],
            previous_names: Vec::new(),
            when_denied: None,
            when_denied_route: None,
        });
        let tests = TestBlock {
            assertions: vec![TestAssertion::PolicyAllow {
                actors: vec!["@role.admin".to_string(), "@role.editor".to_string()],
            }],
            span_ref: None,
        };
        // `@policy.create` lowers to `Atom("policy.create")`.
        f.commands.push(cmd_with_policy(
            "c",
            PolicyRef::Atom("policy.create".to_string()),
            Some(tests),
        ));
        let layer = compute(&[f]);
        assert_eq!(layer.total, 2);
        assert_eq!(layer.covered, 2);
    }

    #[test]
    fn feature_with_no_roles_in_policies_yields_zero() {
        let mut f = empty_feature("f");
        f.commands.push(cmd_with_policy(
            "c",
            PolicyRef::Atom("scope.same_org".to_string()),
            None,
        ));
        let layer = compute(&[f]);
        assert_eq!(layer.total, 0);
        assert_eq!(layer.covered, 0);
    }
}
