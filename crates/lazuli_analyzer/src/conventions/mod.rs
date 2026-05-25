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
//! * `ConventionSynthDiagnostic` (with `CrudSynthDiagnostic` alias)
//!   — §11 diagnostics surfaced by the synth pass.
//! * `synthesize_conventions` — single public entry. Mutates a
//!   feature in place; returns the diagnostic vector.
//! * `MeMode` / `classify_me_mode` — me §5.3 actor-axis taxonomy.
//! * `CategorisedFields` / `categorize_fields` — crud §5.7 field
//!   group projection (Tenant / Auto / Required / Optional).
//! * `OwnerScopeResolution` / `resolve_owner_scope` +
//!   `build_owner_scope_where_for_test` /
//!   `build_owner_scope_cte_prefix_for_test` — `@owner_axis`
//!   composition rules.
//! * `build_create_command` / `build_update_command` /
//!   `build_delete_command` / `build_lookup_query` /
//!   `build_list_query` / `build_lookup_my_query` — the per-bundle
//!   IR builders.
//! * `check_command_signature_mismatch` /
//!   `check_query_signature_mismatch` — author-override sanity
//!   checks (warns when an author-written same-named callable
//!   diverges from the canonical shape).
//! * `synth_crud_invalidates` / `default_synth_command` /
//!   `crud_write_rate_limit` / `input_to_command_input` /
//!   `input_field_assignments` — small shared synth helpers.

use crate::helpers::{levenshtein, pascal_to_snake, quoted_ident, quoted_table};

use lazuli_ir as ir;

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

/// §11 diagnostic codes emitted by `synthesize_conventions`. Cell C4
/// formats these into user-facing strings; Cell C3 just records them.
///
/// Originally `CrudSynthDiagnostic`; extended to cover both `crud` and
/// `me` bundles when M2 of `ir-resource-conventions-me` landed
/// (variants prefixed `Me*`). A type alias preserves the legacy name
/// for any callers; the canonical name going forward is
/// `ConventionSynthDiagnostic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConventionSynthDiagnostic {
    /// `crud_synth_policy_not_found` — feature has no `authenticated`
    /// policy. Carries the resource name for the suggestion. Also fires
    /// for `me_synth_policy_not_found` (the `me` bundle reuses
    /// `authenticated` per `ir-resource-conventions-me.md` §5.4); Cell
    /// M3 selects the user-visible code by reading
    /// `resource.conventions`.
    PolicyNotFound { resource: String },
    /// `crud_synth_no_required_fields` — every required field is in the
    /// Tenant or Auto group, so `create_<resource>.input` would be
    /// empty. Likely an authoring mistake. Crud-only.
    NoRequiredFields { resource: String },
    /// `@correctness.crud_synth_author_signature_mismatch` — author wrote a same-named
    /// command/query but its input field list or return type diverges
    /// from the canonical convention shape. Carries the resource +
    /// synth name + a short reason for Cell C4 to format. Crud-only.
    SignatureMismatch {
        resource: String,
        synth_name: String,
        reason: String,
    },
    /// `me_synth_no_actor_resolution` — resource declared
    /// `conventions [me]` but has neither `user: User required` nor
    /// `org: Org required` AND is not itself named `User`. The synth
    /// has no key to filter on. See
    /// `ir-resource-conventions-me.md` §11.1 (named
    /// `me_synth_no_owner_axis` in the proposal; M2's diagnostic key
    /// is `me_synth_no_actor_resolution` per the cell brief — same
    /// condition, more explicit wording).
    MeNoActorResolution { resource: String },
    /// `me_synth_signature_mismatch` — author wrote
    /// `query lookup_my_<resource>` (or the declarative
    /// `query.lookup my_<resource>`) whose return shape diverges
    /// from the canonical `me` synth (route-less Lookup query
    /// returning the resource row).
    MeSignatureMismatch {
        resource: String,
        synth_name: String,
        reason: String,
    },
    /// `owner_axis_unknown_through` — `@owner_axis(through: <col>)`
    /// names a column that doesn't exist on the FK target resource.
    /// O3 formats the user-facing message with a nearest-name hint.
    /// See `ir-resource-conventions-owner-scope.md` §7.4 + §11.1.
    OwnerAxisUnknownThrough {
        resource: String,
        field: String,
        through: String,
        fk_target: String,
        suggestion: Option<String>,
    },
    /// `owner_axis_through_not_user_keyed` — the FK target's `through:`
    /// column is not typed as `User` (or `@semantic.UserID`). The
    /// emitted chain can't resolve to `ctx.User.ID`. O3 surfaces this
    /// as a warning. See §7.4 + §11.1.
    OwnerAxisThroughNotUserKeyed {
        resource: String,
        field: String,
        through: String,
        fk_target: String,
    },
    /// `owner_axis_collides_with_unique_user` — the resource carries
    /// BOTH `user: User required unique` AND
    /// `@owner_axis(through: <col>)` on another field. The two scopes
    /// would compose redundantly; the unique-user mode already
    /// provides ownership. See §7.4 + §11.1.
    OwnerAxisCollidesWithUniqueUser { resource: String, field: String },
}

