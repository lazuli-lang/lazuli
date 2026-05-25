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
///    gap from the hostpoint Phase 4 audit 2026-05-17.
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
/// Closed-catalog axes (closes the hostpoint 2026-05-17 SHIP-NOW gap):
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
                // ctx-as-key codegen gap surfaced by the hostpoint
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

#[cfg(test)]
mod tests {
    //! Scope tests — these all exercise `@scope.*` / `owner_scope_sql`
    //! lowering through the `emit_command_file` integration point. They
    //! were lifted out of `file_emit.rs` (wave R8-2b) so the scope
    //! concern (production code + behavioural tests) lives in one file.
    //! The tests still call `emit_command_file` because the behaviour
    //! under test (WHERE-key bindings, FromCtxOwnedVia projection,
    //! CreatesWithOwnerCheck) only surfaces when the orchestrator runs
    //! `resolve_where_keys` + `resolve_scope_bindings` end-to-end.
    use super::super::test_support::{
        base_command, base_feature, emit_with_customer_fallback as emit, local_qname,
        simple_resource, typed_slot,
    };
    use lazuli_ir::{
        Assignment, BuiltinType, CommandEffect, CommandInput, CommandKind, CreateEffect,
        DeleteEffect, Expr, Feature, HandlerRef, Path, Policies, PolicyRef, ReturnsEffect,
        RouteSlot, TypeRef, UpdateEffect,
    };

    fn scope_field(name: &str) -> lazuli_ir::Field {
        lazuli_ir::Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn feature_with_owner_scope_policy() -> Feature {
        let mut feature = base_feature("account");
        let mut resource = simple_resource("UserSession");
        resource.fields.push(scope_field("user_id"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "delete".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        feature
    }

    #[test]
    fn deletes_with_scope_owner_injects_user_id_where_binding() {
        let mut feature = feature_with_owner_scope_policy();
        let mut cmd = base_command("revoke_session");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("UserSession"),
        });
        cmd.policy = PolicyRef::Local("delete".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "still binds id from input:\n{out}"
        );
        assert!(
            out.contains("\"user_id\": lazuli.FromCtx(\"user.id\"),"),
            "@scope.owner should inject user_id WHERE binding:\n{out}"
        );
        assert!(
            out.contains("// scope: @scope.owner resolved → user_id = ctx.user.id"),
            "scope comment should surface for reviewers:\n{out}"
        );
    }

    #[test]
    fn updates_with_scope_owner_injects_user_where_binding_when_user_id_absent() {
        // Resource has `user` (not `user_id`) — closed-catalog falls
        // through to the second candidate per priority.
        let mut feature = base_feature("messaging");
        let mut resource = simple_resource("NotificationDelivery");
        resource.fields.push(scope_field("user"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("mark_notification_read");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("NotificationDelivery"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("update".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"user\": lazuli.FromCtx(\"user.id\"),"),
            "@scope.owner should resolve to `user` field when user_id absent:\n{out}"
        );
    }

    #[test]
    fn updates_with_scope_same_org_injects_org_id_where_binding() {
        let mut feature = base_feature("billing");
        let mut resource = simple_resource("Charge");
        resource.fields.push(scope_field("org_id"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@scope.same_org".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("flag_review");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Charge"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("update".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"org_id\": lazuli.FromCtx(\"user.org_id\"),"),
            "@scope.same_org should inject org_id WHERE binding:\n{out}"
        );
    }

    #[test]
    fn no_scope_atom_emits_baseline_where_binding() {
        let mut feature = base_feature("account");
        let mut resource = simple_resource("UserSession");
        resource.fields.push(scope_field("user_id"));
        feature.resources.push(resource);
        // No @scope.* atom in the policy.
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "admin".to_owned(),
                atoms: vec!["@role.admin".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("purge");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("UserSession"),
        });
        cmd.policy = PolicyRef::Local("admin".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // Baseline binds id from input. No scope injection.
        assert!(out.contains("\"id\": lazuli.FromInput(\"ID\"),"));
        assert!(
            !out.contains("FromCtx(\"user.id\")"),
            "no @scope.* atom → no auto-injected scope binding:\n{out}"
        );
    }

    #[test]
    fn updates_with_scope_owner_traverses_relation_when_no_direct_column() {
        // Property has no direct owner column but `host: Host required`
        // references the Host resource which has `user_id`. Codegen
        // should emit FromCtxOwnedVia("Host", "user_id", "user.id").
        let mut feature = base_feature("catalog");

        let mut host = simple_resource("Host");
        host.fields.push(scope_field("user_id"));
        feature.resources.push(host);

        let mut property = simple_resource("Property");
        // `host` field referencing the Host resource.
        property.fields.push(lazuli_ir::Field {
            name: "host".to_owned(),
            type_ref: TypeRef::Unresolved("Host".to_owned()),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        });
        feature.resources.push(property);

        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };

        let mut cmd = base_command("publish_property");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Property"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("update".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"host\": lazuli.FromCtxOwnedVia(\"Host\", \"user_id\", \"user.id\"),"),
            "@scope.owner should traverse host → Host.user_id when Property has no direct column:\n{out}"
        );
        assert!(
            out.contains("// scope: @scope.owner resolved via host → Host.user_id = ctx.user.id"),
            "scope comment should document the traversal:\n{out}"
        );
    }

    #[test]
    fn scope_owner_without_matching_column_skips_silently() {
        // Resource has no owner-like column. Codegen must not invent a
        // binding; doctor surfaces the warning separately.
        let mut feature = base_feature("trust");
        let mut resource = simple_resource("Review");
        resource.fields.push(scope_field("status")); // unrelated field
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("flag");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Id, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Review"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("update".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            !out.contains("FromCtx(\"user.id\")"),
            "no matching column → no scope binding emitted:\n{out}"
        );
    }

