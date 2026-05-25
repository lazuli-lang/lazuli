//! `conventions [...]` auto-synthesis cluster.
//!
//! The closed-catalog convention vocabulary (`crud`, `me`) lifts a
//! resource's field list into the canonical Command / Query IR shapes
//! described by `ir-resource-conventions-crud.md` (5 entries) and
//! `ir-resource-conventions-me.md` (1 entry). The synthesis pass is
//! purely additive — author-written same-named items always win
//! (override semantics §6).
//!
//! ## RULE-VOCAB-03 zero-workflow guarantee
//!
//! Every `if` / `match` in this module is **authoring-time dispatch** —
//! it picks which IR node shape the synth pass emits. The emitted IR
//! itself contains zero control flow; downstream codegen lowers each
//! synthesized command/query to one fixed SQL per crud §7 / me §7.
//!
//! ## Module layout
//!
//! This module is the single public entry — `synthesize_conventions` —
//! plus the per-cluster helpers in the sibling modules:
//!
//! * `diagnostics` — `ConventionSynthDiagnostic` enum + `CrudSynthDiagnostic` alias.
//! * `me_mode` — `MeMode`, `classify_me_mode`, `build_lookup_my_query`.
//! * `fields` — `CategorisedFields`, `categorize_fields`, input projection.
//! * `owner_scope` — `OwnerScopeResolution`, `resolve_owner_scope`, test exports.
//! * `signature` — `CanonicalReturn` + `check_*_signature_mismatch`.
//! * `crud` — `build_*_command` / `build_*_query` + shared synth defaults.

use crate::helpers::pascal_to_snake;

use lazuli_ir as ir;

mod crud;
mod diagnostics;
mod fields;
mod me_mode;
mod owner_scope;
mod signature;

pub use diagnostics::{ConventionSynthDiagnostic, CrudSynthDiagnostic};
pub use owner_scope::{build_owner_scope_cte_prefix_for_test, build_owner_scope_where_for_test};

pub(crate) use crud::build_list_query;

use crud::{
    build_create_command, build_delete_command, build_lookup_query, build_update_command,
    synth_crud_invalidates,
};
use fields::categorize_fields;
use me_mode::{build_lookup_my_query, check_me_lookup_signature_mismatch, classify_me_mode};
use owner_scope::{OwnerScopeResolution, resolve_owner_scope};
use signature::{
    CanonicalReturn, check_command_signature_mismatch, check_query_signature_mismatch,
};

// =============================================================================
// `conventions [crud]` auto-synthesis pass
// =============================================================================
//
// Spec: `docs/proposals/ir-resource-conventions-crud.md` §5.
//
// For each `Resource` with `ConventionRef::Crud` in `conventions`, the
// pass appends 5 entries to the feature (3 commands + 2 queries) using
// the shapes from §5.2 through §5.6. Override semantics (§6): any name
// already authored in the feature is left alone — no warning, no
// `@deprecated`, no doctor flag. The other 4 still synthesize.
//
// RULE-VOCAB-03 (§7) — zero workflow: each synth maps to exactly one
// of the existing declarative IR shapes (`CommandEffect::Creates` /
// `Updates` / `Deletes`, `Query::Lookup`, `Query::List`). No new
// lowering path is introduced; the pass just produces IR nodes the
// existing emitters already know how to lower to one SQL each.
//
// Diagnostics (§11) returned via the `Vec<CrudSynthDiagnostic>` return
// value — Cell C4 wires them to the user-facing doctor surface.

