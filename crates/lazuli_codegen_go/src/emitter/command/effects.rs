//! Cell E3 — `CommandEffect` lowering (Creates / Updates / Deletes /
//! Returns / None).
//!
//! Extracted from `command/mod.rs` as part of the rails-style split.
//! This module owns the `Effect:` slot on the runtime
//! `lazuli.Command[I, O]` envelope plus the binding-row formatters
//! that compose the per-column `lazuli.Bindings{...}` map:
//!
//! - `emit_effect` — dispatcher; routes per `CommandEffect` variant.
//! - `emit_creates_effect` — `lazuli.Creates(...)` /
//!   `lazuli.CreatesWithOwnerCheck(...)` (CTE-INSERT shape).
//! - `emit_updates_effect` — `lazuli.Updates(...)` with composite
//!   route/scope-aware WHERE bindings and optional lifecycle Apply
//!   comment block.
//! - `emit_deletes_effect` — `lazuli.Deletes(...)` mirroring updates
//!   plus bulk-mode handling (no route + scope = scope-only WHERE).
//! - `handler_ref_name` — short-name extractor for `Returns` /
//!   cap_file-synthesised `None` handlers.
//! - `format_binding_row` / `format_binding_source` / `format_path_source` —
//!   `Expr` → `lazuli.From*(...)` source constructor lowering.
//! - `collect_optional_inputs` — input-slot optionality set,
//!   threaded through binding rows so optional slots render as
//!   `FromInputOptional` (partial-write semantics for `conventions [crud]`).

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{
    Command, CommandEffect, CommandInput, CreateEffect, DeleteEffect, Expr, HandlerRef,
    UpdateEffect,
};

use super::super::printer::GoPrinter;
use super::super::types::{self, TypeCtx};
use super::effects_format::format_binding_row;
use super::lifecycle::{LifecycleCommand, lifecycle_machine_var};
use super::scope::{
    ScopeBinding, WhereKeyBinding, command_pascal_to_snake, emit_scope_binding_row,
    resolve_where_keys,
};
use super::{escape_string, pascal_case, resource_var_for_qname};