/// Legacy alias preserved during the M2 rename. Downstream code should
/// migrate to `ConventionSynthDiagnostic` over time; M3 carries the
/// final downstream migration into doctor / inspect.
pub type CrudSynthDiagnostic = ConventionSynthDiagnostic;

impl ConventionSynthDiagnostic {
    pub fn diagnostic_code(&self) -> &'static str {
        match self {
            ConventionSynthDiagnostic::SignatureMismatch { .. } => {
                "@correctness.crud_synth_author_signature_mismatch"
            }
            ConventionSynthDiagnostic::PolicyNotFound { .. } => "crud_synth_policy_not_found",
            ConventionSynthDiagnostic::NoRequiredFields { .. } => "crud_synth_no_required_fields",
            ConventionSynthDiagnostic::MeNoActorResolution { .. } => "me_synth_no_actor_resolution",
            ConventionSynthDiagnostic::MeSignatureMismatch { .. } => "me_synth_signature_mismatch",
            ConventionSynthDiagnostic::OwnerAxisUnknownThrough { .. } => {
                "owner_axis_unknown_through"
            }
            ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed { .. } => {
                "owner_axis_through_not_user_keyed"
            }
            ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser { .. } => {
                "owner_axis_collides_with_unique_user"
            }
        }
    }

    pub fn severity(&self) -> &'static str {
        match self {
            ConventionSynthDiagnostic::SignatureMismatch { .. } => "warning",
            _ => "error",
        }
    }
}

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

// =============================================================================
// `conventions [me]` synthesis helpers — Cell M2
//
// Spec: `docs/proposals/ir-resource-conventions-me.md` §§5.3, 5.5, 5.6.
//
// `classify_me_mode` is the entire decision surface — a 4-row truth
// table over resource shape, evaluated once at synth time. Once a mode
// is picked, `build_lookup_my_query` emits ONE fixed `Query::Lookup`
// shape with a mode-specific `keys` vector (the WHERE-clause builder).
// **The emitted IR contains zero branches** — RULE-VOCAB-03 (me §7).
// =============================================================================

/// me §5.3 — the four key-resolution modes. Classification is static;
/// each variant carries no runtime state. The selected variant uniquely
/// determines the `KeyClause` vector emitted into the synthesized
/// `Query::Lookup` (me §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeMode {
    /// Resource has `user: User required` (with or without `unique`)
    /// AND an `org`-bearing field. `WHERE org_id = ctx.User.OrgID AND
    /// "user" = ctx.User.ID`.
    UserKeyed,
    /// Resource has `user: User required` AND no `org` field.
    /// `WHERE "user" = ctx.User.ID`.
    UserKeyedNoOrg,
    /// Resource has `org: Org required` AND no `user: User required`.
    /// `WHERE org_id = ctx.User.OrgID`.
    OrgKeyed,
    /// Resource IS the User table (name == "User"). `WHERE id = ctx.User.ID`.
    SelfKeyed,
}

