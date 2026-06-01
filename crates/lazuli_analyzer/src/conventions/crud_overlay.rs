//! Spec 0018 — `crud` overlay composition (analyzer-only).
//!
//! The `crud` block authored on a `conventions [crud]` resource carries
//! per-effect overlays (`create`/`update`/`delete`). This module lowers
//! the AST overlay into an IR-ready [`ResolvedCrudOverlay`] and MERGES it
//! into the synthesized `create_<r>` / `update_<r>` / `delete_<r>`
//! commands DURING the conventions pass — before lowering — so the
//! emitted IR is byte-identical to the equivalent hand-rolled command.
//!
//! **No new IR shape.** Every overlaid command still maps to exactly one
//! existing `CommandEffect` (`Creates`/`Updates`/`Deletes`). The overlay
//! is consumed here and never reaches `ir::Resource` (RULE-VOCAB-03).
//!
//! ## Merge semantics
//!
//! * `policy`         — REPLACES the synth's `authenticated` default.
//!   Lowered via the same [`lower_policy_atom`] the hand-rolled
//!   `policy @policy.<x>` uses, so the resulting `PolicyRef` is identical.
//! * `validate`       — IR-irrelevant. The hand-rolled `validate @validator.*`
//!   is Doctor-only (it does not lower to an `ir::Command` field), so it
//!   carries no IR weight either; recorded on the resolved overlay for
//!   surface/doctor parity but never written into the command.
//! * `assign`         — ADDS `ir::Assignment`s to the synthesized `creates`/
//!   `updates` effect, after the auto-generated `<field> = input.<field>`
//!   rows, in author order. Lowered via the same [`lower_raw_expr`] the
//!   hand-rolled effect-assignment block uses, so identical RHS text
//!   produces identical `ir::Expr`.
//! * `emits`          — APPENDS event names to `Command.emits`.
//! * `input excludes` — REMOVES the named fields from the synth-generated
//!   input AND drops their auto-generated `<field> = input.<field>`
//!   assignment (so a field the author overlays with `assign <field> = ...`
//!   isn't also bound from a now-absent input slot).

use std::collections::HashMap;

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::expr::{lower_policy_atom, lower_raw_expr};

/// IR-ready overlay for one resource, keyed by effect. Built once per
/// resource from the AST [`syntax::CrudOverlayAst`] and consumed by the
/// synth pass.
#[derive(Debug, Default, Clone)]
pub(crate) struct ResolvedCrudOverlay {
    pub(crate) create: Option<ResolvedEffectOverlay>,
    pub(crate) update: Option<ResolvedEffectOverlay>,
    pub(crate) delete: Option<ResolvedEffectOverlay>,
}

/// IR-ready per-effect overlay. `policy` is pre-lowered; `assigns` are
/// pre-lowered `ir::Assignment`s; `emits` / `input_excludes` are plain
/// names; `validate` is carried for parity only (IR-irrelevant).
#[derive(Debug, Default, Clone)]
pub(crate) struct ResolvedEffectOverlay {
    pub(crate) policy: Option<ir::PolicyRef>,
    pub(crate) validate: Vec<String>,
    pub(crate) input_excludes: Vec<String>,
    pub(crate) assigns: Vec<ir::Assignment>,
    pub(crate) emits: Vec<String>,
}

/// Lower an AST overlay into the IR-ready form. Reuses the exact lowering
/// helpers the hand-rolled command path uses (`lower_policy_atom` for
/// `policy`, `lower_raw_expr` for the `assign` RHS), which is what makes
/// the merged IR byte-identical to a hand-rolled equivalent.
pub(crate) fn resolve_crud_overlay(ast: &syntax::CrudOverlayAst) -> ResolvedCrudOverlay {
    ResolvedCrudOverlay {
        create: ast.create.as_ref().map(resolve_effect_overlay),
        update: ast.update.as_ref().map(resolve_effect_overlay),
        delete: ast.delete.as_ref().map(resolve_effect_overlay),
    }
}

