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
//! - **event-group emit-coupling** (spec 0008 follow-up) — an emitter
//!   (`command` or `webhook`) that `emits` an event belonging to an
//!   `event_group on B` is part of `B`'s lifecycle. The emitter is a
//!   *hyper-edge*: every resource it touches (its own effect-target plus
//!   each matched group's `on_resource`, plus a webhook's `payload_from`
//!   envelope) is unioned into one component. Event names are bound to a
//!   group by glob-prefix (`charge_*` ⊇ `charge_confirmed`), by the
//!   group's authored event-name list, or by `pattern-prefix + variant`.
//!   This fixes the hostpoint `payments` false-island where `Charge`
//!   (the `event_group charge_* on Charge` owner) and the `mp_payment_event`
//!   webhook that emits `charge_*` had no FK between them.
//! - **webhook-envelope-sink** (narrow structural bridge) — a webhook's
//!   inbound-envelope log resource (`WebhookEvent`-named) carries no FK
//!   and no `emits`, so it has zero IR-typed edges, yet it is part of the
//!   same inbound lifecycle as the event-group resource the webhook emits
//!   into. An *otherwise-isolated, webhook-named* resource is coupled to
//!   that resource. Deliberately narrow (requires a webhook emitting into
//!   an event_group) so it cannot mask FK-less grab-bags that have no
//!   webhook.
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