/// Run the `conventions [...]` auto-synthesis pass on a feature.
/// Today covers two bundles in catalog order: `crud` (5 entries) and
/// `me` (1 entry — `lookup_my_<resource>`). Returns diagnostics from
/// crud §11 + me §11; Cell C4 / M3 wires the user-facing rendering.
/// Public so doctor / tests can call it directly.
///
/// **RULE-VOCAB-03 (crud §7 + me §7) — zero workflow:** every `if`/
/// `match` in this function is **authoring-time** dispatch — it
/// selects which IR node shape to emit. The emitted IR nodes contain
/// zero control flow; downstream codegen lowers each to one fixed
/// SQL per crud §7 / me §7.
pub fn synthesize_conventions(feature: &mut ir::Feature) -> Vec<CrudSynthDiagnostic> {
    let mut diagnostics: Vec<CrudSynthDiagnostic> = Vec::new();
    let mut to_add_commands: Vec<ir::Command> = Vec::new();
    let mut to_add_queries: Vec<ir::Query> = Vec::new();
    // §11 inspect surface (Cell C4 / M3) — Feature.synth_origins
    // records every name in a convention's set: `Synthesized(<bundle>)`
    // for names the pass appended; `AuthorOverride(<bundle>)` for names
    // the author wrote (synth skipped per crud §6 / me §6). Inspect
    // uses these markers to render `[conv:<bundle>]` /
    // `[author override; convention skipped]`.
    let mut synth_origins_inserts: Vec<(String, ir::ConventionOrigin)> = Vec::new();

    let existing_command_names: std::collections::HashSet<String> =
        feature.commands.iter().map(|c| c.name.clone()).collect();
    let existing_query_names: std::collections::HashSet<String> = feature
        .queries
        .iter()
        .map(|q| q.name().to_owned())
        .collect();

    // §5.8 — default policy is the feature's `authenticated` policy.
    let has_authenticated = feature
        .policies
        .categories
        .iter()
        .any(|p| p.name == "authenticated");

    for resource in &feature.resources {
        // Per-bundle dispatch — each resource may declare zero, one,
        // or both bundles in `conventions [...]`. Bundle blocks are
        // independent; the override-collision logic (`existing_*`
        // sets) is shared. crud §6.1 / me §6.1: zero name collisions
        // by construction because `crud` owns `lookup_<r>` while `me`
        // owns `lookup_my_<r>`.
        let has_crud = resource.conventions.contains(&ir::ConventionRef::Crud);
        let has_me = resource.conventions.contains(&ir::ConventionRef::Me);
        if !has_crud && !has_me {
            continue;
        }

        // owner-scope §7.3 — resolve once per resource so the crud and
        // me blocks share one decision. Composability §5.3 / §6.1:
        // one annotation drives mode for every bundle that synths
        // against the resource. Diagnostics (§11.1) are pushed
        // regardless of which bundles are active (they're a property
        // of the resource shape, not of the bundle).
        let owner_scope = resolve_owner_scope(feature, resource, &mut diagnostics);

        // ===== `crud` bundle (§5) — gated; runs only when declared. =====
        if has_crud {
            // §5.8 — guard: policy `authenticated` must exist.
            if !has_authenticated {
                diagnostics.push(CrudSynthDiagnostic::PolicyNotFound {
                    resource: resource.name.clone(),
                });
                // We still synthesize with `PolicyRef::Local("authenticated")`
                // even though it's unresolved — Cell C4 will surface the
                // diagnostic; the IR shape stays uniform. This mirrors the
                // FR-3a auto-photo precedent (which returns silently when
                // no policy is found; here we surface a typed diagnostic
                // instead).
            }

            let categorised = categorize_fields(resource);

            // §11 `crud_synth_no_required_fields` — `create.input` would be
            // empty if every required-on-resource field is Tenant or Auto.
            // Detect by looking at the create-input list.
            let create_input_fields = categorised.create_input_fields();
            if create_input_fields.is_empty() {
                diagnostics.push(CrudSynthDiagnostic::NoRequiredFields {
                    resource: resource.name.clone(),
                });
            }

            let resource_snake = pascal_to_snake(&resource.name);

            // §5.1 — the 5 synth names, in canonical order.
            let create_name = format!("create_{}", resource_snake);
            let update_name = format!("update_{}", resource_snake);
            let delete_name = format!("delete_{}", resource_snake);
            let lookup_name = format!("lookup_{}", resource_snake);
            let list_name = format!("list_{}s", resource_snake);

            // §6 — per-name override. If the author wrote the same name we
            // skip *just that name* with no warning, unless the author's
            // signature diverges from the canonical shape — that lands the
            // `crud_synth_author_signature_mismatch` diagnostic (§11 / §9).
            //
            // The `if existing_*.contains(...)` checks below are
            // authoring-time controls (which synth to add), NOT lowering
            // control flow over the emitted IR — RULE-VOCAB-03 (§7) is
            // preserved.

            // 1) create_<resource>
            if existing_command_names.contains(&create_name) {
                if let Some(reason) = check_command_signature_mismatch(
                    feature,
                    &create_name,
                    &create_input_fields,
                    CanonicalReturn::CreatesResource(&resource.name),
                ) {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: create_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    create_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                let mut cmd =
                    build_create_command(&create_name, &resource.name, &create_input_fields);
                // §8.5.A — owner-scope create-side CTE-INSERT. The CREATE
                // synth carries the *full* OwnerScopeSql (cte_owner_check
                // populated) so codegen can paste the CTE prefix in front
                // of the INSERT. Tenant-only resources keep
                // `owner_scope_sql: None` and emit the same shape as
                // before this cell.
                if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                    cmd.owner_scope_sql = Some(scope.clone());
                }
                cmd.invalidates =
                    synth_crud_invalidates(&lookup_name, &list_name, has_me, &resource_snake);
                to_add_commands.push(cmd);
                synth_origins_inserts.push((
                    create_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }

            // 2) update_<resource>
            if existing_command_names.contains(&update_name) {
                let canonical_update_inputs = categorised.update_input_fields();
                if let Some(reason) = check_command_signature_mismatch(
                    feature,
                    &update_name,
                    &canonical_update_inputs,
                    CanonicalReturn::UpdatesResource(&resource.name),
                ) {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: update_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    update_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                let mut cmd = build_update_command(
                    &update_name,
                    &resource.name,
                    &categorised.update_input_fields(),
                );
                // §8.2 — owner-scope WHERE on UPDATE. The carrier carries
                // ONLY the `where_predicate`; codegen drops the
                // `cte_owner_check` (None here, since UPDATE doesn't need
                // the CTE wrapper). We share the resolution by cloning;
                // codegen reads only what it needs per shape.
                if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                    cmd.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
                cmd.invalidates =
                    synth_crud_invalidates(&lookup_name, &list_name, has_me, &resource_snake);
                to_add_commands.push(cmd);
                synth_origins_inserts.push((
                    update_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }

            // 3) delete_<resource>
            if existing_command_names.contains(&delete_name) {
                if let Some(reason) = check_command_signature_mismatch(
                    feature,
                    &delete_name,
                    &[],
                    CanonicalReturn::DeletesResource(&resource.name),
                ) {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: delete_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    delete_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                let mut cmd = build_delete_command(&delete_name, &resource.name);
                // §8.1 — owner-scope WHERE on DELETE. Same shape as the
                // pre-absorption hand-rolled handler in §1.1 trigger
                // evidence. CTE not used on DELETE; only the predicate.
                if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                    cmd.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
                cmd.invalidates =
                    synth_crud_invalidates(&lookup_name, &list_name, has_me, &resource_snake);
                to_add_commands.push(cmd);
                synth_origins_inserts.push((
                    delete_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }

            // 4) lookup_<resource>
            let mut canonical_lookup = build_lookup_query(&lookup_name, &resource.name);
            // §8.3 — owner-scope WHERE on LOOKUP. The Lookup query's
            // canonical keys (id = $1) get extended with the chain
            // predicate emitted by codegen via `owner_scope_sql`.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                if let ir::Query::Lookup(lq) = &mut canonical_lookup {
                    lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
            }
            if existing_query_names.contains(&lookup_name) {
                if let Some(reason) =
                    check_query_signature_mismatch(feature, &lookup_name, &canonical_lookup)
                {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: lookup_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    lookup_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                to_add_queries.push(canonical_lookup);
                synth_origins_inserts.push((
                    lookup_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }

            // 5) list_<resource>s
            let mut canonical_list = build_list_query(&list_name, &resource.name);
            // §8.4 — owner-scope WHERE on LIST. Same predicate; the
            // synth's pagination shape is unaffected.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                if let ir::Query::List(lq) = &mut canonical_list {
                    lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
            }
            if existing_query_names.contains(&list_name) {
                if let Some(reason) =
                    check_query_signature_mismatch(feature, &list_name, &canonical_list)
                {
                    diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                        resource: resource.name.clone(),
                        synth_name: list_name.clone(),
                        reason,
                    });
                }
                synth_origins_inserts.push((
                    list_name.clone(),
                    ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
                ));
            } else {
                to_add_queries.push(canonical_list);
                synth_origins_inserts.push((
                    list_name.clone(),
                    ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
                ));
            }
        } // ===== end `crud` bundle =====

        // ===== `me` bundle (me §5) — singleton-per-actor lookup. =====
        //
        // Authoring-time mode classification (me §5.3). The synth picks
        // ONE of four shapes from the resource's static structure; the
        // emitted IR node contains zero branches (me §7 / RULE-VOCAB-03).
        if has_me {
            // me §5.4 — default policy is `authenticated`. Reuses the
            // crud policy probe; a missing policy emits the diagnostic
            // (no _Me suffix on the variant — `PolicyNotFound` covers
            // both bundles since the policy slot has the same name).
            if !has_authenticated {
                // Only emit once per resource even if both bundles
                // declared `me` and `crud`; the crud block above will
                // have already pushed `PolicyNotFound` if it ran.
                // Dedupe by inspecting `diagnostics` for an existing
                // entry on this resource.
                let already_emitted = diagnostics.iter().any(|d| {
                    matches!(
                        d,
                        ConventionSynthDiagnostic::PolicyNotFound { resource: r }
                            if r == &resource.name
                    )
                });
                if !already_emitted {
                    diagnostics.push(ConventionSynthDiagnostic::PolicyNotFound {
                        resource: resource.name.clone(),
                    });
                }
            }

            let resource_snake = pascal_to_snake(&resource.name);
            let lookup_my_name = format!("lookup_my_{}", resource_snake);

            // me §5.3 — classify the resource's actor axis. Four-mode
            // closed table; `None` triggers `me_synth_no_actor_resolution`.
            // The classification is a STATIC truth table over resource
            // shape; no runtime branching is introduced into the
            // emitted IR.
            let mode = classify_me_mode(resource);

            match mode {
                Some(m) => {
                    // me §6 — per-name override. Author wrote
                    // `lookup_my_<resource>` (or the `query.lookup
                    // my_<resource>` declarative form, which lowers to
                    // the same IR `Query::Lookup` name).
                    if existing_query_names.contains(&lookup_my_name) {
                        if let Some(reason) = check_me_lookup_signature_mismatch(
                            feature,
                            &lookup_my_name,
                            &resource.name,
                        ) {
                            diagnostics.push(ConventionSynthDiagnostic::MeSignatureMismatch {
                                resource: resource.name.clone(),
                                synth_name: lookup_my_name.clone(),
                                reason,
                            });
                        }
                        synth_origins_inserts.push((
                            lookup_my_name.clone(),
                            ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me),
                        ));
                    } else {
                        let mut q = build_lookup_my_query(&lookup_my_name, &resource.name, m);
                        // §6.1 composition — `[crud, me]` + `@owner_axis`
                        // composes uniformly: the `me` synth also reads
                        // the resource-level annotation and appends the
                        // chain predicate. The unique-user variant is
                        // mutually exclusive with `@owner_axis` per
                        // §11.1 collision check, so this path only
                        // attaches scope when the resource is NOT
                        // user-keyed and the resolution succeeded.
                        if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                            if let ir::Query::Lookup(lq) = &mut q {
                                lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                                    cte_owner_check: None,
                                    ..scope.clone()
                                });
                            }
                        }
                        to_add_queries.push(q);
                        synth_origins_inserts.push((
                            lookup_my_name.clone(),
                            ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me),
                        ));
                    }
                }
                None => {
                    // me §11.1 — no actor axis. Resource has no `user`
                    // field, no `org` field, and is not itself the
                    // `User` resource. The synth has no key to filter
                    // on; emit diagnostic, skip synth.
                    diagnostics.push(ConventionSynthDiagnostic::MeNoActorResolution {
                        resource: resource.name.clone(),
                    });
                }
            }
        } // ===== end `me` bundle =====
    }

    feature.commands.extend(to_add_commands);
    feature.queries.extend(to_add_queries);
    feature.synth_origins.extend(synth_origins_inserts);
    diagnostics
}

// Per-cluster helpers live in sibling modules:
//   - me_mode.rs       MeMode + classify_me_mode + build_lookup_my_query
//                      + check_me_lookup_signature_mismatch
//   - fields.rs        CategorisedFields + categorize_fields +
//                      input_to_command_input + input_field_assignments
//   - owner_scope.rs   OwnerScopeResolution + resolve_owner_scope +
//                      build_owner_scope_*_for_test
//   - signature.rs     CanonicalReturn + check_command_signature_mismatch
//                      + check_query_signature_mismatch
//   - crud.rs          build_create_/update_/delete_command +
//                      build_lookup_/list_query + synth_crud_invalidates
//                      + default_synth_command + crud_write_rate_limit
//   - diagnostics.rs   ConventionSynthDiagnostic + CrudSynthDiagnostic alias
