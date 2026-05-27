//! Cell E3 — `@scope.*` policy lowering and WHERE-key resolution.
//!
//! Extracted from `command/mod.rs` as part of the rails-style split.
//! This module owns the half of `Command` codegen that asks: *given
//! the command's policy and route, which WHERE clauses does the
//! runtime need to scope an `Updates` / `Deletes` to a single row?*
//!
//! Two axes are folded together here:
//!
//! - **Route / input keys** — `WhereKeyBinding` + `resolve_where_keys`.
//!   Walks `command.route` slots, then falls back to a single typed
//!   input slot, then to the legacy `{"id": FromInput("ID")}`.
//! - **Policy `@scope.*` atoms** — `ScopeBinding`, `resolve_scope_bindings`,
//!   `command_policy_atoms`, plus `find_scope_column` /
//!   `find_owner_via_relation` for column resolution. Closed-catalog
//!   axes: `@scope.owner`, `@scope.same_org`, `@scope.self`. Each
//!   binding becomes a `Where`-map row, surfacing as `FromCtx` (direct
//!   ownership) or `FromCtxOwnedVia` (relation traversal — single hop).
//!
//! `owner_scope_binding` projects the analyzer-composed
//! `OwnerScopeSql` (see `ir-resource-conventions-owner-scope.md` §7.3)
//! into the same shape, so the synth `delete_<resource>` /
//! `update_<resource>` commands emit byte-stable WHERE rows.

use lazuli_ir::{
    Command, CommandEffect, CommandInput, Feature, OwnerScopeSql, Policies, PolicyRef, Resource,
};

use super::super::printer::GoPrinter;
use super::policy::walk_policy_expr_atoms;
use super::{escape_string, pascal_case};

pub(super) struct WhereKeyBinding {
    pub(super) column: String,
    pub(super) input_field: String,
}

/// Resolve the WHERE-key bindings the runtime needs to scope an
/// `Updates` / `Deletes` effect to a specific row. Priority:
///
/// 1. `command.route` slots — `route id: ID` / `route customer_id: ID`
///    drive the WHERE columns. Multi-slot routes produce a composite
///    key (every slot becomes one `<col> = FromInput(...)` binding).
/// 2. Single typed input slot — `input { endpoint: Text required }`
///    treats `endpoint` as the WHERE key. Closes the "alt-key WHERE"
///    gap from the the canonical pilot Phase 4 audit 2026-05-17.
/// 3. Legacy fallback — `{"id": FromInput("ID")}` for commands that
///    declare neither route nor a single-slot input. Mirrors pre-Wave-8
///    behaviour.
pub(super) fn resolve_where_keys(command: &Command) -> Vec<WhereKeyBinding> {
    if !command.route.is_empty() {
        return command
            .route
            .iter()
            .map(|slot| WhereKeyBinding {
                column: slot.name.clone(),
                input_field: pascal_case(&slot.name),
            })
            .collect();
    }
    if let CommandInput::Typed(slots) = &command.input {
        if slots.len() == 1 {
            return vec![WhereKeyBinding {
                column: slots[0].name.clone(),
                input_field: pascal_case(&slots[0].name),
            }];
        }
    }
    vec![WhereKeyBinding {
        column: "id".to_owned(),
        input_field: "ID".to_owned(),
    }]
}

/// Emit a single `Where` map row for a resolved scope binding. The
/// shape switches on `binding.via`: direct ownership uses `FromCtx`;
/// relation-traversal uses `FromCtxOwnedVia` with the (related_table,
/// owner_column) pair so the runtime composes a subquery WHERE.
pub(super) fn emit_scope_binding_row(p: &mut GoPrinter, binding: &ScopeBinding) {
    match &binding.via {
        None => {
            p.line(&format!(
                "// scope: {atom} resolved → {column} = ctx.{ctx_path}",
                atom = binding.atom,
                column = binding.column,
                ctx_path = binding.ctx_path
            ));
            p.line(&format!(
                "\"{column}\": lazuli.FromCtx(\"{ctx_path}\"),",
                column = escape_string(&binding.column),
                ctx_path = binding.ctx_path
            ));
        }
        Some((related_table, owner_column)) => {
            p.line(&format!(
                "// scope: {atom} resolved via {column} → {related_table}.{owner_column} = ctx.{ctx_path}",
                atom = binding.atom,
                column = binding.column,
                related_table = related_table,
                owner_column = owner_column,
                ctx_path = binding.ctx_path,
            ));
            p.line(&format!(
                "\"{column}\": lazuli.FromCtxOwnedVia(\"{related_table}\", \"{owner_column}\", \"{ctx_path}\"),",
                column = escape_string(&binding.column),
                related_table = escape_string(related_table),
                owner_column = escape_string(owner_column),
                ctx_path = binding.ctx_path,
            ));
        }
    }
}