/// me §5.3 — classify the resource's actor axis. Pure inspection of the
/// resource's field list + name. Returns `None` only when the resource
/// has neither `user` nor `org` and is not named `User`, triggering
/// `me_synth_no_actor_resolution`.
///
/// **RULE-VOCAB-03 affirmation**: this function is the entire
/// authoring-time decision surface for the `me` bundle. Its `if`/`match`
/// statements pick which IR shape the *synth pass* emits; the emitted
/// IR contains exactly one fixed `Query::Lookup` per call site, with
/// no branches in the runtime lowering path.
pub(crate) fn classify_me_mode(resource: &ir::Resource) -> Option<MeMode> {
    // me §5.3 row 4 — `self_keyed`: the resource IS the User table.
    // Checked first because a resource literally named `User` could in
    // principle declare its own `user` self-reference field; the
    // self-keyed shape (`WHERE id = ctx.User.ID`) is the correct one.
    if resource.name == "User" {
        return Some(MeMode::SelfKeyed);
    }

    let has_user_required = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });
    let has_org_field = resource.fields.iter().any(|f| {
        f.name == "org" && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "Org")
    });

    // me §5.3 rows 1 and 2 — `user_keyed` variants.
    if has_user_required {
        if has_org_field {
            return Some(MeMode::UserKeyed);
        }
        return Some(MeMode::UserKeyedNoOrg);
    }

    // me §5.3 row 3 — `org_keyed` (org-singleton resource).
    let has_org_required = resource.fields.iter().any(|f| {
        f.name == "org"
            && f.required
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "Org")
    });
    if has_org_required {
        return Some(MeMode::OrgKeyed);
    }

    // me §5.3 row 5 — no key. Diagnostic.
    None
}

/// me §5.2 — build the `lookup_my_<resource>` `Query::Lookup` IR. The
/// `keys` vector is the WHERE-clause builder; its shape is fixed by
/// `mode` at synth time (me §7 / RULE-VOCAB-03). The emitted IR carries
/// no `params` (route-less per me §5.2) and no `filters`.
///
/// Path-on-the-right-hand-side uses `ctx.User.*` paths, mirroring the
/// already-proven IR shape used by hand-authored
/// `query.lookup ... filters` blocks (e.g.,
/// `traveler.lzi:79-83` references `ctx.actor.user_id`; the IR-level
/// `KeyClause.equals` carries an `Expr::Path` per `Path::from_segments`).
pub(crate) fn build_lookup_my_query(name: &str, resource: &str, mode: MeMode) -> ir::Query {
    let _ = resource; // reserved for future signature-mismatch detail
    // Runtime `readCtx` (runtime/go/lazuli/handle.go:893) accepts only
    // canonical snake-case ctx paths: `actor.user_id` / `actor.org_id`.
    // PascalCase variants (`ctx.User.OrgID`) fall through to default
    // and return 500 "unknown ctx path". Emit the canonical segments
    // matching the existing commands' FromCtx convention.
    let keys: Vec<ir::KeyClause> = match mode {
        // §5.3 user_keyed: WHERE org_id = ctx.actor.org_id AND "user" = ctx.actor.user_id
        MeMode::UserKeyed => vec![
            ir::KeyClause {
                path: ir::Path::from_segments(["org".to_owned()]),
                equals: ir::Expr::Path(ir::Path::from_segments([
                    "ctx".to_owned(),
                    "actor".to_owned(),
                    "org_id".to_owned(),
                ])),
            },
            ir::KeyClause {
                path: ir::Path::from_segments(["user".to_owned()]),
                equals: ir::Expr::Path(ir::Path::from_segments([
                    "ctx".to_owned(),
                    "actor".to_owned(),
                    "user_id".to_owned(),
                ])),
            },
        ],
        // §5.3 user_keyed_no_org: WHERE "user" = ctx.actor.user_id
        MeMode::UserKeyedNoOrg => vec![ir::KeyClause {
            path: ir::Path::from_segments(["user".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "user_id".to_owned(),
            ])),
        }],
        // §5.3 org_keyed: WHERE org_id = ctx.actor.org_id
        MeMode::OrgKeyed => vec![ir::KeyClause {
            path: ir::Path::from_segments(["org".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "org_id".to_owned(),
            ])),
        }],
        // §5.3 self_keyed: WHERE id = ctx.actor.user_id
        MeMode::SelfKeyed => vec![ir::KeyClause {
            path: ir::Path::from_segments(["id".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "user_id".to_owned(),
            ])),
        }],
    };

    ir::Query::Lookup(ir::LookupQuery {
        name: name.to_owned(),
        public_contract: None,
        // me §5.2 — NO route, NO params. The actor IS the input.
        params: Vec::new(),
        keys,
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        // me §5.4 — default policy is `authenticated`.
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// me §11.1 — `me_synth_signature_mismatch` trigger. Compares an
/// author-written `lookup_my_<resource>` query to the canonical shape.
/// Returns `None` when the signatures match.
///
/// The canonical `me` synth produces a `Query::Lookup` (route-less,
/// returning the resource row). Mismatches the author can introduce:
/// - Wrong query kind (`Query::List` or `Query::Sql` under the same
///   name). The `me` bundle owns the `lookup_my_*` name prefix.
/// - Author-supplied `params` (the canonical shape is parameter-less).
fn check_me_lookup_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    resource: &str,
) -> Option<String> {
    let _ = resource; // reserved for future richer diff messages.
    let query = feature.queries.iter().find(|q| q.name() == name)?;

    match query {
        ir::Query::Lookup(lq) => {
            // me §5.2 — canonical shape is parameter-less. An author
            // who introduces `params` diverges from the canonical
            // route-less actor-keyed shape.
            if !lq.params.is_empty() {
                return Some(format!(
                    "author-written `{}` declares params; canonical `me` shape is route-less + parameter-less",
                    name
                ));
            }
            None
        }
        // §11.1 mismatch — `lookup_my_<r>` should be a Lookup query.
        _ => Some(format!(
            "author-written `{}` is not a `query.lookup`; canonical `me` shape is route-less Lookup",
            name
        )),
    }
}

/// §5.7 field categorisation result. Each field on a resource lands in
/// exactly one group; `create_input_fields` and `update_input_fields`
/// project the Required + Optional groups into the canonical input lists
/// per §5.2 / §5.3.
pub(crate) struct CategorisedFields<'a> {
    required: Vec<&'a ir::Field>,
    optional: Vec<&'a ir::Field>,
}

