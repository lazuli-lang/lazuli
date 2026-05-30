//! `query.compose` doctor diagnostics — the composite-read capability home.
//!
//! `query.compose` (proposal `ir-composite-read-primitive-2026-05-29.md`) is the
//! checked sibling of `query.sql`: a declarative composite read rooted at one
//! resource, projecting columns from itself and FK-reachable neighbors plus a
//! CLOSED 4-member per-row sub-select catalog, lowering to one analyzable SELECT.
//! Its entire value is that a malformed composite read becomes a `lazuli doctor`
//! **error** instead of compiling silently in hand-written Go.
//!
//! W1 (parser/AST), W2 (analyzer lowering — in-feature FK-path resolution,
//! projection-source resolution, the closed sub-select shapes, and the
//! load-bearing `scope_origin` stamp) ran first. This module is **W3**: the
//! doctor codes that turn the residual malformations W2 trusts/defers into
//! build-time failures. Each rule reads the resolved [`lazuli_ir::ComposeQuery`]
//! off the lowered [`lazuli_ir::Feature`]/[`lazuli_ir::Module`] — never the SQL
//! text — so the checks are exact, not heuristic.
//!
//! ## Catalog (severities mirror proposal §7)
//!
//! - [`join_path_001`] — `COMPOSE-JOIN-PATH-001` (error): an FK-path JOIN hop
//!   that doesn't resolve once the **Module graph** is available (the
//!   cross-feature hops W2's in-feature resolver trusts/defers).
//! - [`projection_source_001`] — `COMPOSE-PROJECTION-SOURCE-001` (error): a
//!   projected column with no resolvable source column on its root/joined
//!   resource (W2 resolved the alias/subselect *name*; W3 resolves the *column*).
//! - [`subselect_relation_001`] — `COMPOSE-SUBSELECT-RELATION-001` (error): a
//!   subselect `related_by` correlation FK that is wrong/missing.
//! - [`subselect_predicate_field_001`] — `COMPOSE-SUBSELECT-PREDICATE-FIELD-001`
//!   (error): a `where`/`filter` clause referencing a field not on the
//!   subselect's resource, OR a forbidden ordered operator (`<`/`>`) — the
//!   IR-level backstop for the §3.2 closed-predicate rule.
//! - [`scope_ungrounded_001`] — `COMPOSE-SCOPE-UNGROUNDED-001` (error): THE
//!   load-bearing security code. A tenant-bearing root whose
//!   `scope_origin == Overridden` with no accompanying `policy` is a
//!   cross-tenant leak.
//! - [`subselect_catalog_001`] — `COMPOSE-SUBSELECT-CATALOG-001` (error): closure
//!   enforcement — only the 4 subselect kinds / 5 aggregate fns, no grouped
//!   shape.
//! - [`nullability_mismatch_001`] — `COMPOSE-NULLABILITY-MISMATCH-001` (warning):
//!   a LEFT-joined (nullable) projected column mapped to a non-optional record
//!   field.
//! - [`demotable_to_list_001`] — `COMPOSE-DEMOTABLE-TO-LIST-001` (warning): a
//!   compose with no joins AND no subselects is a `query.list` in costume.
//!
//! The complementary `VOCAB-SQL-COMPOSABLE-001` (warning) lives in the existing
//! [`crate::vocab`] home — it nudges from the `query.sql` side, so it is homed
//! with the other `VOCAB-*` codes, not here.

pub mod demotable_to_list_001;
pub mod join_path_001;
pub mod nullability_mismatch_001;
pub mod projection_source_001;
pub mod scope_ungrounded_001;
pub mod subselect_catalog_001;
pub mod subselect_predicate_field_001;
pub mod subselect_relation_001;

use lazuli_ir::{ComposeQuery, Feature, FkPath, Query, Resource, TypeRef};

/// Convert a PascalCase resource name to its snake_case relation anchor —
/// the local mirror of the analyzer's `pascal_to_snake`. `ChatMessage` →
/// `chat_message`. Used to recognise an FK path's leading source anchor.
pub(crate) fn pascal_to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Find an in-scope resource by PascalCase name.
pub(crate) fn resource_by_name<'a>(resources: &'a [Resource], name: &str) -> Option<&'a Resource> {
    resources.iter().find(|r| r.name == name)
}

/// The FK target resource name for `field_name` on `resource`, when that field
/// is a relation edge: a `UserDefined`-typed field (returns the referenced
/// resource name) or an `ID` field bearing a GAP-12 `cross_feature_target`
/// (returns the annotated target). `None` when the field is absent / not a
/// relation. Mirrors the analyzer's `fk_target_resource_name`.
pub(crate) fn fk_target_of(resource: &Resource, field_name: &str) -> Option<String> {
    let field = resource.fields.iter().find(|f| f.name == field_name)?;
    if let TypeRef::UserDefined(qname) = &field.type_ref {
        return Some(qname.name.clone());
    }
    field
        .cross_feature_target
        .as_ref()
        .map(|t| t.resource.clone())
}

/// Walk an [`FkPath`] from `root` through `resources`, returning the resource
/// the path lands on when EVERY hop resolves in-scope. The path's first
/// segment is the source-resource anchor (snake-cased root name); each
/// subsequent segment is an FK field walked onto its target.
///
/// Returns `None` when a hop names no FK field (unresolvable) OR when a hop's
/// target leaves the supplied resource set (cross-feature, not resolvable
/// here). Callers distinguish the two cases via [`fk_target_of`] / their own
/// follow-up as needed; for column checks "couldn't fully resolve" simply
/// means "don't emit a column finding" (a separate code owns the unresolved
/// hop).
pub(crate) fn resolve_fk_path_target<'a>(
    root: &'a Resource,
    path: &FkPath,
    resources: &'a [Resource],
) -> Option<&'a Resource> {
    let (anchor, hops) = path.segments.split_first()?;
    if *anchor != pascal_to_snake(&root.name) {
        return None;
    }
    let mut current = root;
    for hop in hops {
        let target = fk_target_of(current, hop)?;
        current = resource_by_name(resources, &target)?;
    }
    Some(current)
}

/// Pull every [`ComposeQuery`] out of a feature's `queries`, paired with
/// nothing else — the shared harvest every rule in this module starts from.
///
/// Keeping it here (rather than re-walking `feature.queries` per rule) is the
/// `test_support`-style shared helper the Rails-layout rule asks for: one
/// place owns "which queries are composes".
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::compose::composes_of;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// for compose in composes_of(&feature) {
///     let _ = compose.name.as_str();
/// }
/// ```
pub fn composes_of(feature: &Feature) -> impl Iterator<Item = &ComposeQuery> {
    feature.queries.iter().filter_map(|q| match q {
        Query::Compose(c) => Some(c),
        _ => None,
    })
}