/// Project `Command.owner_scope_sql` (composed by the analyzer under
/// `ir-resource-conventions-owner-scope.md` §7.3) into a `Where`-map
/// row on a synth `delete_<resource>` / `update_<resource>`. The shape
/// matches the existing `@scope.owner via relation` projection
/// (`FromCtxOwnedVia` → runtime composes the IN-subquery) so the
/// emitted SQL ends up isomorphic to spec §8.1 / §8.2 verbatim.
///
/// We read the *structural* fields of `OwnerScopeSql` (field_name,
/// fk_target, through_column) rather than splicing the pre-composed
/// `where_predicate` string because the runtime SQL builder is the
/// authoritative composer for `$N` placeholder offsets and identifier
/// quoting (`whereConditionFragment` in `runtime/go/lazuli/handle.go`).
/// The analyzer's `where_predicate` is a literal-form audit string for
/// inspect / doctor surfaces; this projection ensures the same shape
/// is rendered through the canonical `FromCtxOwnedVia` →
/// `<fk_col> IN (SELECT id FROM <fk_table> WHERE <through_col> = $N)`
/// pipeline that already powers `@scope.owner` relation-traversal.
///
/// Returns `None` when `owner_scope_sql` is absent (today's
/// tenant-only default for resources without `@owner_axis`).
pub(super) fn owner_scope_binding(scope: Option<&OwnerScopeSql>) -> Option<ScopeBinding> {
    let scope = scope?;
    Some(ScopeBinding {
        atom: "@owner_axis".to_owned(),
        column: scope.field_name.clone(),
        ctx_path: "user.id".to_owned(),
        via: Some((
            command_pascal_to_snake(&scope.fk_target),
            scope.through_column.clone(),
        )),
    })
}

/// Snake-case lowering for PascalCase resource names. Mirrors the
/// transform `quoteResourceTable` applies on the runtime side
/// (`UserSession` → `user_session`) so the `relatedTable` argument to
/// `FromCtxOwnedVia` round-trips with the migration-emitted SQL table
/// name. Duplicated locally (query.rs has its own copy) to avoid a
/// cross-module dependency that's not on the casing module's public
/// surface today.
pub(super) fn command_pascal_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// One `@scope.<axis>` policy atom that resolves to a runtime WHERE
/// binding on the effect's resource. The codegen appends each binding
/// to the `Updates` / `Deletes` `Where` map; the runtime then
/// composes `WHERE id = $1 AND <column> = <ctx_path>`.
///
/// Closed-catalog axes (closes the the canonical pilot 2026-05-17 SHIP-NOW gap):
///
/// | Atom              | Column priority                                   | Ctx path           |
/// |-------------------|---------------------------------------------------|--------------------|
/// | `@scope.owner`    | `user_id` > `user` > `owner_id` > `owner`         | `user.id`          |
/// | `@scope.same_org` | `org_id` > `org` > `tenant_id` > `tenant`         | `user.org_id`      |
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeBinding {
    /// The atom that introduced this binding, e.g. `@scope.owner`.
    pub atom: String,
    /// The resolved resource column the WHERE binds to.
    pub column: String,
    /// The `ctx.*` path the runtime reads, matching `readCtx` keys in
    /// `runtime/go/lazuli/handle.go`.
    pub ctx_path: String,
    /// `Some((related_table, owner_column))` when the binding traverses
    /// a relation (e.g. `property.host → host.user_id`). Codegen emits
    /// `lazuli.FromCtxOwnedVia(related_table, owner_column, ctx_path)`
    /// and the runtime composes
    /// `<column> IN (SELECT id FROM <related_table> WHERE <owner_column> = $N)`.
    /// `None` when the resource has a direct owner column (the original
    /// single-hop case shipped in `c0a4609`).
    pub via: Option<(String, String)>,
}