impl<'a> CategorisedFields<'a> {
    /// §5.2 — `create.input` shape: required fields stay required,
    /// optional fields stay optional. Order matches resource field order.
    fn create_input_fields(&self) -> Vec<(&'a ir::Field, bool)> {
        let mut out: Vec<(&'a ir::Field, bool)> = Vec::new();
        for field in &self.required {
            out.push((field, true));
        }
        for field in &self.optional {
            out.push((field, false));
        }
        out
    }

    /// §5.3 — `update.input` shape: every non-immutable field becomes
    /// optional regardless of its required-on-resource flag. Field-by-
    /// field optional update — fields omitted from input are not touched.
    fn update_input_fields(&self) -> Vec<(&'a ir::Field, bool)> {
        let mut out: Vec<(&'a ir::Field, bool)> = Vec::new();
        for field in &self.required {
            out.push((field, false));
        }
        for field in &self.optional {
            out.push((field, false));
        }
        out
    }
}

/// §5.7 — split a resource's fields into Tenant / Auto / Required /
/// Optional groups. Only Required + Optional are returned (Tenant and
/// Auto have no presence in the synth input lists).
pub(crate) fn categorize_fields(resource: &ir::Resource) -> CategorisedFields<'_> {
    // Detect the `user: User required unique` shape per §5.7.
    let has_user_unique = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && f.unique
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });

    // Discriminator field is in the Auto group when lifecycle is set.
    let lifecycle_discriminator: Option<&str> = resource
        .lifecycle
        .as_ref()
        .map(|lc| lc.discriminator_field.as_str());

    let mut required: Vec<&ir::Field> = Vec::new();
    let mut optional: Vec<&ir::Field> = Vec::new();

    for field in &resource.fields {
        // Tenant group.
        let is_tenant = field.name == "org" || (has_user_unique && field.name == "user");
        // Auto group.
        let is_auto = matches!(field.name.as_str(), "id" | "created_at" | "updated_at")
            || lifecycle_discriminator.is_some_and(|d| d == field.name);

        if is_tenant || is_auto {
            continue;
        }

        if field.required {
            required.push(field);
        } else {
            optional.push(field);
        }
    }

    CategorisedFields { required, optional }
}

