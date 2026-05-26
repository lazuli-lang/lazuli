//! Diagnostics for the policy/scope/refs family.
//!
//! The closed-namespace policy vocabulary (`@policy.*`, `@role.*`,
//! `@scope.*`, `@actor.*`) is one of the highest-stakes surfaces in
//! Lazuli — it's the input to every code-generation step and to every
//! audit. Sub-concerns:
//!
//! | Module | Concern |
//! |---|---|
//! | [`refs`] | `feature.refs` must list exactly the namespaces the feature uses. |
//! | [`namespace`] | every `policy <expr>` statement targets a namespaced atom (`@role.*`, `@scope.*`, `@actor.*`) or a feature-local `@policy.<category>`. Commands/workflows must use `@policy.*`. |
//! | [`scope_override`] | `scope override` inside a query block requires an explicit `policy` and a `reason` child. |
//! | [`rate_limit`] | public or mutating commands must declare `rate_limit ...` or explicit `rate_limit none` with a `reason`. |
//!
//! Helpers exposed at `crate::*` for use by other catalog modules
//! (`policy_statement_ref`, `is_namespaced_atom`, `collect_policy_atom_map`,
//! `policy_ref_is_public`, …) live in the sub-modules and ride the
//! `pub(crate) use diagnostics::policy::*;` re-export.

mod namespace;
mod rate_limit;
mod refs;
mod scope_override;

#[allow(unused_imports)]
pub(crate) use namespace::{
    collect_policy_categories, is_namespaced_atom, policy_atoms_from_dictionary_line,
    policy_namespace_diagnostics, policy_statement_ref,
};
#[allow(unused_imports)]
pub(crate) use rate_limit::{
    collect_policy_atom_map, command_rate_limit_contract_diagnostics,
    command_rate_limit_diagnostics, policy_ref_is_public, CommandSecurityFacts,
};
#[allow(unused_imports)]
pub(crate) use refs::{refs_block_diagnostics, refs_facts_diagnostics, FeatureRefsFacts};
#[allow(unused_imports)]
pub(crate) use scope_override::{
    query_scope_override_diagnostics, scope_override_policy_diagnostics, QuerySecurityFacts,
};