    // -------------------------------------------------------------------------
    // Alt-key WHERE binding (Wave 8). When a delete/update command has no
    // `route` and a single typed input slot whose name is NOT `id`, the
    // codegen now uses that slot as the WHERE key (column + Go input
    // field). Closes the hostpoint Phase 4 codegen gap surfaced 2026-05-17.
    // -------------------------------------------------------------------------

    #[test]
    fn deletes_with_single_input_slot_uses_alt_key_when_not_id() {
        let mut feature = base_feature("messaging");
        let mut resource = simple_resource("WebPushSubscription");
        resource.fields.push(scope_field("endpoint"));
        resource.fields.push(scope_field("user"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "delete".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("unregister_web_push");
        cmd.input = CommandInput::Typed(vec![typed_slot("endpoint", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("WebPushSubscription"),
        });
        cmd.policy = PolicyRef::Local("delete".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"endpoint\": lazuli.FromInput(\"Endpoint\"),"),
            "single-slot input `endpoint` should drive WHERE:\n{out}"
        );
        assert!(
            !out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "no `id` binding should leak when input slot is `endpoint`:\n{out}"
        );
        assert!(
            out.contains("\"user\": lazuli.FromCtx(\"user.id\"),"),
            "@scope.owner should still inject the ownership column:\n{out}"
        );
    }

    #[test]
    fn updates_with_route_slot_uses_route_as_where_key() {
        let mut feature = base_feature("trust");
        let mut resource = simple_resource("Review");
        resource.fields.push(scope_field("status"));
        feature.resources.push(resource);
        let mut cmd = base_command("flag");
        cmd.route = vec![lazuli_ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Typed(vec![typed_slot("reason", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Review"),
            assignments: Vec::new(),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // Route drives the WHERE key. `reason` is the body slot, not a
        // WHERE key candidate.
        assert!(
            out.contains("\"id\": lazuli.FromInput(\"ID\")"),
            "route id should drive WHERE:\n{out}"
        );
        assert!(
            !out.contains("\"reason\": lazuli.FromInput(\"Reason\"),"),
            "non-route, non-key input should not leak into WHERE bindings:\n{out}"
        );
        // LAZ-route-id-codegen-go (Cell A1) — the route id slot must
        // ALSO be present on the Input struct so the FromInput("ID")
        // binding above resolves at dispatch.
        assert!(
            out.contains("ID     lazuli.ID `json:\"id\" validate:\"required\"`"),
            "route id slot must land on the Input struct as `ID lazuli.ID`:\n{out}"
        );
        assert!(
            out.contains("Reason string    `json:\"reason\" validate:\"required\"`"),
            "body Reason field must still be present:\n{out}"
        );
    }

    // -------------------------------------------------------------------------
    // @scope.self — ctx-as-key WHERE binding (Wave 9 / hostpoint codegen gap G).
    // Closes `account.choose_role` UPDATE WHERE id = ctx.user.id.
    // -------------------------------------------------------------------------

    #[test]
    fn updates_with_scope_self_uses_ctx_user_id_as_where_key() {
        let mut feature = base_feature("account");
        let mut resource = simple_resource("User");
        resource.fields.push(scope_field("role"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "choose_role".to_owned(),
                atoms: vec!["@scope.self".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("choose_role");
        cmd.input = CommandInput::Typed(vec![typed_slot("role", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("User"),
            assignments: Vec::new(),
        });
        cmd.policy = PolicyRef::Local("choose_role".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // @scope.self drives WHERE via ctx; the `role` input slot is
        // a body field, not a key.
        assert!(
            out.contains("\"id\": lazuli.FromCtx(\"user.id\"),"),
            "@scope.self should bind id from ctx.user.id:\n{out}"
        );
        assert!(
            !out.contains("\"id\": lazuli.FromInput(\""),
            "@scope.self must suppress the route/input id binding (no double-id):\n{out}"
        );
        assert!(
            out.contains("// scope: @scope.self resolved → id = ctx.user.id"),
            "scope comment should document the ctx-key pattern:\n{out}"
        );
    }

    // -------------------------------------------------------------------------
    // Bulk delete — @scope.owner with no route AND no typed input
    // (Wave 9 / hostpoint codegen gap H). Closes `account.logout` etc.
    // -------------------------------------------------------------------------

    #[test]
    fn deletes_in_bulk_mode_drops_legacy_id_binding() {
        let mut feature = base_feature("account");
        let mut resource = simple_resource("UserSession");
        resource.fields.push(scope_field("user_id"));
        feature.resources.push(resource);
        feature.policies = Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "logout".to_owned(),
                atoms: vec!["@scope.owner".to_owned()],
                previous_names: Vec::new(),
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };
        let mut cmd = base_command("logout");
        cmd.input = CommandInput::Empty;
        // No route slots either.
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("UserSession"),
        });
        cmd.policy = PolicyRef::Local("logout".to_owned());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            !out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "bulk delete must NOT emit legacy id-from-input binding:\n{out}"
        );
        assert!(
            out.contains("\"user_id\": lazuli.FromCtx(\"user.id\"),"),
            "scope.owner should still inject the ownership binding:\n{out}"
        );
        assert!(
            out.contains("// bulk: no id/route key"),
            "bulk-mode comment should be visible for reviewers:\n{out}"
        );
    }

    #[test]
    fn deletes_with_multi_route_emits_composite_where() {
        let mut feature = base_feature("customer_tags");
        let resource = simple_resource("CustomerTagAssignment");
        feature.resources.push(resource.clone());
        let mut cmd = base_command("remove_tag");
        cmd.route = vec![
            lazuli_ir::RouteSlot {
                name: "customer_id".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
                from: None,
                kind: lazuli_ir::RouteSlotKind::Plain,
            },
            lazuli_ir::RouteSlot {
                name: "tag_id".to_owned(),
                type_ref: TypeRef::Builtin(BuiltinType::Id),
                from: None,
                kind: lazuli_ir::RouteSlotKind::Plain,
            },
        ];
        cmd.input = CommandInput::Empty;
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("CustomerTagAssignment"),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("\"customer_id\": lazuli.FromInput(\"CustomerID\"),"),
            "first route slot should bind (note `id` acronym uppercases per is_acronym):\n{out}"
        );
        assert!(
            out.contains("\"tag_id\": lazuli.FromInput(\"TagID\"),"),
            "second route slot should bind:\n{out}"
        );
        // LAZ-route-id-codegen-go (Cell A1) — Empty-input + route slots
        // must STILL emit a synthetic Input struct carrying the route
        // fields. Without it, FromInput("CustomerID") / FromInput("TagID")
        // would resolve against `struct{}` and return 400 bad_request.
        assert!(
            out.contains("type RemoveCustomerTagAssignmentTagInput struct {"),
            "Empty input + route slots must still emit an Input struct:\n{out}"
        );
        assert!(
            out.contains("CustomerID lazuli.ID `json:\"customer_id\" validate:\"required\"`"),
            "first composite-route slot must surface on the Input struct:\n{out}"
        );
        assert!(
            out.contains("TagID      lazuli.ID `json:\"tag_id\" validate:\"required\"`"),
            "second composite-route slot must surface on the Input struct:\n{out}"
        );
    }

    /// `command me returns User` — the IR lowers to
    /// `CommandEffect::Returns(ReturnsEffect { return_type: UserDefined("User") })`.
    /// The emitted Output generic must be the full resource struct
    /// (`Customer` same-feature, `<owner>gen.Customer` cross-feature),
    /// NOT the `lazuli.ID` FK collapse used for resource-field positions.
    /// Closes the `account.me` 500-internal at dispatch — the runtime's
    /// `ReturnsFromRegistry[I, O]` type-asserts the registered fn as
    /// `func(*Ctx, I) (O, error)`; with `O = lazuli.ID` and the
    /// registered handler returning `(User, error)`, the assertion
    /// fails and the runtime emits a 500 internal.
    #[test]
    fn returns_user_defined_resource_emits_full_struct_not_id() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("me");
        cmd.input = CommandInput::Empty;
        cmd.effect = CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::UserDefined(local_qname("Customer")),
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        // Output generic in the Command[I, O] declaration is the full
        // struct (`Customer`) — NOT `lazuli.ID`. `command_var_name`
        // composes `meCustomer` from `verb=me, resource=Customer`.
        assert!(
            out.contains("var meCustomer = lazuli.Command[struct{}, Customer]{"),
            "Command[I, O] should pin O to the resource struct, got:\n{out}"
        );
        // Effect's ReturnsFromRegistry generic pins the same struct.
        assert!(
            out.contains("Effect: lazuli.ReturnsFromRegistry[struct{}, Customer]("),
            "ReturnsFromRegistry should pin O to Customer (not lazuli.ID), got:\n{out}"
        );
        assert!(
            !out.contains("ReturnsFromRegistry[struct{}, lazuli.ID]"),
            "regression: ReturnsFromRegistry must NOT collapse Customer to lazuli.ID:\n{out}"
        );
        // Handler comment matches the registered fn shape — the
        // emitted Wire comment names `Customer` as the return type.
        assert!(
            out.contains("(Customer, error)"),
            "handler signature comment should return Customer, got:\n{out}"
        );
    }

    // -------------------------------------------------------------------------
    // Owner-scope projection — cell `codegen-os-projection`. The analyzer
    // composes `Command.owner_scope_sql` per spec
    // `ir-resource-conventions-owner-scope.md` §7.3; this codegen cell
    // pastes the carrier through `FromCtxOwnedVia` (DELETE/UPDATE) and
    // `CreatesWithOwnerCheck` (CREATE) so the emitted SQL matches §8.1 /
    // §8.5.A verbatim after the existing tenant predicates.
    // -------------------------------------------------------------------------

    fn owner_scope_sql_property() -> lazuli_ir::OwnerScopeSql {
        // Mirrors the analyzer's cell-O2 output for Hostpoint's
        // `Property.host: Host required @owner_axis(through: user)`.
        lazuli_ir::OwnerScopeSql {
            field_name: "host".to_owned(),
            fk_target: "Host".to_owned(),
            through_column: "user".to_owned(),
            where_predicate: "host IN (SELECT id FROM \"host\" WHERE \"user\" = ctx.User.ID)"
                .to_owned(),
            cte_owner_check: None,
        }
    }

    #[test]
    fn delete_with_owner_scope_sql_emits_owned_via_binding() {
        // Spec §8.1: synth `delete_property` lowers to
        // `DELETE FROM "property" WHERE id = $1 AND org_id = $2 AND
        //   host IN (SELECT id FROM "host" WHERE "user" = $3)`.
        // Codegen projection: existing `id` binding from route +
        // tenant via baseScopeConditions + FromCtxOwnedVia for the
        // ownership chain. We assert the emitted Go contains the
        // owned-via binding row in the Deletes effect's Where map.
        let mut feature = base_feature("catalog");
        let mut resource = simple_resource("Property");
        resource.fields.push(scope_field("host"));
        feature.resources.push(resource);

        let mut cmd = base_command("delete_property");
        cmd.kind = CommandKind::Delete;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("Property"),
        });
        cmd.owner_scope_sql = Some(owner_scope_sql_property());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"host\": lazuli.FromCtxOwnedVia(\"host\", \"user\", \"user.id\"),"),
            "DELETE with owner_scope_sql should emit FromCtxOwnedVia binding:\n{out}"
        );
        assert!(
            out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "existing route-key id binding must remain:\n{out}"
        );
        assert!(
            out.contains("// scope: @owner_axis resolved via host"),
            "scope-binding comment must document the owner-axis traversal:\n{out}"
        );
    }

    #[test]
    fn delete_without_owner_scope_sql_emits_unchanged_tenant_only_shape() {
        // Resources without `@owner_axis` carry `owner_scope_sql: None`.
        // The emitted Go must be identical to today's tenant-only DELETE
        // shape — no FromCtxOwnedVia binding leaks into the Where map.
        let mut feature = base_feature("billing");
        feature.resources.push(simple_resource("Charge"));

        let mut cmd = base_command("delete_charge");
        cmd.kind = CommandKind::Delete;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.effect = CommandEffect::Deletes(DeleteEffect {
            resource: local_qname("Charge"),
        });
        cmd.owner_scope_sql = None;
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            !out.contains("FromCtxOwnedVia"),
            "DELETE without owner_scope_sql must NOT emit owned-via:\n{out}"
        );
        assert!(
            !out.contains("@owner_axis"),
            "no owner-axis annotation should appear in emitted code when carrier is None:\n{out}"
        );
        assert!(
            out.contains("\"id\": lazuli.FromInput(\"ID\"),"),
            "baseline route-key binding must be present:\n{out}"
        );
    }