pub(super) fn emit_effect(
    p: &mut GoPrinter,
    feature_name: &str,
    command: &Command,
    command_name: &str,
    effect: &CommandEffect,
    handler: Option<&HandlerRef>,
    input_type: &str,
    ctx: &TypeCtx<'_>,
    let_bindings: &BTreeMap<&str, &Expr>,
    lifecycle_transition: Option<&LifecycleCommand<'_>>,
    scope_bindings: &[ScopeBinding],
) {
    match effect {
        CommandEffect::Creates(create) => emit_creates_effect(p, command, create, let_bindings),
        CommandEffect::Updates(update) => emit_updates_effect(
            p,
            command,
            update,
            let_bindings,
            lifecycle_transition,
            scope_bindings,
        ),
        CommandEffect::Deletes(delete) => emit_deletes_effect(p, command, delete, scope_bindings),
        // W4 GAP-REORDER-01 — `reorder <Resource> by <position>`. Wire-thin:
        // the runtime `Reorder` builder composes a single batch UPDATE of the
        // position column from the ordered id list.
        CommandEffect::Reorders(reorder) => {
            let resource_var = resource_var_for_qname(&reorder.resource);
            p.line(&format!(
                "Effect: lazuli.Reorder(&{resource_var}, \"{}\"),",
                escape_string(&reorder.position_field),
            ));
        }
        CommandEffect::Returns(ret) => {
            // Resolve the Output generic via the return-position resolver
            // so resource refs (`returns User`) render as the full struct
            // (`User` same-feature, `<owner>gen.User` cross-feature) and
            // not the FK collapse (`lazuli.ID`). The collapse is right
            // for fields (BIGINT column scan) but wrong for the return
            // axis — the handler returns the row, not the id. Mirrors
            // `query.rs`'s `Query[A, R]` resolution. Closes the
            // `command me returns User` 500-internal at dispatch.
            let (return_type, _import) = types::go_return_type_for(&ret.return_type, ctx);
            // Prefer explicit handler ref name (handler @fn.<name>)
            // when present; fall back to the command name. Either way
            // the registry name is `<feature>.<name>` so dispatches
            // don't collide across features.
            let handler_short =
                handler_ref_name(handler).unwrap_or_else(|| command_name.to_owned());
            let qualified = format!("{feature_name}.{handler_short}");
            p.line(&format!(
                "Effect: lazuli.ReturnsFromRegistry[{input_type}, {return_type}](\"{qualified}\"),"
            ));
            p.line(&format!(
                "// Wire {pascal} as `func(ctx *lazuli.Ctx, input {input_type}) ({return_type}, error)`",
                pascal = pascal_case(&handler_short),
            ));
            p.line(&format!(
                "// then register with `lazuli.RegisterFn(\"{qualified}\", {pascal})` at init().",
                pascal = pascal_case(&handler_short),
            ));
        }
        CommandEffect::None => {
            // cap_file-synthesised commands (auto-photo confirm/clear) have no
            // explicit `handler @fn.X` ref in IR — the analyzer leaves
            // `command.handler = None` and the runtime fn is registered by
            // the parallel `auto_photo.gen.go` emitter under the qualified
            // name `<feature>.<command_name>`. Without this branch we'd fall
            // through to `Effect: nil` and dispatch 500s with "command has no
            // effect" the moment the synthesised confirm fires. Take the
            // cap_file shortcut: pin the registry key to the command name,
            // generic `struct{}` (the auto_photo handler's return type).
            if let Some(handler_short) = handler_ref_name(handler) {
                let qualified = format!("{feature_name}.{handler_short}");
                // `command X` with no `returns` clause and an `@fn`
                // handler — the Go handler stub is emitted as
                // `(struct{}, error)` (see `handlers.rs`) so the
                // registry generic must match. Emitting `any` here
                // breaks the runtime's `ReturnsFromRegistry` type-assert
                // (the registered fn yields `struct{}`, the dispatch
                // path expects `any` — Go's interface conversion blows
                // up). The implicit no-return contract is `struct{}`
                // and that's what we pin.
                p.line(&format!(
                    "Effect: lazuli.ReturnsFromRegistry[{input_type}, struct{{}}](\"{qualified}\"),"
                ));
                p.line(&format!(
                    "// Wire {pascal} as `func(ctx *lazuli.Ctx, input {input_type}) (struct{{}}, error)`",
                    pascal = pascal_case(&handler_short),
                ));
                p.line(&format!(
                    "// then register with `lazuli.RegisterFn(\"{qualified}\", {pascal})` at init().",
                    pascal = pascal_case(&handler_short),
                ));
            } else if command.synthesized_from_cap_file.is_some() {
                let qualified = format!("{feature_name}.{command_name}");
                p.line(&format!(
                    "Effect: lazuli.ReturnsFromRegistry[{input_type}, struct{{}}](\"{qualified}\"),"
                ));
                p.line(&format!(
                    "// cap_file synth: handler registered by auto_photo.gen.go under \"{qualified}\"."
                ));
            } else {
                p.line("Effect: nil,");
                p.line(
                    "// No-effect commands are pure-read legacy APIs invoked via command.Invoke.",
                );
            }
        }
    }
}

/// Extract the short handler name from a `handler @fn.<name>` ref.
/// File handlers (`handler "./path.go"`, namespace `path`) and absent
/// handlers return `None` — callers fall back to the command name for
/// the qualified registry key.
pub(super) fn handler_ref_name(handler: Option<&HandlerRef>) -> Option<String> {
    let h = handler?;
    if h.namespace == "fn" {
        Some(h.name.clone())
    } else {
        None
    }
}