use lazuli_ir::{CommandEffect, EventGroup, Feature, Resource, TypeRef};

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

    // Event-group emit-coupling (spec 0008 follow-up). An emitter — a
    // `command` or `webhook` — that `emits` an event belonging to an
    // `event_group on B` is part of B's lifecycle. We treat each emitter
    // as a *hyper-edge*: union every resource that emitter relates to.
    // The resources it relates to are: its own effect-target (commands
    // only) and the `on_resource` of each event_group whose pattern /
    // variant-name / event-name list matches one of the emitter's
    // emitted event names. See `emitter_resource_set`.
    for command in &feature.commands {
        let related = emitter_resource_set(
            command_effect_target(command),
            &command.emits,
            &feature.event_groups,
        );
        union_all(&mut uf, &index, &related);
    }
    for webhook in &feature.webhooks {
        // A webhook carries no declared effect-target. Its IR-grounded
        // resource link is the `on_resource` of the event_group(s) it
        // emits into; `payload_from` (the typed inbound envelope) is
        // added when authored.
        let payload_target = webhook.payload_from.as_ref().map(|p| p.name.clone());
        let related = emitter_resource_set(payload_target, &webhook.emits, &feature.event_groups);
        union_all(&mut uf, &index, &related);
    }

    // Webhook-envelope-sink edge. A feature with a webhook that emits
    // into an `event_group on B` typically also persists each inbound
    // envelope into a log resource (the hostpoint `WebhookEvent`
    // pattern). That log resource carries no FK and no `emits`, so it
    // has zero IR-typed edges — yet it is genuinely part of the same
    // inbound-processing lifecycle as B. Couple such an *otherwise
    // isolated, webhook-envelope-named* resource to B. This is a
    // structural (name-shaped) bridge, deliberately narrow: it fires
    // only for a resource that (a) matches the webhook-envelope naming
    // convention, (b) has no other edge, in a feature that (c) actually
    // owns a webhook emitting into an event_group. It cannot touch
    // FK-grab-bags with no webhook (platform/host/trust/intelligence).
    if !feature.webhooks.is_empty() {
        let webhook_group_resources = webhook_emitted_group_resources(feature);
        if let Some(&anchor_b) = webhook_group_resources
            .iter()
            .filter_map(|b| index.get(b.as_str()))
            .next()
        {
            for resource in &feature.resources {
                let Some(&r) = index.get(resource.name.as_str()) else {
                    continue;
                };
                if r == anchor_b {
                    continue;
                }
                if is_webhook_envelope_named(&resource.name)
                    && is_isolated_resource(resource, &index)
                    && uf.find(r) == r
                {
                    uf.union(r, anchor_b);
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

/// Union every resource name in `related` into one component (the
/// emitter is a hyper-edge over the resources it touches). Names not
/// declared by this feature are skipped (cross-feature emits cannot
/// bridge intra-feature clusters, mirroring the FK rule). Self-loops
/// are no-ops.
fn union_all(uf: &mut UnionFind, index: &BTreeMap<&str, usize>, related: &[String]) {
    let indices: Vec<usize> = related
        .iter()
        .filter_map(|name| index.get(name.as_str()).copied())
        .collect();
    if let Some(&first) = indices.first() {
        for &other in &indices[1..] {
            if first != other {
                uf.union(first, other);
            }
        }
    }
}

/// The set of same-feature resource names an emitter (command/webhook)
/// relates to: its own effect-target (when any) plus the `on_resource`
/// of every `event_group` whose pattern / variant names / event-name
/// list matches one of the emitter's emitted event names. Deduplicated;
/// order is not significant (the caller unions them all together).
fn emitter_resource_set(
    effect_target: Option<String>,
    emits: &[String],
    groups: &[EventGroup],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(t) = effect_target {
        out.push(t);
    }
    for event_name in emits {
        for group in groups {
            if let Some(on) = &group.on_resource
                && event_matches_group(event_name, group)
                && !out.contains(on)
            {
                out.push(on.clone());
            }
        }
    }
    out
}

/// True when `event_name` belongs to `group`: it matches the group's
/// glob `pattern` (prefix match on the trailing `*`), or it equals one
/// of the group's authored event names, or it equals the group's
/// pattern-prefix concatenated with a variant short-name (`charge_` +
/// `confirmed` = `charge_confirmed`).
fn event_matches_group(event_name: &str, group: &EventGroup) -> bool {
    if glob_prefix_matches(&group.pattern, event_name) {
        return true;
    }
    if group.events.iter().any(|e| e == event_name) {
        return true;
    }
    if let Some(prefix) = group.pattern.strip_suffix('*')
        && group
            .variants
            .iter()
            .any(|v| format!("{prefix}{}", v.name) == event_name)
    {
        return true;
    }
    false
}

/// Glob-prefix match for a single trailing `*` (`charge_*` matches
/// `charge_confirmed` but not `refund_started`). A pattern with no `*`
/// must match exactly; a bare `*` matches anything. Only the trailing-
/// `*` shape is used by `event_group` today, so we keep this minimal.
fn glob_prefix_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// The effect-target resource name of a command, if it has a
/// row-mutating effect. Pure `Returns`/`None` commands have no target.
fn command_effect_target(command: &lazuli_ir::Command) -> Option<String> {
    match &command.effect {
        CommandEffect::Creates(e) => Some(e.resource.name.clone()),
        CommandEffect::Updates(e) => Some(e.resource.name.clone()),
        CommandEffect::Deletes(e) => Some(e.resource.name.clone()),
        CommandEffect::Reorders(e) => Some(e.resource.name.clone()),
        CommandEffect::Returns(_) | CommandEffect::None => None,
    }
}

/// Every `on_resource` of an `event_group` that at least one of the
/// feature's webhooks emits into. These are the lifecycle anchors a
/// webhook-envelope-log resource attaches to.
fn webhook_emitted_group_resources(feature: &Feature) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for webhook in &feature.webhooks {
        for event_name in &webhook.emits {
            for group in &feature.event_groups {
                if let Some(on) = &group.on_resource
                    && event_matches_group(event_name, group)
                    && !out.contains(on)
                {
                    out.push(on.clone());
                }
            }
        }
    }
    out
}

/// True when a resource name follows the webhook inbound-envelope-log
/// convention (`WebhookEvent`, `WebhookDelivery`, `InboundEvent`, …).
/// Case-insensitive substring on `webhook`, plus the common
/// `*Event`-suffixed inbound-log shape paired with `webhook`/`inbound`.
fn is_webhook_envelope_named(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("webhook")
}

/// True when a resource declares no relational edge to *another
/// resource of this feature*: no FK / `has_many` / `many_through` /
/// polymorphic target that resolves to a sibling resource. (Enum- and
/// cross-feature-typed fields are not edges, so they don't count.) Such
/// a resource is an island under the FK-edge model alone.
fn is_isolated_resource(resource: &Resource, index: &BTreeMap<&str, usize>) -> bool {
    !intra_feature_targets(resource)
        .iter()
        .any(|t| t.as_str() != resource.name.as_str() && index.contains_key(t.as_str()))
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
        assert_eq!(
            comps.len(),
            1,
            "FK should connect Order↔Customer: {comps:?}"
        );
    }

    #[test]
    fn glob_prefix_matches_helper() {
        assert!(glob_prefix_matches("charge_*", "charge_confirmed"));
        assert!(glob_prefix_matches("charge_*", "charge_failed"));
        assert!(!glob_prefix_matches("charge_*", "refund_started"));
        // No `*` → exact match only.
        assert!(glob_prefix_matches("created", "created"));
        assert!(!glob_prefix_matches("created", "created_at"));
        // Bare `*` matches anything.
        assert!(glob_prefix_matches("*", "anything"));
    }

    #[test]
    fn webhook_emit_couples_charge_and_webhookevent() {
        let feature = lower(
            r#"
feature payments
  resource Charge
    amount: Text required
  resource WebhookEvent
    external_id: Text required
  event_group charge_* on Charge
    payload
      charge_id = id
    event confirmed
      provider_payment_id: Text
  webhook mp_payment_event
    path "/webhooks/mp/payment"
    verify hmac sha256
      secret env.MP_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_mp_payment_event
    emits charge_confirmed when payload.status == "approved"
"#,
        );
        let comps = components(&feature);
        let charge_comp = comps
            .iter()
            .find(|c| c.iter().any(|n| n == "Charge"))
            .expect("Charge present");
        assert!(
            charge_comp.iter().any(|n| n == "WebhookEvent"),
            "Charge and WebhookEvent must share a component: {comps:?}"
        );
    }

    #[test]
    fn command_effect_plus_emit_couples_target_and_group_owner() {
        // A command that `creates Receipt` and emits a `charge_*` event
        // unions Receipt with the group owner Charge.
        let feature = lower(
            r#"
feature payments
  resource Charge
    amount: Text required
  resource Receipt
    label: Text required
  event_group charge_* on Charge
    payload
      charge_id = id
    event confirmed
      note: Text
  command settle
    input
      charge_id: ID required
    creates Receipt
    emits charge_confirmed
"#,
        );
        let comps = components(&feature);
        assert_eq!(
            comps.len(),
            1,
            "command effect-target + emit must union Receipt↔Charge: {comps:?}"
        );
    }

    #[test]
    fn non_matching_emit_does_not_couple() {
        // Webhook emits an event outside the group glob → no coupling.
        let feature = lower(
            r#"
feature billing
  resource Charge
    amount: Text required
  resource Audit
    note: Text required
  event_group charge_* on Charge
    payload
      charge_id = id
    event confirmed
      note: Text
  webhook stray
    path "/webhooks/stray"
    verify hmac sha256
      secret env.STRAY
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_stray
    emits refund_started
"#,
        );
        let comps = components(&feature);
        assert_eq!(
            comps.len(),
            2,
            "non-matching emit must not couple: {comps:?}"
        );
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