/// Resolve `@scope.*` atoms on the command's policy to concrete
/// WHERE bindings against the effect resource. Returns an empty slice
/// when the command has no scope atoms, no targetable effect, or no
/// matching column on the resource (the doctor surfaces the latter
/// case via a dedicated diagnostic so the silent-skip is observable).
pub(super) fn resolve_scope_bindings(command: &Command, feature: &Feature) -> Vec<ScopeBinding> {
    let resource_name = match &command.effect {
        CommandEffect::Updates(u) => Some(&u.resource),
        CommandEffect::Deletes(d) => Some(&d.resource),
        _ => return Vec::new(),
    };
    let Some(resource_qname) = resource_name else {
        return Vec::new();
    };
    // Only resolve when the resource lives in this feature. Cross-feature
    // scope lowering is a follow-up (would need the full Module).
    if let Some(feature_part) = &resource_qname.feature {
        if feature_part != &feature.name {
            return Vec::new();
        }
    }
    let Some(resource) = feature
        .resources
        .iter()
        .find(|r| r.name == resource_qname.name)
    else {
        return Vec::new();
    };

    let atoms = command_policy_atoms(command, &feature.policies);
    let mut out = Vec::new();
    for atom in &atoms {
        match atom.as_str() {
            "@scope.owner" => {
                if let Some(column) = find_scope_column(resource, OWNER_COLUMNS) {
                    out.push(ScopeBinding {
                        atom: atom.clone(),
                        column: column.to_owned(),
                        ctx_path: "user.id".to_owned(),
                        via: None,
                    });
                } else if let Some(via) = find_owner_via_relation(resource, feature) {
                    out.push(ScopeBinding {
                        atom: atom.clone(),
                        column: via.fk_column,
                        ctx_path: "user.id".to_owned(),
                        via: Some((via.related_table, via.related_owner_column)),
                    });
                }
            }
            "@scope.same_org" => {
                if let Some(column) = find_scope_column(resource, SAME_ORG_COLUMNS) {
                    out.push(ScopeBinding {
                        atom: atom.clone(),
                        column: column.to_owned(),
                        ctx_path: "user.org_id".to_owned(),
                        via: None,
                    });
                }
            }
            "@scope.self" => {
                // The acting user IS the target row. Closes the
                // ctx-as-key codegen gap surfaced by the the canonical pilot
                // Phase 4 audit (e.g. `account.choose_role` updates
                // the row whose `id` equals `ctx.user.id`). Only
                // meaningful when the resource is `User`-like — every
                // resource that has an `id` field qualifies; the
                // policy author is responsible for ensuring the row
                // identity matches the actor.
                out.push(ScopeBinding {
                    atom: atom.clone(),
                    column: "id".to_owned(),
                    ctx_path: "user.id".to_owned(),
                    via: None,
                });
            }
            _ => {}
        }
    }
    out
}

/// Capture for relation-traversal `@scope.owner`. When `Property.host`
/// references resource `Host` and `Host.user_id` is the owner column,
/// codegen emits `host IN (SELECT id FROM "host" WHERE user_id = $N)`.
pub(super) struct OwnerViaRelation {
    /// The local resource field that references the related resource
    /// (e.g. `host` on `Property` referencing `Host`).
    pub(super) fk_column: String,
    /// The related resource's SQL table name (e.g. `"host"`).
    pub(super) related_table: String,
    /// The owner column on the related resource (e.g. `"user_id"`).
    pub(super) related_owner_column: String,
}

/// Find an indirect ownership chain when the resource has no direct
/// owner column. Walks the resource's fields, checks each `TypeRef` for
/// a reference to another local resource (`TypeRef::UserDefined` or
/// `TypeRef::Unresolved` matching another resource's name), and returns
/// the first such field whose target resource has a direct owner column.
///
/// One-hop only: `property.host → host.user_id` works; deeper chains
/// (`property → listing → host → user_id`) are out of scope until a
/// 3rd pilot demands them.
pub(super) fn find_owner_via_relation(
    resource: &Resource,
    feature: &Feature,
) -> Option<OwnerViaRelation> {
    use lazuli_ir::TypeRef;
    for field in &resource.fields {
        let related_name = match &field.type_ref {
            TypeRef::UserDefined(q) => q.name.clone(),
            TypeRef::Unresolved(name) => name.clone(),
            _ => continue,
        };
        // Skip self-references and types that don't resolve to a local
        // resource (records, enums, scalars).
        if related_name == resource.name {
            continue;
        }
        let Some(related) = feature.resources.iter().find(|r| r.name == related_name) else {
            continue;
        };
        let Some(owner_col) = find_scope_column(related, OWNER_COLUMNS) else {
            continue;
        };
        return Some(OwnerViaRelation {
            fk_column: field.name.clone(),
            related_table: related.name.clone(),
            related_owner_column: owner_col.to_owned(),
        });
    }
    None
}

const OWNER_COLUMNS: &[&str] = &["user_id", "user", "owner_id", "owner"];
const SAME_ORG_COLUMNS: &[&str] = &["org_id", "org", "tenant_id", "tenant"];

