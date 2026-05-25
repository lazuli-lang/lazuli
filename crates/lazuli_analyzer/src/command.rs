//! Command effect + invalidates lowering — the v1 mutation slot.
//!
//! ## Why this slot exists
//!
//! Command lowering is the analyzer's biggest single domain: between the
//! `command <name>` declaration, its `creates|updates|deletes` effect
//! body, its `target query.<name>(args)` callouts, and its
//! `invalidates query.<name>` cache discards, the projection from
//! `syntax::CommandDecl` onto `ir::Command` is the largest single hop
//! in the pipeline. To keep the bulk of `lib.rs` focused on top-level
//! `analyze_*` entry points, this module owns the effect-and-target
//! cluster — the four leaf builders that lift verbatim AST nodes onto
//! `ir::CommandEffect` / `ir::TargetExpr` / `ir::LetBinding` /
//! `ir::NamedArg` / `ir::Assignment`, plus the `invalidates query.*`
//! reference resolution that's shared with surface lowering.
//!
//! The orchestrator `lower_command_decl` itself stays in `lib.rs` for
//! now (it threads through too many sibling helpers — constraint
//! validation, rate-limit, retry, public-contract, deprecated,
//! handler — to move without dragging half the analyzer with it). The
//! Wave 4.6 R2 charter cuts this work into stages; later stages will
//! pull `lower_command_decl` itself once the shared helpers it calls
//! have all been promoted.
//!
//! ## What lives here
//!
//! * `lower_command_effect` — discriminate `creates|updates|deletes`
//!   onto the typed `ir::CommandEffect` variant.
//! * `lower_target_expr` — lift `target query.<name>(args)` onto
//!   `ir::TargetExpr`. Reused by `lower_job_body` (workflow side).
//! * `lower_let_binding` — `let <name> = <expr>` for command-scoped
//!   intermediate bindings.
//! * `lower_named_arg` — keyword args inside `target` / `invalidates`
//!   calls.
//! * `lower_assignment` — `<field>: <expr>` rows inside
//!   `creates`/`updates` effects.
//! * `lower_invalidates_query_ref` — resolve the `query.<name>` /
//!   `<feature>.query.<name>` namespace marker against the current
//!   feature. Shared with `lower_view_ast` (surface actions).
//!
//! ## What does NOT live here
//!
//! Anything tied to constraint lifting, rate-limit, retry, deprecated
//! deserialization, public-contract carving, route-slot kind
//! discrimination — those still live in `lib.rs` per the staged
//! charter. The boundary is: this module touches only `ir::Command`
//! body-internals; it never builds an `ir::Command` envelope itself.
//!
//! Source AST shapes: `lazuli_syntax::CommandEffectDecl`,
//! `lazuli_syntax::TargetExprDecl`, `lazuli_syntax::LetBindingDecl`,
//! `lazuli_syntax::TargetArgDecl`, `lazuli_syntax::AssignmentDecl`.
//! Destination IR shapes: `lazuli_ir::CommandEffect`,
//! `lazuli_ir::TargetExpr`, `lazuli_ir::LetBinding`,
//! `lazuli_ir::NamedArg`, `lazuli_ir::Assignment`,
//! `lazuli_ir::QualifiedName`.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::expr::{lower_qualified_name, lower_raw_expr};

/// Phase L Tier 4b — shared lowering for `target query.<name>(args)`.
/// Reused by `lower_job_body` (Tier 3) and `lower_command_decl`
/// (Tier 4b) — closes the Tier 3 raw-spine carve-out.
pub(crate) fn lower_target_expr(t: &syntax::TargetExprDecl) -> ir::TargetExpr {
    ir::TargetExpr {
        query: lower_qualified_name(&t.query),
        args: t.args.iter().map(lower_named_arg).collect(),
    }
}

pub(crate) fn lower_let_binding(l: &syntax::LetBindingDecl) -> ir::LetBinding {
    ir::LetBinding {
        name: l.name.clone(),
        value: lower_raw_expr(&l.value),
    }
}

pub(crate) fn lower_named_arg(arg: &syntax::TargetArgDecl) -> ir::NamedArg {
    ir::NamedArg {
        name: arg.name.clone(),
        value: lower_raw_expr(&arg.value),
    }
}

pub(crate) fn lower_assignment(a: &syntax::AssignmentDecl) -> ir::Assignment {
    ir::Assignment {
        field: a.field.clone(),
        value: lower_raw_expr(&a.value),
    }
}

pub(crate) fn lower_command_effect(effect: &syntax::CommandEffectDecl) -> ir::CommandEffect {
    let resource = lower_qualified_name(&effect.resource);
    let assignments: Vec<ir::Assignment> =
        effect.assignments.iter().map(lower_assignment).collect();
    match effect.kind {
        syntax::CommandEffectKindDecl::Creates => ir::CommandEffect::Creates(ir::CreateEffect {
            resource,
            from_input: effect.from_input,
            assignments,
        }),
        syntax::CommandEffectKindDecl::Updates => ir::CommandEffect::Updates(ir::UpdateEffect {
            resource,
            assignments,
        }),
        syntax::CommandEffectKindDecl::Deletes => {
            ir::CommandEffect::Deletes(ir::DeleteEffect { resource })
        }
    }
}

/// Lower `invalidates` query refs into the cache-invalidation IR shape.
/// The authored namespace marker (`query.`) is syntax only:
///
/// - `query.foo` -> `<current_feature>.foo`
/// - `bar.query.baz` -> `bar.baz`
pub(crate) fn lower_invalidates_query_ref(current_feature: &str, text: &str) -> ir::QualifiedName {
    let trimmed = text.trim();
    let parts: Vec<&str> = trimmed.split('.').collect();
    match parts.as_slice() {
        ["query", name] if !name.is_empty() => ir::QualifiedName {
            feature: Some(current_feature.to_owned()),
            name: (*name).to_owned(),
        },
        [feature, "query", name] if !feature.is_empty() && !name.is_empty() => ir::QualifiedName {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        [name] if !name.is_empty() => ir::QualifiedName {
            feature: Some(current_feature.to_owned()),
            name: (*name).to_owned(),
        },
        _ => lower_qualified_name(trimmed),
    }
}
