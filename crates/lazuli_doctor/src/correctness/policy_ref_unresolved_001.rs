//! POLICY-REF-UNRESOLVED-001 — a command / query / api policy reference that
//! resolves to no enforceable atoms.
//!
//! ## Trigger
//!
//! Fires when an author-declared `policy <ref>` on a command, query, or api
//! references a named policy category that exists in NO feature in the package
//! — including a cross-feature `<feature>.policy.<name>` whose owning feature
//! or category is missing (`PolicyRef::External`), and a feature-local
//! `@policy.<name>` / bare `<name>` with no matching `policies` category.
//!
//! ## Why it is a SECURITY rule (fail-open class)
//!
//! The per-feature Go codegen pass resolves `@policy.<name>` against THIS
//! feature's `policies` block only; it has no view of another feature's
//! categories. An unresolved reference used to lower to a Name-only
//! `lazuli.Policy{Name: "..."}` with NO atoms — a command guarded by it shipped
//! EFFECTIVELY UNGUARDED at any runtime call site that does not treat an empty
//! atom list as deny (e.g. `Api.Invoke` runs no policy check at all). Codegen
//! now fails CLOSED (emits a `predicate.deny` atom), but a permanently-denied
//! command is still a bug the author must see at build time — that is this
//! rule. It is the static, build-time half of the POLICY-REF-UNRESOLVED fix;
//! the codegen deny atom is the defense-in-depth runtime half.
//!
//! ## Severity
//!
//! `error` — an unresolvable policy reference is never intentional: either the
//! reference is a typo, or a cross-feature policy was never exported / the
//! `policies` category does not exist. Shipping it means a command that is
//! either unguarded (pre-fix) or permanently denied (post-fix) — both wrong.
//!
//! ## Not flagged (resolvable / out of scope)
//!
//! - `PolicyRef::None` — no per-callable policy; the feature default / public
//!   fallback applies (covered by `MISSING-POLICY-ON-QUERY-001`).
//! - A structured `policy <expr>` (`policy_expr`) — has its own atoms, never
//!   routed through category resolution.
//! - A closed-catalog atom `@role.*` / `@scope.*` / `@actor.*` — resolves to a
//!   single atom directly, no category lookup needed.
//! - `PolicyRef::Unresolved(_)` — legacy raw-text form; left to the legacy
//!   text resolver, not re-litigated here.

use std::collections::BTreeSet;

use lazuli_ir::{Feature, Policies, PolicyRef};

/// Built-in policy names that resolve WITHOUT a declared `policies` category.
/// `public` (anonymous) and `authenticated` (any signed-in user) map directly
/// to the closed `@scope.*` runtime atoms; the CRUD/me-mode conventions synth
/// and marketing reads use them without declaring a category. They are never
/// findings.
const BUILTIN_POLICY_NAMES: &[&str] = &["public", "authenticated"];

fn is_builtin_policy(name: &str) -> bool {
    BUILTIN_POLICY_NAMES.contains(&name)
}

/// One POLICY-REF-UNRESOLVED-001 finding — a callable whose policy reference
/// names a category present in no feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Feature owning the callable.
    pub feature: String,
    /// Callable kind tag — `command`, `query`, or `api`.
    pub kind: &'static str,
    /// Callable name.
    pub name: String,
    /// The unresolved reference as written, e.g. `accounts.policy.restricted`
    /// or `@policy.nonexistent`.
    pub reference: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "POLICY-REF-UNRESOLVED-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::correctness::policy_ref_unresolved_001::Finding;
    ///
    /// let f = Finding {
    ///     feature: "billing".into(),
    ///     kind: "command",
    ///     name: "charge".into(),
    ///     reference: "accounts.policy.restricted".into(),
    /// };
    /// assert!(f.message().contains("accounts.policy.restricted"));
    /// assert!(f.message().contains("DENIED"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{kind} `{name}` references policy `{reference}`, which resolves to no \
             declared policy category in any feature. A cross-feature reference \
             must name an existing `policies` category in the owning feature; a \
             feature-local `@policy.<name>` must match a category in this feature. \
             Codegen fails CLOSED — the command/query is permanently DENIED (403) \
             until the reference is fixed. Declare the category, or correct the \
             reference.",
            kind = self.kind,
            name = self.name,
            reference = self.reference,
        )
    }
}