pub(super) fn find_scope_column<'a>(resource: &'a Resource, priority: &[&str]) -> Option<&'a str> {
    for candidate in priority {
        if resource.fields.iter().any(|f| f.name == *candidate) {
            return resource
                .fields
                .iter()
                .find(|f| f.name == *candidate)
                .map(|f| f.name.as_str());
        }
    }
    None
}

/// Walk the command's policy to extract its full atom list. Resolves
/// `PolicyRef::Local("name")` through `feature.policies.categories`
/// and falls back to the policy_expr atoms when present. Idempotent —
/// duplicates collapse to a stable set.
pub(super) fn command_policy_atoms(command: &Command, policies: &Policies) -> Vec<String> {
    let mut atoms: Vec<String> = Vec::new();
    match &command.policy {
        PolicyRef::Local(name) => {
            if let Some(category) = policies.categories.iter().find(|c| c.name == *name) {
                for a in &category.atoms {
                    if !atoms.contains(a) {
                        atoms.push(a.clone());
                    }
                }
            }
        }
        PolicyRef::Atom(atom) => {
            let formatted = format!("@{atom}");
            if !atoms.contains(&formatted) {
                atoms.push(formatted);
            }
        }
        PolicyRef::External {
            feature: _,
            name: _,
        } => {
            // Cross-feature policy resolution requires the full module;
            // out of scope for the first scope-lowering pass.
        }
        PolicyRef::None | PolicyRef::Unresolved(_) => {}
    }
    if let Some(expr) = command.policy_expr.as_ref() {
        let mut expr_atoms = Vec::new();
        walk_policy_expr_atoms(expr, &mut expr_atoms);
        for a in expr_atoms {
            if !atoms.contains(&a) {
                atoms.push(a);
            }
        }
    }
    atoms
}

/// Test-host siblings — each owns a coherent sub-cluster of the scope
/// production code, wired in from `command/mod.rs` as
/// `#[cfg(test)] mod` so they compile only under `cargo test`:
///
/// - `scope_owner_tests` covers `@scope.owner` / `@scope.same_org`
///   atom-driven auto-injection + traversal via relation.
/// - `scope_where_keys_tests` covers `resolve_where_keys` (single-input
///   alt-key, route slots, `@scope.self` ctx-as-key, bulk mode,
///   composite multi-route, full-struct `Returns`).
/// - `owner_scope_sql_tests` covers `Command.owner_scope_sql` projection
///   into `FromCtxOwnedVia` + `CreatesWithOwnerCheck` (including the
///   PascalCase → snake_case lowering and partial-write
///   `FromInputOptional` behaviour).
#[cfg(test)]
mod tests {
    //! Residual scope-adjacent inline test — `CommandEffect::None` +
    //! `@fn.*` handler shape. Kept inline because it's a single
    //! ~30-LOC test that doesn't fit any of the three sibling test-host
    //! files' sub-concerns. Moving it solo would be churn, not clarity.
    use super::super::test_support::{
        base_command, base_feature, emit_with_customer_fallback as emit,
    };
    use lazuli_ir::{CommandEffect, CommandInput, HandlerRef};

    /// `command logout` (no `returns`, `handler @fn.logout`) — the IR
    /// lowers to `CommandEffect::None` with a handler ref. The Go
    /// handler stub is generated as `(struct{}, error)`. The emitted
    /// `ReturnsFromRegistry` Output generic MUST be `struct{}` so the
    /// runtime's type-assert (`fn.(func(*Ctx, I) (O, error))`) matches.
    /// Previously emitted `any`, which failed the assert and 500'd.
    #[test]
    fn none_effect_with_fn_handler_emits_struct_output_generic() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("logout");
        cmd.input = CommandInput::Empty;
        cmd.effect = CommandEffect::None;
        cmd.handler = Some(HandlerRef {
            namespace: "fn".to_owned(),
            name: "logout".to_owned(),
            span_ref: None,
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "Effect: lazuli.ReturnsFromRegistry[struct{}, struct{}](\"customer.logout\"),"
            ),
            "no-returns + @fn handler should emit O=struct{{}} (matches Go handler stub):\n{out}"
        );
        assert!(
            !out.contains("ReturnsFromRegistry[struct{}, any]"),
            "regression: O=any breaks the runtime type-assert against the registered (struct{{}}, error) handler:\n{out}"
        );
        assert!(
            out.contains(
                "// Wire Logout as `func(ctx *lazuli.Ctx, input struct{}) (struct{}, error)`"
            ),
            "handler signature comment should match the (struct{{}}, error) shape, got:\n{out}"
        );
    }
}