// =============================================================================
// `@owner_axis(through: <col>)` synth-pass extension — Cell O2.
//
// Spec: `docs/proposals/ir-resource-conventions-owner-scope.md`
// §7.3 (`build_where_clause` extension), §8 (auto-synth worked
// examples), §8.5.A (CTE-INSERT for create-side verification),
// §9 (override semantics), §11.1 (3 new doctor codes).
//
// **RULE-VOCAB-03 (§7 + §8.6)**: each shape composed here lowers to
// exactly ONE SQL statement. The CTE-INSERT (§8.5.A) is a single
// CTE-wrapped INSERT — Postgres evaluates the CTE either yields a
// row and the INSERT fires once, or yields zero rows and the INSERT
// fires zero times. No procedural sequencing; no runtime branching;
// no two-roundtrip check-then-insert.
// =============================================================================

/// §7.3 — resolution result for the owner-scope synth lookup. Returned
/// by `resolve_owner_scope`, consumed by the crud + me synth blocks.
///
/// - `Scoped(...)`: at least one `@owner_axis` field resolved cleanly;
///   the analyzer should emit the WHERE / CTE fragments for downstream
///   codegen consumption.
/// - `Tenant`: no `@owner_axis` annotation present — fall back to the
///   pre-existing tenant-only synth (today's default).
/// - `Diagnostic(...)`: an `@owner_axis` annotation was found but
///   doesn't resolve cleanly — surface the diagnostic and skip
///   owner-scope emission for the offending field. Other fields on
///   the same resource may still resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OwnerScopeResolution {
    Scoped(ir::OwnerScopeSql),
    Tenant,
}

