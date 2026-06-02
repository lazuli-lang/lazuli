//! Field read-policy emission — `FieldReadPolicies` on the `Resource[T]`
//! value literal.
//!
//! W1-2 SEC-FIELDPOLICY-READ-NULL. The runtime read paths (`RunList` /
//! `RunLookup`) previously emitted `SELECT *` and applied no field-level
//! read projection, so a column gated by `policies fields <R>` `read:`
//! (`password_hash read: @actor.system`, role-gated PII, ...) was pulled
//! from the DB regardless of the caller — and only NOT leaked by luck
//! (when the destination struct happened to omit it). This module emits
//! the per-column read policy onto the `Resource[T]` value so the runtime
//! can null out columns the active actor may not read.
//!
//! ## Scope of this pass (robust subset)
//!
//! A field read policy is emitted ONLY when every one of its read atoms is
//! drawn from the closed set the runtime policy evaluator (`atomMatches`)
//! understands today: `@actor.{system,user,anonymous}`, `@role.<name>`,
//! `@scope.<name>`. The critical `@actor.system` form (the
//! `password_hash` leak in the W1-2 audit) is fully covered: non-system
//! callers fail the policy and the column is projected as `NULL`.
//!
//! Richer atoms that the runtime cannot yet evaluate — `@actor.self`,
//! `@actor.role.requires(<ROLE>)`, or a `|`-compound OR form (which the
//! field-policy parser keeps as one verbatim atom string) — are SKIPPED
//! here rather than emitted, because emitting them would fail-closed for
//! EVERY actor (the runtime returns false for an atom it does not
//! recognise), nulling the column even for the admins the policy intends
//! to admit. Skipping leaves those columns projected exactly as before
//! (no NEW leak vs the pre-fix `SELECT *`), and a `// TODO` marks the gap
//! so the role-gated nuance lands when the runtime grows `@actor.self` /
//! `@actor.role.requires` evaluation. See the W1-2 report.

use lazuli_ir::{Feature, Resource};

use crate::emitter::printer::GoPrinter;

/// One column → emittable read policy, resolved from the feature's
/// `policies fields <R>` block for `resource`.
struct FieldReadGate {
    /// DB column name (== field name; matches the struct `db` tag and the
    /// runtime's `FieldReadPolicies` map key).
    column: String,
    /// Resolved `(namespace, name)` atom pairs (OR semantics). Always
    /// non-empty for an emitted gate.
    atoms: Vec<(String, String)>,
    /// The original author atom string(s) joined, for the runtime
    /// `Policy.Name` diagnostic slot.
    display: String,
}

/// Split an `@<ns>.<name>` atom string into `(namespace, name)` when it
/// is a member of the closed runtime-evaluable set; `None` otherwise.
///
/// Recognised:
/// - `@actor.system` / `@actor.user` / `@actor.anonymous`
/// - `@role.<name>`
/// - `@scope.<name>`
///
/// Rejected (returns `None`, so the whole gate is skipped): anything with
/// a parenthesised call (`@actor.role.requires(ADMIN)`), an `@actor.self`
/// / `@actor.owner` form the runtime does not evaluate, a `|`-compound
/// (kept verbatim by the parser), or any namespace outside the closed set.
fn runtime_evaluable_atom(atom: &str) -> Option<(String, String)> {
    let stripped = atom.trim().strip_prefix('@')?;
    // Reject compound / call forms outright — these are not single atoms
    // the runtime can match.
    if stripped.contains('|') || stripped.contains('(') || stripped.contains(' ') {
        return None;
    }
    let mut parts = stripped.splitn(2, '.');
    let ns = parts.next()?;
    let nm = parts.next()?;
    if nm.is_empty() || nm.contains('.') {
        // A trailing dotted tail (`actor.role.requires`) is not a closed
        // atom — `actor` only carries `system`/`user`/`anonymous`.
        return None;
    }
    match ns {
        // Closed actor catalog the runtime `atomMatches` evaluates.
        "actor" => matches!(nm, "system" | "user" | "anonymous")
            .then(|| (ns.to_owned(), nm.to_owned())),
        // Any role name is a genuine `ctx.User.Roles` membership check.
        "role" => Some((ns.to_owned(), nm.to_owned())),
        // Only the scope names the runtime actually resolves to a
        // meaningful answer. `self` / `owner` (and any out-of-catalog
        // name like `account_owner`) fail-closed in the runtime, so
        // emitting them would null the column for valid actors — defer
        // them to the role-gated follow-up instead.
        "scope" => matches!(nm, "public" | "authenticated" | "same_org")
            .then(|| (ns.to_owned(), nm.to_owned())),
        _ => None,
    }
}

