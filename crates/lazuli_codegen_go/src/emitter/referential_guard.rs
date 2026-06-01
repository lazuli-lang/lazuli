//! Spec 0014 — referential-guard emission (`<feature>/guards.gen.go`).
//!
//! Lowers every `restrict on_delete references <relation> via <fk> [where
//! <predicate>]` clause on a resource into a Go precondition function that
//! runs a tenant-scoped, soft-delete-aware `EXISTS` probe before the
//! resource is deleted, rejecting with `runtime.ErrReferencedInUse` when a
//! live reference is found.
//!
//! Both the hand-written COUNT and EXISTS variants the pilots wrote (15×
//! across 11 features) collapse onto this single canonical `EXISTS` shape
//! (EXISTS short-circuits and is strictly cheaper). The load-bearing
//! correctness property — and what the golden tests assert — is that the
//! emitted SQL carries:
//!
//!   - `AND tenant_id = $N`     iff the referencing relation is tenant-scoped
//!   - `AND deleted_at IS NULL` iff the referencing relation is soft-deletable
//!   - `AND (<extra_where>)`    iff the clause carried a `where` subset filter
//!
//! These predicates come from the IR's *derived* `tenant_scoped` /
//! `soft_delete` flags (set by the analyzer from the relation's schema, not
//! the author's line), so they can never be forgotten — the exact bug class
//! the primitive eliminates.

use lazuli_ir::{Feature, Resource, RestrictOnDelete};

use super::printer::GoPrinter;

/// Emit `<feature>/guards.gen.go` for a feature, or `None` when no resource
/// in the feature declares a `restrict on_delete` clause (so `module.rs`
/// skips the file entirely, keeping the output listing signal-rich).
pub fn emit_referential_guard_file(source_label: &str, feature: &Feature) -> Option<String> {
    // Sort resources by name so the file is byte-stable regardless of IR
    // vec order; only resources carrying at least one guard contribute.
    let mut guarded: Vec<&Resource> = feature
        .resources
        .iter()
        .filter(|r| !r.restrict_on_delete.is_empty())
        .collect();
    guarded.sort_by(|a, b| a.name.cmp(&b.name));
    if guarded.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    p.banner(source_label, &super::casing::gen_package_name(&feature.name));
    p.line("import (");
    p.indent();
    p.line("\"context\"");
    p.blank();
    p.line("\"lazuli.dev/runtime/lazuli\"");
    p.line("\"lazuli.dev/runtime/lazuli/runtime\"");
    p.dedent();
    p.line(")");
    p.blank();

    let mut first = true;
    for resource in &guarded {
        for guard in &resource.restrict_on_delete {
            if !first {
                p.blank();
            }
            first = false;
            emit_one_guard(&mut p, &resource.name, guard);
        }
    }

    Some(p.finish())
}

/// Emit one guard precondition function for a single `restrict on_delete`
/// clause on `protected` (the resource being protected from deletion).
fn emit_one_guard(p: &mut GoPrinter, protected: &str, guard: &RestrictOnDelete) {
    let fn_name = guard_fn_name(protected, &guard.relation);
    let where_clause = source_clause(protected, guard);

    p.line(&format!("// referential guard: {where_clause}"));
    p.line(&format!(
        "// rejects deletion of {protected} while a live {relation} references it.",
        relation = guard.relation
    ));
    // `db lazuli.DBTX` — the minimal `QueryRow(ctx, sql, args...) pgx.Row`
    // interface defined in the runtime; `*pgxpool.Pool` (what `lazuli.DB()`
    // returns, the delete-command wiring's handle) satisfies it. `tenantID`
    // / `id` are `any` so the int64 `ctx.Tenant.OrgID` + `input.<Id>` the
    // call site holds pass straight through pgx's arg binding.
    p.line(&format!(
        "func {fn_name}(ctx context.Context, db lazuli.DBTX, tenantID, id any) error {{"
    ));
    p.indent();
    // The query is a raw Go string literal; emit it with backtick quoting so
    // the multi-line SQL reads exactly as the hand-written guards did.
    p.line("const q = `SELECT EXISTS (");
    p.line(&format!("\tSELECT 1 FROM {}", guard.relation));
    p.line(&format!("\tWHERE {} = $1", guard.fk));
    let mut next_param = 2;
    if guard.tenant_scoped {
        p.line(&format!("\t  AND tenant_id = ${next_param}"));
        next_param += 1;
    }
    if guard.soft_delete {
        p.line("\t  AND deleted_at IS NULL");
    }
    if let Some(extra) = &guard.extra_where {
        p.line(&format!("\t  AND ({extra})"));
    }
    let _ = next_param;
    p.line(")`");
    p.line("var inUse bool");
    // Argument order mirrors the placeholders: id ($1) always, tenantID ($2)
    // only when tenant-scoped.
    let args = if guard.tenant_scoped {
        "q, id, tenantID"
    } else {
        "q, id"
    };
    p.line(&format!(
        "if err := db.QueryRow(ctx, {args}).Scan(&inUse); err != nil {{"
    ));
    p.indent();
    p.line("return err");
    p.dedent();
    p.line("}");
    p.line("if inUse {");
    p.indent();
    // Spec 0014 GAP-2 — reject with the authored per-guard domain code when
    // present (`runtime.NewReferencedInUseError("<CODE>")`), else the bare
    // back-compat sentinel.
    match &guard.error_code {
        Some(code) => p.line(&format!("return runtime.NewReferencedInUseError({code:?})")),
        None => p.line("return runtime.ErrReferencedInUse"),
    }
    p.dedent();
    p.line("}");
    p.line("return nil");
    p.dedent();
    p.line("}");
}

/// Render the authored clause back as a comment (`restrict on_delete
/// references <rel> via <fk> [where <pred>]`) so the generated function is
/// self-documenting and traceable to the `.lzi` source.
fn source_clause(_protected: &str, guard: &RestrictOnDelete) -> String {
    let mut s = format!(
        "restrict on_delete references {} via {}",
        guard.relation, guard.fk
    );
    if let Some(code) = &guard.error_code {
        s.push_str(&format!(" error {code}"));
    }
    if let Some(extra) = &guard.extra_where {
        s.push_str(&format!(" where {extra}"));
    }
    s
}

/// Deterministic Go identifier for one guard: `guard<Protected><Relation>Refs`.
/// `pub(crate)` so the command emitter's delete-guard prelude (Spec 0014 BUG-2
/// wiring) can name the exact same function this file emits — the two MUST
/// agree byte-for-byte or the generated call references an undefined symbol.
pub(crate) fn guard_fn_name(protected: &str, relation: &str) -> String {
    format!(
        "guard{}{}Refs",
        super::casing::pascal_case(protected),
        super::casing::pascal_case(relation)
    )
}

#[cfg(test)]
mod tests;