fn emit_creates_effect(
    p: &mut GoPrinter,
    command: &Command,
    create: &CreateEffect,
    let_bindings: &BTreeMap<&str, &Expr>,
) {
    let resource_var = resource_var_for_qname(&create.resource);
    let optional_inputs = collect_optional_inputs(command);
    if create.assignments.is_empty() && create.from_input {
        // `creates Customer from input` with no explicit assignments —
        // the analyzer's `conventions [crud]` synth was supposed to
        // materialise one assignment per input slot but didn't (older
        // pre-`input_field_assignments` build). Keep the legacy TODO
        // emit so the gap is visible at codegen time instead of at
        // dispatch time.
        p.line(&format!(
            "Effect: lazuli.Creates(&{resource_var}, lazuli.Bindings{{}}),"
        ));
        p.line(
            "// TODO(from-input): `creates X from input` — expand bindings from typed input slots.",
        );
        return;
    }

    // `ir-resource-conventions-owner-scope.md` §8.5.A — CTE-INSERT
    // shape. When the analyzer attached `owner_scope_sql` with a
    // populated `cte_owner_check` on a synth `create_<resource>`, the
    // emitted INSERT becomes the CTE-prefixed form so the actor's
    // ownership of the FK target row is verified in the same SQL
    // statement (RULE-VOCAB-03 single-statement guarantee). Codegen
    // here projects the analyzer's structural OwnerScopeSql carrier
    // (`field_name`, `fk_target`, `through_column`) into the runtime's
    // `OwnerCheckSpec`; the runtime composes the CTE prefix from
    // those fields, matching the spec's `WITH owner_check AS (SELECT
    // 1 FROM "<fk_table>" WHERE id = $<fk> AND "<through>" = ctx.User.ID)`
    // shape verbatim.
    let owner_check = command
        .owner_scope_sql
        .as_ref()
        .filter(|s| s.cte_owner_check.is_some());

    if let Some(scope) = owner_check {
        let fk_table = command_pascal_to_snake(&scope.fk_target);
        p.line(&format!(
            "Effect: lazuli.CreatesWithOwnerCheck(&{resource_var}, lazuli.Bindings{{"
        ));
        p.indent();
        for assignment in &create.assignments {
            p.line(&format_binding_row(
                assignment,
                let_bindings,
                &optional_inputs,
            ));
        }
        p.dedent();
        p.line("}, lazuli.OwnerCheckSpec{");
        p.indent();
        p.line(&format!(
            "FKColumn:      \"{}\",",
            escape_string(&scope.field_name)
        ));
        p.line(&format!("RelatedTable:  \"{}\",", escape_string(&fk_table)));
        p.line(&format!(
            "ThroughColumn: \"{}\",",
            escape_string(&scope.through_column)
        ));
        p.dedent();
        p.line("}),");
        return;
    }

    p.line(&format!(
        "Effect: lazuli.Creates(&{resource_var}, lazuli.Bindings{{"
    ));
    p.indent();
    for assignment in &create.assignments {
        p.line(&format_binding_row(
            assignment,
            let_bindings,
            &optional_inputs,
        ));
    }
    p.dedent();
    p.line("}),");
}