/// §7.3 — resolve a resource's `@owner_axis` annotations into an
/// emittable `OwnerScopeSql` carrier, OR emit diagnostics for the 3
/// new doctor codes (§11.1) when the annotation can't resolve.
///
/// The function visits every field; the *first* cleanly-resolving
/// `@owner_axis` wins for the synth's WHERE-clause (the pilot has
/// exactly one owner-axis per resource — Property's `host`,
/// Service's `host`, CustomServiceCategory's `host`). Multi-axis
/// composition is deferred per §13.
///
/// Diagnostics are pushed into `diagnostics_out` for the caller; the
/// return value indicates whether the synth should still emit
/// owner-scope IR (yes for `Scoped`, no for `Tenant` — which also
/// covers "diagnostic emitted, fell back to tenant-only").
pub(crate) fn resolve_owner_scope(
    feature: &ir::Feature,
    resource: &ir::Resource,
    diagnostics_out: &mut Vec<ConventionSynthDiagnostic>,
) -> OwnerScopeResolution {
    // §7.4 / §11.1 `owner_axis_collides_with_unique_user` — resource
    // carries BOTH the user-keyed shape (`user: User required unique`)
    // AND an `@owner_axis(through: ...)` on another field. The two
    // scopes would compose redundantly; the unique-user mode already
    // restricts to `WHERE "user" = ctx.User.ID`.
    let has_user_unique = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && f.unique
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });

    let mut emitted_collision_diag = false;
    let mut chosen: Option<ir::OwnerScopeSql> = None;

    for field in &resource.fields {
        let Some(axis) = field.owner_axis.as_ref() else {
            continue;
        };

        // §11.1 — collision check: declarative `user-keyed` mode
        // already provides ownership; surface a warning and skip the
        // owner-axis emission to avoid double-restriction. We emit
        // the diagnostic once per resource even if multiple fields
        // collide (rare; the spec describes "the resource has BOTH").
        if has_user_unique {
            if !emitted_collision_diag {
                diagnostics_out.push(ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser {
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                });
                emitted_collision_diag = true;
            }
            continue;
        }

        // The annotated field must be a UserDefined FK to another
        // resource. Primitive-field misuse is `owner_axis_on_non_fk`
        // and lives in O1's parser-time surface (§7.4); the analyzer
        // re-checks defensively so a hand-constructed IR fixture is
        // still surfaced (otherwise this code path would silently
        // skip the annotation).
        let ir::TypeRef::UserDefined(fk_qname) = &field.type_ref else {
            // Out-of-scope for O2 — O1 owns this diagnostic. Skip
            // silently rather than double-emit; downstream check
            // catches it.
            continue;
        };
        let fk_target = fk_qname.name.clone();

        // §11.1 `owner_axis_unknown_through` — the `through:` column
        // doesn't exist on the FK target resource. Resolve the FK
        // target in the feature's resource list.
        //
        // Cross-feature note: the FK target may live in another
        // feature (Hostpoint's catalog.Property → host.Host is the
        // motivating case). Synth runs per-feature without a Module
        // handle, so we can only validate the through-column when the
        // target is in the SAME feature. For cross-feature targets we
        // skip the diagnostic checks and trust the @owner_axis
        // annotation — the doctor pass (which has Module context)
        // surfaces missing-FK-target / wrong-through-type errors at a
        // higher layer. The SQL composition below only needs
        // `fk_target` (name) and `axis.through_column` (column name)
        // verbatim from the annotation; it does NOT need fk_resource
        // to exist locally.
        let fk_resource = feature.resources.iter().find(|r| r.name == fk_target);

        if let Some(fk_resource) = fk_resource {
            let through_field = fk_resource
                .fields
                .iter()
                .find(|f| f.name == axis.through_column);
            let Some(through_field) = through_field else {
                let suggestion = nearest_field_name(&axis.through_column, &fk_resource.fields);
                diagnostics_out.push(ConventionSynthDiagnostic::OwnerAxisUnknownThrough {
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    through: axis.through_column.clone(),
                    fk_target: fk_target.clone(),
                    suggestion,
                });
                continue;
            };

            // §11.1 `owner_axis_through_not_user_keyed` — the resolved
            // `through:` column must be typed as `User` (a UserDefined
            // ref to the User resource). Other actor types
            // (`@semantic.UserID` etc.) are deferred per §13.
            let is_user_keyed = matches!(
                &through_field.type_ref,
                ir::TypeRef::UserDefined(q) if q.name == "User"
            );
            if !is_user_keyed {
                diagnostics_out.push(ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed {
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    through: axis.through_column.clone(),
                    fk_target: fk_target.clone(),
                });
                // Warning, not error per §11.1 — still emit the chain so
                // codegen can produce SQL the author can hand-correct.
            }
        }
        // else: cross-feature FK target — skip per-field validation,
        // trust annotation, compose SQL below.

        // §7.3 / §8.1-8.4 — compose the WHERE predicate fragment.
        // Shape per §1.1 trigger evidence: literal Postgres
        // `<fk_col> IN (SELECT id FROM "<fk_table>" WHERE "<through>" = ctx.User.ID)`.
        // Single statement; the IN-subquery is a semi-join in the
        // planner (§8.6). The `ctx.User.ID` literal is a
        // codegen-substituted placeholder — downstream codegen
        // resolves to `$N` per its parameter-binding policy.
        let where_predicate = format!(
            "{fk_col} IN (SELECT id FROM {fk_table} WHERE {through} = ctx.User.ID)",
            fk_col = field.name,
            fk_table = quoted_table(&fk_target),
            through = quoted_ident(&axis.through_column),
        );

        // §8.5.A — CTE prefix for `create_<resource>`. The CREATE
        // synth pastes this in front of its INSERT; the INSERT then
        // selects FROM the CTE so a zero-row CTE yields a zero-row
        // INSERT (the synth surfaces a `not_owner` envelope via
        // existing RowsAffected==0 handling in `delete_*` per §8.7,
        // mirrored on create-side). One SQL statement total.
        let cte_owner_check = Some(format!(
            "WITH owner_check AS (SELECT 1 FROM {fk_table} WHERE id = ${fk_col} AND {through} = ctx.User.ID)",
            fk_col = field.name,
            fk_table = quoted_table(&fk_target),
            through = quoted_ident(&axis.through_column),
        ));

        if chosen.is_none() {
            chosen = Some(ir::OwnerScopeSql {
                field_name: field.name.clone(),
                fk_target,
                through_column: axis.through_column.clone(),
                where_predicate,
                cte_owner_check,
            });
        }
        // Multi-axis composition (multiple `@owner_axis` on one
        // resource) is deferred per §13. We take the first.
    }

    match chosen {
        Some(scope) => OwnerScopeResolution::Scoped(scope),
        None => OwnerScopeResolution::Tenant,
    }
}