/// Collect the emittable field read gates for `resource` from the
/// feature's `policies fields` block. A field with no `read:` axis, or
/// whose read axis carries ANY non-runtime-evaluable atom, is omitted
/// (see module docs). Order follows the IR `Vec` for determinism.
fn collect_field_read_gates(feature: &Feature, resource: &Resource) -> Vec<FieldReadGate> {
    let mut gates = Vec::new();
    for fp_block in &feature.policies.fields {
        if fp_block.resource.name != resource.name {
            continue;
        }
        for fp in &fp_block.fields {
            let Some(read) = &fp.read else { continue };
            if read.is_empty() {
                continue;
            }
            let mut atoms = Vec::with_capacity(read.len());
            let mut all_ok = true;
            for atom in read {
                match runtime_evaluable_atom(atom) {
                    Some(pair) => atoms.push(pair),
                    None => {
                        all_ok = false;
                        break;
                    }
                }
            }
            if !all_ok || atoms.is_empty() {
                continue;
            }
            gates.push(FieldReadGate {
                column: fp.field.clone(),
                atoms,
                display: read.join(" | "),
            });
        }
    }
    gates
}

/// True when `resource` declares at least one field whose read policy
/// carries a non-runtime-evaluable atom — used to decide whether to emit
/// the explanatory `// TODO` next to the `FieldReadPolicies` map.
fn has_deferred_read_gate(feature: &Feature, resource: &Resource) -> bool {
    for fp_block in &feature.policies.fields {
        if fp_block.resource.name != resource.name {
            continue;
        }
        for fp in &fp_block.fields {
            let Some(read) = &fp.read else { continue };
            if read.is_empty() {
                continue;
            }
            if read.iter().any(|a| runtime_evaluable_atom(a).is_none()) {
                return true;
            }
        }
    }
    false
}

/// Emit the `FieldReadPolicies: map[string]lazuli.Policy{...}` field onto
/// the `Resource[T]` value literal, when the resource declares at least
/// one runtime-evaluable field read policy. The runtime read paths consult
/// this map to null out columns the active actor may not read (W1-2).
/// Emits nothing when there is no emittable gate (the runtime then projects
/// every declared column, matching the prior behaviour for that resource).
pub(super) fn emit_resource_value_field_read_policies(
    p: &mut GoPrinter,
    feature: &Feature,
    resource: &Resource,
) {
    let gates = collect_field_read_gates(feature, resource);
    if gates.is_empty() {
        return;
    }
    p.line("// FieldReadPolicies gates each listed column's read at the SQL");
    p.line("// projection: a caller failing the policy gets `NULL` for the column");
    p.line("// instead of its value (W1-2 SEC-FIELDPOLICY-READ-NULL).");
    if has_deferred_read_gate(feature, resource) {
        p.line("// TODO(runtime): role-gated reads using `@actor.self` /");
        p.line("// `@actor.role.requires(<ROLE>)` / `|`-compound atoms are NOT yet");
        p.line("// projected here — the runtime policy evaluator has no semantics");
        p.line("// for them, so emitting would fail-closed for every actor. Those");
        p.line("// columns stay fully projected until the runtime grows the eval.");
    }
    p.line("FieldReadPolicies: map[string]lazuli.Policy{");
    p.indent();
    for gate in &gates {
        let atoms_literal = render_atoms(&gate.atoms);
        p.line(&format!(
            "\"{}\": {{Name: \"{}\", Atoms: []lazuli.PolicyAtom{{{atoms_literal}}}}},",
            gate.column,
            escape_go_str(&gate.display),
        ));
    }
    p.dedent();
    p.line("},");
}

/// Render the OR atom list for a field read policy. Mirrors the command
/// policy emitter's category rendering: 2+ atoms are wrapped in a
/// `( A or B )` predicate group so the runtime's recursive-descent
/// evaluator (`evalOr`) consumes them as OR branches; a single atom
/// renders bare.
fn render_atoms(atoms: &[(String, String)]) -> String {
    let render_one = |ns: &str, nm: &str| format!("{{Namespace: \"{ns}\", Name: \"{nm}\"}}");
    if atoms.len() >= 2 {
        let mut parts = vec!["{Namespace: \"predicate\", Name: \"(\"}".to_owned()];
        for (i, (ns, nm)) in atoms.iter().enumerate() {
            if i > 0 {
                parts.push("{Namespace: \"predicate\", Name: \"or\"}".to_owned());
            }
            parts.push(render_one(ns, nm));
        }
        parts.push("{Namespace: \"predicate\", Name: \")\"}".to_owned());
        parts.join(", ")
    } else {
        let (ns, nm) = &atoms[0];
        render_one(ns, nm)
    }
}

/// Minimal Go double-quoted string escaper for the `Name:` diagnostic
/// slot. The atom alphabet is tame, but `|` / spaces appear in the
/// display form so we escape conservatively.
fn escape_go_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out
}
