//! Intra-feature resource-relation graph — shared builder for the
//! cohesion family (`LZI-FEATURE-COHESION-002` and future 0009 rules).
//!
//! This module is infrastructure, not a rule: it has no severity and
//! emits no diagnostic. It builds the undirected graph whose nodes are
//! the resources a `Feature` declares and whose edges are the
//! relational links *between two resources of the same feature*, then
//! partitions the nodes into connected components via union-find.
//!
//! Fires when — N/A (helper; the consuming rule
//! `feature_cohesion_002` decides when to fire). Documented here only
//! so the meta-lint sees a trigger cue + severity disclaimer for a
//! non-rule module: severity is delegated to the caller.
//!
//! ## Edge model (intra-feature only)
//!
//! An undirected edge connects resources `A` and `B` of the same
//! `Feature` when any of these holds (or its mirror):
//!
//! - **FK field** — `A` declares a field whose type resolves to `B`
//!   (`TypeRef::UserDefined(B)` — a belongs-to / foreign-key column).
//! - **`has_many`** — `A` declares a collection field of `B`
//!   (`TypeRef::Many(UserDefined(B))`), or an M:N `many_through ... to B`.
//! - **`on_delete` / polymorphic** — `A` declares a `polymorphic_ref`
//!   whose `targets` include `B`.
//!
//! Cross-feature references (`Field.cross_feature_target`, reached via
//! `uses`) are deliberately **not** edges: the graph is intra-feature,
//! so an FK that points at a resource owned by another feature cannot
//! bridge two otherwise-disconnected clusters of this feature. That is
//! what keeps the signal honest — `platform.lzi`'s three resources have
//! no *same-feature* link, even if each separately points outward.
//!
//! A feature with 0 or 1 resources is trivially one component.

use std::collections::BTreeMap;

use lazuli_ir::{Feature, Resource, TypeRef};

/// The connected components of a feature's intra-feature resource
/// graph, each rendered as a sorted list of resource names.
///
/// Components themselves are ordered by their lexicographically-smallest
/// member, so the output is deterministic across runs (no HashMap
/// iteration order leaking into diagnostics).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::lzi_hygiene::cohesion_graph::components;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature");
/// let comps = components(&feature);
/// if comps.len() >= 2 {
///     // bundles independent capabilities
/// }
/// ```
pub fn components(feature: &Feature) -> Vec<Vec<String>> {
    let names: Vec<&str> = feature.resources.iter().map(|r| r.name.as_str()).collect();
    if names.len() <= 1 {
        // 0 or 1 resource is trivially one component. Return the single
        // node (if any) as a one-element component so callers can render
        // it uniformly.
        return names.into_iter().map(|n| vec![n.to_owned()]).collect();
    }

    // Map resource name → dense index for union-find.
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, name) in names.iter().enumerate() {
        index.insert(name, i);
    }

    let mut uf = UnionFind::new(names.len());

    for resource in &feature.resources {
        let Some(&a) = index.get(resource.name.as_str()) else {
            continue;
        };
        for target in intra_feature_targets(resource) {
            if let Some(&b) = index.get(target.as_str()) {
                if a != b {
                    uf.union(a, b);
                }
            }
        }
    }

    // Group node indices by their union-find root.
    let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (i, name) in names.iter().enumerate() {
        let root = uf.find(i);
        groups.entry(root).or_default().push((*name).to_owned());
    }

    let mut out: Vec<Vec<String>> = groups
        .into_values()
        .map(|mut members| {
            members.sort();
            members
        })
        .collect();
    // Order components by their smallest member for stable output.
    out.sort_by(|x, y| x.first().cmp(&y.first()));
    out
}

/// All same-feature resource names that `resource` declares a relation
/// to: belongs-to / FK fields, `has_many` collections, `many_through`
/// partners, and polymorphic-ref targets. Cross-feature targets are
/// excluded (the caller filters to same-feature names anyway, but FK
/// fields carrying `cross_feature_target` are skipped here so a logical
/// cross-feature pointer never masquerades as an intra-feature edge).
fn intra_feature_targets(resource: &Resource) -> Vec<String> {
    let mut targets = Vec::new();

    for field in &resource.fields {
        // GAP-12 cross-feature FK: explicitly NOT an intra-feature edge.
        if field.cross_feature_target.is_some() {
            continue;
        }
        if let Some(name) = resource_name_of_type(&field.type_ref) {
            targets.push(name);
        }
    }

    // GAP-07 `many_through <Junction> to <Partner>` — the partner is an
    // intra-feature relation (the junction is a synthesized sibling).
    for mt in &resource.many_through {
        targets.push(mt.partner.clone());
        targets.push(mt.junction.clone());
    }

    // GAP-13 polymorphic refs — each declared target is a relation
    // (an `on_delete`-style discriminated FK).
    for poly in &resource.polymorphic_refs {
        for t in &poly.targets {
            targets.push(t.clone());
        }
    }

    targets
}

/// The resource name a field type points at, if the type is a
/// user-defined resource reference (single FK) or a collection of one
/// (`has_many`). Builtins, enums, and capabilities are not relations.
fn resource_name_of_type(type_ref: &TypeRef) -> Option<String> {
    match type_ref {
        TypeRef::UserDefined(q) => Some(q.name.clone()),
        TypeRef::Many(inner) => resource_name_of_type(inner),
        _ => None,
    }
}

/// Classic union-find (disjoint-set) with path compression + union by
/// rank. Indices are dense `0..n`.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let root = self.find(self.parent[x]);
            self.parent[x] = root;
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let skeletons =
            lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
        lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
    }

    #[test]
    fn zero_resources_is_one_empty_component_list() {
        let feature = lower("feature empty\n");
        // No resources → no components.
        assert!(components(&feature).is_empty());
    }

    #[test]
    fn single_resource_is_one_component() {
        let feature = lower(
            r#"
feature solo
  resource Widget
    label: Text required
"#,
        );
        let comps = components(&feature);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0], vec!["Widget".to_string()]);
    }

    #[test]
    fn fk_field_connects_two_resources() {
        let feature = lower(
            r#"
feature shop
  resource Order
    customer: Customer required
  resource Customer
    name: Text required
"#,
        );
        let comps = components(&feature);
        assert_eq!(comps.len(), 1, "FK should connect Order↔Customer: {comps:?}");
    }

    #[test]
    fn unrelated_resources_are_separate_components() {
        let feature = lower(
            r#"
feature platform
  resource LegalDoc
    body: Text required
  resource PlatformConfig
    key: Text required
  resource DataRequest
    email: Text required
"#,
        );
        let comps = components(&feature);
        assert_eq!(comps.len(), 3, "no edges → 3 components: {comps:?}");
    }
}
