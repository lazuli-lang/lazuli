//! FK-aware topological sort over `(feature, resource)` pairs.
//!
//! WAR-RUNTIME-MIGRATION-03 — every FK target's `CREATE TABLE` must
//! run BEFORE the referencing `FOREIGN KEY` constraint, otherwise the
//! migration refuses to apply. This module owns Kahn's algorithm over
//! the resource dependency graph plus the helpers that decide whether
//! a given `UserDefined` reference resolves to a real FK target.
//!
//! Lexical `(feature, resource)` order breaks ties so the output stays
//! stable for resources with no dependency between them. Cycles fall
//! back to lexical order with a warning on stderr — a cycle in FK
//! direction is a data-model bug the doctor should catch separately;
//! codegen degrades gracefully so the emit still produces consumable
//! migrations.

use lazuli_ir::{Feature, Module, Resource, TypeRef};

use super::super::cross_feature::CrossFeatureIndex;
use super::sql_builder::feature_declares_resource;

/// Kahn-style topo sort over (feature, resource) pairs. Edges run from
/// referencing resource → FK target resource (target must come first).
/// Ties break by lexical (feature, resource) order so output is stable
/// for unrelated subgraphs. Cycles fall back to lexical order with a
/// warning on stderr — a cycle in FK direction is a data-model bug the
/// doctor should catch separately; codegen degrades gracefully so the
/// emit still produces consumable migrations.
pub(super) fn topo_sort_resources<'a>(
    module: &'a Module,
    resources: &[(&'a Feature, &'a Resource)],
    cross_index: &CrossFeatureIndex<'a>,
) -> Vec<(&'a Feature, &'a Resource)> {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    type Key = (String, String);

    let key = |f: &Feature, r: &Resource| -> Key { (f.name.clone(), r.name.clone()) };

    // Build lookup table from (feature, resource) → index into the input
    // slice. Lexical key ordering is what BTreeMap provides natively, so
    // we lean on it for the tiebreaker.
    let mut by_key: BTreeMap<Key, usize> = BTreeMap::new();
    for (idx, (feature, resource)) in resources.iter().enumerate() {
        by_key.insert(key(feature, resource), idx);
    }

    // Adjacency: target_key → set of dependents. In-degree counts how
    // many FK targets a resource still depends on.
    let mut dependents: BTreeMap<Key, BTreeSet<Key>> = BTreeMap::new();
    let mut in_degree: BTreeMap<Key, usize> = BTreeMap::new();
    for (feature, resource) in resources {
        in_degree.insert(key(feature, resource), 0);
    }

    for (feature, resource) in resources {
        let dependent_key = key(feature, resource);
        for field in &resource.fields {
            let TypeRef::UserDefined(qname) = &field.type_ref else {
                continue;
            };
            // Resolve target feature exactly the way `foreign_key_owner`
            // does — same-feature and cross-feature both produce a FK.
            let owner = match qname.feature.as_deref() {
                Some(owner) => Some(owner.to_owned()),
                None => cross_index.owner(&qname.name).map(str::to_owned),
            };
            let Some(owner) = owner else { continue };
            if !feature_declares_resource(module, &owner, &qname.name) {
                continue;
            }
            let target_key = (owner, qname.name.clone());
            // Skip self-references — they don't change topo order and
            // would otherwise show as a 1-node cycle.
            if target_key == dependent_key {
                continue;
            }
            let inserted = dependents
                .entry(target_key)
                .or_default()
                .insert(dependent_key.clone());
            if inserted {
                *in_degree.entry(dependent_key.clone()).or_insert(0) += 1;
            }
        }
    }

    // Kahn's algorithm: ready queue holds zero-in-degree nodes sorted
    // lexically. Pop the smallest, emit it, decrement in-degree of its
    // dependents, requeue any that hit zero.
    let mut ready: BTreeSet<Key> = in_degree
        .iter()
        .filter_map(|(k, &deg)| (deg == 0).then(|| k.clone()))
        .collect();

    let mut ordered: Vec<(&'a Feature, &'a Resource)> = Vec::with_capacity(resources.len());
    let mut emitted: BTreeSet<Key> = BTreeSet::new();

    while let Some(next_key) = ready.iter().next().cloned() {
        ready.remove(&next_key);
        if let Some(&idx) = by_key.get(&next_key) {
            ordered.push(resources[idx]);
            emitted.insert(next_key.clone());
        }
        if let Some(deps) = dependents.get(&next_key) {
            for dependent in deps {
                if let Some(deg) = in_degree.get_mut(dependent) {
                    if *deg > 0 {
                        *deg -= 1;
                        if *deg == 0 {
                            ready.insert(dependent.clone());
                        }
                    }
                }
            }
        }
    }

    if ordered.len() != resources.len() {
        // Cycle detected — append the remainder in lexical order so the
        // emit completes. Doctor should flag the cycle separately; codegen
        // refuses to silently drop tables.
        eprintln!(
            "warning: migration FK topo sort detected a cycle ({} of {} resources unresolved); falling back to lexical for the remainder",
            resources.len() - ordered.len(),
            resources.len()
        );
        let mut remainder: Vec<&Key> = in_degree.keys().filter(|k| !emitted.contains(*k)).collect();
        remainder.sort();
        for k in remainder {
            if let Some(&idx) = by_key.get(k) {
                ordered.push(resources[idx]);
            }
        }
    }

    let mut deque: VecDeque<(&'a Feature, &'a Resource)> = VecDeque::from(ordered);
    let mut out: Vec<(&'a Feature, &'a Resource)> = Vec::with_capacity(deque.len());
    while let Some(item) = deque.pop_front() {
        out.push(item);
    }
    out
}

pub(super) fn foreign_key_owner<'a>(
    module: &'a Module,
    feature: &'a Feature,
    qname: &'a lazuli_ir::QualifiedName,
    cross_index: &CrossFeatureIndex<'a>,
) -> Option<&'a str> {
    let _ = feature;
    // `feature` is unused today — kept in the signature so call sites
    // that pass the referencing feature don't have to drop the argument
    // when this filter widens (e.g. allowing same-feature self-refs to
    // become FKs only when a flag is set).
    // Resolve the owning feature regardless of whether the reference is
    // cross-feature or same-feature. The previous filter dropped
    // same-feature refs entirely, which silently lost referential
    // integrity for in-feature relations (e.g. a `Category.parent:
    // Category` parent-child link emitted no FK constraint at all).
    //
    // Discovered while writing the cross-feature regression test for
    // bug #9 (2026-05-15) — `Membership.workspace: Workspace` in the
    // same feature as `Workspace` produced no FK while the cross-feature
    // `Membership.user: User` did. Now both paths emit an FK pointing at
    // the matching `CREATE TABLE`.
    let owner = match qname.feature.as_deref() {
        Some(owner) => Some(owner),
        None => cross_index.owner(&qname.name),
    }?;

    feature_declares_resource(module, owner, &qname.name).then_some(owner)
}