/// §11.1 `owner_axis_unknown_through` — produce a nearest-name
/// suggestion from the FK target's field list. Returns `None` when
/// the closest candidate is not similar enough to be useful (Levenshtein
/// distance > half the input length — same threshold used by the
/// pre-existing nearest-string suggestions elsewhere in the doctor).
fn nearest_field_name(target: &str, fields: &[ir::Field]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for f in fields {
        let dist = levenshtein(target, &f.name);
        match best {
            Some((b, _)) if dist >= b => {}
            _ => best = Some((dist, f.name.as_str())),
        }
    }
    let (dist, name) = best?;
    if dist <= target.len().max(1) / 2 + 1 {
        Some(name.to_owned())
    } else {
        None
    }
}

/// `pub` re-export of the §7.3 WHERE-clause builder for direct test
/// access. Tests assert on the emitted SQL string; downstream codegen
/// pulls the same string off `Command.owner_scope_sql.where_predicate`.
///
/// **Direct call form** (no diagnostic surface). Used in tests that
/// construct a synthetic `Field` + `Resource` and want to round-trip
/// the SQL without running the whole `synthesize_conventions` pass.
/// For real synth, use `resolve_owner_scope` via `synthesize_conventions`.
#[doc(hidden)]
pub fn build_owner_scope_where_for_test(
    fk_col: &str,
    fk_target_resource: &str,
    through_column: &str,
) -> String {
    format!(
        "{fk_col} IN (SELECT id FROM {fk_table} WHERE {through} = ctx.User.ID)",
        fk_col = fk_col,
        fk_table = quoted_table(fk_target_resource),
        through = quoted_ident(through_column),
    )
}

/// §8.5.A — `pub` re-export of the CTE-INSERT prefix builder for
/// direct test access. Same role as `build_owner_scope_where_for_test`.
#[doc(hidden)]
pub fn build_owner_scope_cte_prefix_for_test(
    fk_col: &str,
    fk_target_resource: &str,
    through_column: &str,
) -> String {
    format!(
        "WITH owner_check AS (SELECT 1 FROM {fk_table} WHERE id = ${fk_col} AND {through} = ctx.User.ID)",
        fk_col = fk_col,
        fk_table = quoted_table(fk_target_resource),
        through = quoted_ident(through_column),
    )
}

/// Canonical return shape for `crud_synth_signature_mismatch` (§9 / §11).
/// The carried resource name is read by `check_command_signature_mismatch`
/// to compare against the author's effect target; the query variants
/// are matched only on kind today and reserve the name for Cell C4 if
/// it needs a richer diff message.
#[allow(dead_code)]
enum CanonicalReturn<'a> {
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

/// §5.2 — build `create_<resource>` command IR.
pub(crate) fn build_create_command(
    name: &str,
    resource: &str,
    input_fields: &[(&ir::Field, bool)],
) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Create,
        route: Vec::new(),
        input: input_to_command_input(input_fields),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Creates(ir::CreateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            from_input: true,
            // §5.2 — one `<field> = input.<field>` assignment per input
            // slot so the codegen emits a populated `lazuli.Bindings{}`
            // body. Without this, the synthesized INSERT had no columns
            // to bind and tripped runtime panics at first call. The
            // emitter checks `TypedSlot.required` to decide between
            // `FromInput` (required) and `FromInputOptional` (optional,
            // skip-on-nil so column defaults apply).
            assignments: input_field_assignments(input_fields),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.3 — build `update_<resource>` command IR.
pub(crate) fn build_update_command(
    name: &str,
    resource: &str,
    input_fields: &[(&ir::Field, bool)],
) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Update,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: input_to_command_input(input_fields),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Updates(ir::UpdateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            // §5.3 — every input slot becomes a `<field> = input.<field>`
            // assignment. `update_input_fields` marks all of them
            // optional, so the codegen emits `FromInputOptional` and
            // the runtime skips columns whose input pointer was nil —
            // i.e. fields the wire payload didn't include stay
            // untouched (partial-update semantics, §5.3 third para).
            assignments: input_field_assignments(input_fields),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// Build the canonical `invalidates` list for a synth `create_<R>` /
/// `update_<R>` / `delete_<R>` command. Without this list, clients
/// (TS `useLazuliCommand`) never refresh `lookup_<R>` and
/// `list_<R>s` after a mutation — the cached query result is shown
/// until next manual reload. The 2026-05-22 hostpoint settings save
/// outage surfaced exactly this: after the partial-update bug was
/// fixed, users still saw stale data on re-entering the panel
/// because every synth command shipped with `invalidates: []`.
///
/// When the resource also declares `conventions [me]`, the `me`
/// bundle's `lookup_my_<R>` query is appended too — it shares the
/// same row set, just keyed off the actor instead of the route id.
pub(crate) fn synth_crud_invalidates(
    lookup_name: &str,
    list_name: &str,
    has_me: bool,
    resource_snake: &str,
) -> Vec<ir::InvalidatesSpec> {
    let mut out = vec![
        ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: lookup_name.to_owned(),
            },
            args: Vec::new(),
        },
        ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: list_name.to_owned(),
            },
            args: Vec::new(),
        },
    ];
    if has_me {
        out.push(ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: format!("lookup_my_{}", resource_snake),
            },
            args: Vec::new(),
        });
    }
    out
}