fn resolve_effect_overlay(ast: &syntax::CrudEffectOverlayAst) -> ResolvedEffectOverlay {
    ResolvedEffectOverlay {
        policy: ast.policy.as_deref().map(lower_policy_atom),
        validate: ast.validate.clone(),
        input_excludes: ast.input_excludes.clone(),
        assigns: ast
            .assigns
            .iter()
            .map(|a| ir::Assignment {
                field: a.field.clone(),
                value: lower_raw_expr(&a.value),
            })
            .collect(),
        emits: ast.emits.clone(),
    }
}

/// Build the per-resource overlay map the synth pass consumes, keyed by
/// resource name. Resources without a `crud` block are absent (today's
/// bare synth, byte-identical).
pub(crate) fn collect_crud_overlays(
    resources: &[syntax::ResourceDecl],
) -> HashMap<String, ResolvedCrudOverlay> {
    let mut map = HashMap::new();
    for r in resources {
        if let Some(overlay) = &r.crud_overlay {
            map.insert(r.name.clone(), resolve_crud_overlay(overlay));
        }
    }
    map
}

/// Apply a `create` overlay to the synthesized `create_<r>` command.
pub(crate) fn merge_create(cmd: &mut ir::Command, overlay: &ResolvedEffectOverlay) {
    apply_input_excludes(cmd, &overlay.input_excludes);
    if let Some(policy) = &overlay.policy {
        cmd.policy = policy.clone();
    }
    if let ir::CommandEffect::Creates(effect) = &mut cmd.effect {
        effect.assignments.extend(overlay.assigns.iter().cloned());
    }
    cmd.emits.extend(overlay.emits.iter().cloned());
}

/// Apply an `update` overlay to the synthesized `update_<r>` command.
pub(crate) fn merge_update(cmd: &mut ir::Command, overlay: &ResolvedEffectOverlay) {
    apply_input_excludes(cmd, &overlay.input_excludes);
    if let Some(policy) = &overlay.policy {
        cmd.policy = policy.clone();
    }
    if let ir::CommandEffect::Updates(effect) = &mut cmd.effect {
        effect.assignments.extend(overlay.assigns.iter().cloned());
    }
    cmd.emits.extend(overlay.emits.iter().cloned());
}

/// Apply a `delete` overlay to the synthesized `delete_<r>` command.
/// Delete carries no input/assigns by construction (the synth delete is
/// keyed by route id), so only `policy` + `emits` apply. Soft-delete
/// awareness is owned upstream (spec 0015) — the overlay does not touch it.
pub(crate) fn merge_delete(cmd: &mut ir::Command, overlay: &ResolvedEffectOverlay) {
    if let Some(policy) = &overlay.policy {
        cmd.policy = policy.clone();
    }
    cmd.emits.extend(overlay.emits.iter().cloned());
}

/// Drop the named fields from the command's typed input AND from the
/// auto-generated `<field> = input.<field>` assignment list, so excluding
/// a system/derived field also removes the now-dangling input binding.
fn apply_input_excludes(cmd: &mut ir::Command, excludes: &[String]) {
    if excludes.is_empty() {
        return;
    }
    let drop: std::collections::HashSet<&str> = excludes.iter().map(String::as_str).collect();
    if let ir::CommandInput::Typed(slots) = &mut cmd.input {
        slots.retain(|s| !drop.contains(s.name.as_str()));
        if slots.is_empty() {
            cmd.input = ir::CommandInput::Empty;
        }
    }
    match &mut cmd.effect {
        ir::CommandEffect::Creates(e) => {
            e.assignments.retain(|a| !drop.contains(a.field.as_str()));
        }
        ir::CommandEffect::Updates(e) => {
            e.assignments.retain(|a| !drop.contains(a.field.as_str()));
        }
        _ => {}
    }
}

#[cfg(test)]
mod crud_overlay_tests {
    include!("crud_overlay_tests.rs");
}