fn emit_updates_effect(
    p: &mut GoPrinter,
    command: &Command,
    update: &UpdateEffect,
    let_bindings: &BTreeMap<&str, &Expr>,
    lifecycle_transition: Option<&LifecycleCommand<'_>>,
    scope_bindings: &[ScopeBinding],
) {
    let resource_var = resource_var_for_qname(&update.resource);
    let optional_inputs = collect_optional_inputs(command);
    let scope_columns: BTreeSet<&str> = scope_bindings.iter().map(|b| b.column.as_str()).collect();
    // Suppress route/input-derived where keys when scope_bindings
    // claim the same column (e.g. `@scope.self` claims `id` from
    // ctx; the route's `id` would conflict).
    let where_keys: Vec<WhereKeyBinding> = resolve_where_keys(command)
        .into_iter()
        .filter(|k| !scope_columns.contains(k.column.as_str()))
        .collect();
    let bulk_delete_only = where_keys.is_empty() && !scope_bindings.is_empty();
    if let Some(lifecycle) = lifecycle_transition {
        p.line(&format!(
            "// lifecycle: newState, err := {}.Apply(ctx, current.{}, \"{}\")",
            lifecycle_machine_var(lifecycle.resource),
            pascal_case(&lifecycle.lifecycle.discriminator_field),
            escape_string(&lifecycle.transition.name)
        ));
    }
    p.line(&format!("Effect: lazuli.Updates(&{resource_var},"));
    p.indent();
    if scope_bindings.is_empty() && where_keys.len() == 1 {
        // Backward-compatible compact form for the single-key, no-scope
        // case (the common shape pre-Wave 8).
        let k = &where_keys[0];
        p.line(&format!(
            "lazuli.Bindings{{\"{}\": lazuli.FromInput(\"{}\")}},",
            escape_string(&k.column),
            k.input_field
        ));
    } else {
        p.line("lazuli.Bindings{");
        p.indent();
        if bulk_delete_only {
            p.line("// bulk: no id/route key — `@scope.*` alone scopes the affected rows.");
        }
        for k in &where_keys {
            p.line(&format!(
                "\"{}\": lazuli.FromInput(\"{}\"),",
                escape_string(&k.column),
                k.input_field
            ));
        }
        for binding in scope_bindings {
            emit_scope_binding_row(p, binding);
        }
        p.dedent();
        p.line("},");
    }
    p.line("lazuli.Bindings{");
    p.indent();
    for assignment in &update.assignments {
        p.line(&format_binding_row(
            assignment,
            let_bindings,
            &optional_inputs,
        ));
    }
    p.dedent();
    p.line("},");
    p.dedent();
    p.line("),");
}

fn emit_deletes_effect(
    p: &mut GoPrinter,
    command: &Command,
    delete: &DeleteEffect,
    scope_bindings: &[ScopeBinding],
) {
    let resource_var = resource_var_for_qname(&delete.resource);
    let scope_columns: std::collections::BTreeSet<&str> =
        scope_bindings.iter().map(|b| b.column.as_str()).collect();
    // Suppress route/input-derived where keys when scope_bindings
    // claim the same column (`@scope.self` over `id`, etc.).
    let mut where_keys: Vec<WhereKeyBinding> = resolve_where_keys(command)
        .into_iter()
        .filter(|k| !scope_columns.contains(k.column.as_str()))
        .collect();
    // Bulk delete: when the command authored no route AND no typed
    // input, the legacy fallback `id: FromInput("ID")` is meaningless
    // — there's nothing in the input struct to bind. When the policy
    // carries a scope atom that supplies the WHERE constraint (e.g.
    // `@scope.owner` on `user`), drop the legacy id binding and let
    // the scope atom alone scope the affected rows.
    let bulk_mode = matches!(command.input, CommandInput::Empty)
        && command.route.is_empty()
        && !scope_bindings.is_empty();
    if bulk_mode {
        where_keys.retain(|k| !(k.column == "id" && k.input_field == "ID"));
    }
    let bulk_delete_only = where_keys.is_empty() && !scope_bindings.is_empty();
    p.line(&format!(
        "Effect: lazuli.Deletes(&{resource_var}, lazuli.Bindings{{"
    ));
    p.indent();
    if bulk_delete_only {
        p.line("// bulk: no id/route key — `@scope.*` alone scopes the affected rows.");
    }
    for k in &where_keys {
        p.line(&format!(
            "\"{}\": lazuli.FromInput(\"{}\"),",
            escape_string(&k.column),
            k.input_field
        ));
    }
    for binding in scope_bindings {
        emit_scope_binding_row(p, binding);
    }
    p.dedent();
    p.line("}),");
}


/// Collect the set of optional input slot names on `command`. Returns
/// an empty set when `command.input` is anything other than
/// `CommandInput::Typed` (Json / Empty inputs have no slots to mark
/// optional). The set is consumed by `format_binding_row` to decide
/// between `FromInput` and `FromInputOptional` per binding.
pub(super) fn collect_optional_inputs(command: &Command) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    if let CommandInput::Typed(slots) = &command.input {
        for slot in slots {
            if !slot.required {
                out.insert(slot.name.as_str());
            }
        }
    }
    out
}