/// Build one `<field> = input.<field>` assignment per input slot.
/// Used by both `build_create_command` and `build_update_command` so the
/// codegen has a populated `Bindings` body to emit. The emitter inspects
/// `TypedSlot.required` on the command's input to pick between
/// `FromInput` (required slot) and `FromInputOptional` (optional slot,
/// skip-on-nil at runtime).
pub(crate) fn input_field_assignments(input_fields: &[(&ir::Field, bool)]) -> Vec<ir::Assignment> {
    input_fields
        .iter()
        .map(|(f, _required)| ir::Assignment {
            field: f.name.clone(),
            value: ir::Expr::Path(ir::Path::from_segments([
                "input".to_owned(),
                f.name.clone(),
            ])),
        })
        .collect()
}

/// §5.4 — build `delete_<resource>` command IR.
pub(crate) fn build_delete_command(name: &str, resource: &str) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Delete,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Deletes(ir::DeleteEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.5 — build `lookup_<resource>` query IR.
pub(crate) fn build_lookup_query(name: &str, resource: &str) -> ir::Query {
    let _ = resource;
    ir::Query::Lookup(ir::LookupQuery {
        name: name.to_owned(),
        public_contract: None,
        params: Vec::new(),
        keys: vec![ir::KeyClause {
            path: ir::Path::from_segments(["id".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments(["id".to_owned()])),
        }],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// §5.6 — build `list_<resource>s` query IR.
pub(crate) fn build_list_query(name: &str, resource: &str) -> ir::Query {
    let _ = resource;
    ir::Query::List(ir::ListQuery {
        name: name.to_owned(),
        public_contract: None,
        params: vec![
            ir::TypedSlot {
                name: "limit".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Integer),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
            ir::TypedSlot {
                name: "offset".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Integer),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
        ],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        // §5.6 default limit 50.
        paginate: Some(50),
        modifier: None,
        cache: None,
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// Project a `(field, required)` list into a typed `CommandInput`.
pub(crate) fn input_to_command_input(fields: &[(&ir::Field, bool)]) -> ir::CommandInput {
    if fields.is_empty() {
        return ir::CommandInput::Empty;
    }
    let slots: Vec<ir::TypedSlot> = fields
        .iter()
        .map(|(f, required)| ir::TypedSlot {
            name: f.name.clone(),
            type_ref: f.type_ref.clone(),
            required: *required,
            constraints: f.constraints.clone(),
            validate_skip: false,
        })
        .collect();
    ir::CommandInput::Typed(slots)
}

/// Common command-shape defaults applied to every synthesized CRUD
/// command. `policy authenticated`, `audit default`, `rate_limit` set
/// by the caller (write vs read uses different limits per §5.9).
pub(crate) fn default_synth_command(rate_limit: &str) -> ir::Command {
    ir::Command {
        name: String::new(),
        public_contract: None,
        kind: ir::CommandKind::Returns,
        route: Vec::new(),
        input: ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::None,
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: Some(ir::RateLimitSpec::from_default(rate_limit.to_owned())),
        audit: Some(ir::AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
        }),
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        // owner-scope §7.3 — left `None` here; the synth pass mutates
        // each synthesized command to attach the resolved scope (see
        // `synthesize_conventions`). The default keeps tenant-only
        // shape stable for command IR not produced by the synth pass.
        owner_scope_sql: None,
    }
}

/// §5.9 — create / update / delete share `rate_limit "100 per 10 minutes per ip"`.
pub(crate) fn crud_write_rate_limit() -> &'static str {
    "100 per 10 minutes per ip"
}