/// A `(feature, category)` key for a declared policy category, plus the bare
/// category name (for same-feature `@policy.<name>` / `Local` resolution that
/// does not qualify the feature).
#[derive(Default)]
pub struct PolicyCatalog {
    qualified: BTreeSet<(String, String)>,
    by_name: BTreeSet<String>,
}

impl PolicyCatalog {
    /// Build the global policy-category catalog from every feature's `policies`
    /// block. Both the `(feature, name)` qualified key and the bare `name` are
    /// recorded so same-feature and cross-feature references both resolve.
    pub fn from_features<'a>(features: impl IntoIterator<Item = &'a Feature>) -> Self {
        Self::from_feature_policies(features.into_iter().map(|f| (f.name.as_str(), &f.policies)))
    }

    /// Build the catalog from `(feature_name, &Policies)` pairs — the shape the
    /// doctor's per-feature facts expose (`Tier3FeatureFacts.feature` +
    /// `.policies`) without cloning a full `Feature`.
    pub fn from_feature_policies<'a>(
        entries: impl IntoIterator<Item = (&'a str, &'a Policies)>,
    ) -> Self {
        let mut catalog = PolicyCatalog::default();
        for (feature_name, policies) in entries {
            for cat in &policies.categories {
                catalog
                    .qualified
                    .insert((feature_name.to_owned(), cat.name.clone()));
                catalog.by_name.insert(cat.name.clone());
            }
        }
        catalog
    }

    fn has_qualified(&self, feature: &str, name: &str) -> bool {
        self.qualified.contains(&(feature.to_owned(), name.to_owned()))
    }

    fn has_name(&self, name: &str) -> bool {
        self.by_name.contains(name)
    }
}