    #[test]
    fn update_with_owner_scope_sql_emits_owned_via_binding() {
        // Spec §8.2: synth `update_property` lowers to
        // `UPDATE "property" SET ... WHERE id = $1 AND org_id = $4 AND
        //   host IN (SELECT id FROM "host" WHERE "user" = $5)`.
        let mut feature = base_feature("catalog");
        let mut resource = simple_resource("Property");
        resource.fields.push(scope_field("host"));
        resource.fields.push(scope_field("name"));
        feature.resources.push(resource);

        let mut cmd = base_command("update_property");
        cmd.kind = CommandKind::Update;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, false)]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Property"),
            assignments: vec![Assignment {
                field: "name".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "name"])),
            }],
        });
        cmd.owner_scope_sql = Some(owner_scope_sql_property());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"host\": lazuli.FromCtxOwnedVia(\"host\", \"user\", \"user.id\"),"),
            "UPDATE with owner_scope_sql should emit FromCtxOwnedVia binding:\n{out}"
        );
        // SET-side binding: `name` is an optional input slot (above) so
        // the emitter now picks `FromInputOptional` so the runtime
        // skips the column when the wire payload omits it (partial-
        // update semantics). Required slots keep emitting plain
        // `FromInput`.
        assert!(
            out.contains("\"name\": lazuli.FromInputOptional(\"name\"),"),
            "SET-side optional input must emit FromInputOptional:\n{out}"
        );
    }

    /// Partial-write axis: an UPDATE command whose typed input mixes
    /// required + optional slots must emit `FromInput` for the
    /// required ones and `FromInputOptional` for the optional ones, so
    /// the runtime keeps the existing column value when the wire
    /// payload omits an optional field. Regression for the hostpoint
    /// 2026-05-22 settings-save outage.
    #[test]
    fn update_emits_from_input_optional_for_optional_input_slots() {
        let mut feature = base_feature("widget");
        let mut resource = simple_resource("Widget");
        resource.fields.push(scope_field("name"));
        resource.fields.push(scope_field("color"));
        feature.resources.push(resource);

        let mut cmd = base_command("update_widget");
        cmd.kind = CommandKind::Update;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Typed(vec![
            typed_slot("name", BuiltinType::Text, true),   // required
            typed_slot("color", BuiltinType::Text, false), // optional
        ]);
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Widget"),
            assignments: vec![
                Assignment {
                    field: "name".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "name"])),
                },
                Assignment {
                    field: "color".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "color"])),
                },
            ],
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"name\": lazuli.FromInput(\"name\"),"),
            "required input slot must emit plain FromInput:\n{out}"
        );
        assert!(
            out.contains("\"color\": lazuli.FromInputOptional(\"color\"),"),
            "optional input slot must emit FromInputOptional:\n{out}"
        );
    }

    /// Mirror of the above for CREATE — required slots stay
    /// `FromInput`, optional slots become `FromInputOptional` so the
    /// INSERT skips columns whose wire field was nil and lets the
    /// column default take effect.
    #[test]
    fn create_emits_from_input_optional_for_optional_input_slots() {
        let mut feature = base_feature("widget");
        let mut resource = simple_resource("Widget");
        resource.fields.push(scope_field("name"));
        resource.fields.push(scope_field("color"));
        feature.resources.push(resource);

        let mut cmd = base_command("create_widget");
        cmd.kind = CommandKind::Create;
        cmd.input = CommandInput::Typed(vec![
            typed_slot("name", BuiltinType::Text, true),
            typed_slot("color", BuiltinType::Text, false),
        ]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Widget"),
            from_input: false,
            assignments: vec![
                Assignment {
                    field: "name".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "name"])),
                },
                Assignment {
                    field: "color".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "color"])),
                },
            ],
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("\"name\": lazuli.FromInput(\"name\"),"),
            "required input slot must emit plain FromInput:\n{out}"
        );
        assert!(
            out.contains("\"color\": lazuli.FromInputOptional(\"color\"),"),
            "optional input slot must emit FromInputOptional:\n{out}"
        );
    }

    #[test]
    fn create_with_cte_owner_check_emits_creates_with_owner_check() {
        // Spec §8.5.A: synth `create_property` lowers to
        //   WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $<fk>
        //     AND "user" = ctx.User.ID)
        //   INSERT INTO "property" (...) SELECT ... FROM owner_check
        //   RETURNING ...
        // Codegen projection: switch from `lazuli.Creates(...)` to
        // `lazuli.CreatesWithOwnerCheck(..., OwnerCheckSpec{...})`. The
        // runtime composes the CTE prefix from the spec fields; codegen
        // only emits the carrier.
        let mut feature = base_feature("catalog");
        let mut resource = simple_resource("Property");
        resource.fields.push(scope_field("host"));
        resource.fields.push(scope_field("name"));
        feature.resources.push(resource);

        let mut cmd = base_command("create_property");
        cmd.kind = CommandKind::Create;
        cmd.input = CommandInput::Typed(vec![
            typed_slot("host", BuiltinType::Id, true),
            typed_slot("name", BuiltinType::Text, true),
        ]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Property"),
            from_input: false,
            assignments: vec![
                Assignment {
                    field: "host".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "host"])),
                },
                Assignment {
                    field: "name".to_owned(),
                    value: Expr::Path(Path::from_segments(["input", "name"])),
                },
            ],
        });
        let mut scope = owner_scope_sql_property();
        scope.cte_owner_check = Some(
            "WITH owner_check AS (SELECT 1 FROM \"host\" WHERE id = $host AND \"user\" = ctx.User.ID)"
                .to_owned(),
        );
        cmd.owner_scope_sql = Some(scope);
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "Effect: lazuli.CreatesWithOwnerCheck(&propertyResource, lazuli.Bindings{"
            ),
            "CREATE with cte_owner_check should emit CreatesWithOwnerCheck:\n{out}"
        );
        assert!(
            out.contains("lazuli.OwnerCheckSpec{"),
            "OwnerCheckSpec literal must be emitted:\n{out}"
        );
        assert!(
            out.contains("FKColumn:      \"host\","),
            "OwnerCheckSpec.FKColumn must point at the FK field:\n{out}"
        );
        assert!(
            out.contains("RelatedTable:  \"host\","),
            "OwnerCheckSpec.RelatedTable must be the snake-cased FK target:\n{out}"
        );
        assert!(
            out.contains("ThroughColumn: \"user\","),
            "OwnerCheckSpec.ThroughColumn must match the @owner_axis through: value:\n{out}"
        );
        assert!(
            !out.contains("Effect: lazuli.Creates(&propertyResource"),
            "tenant-only Creates form should NOT appear when CTE is active:\n{out}"
        );
    }

    #[test]
    fn create_without_cte_owner_check_emits_regular_creates() {
        // When `owner_scope_sql.cte_owner_check` is None (or the slot
        // itself is None), the CREATE emit falls back to the tenant-only
        // `lazuli.Creates(...)` shape — no CTE wrapper.
        let mut feature = base_feature("billing");
        feature.resources.push(simple_resource("Charge"));

        let mut cmd = base_command("create_charge");
        cmd.input = CommandInput::Typed(vec![typed_slot("amount", BuiltinType::Integer, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Charge"),
            from_input: false,
            assignments: vec![Assignment {
                field: "amount".to_owned(),
                value: Expr::Path(Path::from_segments(["input", "amount"])),
            }],
        });
        cmd.owner_scope_sql = None;
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("Effect: lazuli.Creates(&chargeResource, lazuli.Bindings{"),
            "CREATE without cte_owner_check must use the regular Creates form:\n{out}"
        );
        assert!(
            !out.contains("CreatesWithOwnerCheck"),
            "tenant-only CREATE must NOT use CreatesWithOwnerCheck:\n{out}"
        );
        assert!(
            !out.contains("OwnerCheckSpec"),
            "tenant-only CREATE must NOT emit OwnerCheckSpec:\n{out}"
        );
    }

    #[test]
    fn owner_scope_sql_snake_cases_pascal_fk_target() {
        // The analyzer's `OwnerScopeSql.fk_target` carries PascalCase
        // (`"Host"`, `"BookingProposal"`), matching the IR's resource
        // name shape. Codegen lowers to snake_case when projecting to
        // `FromCtxOwnedVia` so the runtime's `quoteIdent` round-trips
        // with the migrated SQL table name (`booking_proposal`).
        let mut feature = base_feature("operations");
        let mut resource = simple_resource("Transaction");
        resource.fields.push(scope_field("proposal"));
        feature.resources.push(resource);

        let mut cmd = base_command("cancel_transaction");
        cmd.kind = CommandKind::Update;
        cmd.route = vec![RouteSlot {
            name: "id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            from: None,
            kind: lazuli_ir::RouteSlotKind::Plain,
        }];
        cmd.input = CommandInput::Empty;
        cmd.effect = CommandEffect::Updates(UpdateEffect {
            resource: local_qname("Transaction"),
            assignments: Vec::new(),
        });
        cmd.owner_scope_sql = Some(lazuli_ir::OwnerScopeSql {
            field_name: "proposal".to_owned(),
            fk_target: "BookingProposal".to_owned(),
            through_column: "user".to_owned(),
            where_predicate:
                "proposal IN (SELECT id FROM \"booking_proposal\" WHERE \"user\" = ctx.User.ID)"
                    .to_owned(),
            cte_owner_check: None,
        });
        feature.commands.push(cmd);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "\"proposal\": lazuli.FromCtxOwnedVia(\"booking_proposal\", \"user\", \"user.id\"),"
            ),
            "PascalCase fk_target must be snake-cased in the emitted FromCtxOwnedVia:\n{out}"
        );
    }

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
