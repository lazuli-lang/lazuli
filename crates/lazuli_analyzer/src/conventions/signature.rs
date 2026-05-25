//! `crud_synth_author_signature_mismatch` checks for command + query
//! overrides.
//!
//! Spec: `docs/proposals/ir-resource-conventions-crud.md` §9 / §11.
//!
//! When the author declares a name the synth pass also owns, override
//! semantics (§6) skip the synth — but the author's shape must still
//! match the canonical shape. These checks compare every dimension
//! (effect kind, input field set, params, keys, filters, policy,
//! pagination, owner-scope) and return a `reason: String` suitable
//! for the user-facing diagnostic format.

use lazuli_ir as ir;

/// Canonical return shape for `crud_synth_signature_mismatch` (§9 / §11).
/// The carried resource name is read by `check_command_signature_mismatch`
/// to compare against the author's effect target; the query variants
/// are matched only on kind today and reserve the name for Cell C4 if
/// it needs a richer diff message.
#[allow(dead_code)]
pub(super) enum CanonicalReturn<'a> {
    CreatesResource(&'a str),
    UpdatesResource(&'a str),
    DeletesResource(&'a str),
    ReturnsResource(&'a str),
    ReturnsResourceMany(&'a str),
}

/// §11 `crud_synth_author_signature_mismatch` trigger — compare an authored
/// command to its canonical convention shape and return a reason string
/// when the input field list OR the effect/return type diverges.
/// Returns `None` when the signatures match (no diagnostic). Cell C4
/// formats `reason` into the user-facing message.
pub(crate) fn check_command_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    canonical_inputs: &[(&ir::Field, bool)],
    canonical_return: CanonicalReturn<'_>,
) -> Option<String> {
    let cmd = feature.commands.iter().find(|c| c.name == name)?;

    // Compare effect kind.
    let effect_matches = match (&cmd.effect, &canonical_return) {
        (ir::CommandEffect::Creates(e), CanonicalReturn::CreatesResource(name)) => {
            e.resource.name == *name
        }
        (ir::CommandEffect::Updates(e), CanonicalReturn::UpdatesResource(name)) => {
            e.resource.name == *name
        }
        (ir::CommandEffect::Deletes(e), CanonicalReturn::DeletesResource(name)) => {
            e.resource.name == *name
        }
        _ => false,
    };
    if !effect_matches {
        return Some(format!(
            "effect / target resource diverges from canonical shape for `{}`",
            name
        ));
    }

    // Compare input field names. Order-insensitive set check is enough
    // here — Cell C4 may surface a richer diff.
    let canonical_names: std::collections::BTreeSet<String> = canonical_inputs
        .iter()
        .map(|(f, _)| f.name.clone())
        .collect();
    let author_names: std::collections::BTreeSet<String> = match &cmd.input {
        ir::CommandInput::Short(names) => names.iter().cloned().collect(),
        ir::CommandInput::Typed(slots) => slots.iter().map(|s| s.name.clone()).collect(),
        ir::CommandInput::Empty => std::collections::BTreeSet::new(),
    };
    if author_names != canonical_names {
        return Some(format!(
            "input field list diverges from canonical shape for `{}`",
            name
        ));
    }

    None
}

/// §11 `crud_synth_author_signature_mismatch` trigger for queries. Returns a
/// reason string when the author-written query diverges from the exact
/// canonical query shape the `crud` bundle would have emitted.
pub(crate) fn check_query_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    canonical_query: &ir::Query,
) -> Option<String> {
    let query = feature.queries.iter().find(|q| q.name() == name)?;

    match (query, canonical_query) {
        (ir::Query::Lookup(author), ir::Query::Lookup(canonical)) => {
            if author.params != canonical.params {
                return Some(format!(
                    "query params diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.keys != canonical.keys {
                return Some(format!(
                    "lookup keys diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.filters != canonical.filters {
                return Some(format!(
                    "query filters diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.policy != canonical.policy || author.policy_expr != canonical.policy_expr {
                return Some(format!(
                    "query policy diverges from canonical shape for `{}`",
                    name
                ));
            }
            if author.owner_scope_sql != canonical.owner_scope_sql {
                return Some(format!(
                    "owner-scope query shape diverges from canonical shape for `{}`",
                    name
                ));
            }
            None
        }
        (ir::Query::List(author), ir::Query::List(canonical)) => {
            if author.params != canonical.params {
                return Some(format!(
                    "query params diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.filters != canonical.filters {
                return Some(format!(
                    "query filters diverge from canonical shape for `{}`",
                    name
                ));
            }
            if author.order != canonical.order {
                return Some(format!(
                    "query order diverges from canonical shape for `{}`",
                    name
                ));
            }
            if author.paginate != canonical.paginate {
                return Some(format!(
                    "pagination diverges from canonical shape for `{}`",
                    name
                ));
            }
            if author.policy != canonical.policy || author.policy_expr != canonical.policy_expr {
                return Some(format!(
                    "query policy diverges from canonical shape for `{}`",
                    name
                ));
            }
            if author.owner_scope_sql != canonical.owner_scope_sql {
                return Some(format!(
                    "owner-scope query shape diverges from canonical shape for `{}`",
                    name
                ));
            }
            None
        }
        _ => Some(format!(
            "query kind / return shape diverges from canonical for `{}`",
            name
        )),
    }
}
