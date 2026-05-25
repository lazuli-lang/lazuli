//! `@owner_axis(through: <col>)` synth-pass extension — Cell O2.
//!
//! Spec: `docs/proposals/ir-resource-conventions-owner-scope.md`
//! §7.3 (`build_where_clause` extension), §8 (auto-synth worked
//! examples), §8.5.A (CTE-INSERT for create-side verification),
//! §9 (override semantics), §11.1 (3 new doctor codes).
//!
//! **RULE-VOCAB-03 (§7 + §8.6)**: each shape composed here lowers to
//! exactly ONE SQL statement. The CTE-INSERT (§8.5.A) is a single
//! CTE-wrapped INSERT — Postgres evaluates the CTE either yields a
//! row and the INSERT fires once, or yields zero rows and the INSERT
//! fires zero times. No procedural sequencing; no runtime branching;
//! no two-roundtrip check-then-insert.
//!
//! ## Layout
//!
//! * `OwnerScopeResolution` — outcome enum read by the synth pass.
//! * `resolve_owner_scope` — synth-time resolver (visits fields, emits
//!   diagnostics, picks the first cleanly-resolving owner-axis).
//! * `nearest_field_name` — Levenshtein-bounded "did you mean?" helper.
//! * `build_owner_scope_where_for_test` /
//!   `build_owner_scope_cte_prefix_for_test` — `#[doc(hidden)] pub`
//!   re-exports for direct test access without running the full pass.

use lazuli_ir as ir;

use crate::helpers::{levenshtein, quoted_ident, quoted_table};

use super::diagnostics::ConventionSynthDiagnostic;

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