/// Resolution verdict for a single policy reference against the catalog.
///
/// `Some(reference)` ⇒ the reference is UNRESOLVABLE (a finding); the string is
/// the human-readable reference form. `None` ⇒ resolvable, public, structured,
/// or otherwise not a category reference.
///
/// `default_feature` is the feature the callable lives in (used to resolve
/// unqualified `@policy.<name>` / `Local` references).
pub fn unresolved_reference(
    policy: &PolicyRef,
    default_feature: &str,
    catalog: &PolicyCatalog,
) -> Option<String> {
    match policy {
        // No per-callable policy / legacy raw text — out of scope here.
        PolicyRef::None | PolicyRef::Unresolved(_) => None,
        PolicyRef::Local(name) => {
            // Built-in `public` / `authenticated` resolve without a category.
            if is_builtin_policy(name) {
                return None;
            }
            // Bare `<name>` resolves against any feature's categories
            // (the codegen path keys it to the local feature, but a name
            // declared in this feature is the common case; accept either to
            // avoid false positives).
            if catalog.has_qualified(default_feature, name) || catalog.has_name(name) {
                None
            } else {
                Some(name.clone())
            }
        }
        PolicyRef::Atom(atom) => {
            // Only `policy.<name>` atoms route through category resolution.
            // `@role.*` / `@scope.*` / `@actor.*` are closed single atoms.
            let Some(rest) = atom.strip_prefix("policy.") else {
                return None;
            };
            // A `policy.<feature>.<name>` form qualifies the feature.
            if let Some((feat, name)) = rest.split_once('.') {
                if is_builtin_policy(name)
                    || catalog.has_qualified(feat, name)
                    || catalog.has_name(name)
                {
                    None
                } else {
                    Some(format!("@policy.{rest}"))
                }
            } else if is_builtin_policy(rest)
                || catalog.has_qualified(default_feature, rest)
                || catalog.has_name(rest)
            {
                None
            } else {
                Some(format!("@policy.{rest}"))
            }
        }
        PolicyRef::External { feature, name } => {
            if catalog.has_qualified(feature, name) {
                None
            } else {
                Some(format!("{feature}.policy.{name}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lower a feature whose `policies` block declares `cats` as categories.
    fn feature_with_categories(name: &str, cats: &[&str]) -> Feature {
        let mut src = format!("feature {name}\n");
        if !cats.is_empty() {
            src.push_str("  policies\n");
            for c in cats {
                src.push_str(&format!("    {c}: @role.ADMIN\n"));
            }
        }
        let parsed = lazuli_syntax::parse_feature_skeletons(&src).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&parsed[0]).expect("lower feature")
    }

    #[test]
    fn external_ref_to_missing_category_is_unresolved() {
        let features = vec![feature_with_categories("billing", &["manage"])];
        let catalog = PolicyCatalog::from_features(&features);
        let pol = PolicyRef::External {
            feature: "accounts".to_owned(),
            name: "restricted".to_owned(),
        };
        assert_eq!(
            unresolved_reference(&pol, "billing", &catalog),
            Some("accounts.policy.restricted".to_owned())
        );
    }

    #[test]
    fn external_ref_to_existing_cross_feature_category_resolves() {
        let features = vec![
            feature_with_categories("billing", &["manage"]),
            feature_with_categories("accounts", &["restricted"]),
        ];
        let catalog = PolicyCatalog::from_features(&features);
        let pol = PolicyRef::External {
            feature: "accounts".to_owned(),
            name: "restricted".to_owned(),
        };
        assert_eq!(unresolved_reference(&pol, "billing", &catalog), None);
    }

    #[test]
    fn atom_qualified_cross_feature_resolves_via_catalog() {
        let features = vec![
            feature_with_categories("billing", &["manage"]),
            feature_with_categories("accounts", &["restricted"]),
        ];
        let catalog = PolicyCatalog::from_features(&features);
        // The lowering of `policy @policy.accounts.restricted`.
        let pol = PolicyRef::Atom("policy.accounts.restricted".to_owned());
        assert_eq!(unresolved_reference(&pol, "billing", &catalog), None);
    }

    #[test]
    fn local_unknown_category_is_unresolved() {
        let features = vec![feature_with_categories("catalog", &["view"])];
        let catalog = PolicyCatalog::from_features(&features);
        let pol = PolicyRef::Atom("policy.nonexistent".to_owned());
        assert_eq!(
            unresolved_reference(&pol, "catalog", &catalog),
            Some("@policy.nonexistent".to_owned())
        );
    }

    #[test]
    fn local_known_category_resolves() {
        let features = vec![feature_with_categories("catalog", &["view"])];
        let catalog = PolicyCatalog::from_features(&features);
        let pol = PolicyRef::Local("view".to_owned());
        assert_eq!(unresolved_reference(&pol, "catalog", &catalog), None);
        let pol_atom = PolicyRef::Atom("policy.view".to_owned());
        assert_eq!(unresolved_reference(&pol_atom, "catalog", &catalog), None);
    }

    #[test]
    fn closed_atom_is_never_a_category_finding() {
        let features = vec![feature_with_categories("catalog", &["view"])];
        let catalog = PolicyCatalog::from_features(&features);
        for atom in ["role.ADMIN", "scope.same_org", "actor.system"] {
            let pol = PolicyRef::Atom(atom.to_owned());
            assert_eq!(unresolved_reference(&pol, "catalog", &catalog), None);
        }
    }

    #[test]
    fn builtin_public_and_authenticated_never_fire() {
        let catalog = PolicyCatalog::default();
        for name in ["public", "authenticated"] {
            assert_eq!(
                unresolved_reference(&PolicyRef::Local(name.to_owned()), "f", &catalog),
                None,
                "built-in @policy.{name} must resolve"
            );
            assert_eq!(
                unresolved_reference(
                    &PolicyRef::Atom(format!("policy.{name}")),
                    "f",
                    &catalog
                ),
                None,
                "built-in @policy.{name} (atom form) must resolve"
            );
        }
    }

    #[test]
    fn none_and_unresolved_are_skipped() {
        let catalog = PolicyCatalog::default();
        assert_eq!(unresolved_reference(&PolicyRef::None, "f", &catalog), None);
        assert_eq!(
            unresolved_reference(&PolicyRef::Unresolved("whatever".to_owned()), "f", &catalog),
            None
        );
    }
}
