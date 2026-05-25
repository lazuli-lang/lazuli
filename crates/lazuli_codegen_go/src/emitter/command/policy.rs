//! Cell E3 — `Command` policy lowering helpers.
//!
//! Extracted from `command/mod.rs` as part of the rails-style split.
//! This module owns every helper that maps `PolicyRef` / `PolicyExpr`
//! into the runtime `lazuli.Policy{...}` struct literal that the Go
//! emitter writes onto each `Command[I, O]` value:
//!
//! - `format_policy_with_expr` — primary entry; picks between the
//!   structured `policy_expr` path and the legacy `PolicyRef` path.
//! - `format_policy_with_expr_public` — re-export used by sibling
//!   emitters (`api.rs`, `query.rs`).
//! - `format_local_policy` — resolves `@policy.<name>` against the
//!   feature's `Policies.categories` (WAR-RUNTIME-POLICY-01).
//! - `render_policy_expr_atoms` / `walk_policy_expr_atoms` — flat
//!   atom-list construction for the runtime's recursive-descent
//!   evaluator.
//! - `policy_expr_display_name` / `write_policy_expr_display` —
//!   surface-form re-render for the `Name:` slot of the emitted
//!   `lazuli.Policy{...}`.
//!
//! Atom namespaces produced here are closed by contract:
//! `rbac.role`, `rbac.permission`, `predicate` (`authenticated`,
//! `and`, `or`, `not`, `(`, `)`), plus the original `<ns>` for
//! embedded `@<ns>.<name>` references (`role`, `scope`, `actor`, ...).

use lazuli_ir::{Policies, PolicyExpr, PolicyRef};

use super::escape_string;

pub(in crate::emitter) fn format_policy_with_expr_public(
    policy: &PolicyRef,
    policy_expr: Option<&PolicyExpr>,
    policies: Option<&Policies>,
) -> String {
    format_policy_with_expr(policy, policy_expr, policies)
}

/// Resolve a feature-local `@policy.<name>` reference to its atom
/// decomposition by walking `Policies.categories`. Returns the rendered
/// `lazuli.Policy{Name, Atoms}` literal when the lookup succeeds;
/// `None` when the reference is unresolved (analyzer should have
/// flagged this, but we degrade gracefully to a Name-only emit at the
/// caller).
///
/// Atom decomposition mirrors hostpoint's hand-written `patchPolicy()`
/// workaround (commit 700e95b): each `@<ns>.<name>` string in
/// `PolicyCategory.atoms` parses into `{Namespace, Name}`; when the
/// category carries 2+ atoms, the list is wrapped in infix form
/// `( A and B and C )` so the runtime's recursive-descent walker
/// (evalAnd) consumes one operand at a time. Closes WAR-RUNTIME-POLICY-01.
pub(super) fn format_local_policy(name: &str, policies: &Policies) -> Option<String> {
    let category = policies.categories.iter().find(|c| c.name == name)?;
    let render_atom = |atom: &String| -> String {
        let stripped = atom.strip_prefix('@').unwrap_or(atom);
        let mut parts = stripped.splitn(2, '.');
        let ns = parts.next().unwrap_or("");
        let nm = parts.next().unwrap_or("");
        format!("{{Namespace: \"{ns}\", Name: \"{nm}\"}}")
    };
    let mut atom_literals: Vec<String> = Vec::new();
    if category.atoms.len() >= 2 {
        atom_literals.push("{Namespace: \"predicate\", Name: \"(\"}".to_owned());
        for (i, atom) in category.atoms.iter().enumerate() {
            if i > 0 {
                atom_literals.push("{Namespace: \"predicate\", Name: \"and\"}".to_owned());
            }
            atom_literals.push(render_atom(atom));
        }
        atom_literals.push("{Namespace: \"predicate\", Name: \")\"}".to_owned());
    } else {
        for atom in &category.atoms {
            atom_literals.push(render_atom(atom));
        }
    }
    let inner = atom_literals.join(", ");
    Some(format!(
        "lazuli.Policy{{Name: \"@policy.{}\", Atoms: []lazuli.PolicyAtom{{{inner}}}}},",
        escape_string(name)
    ))
}

/// RB.S6 — render `lazuli.Policy{...}` with optional structured
/// predicate atoms drawn from a parsed `policy <expr>`. When
/// `policy_expr` is present, the rendered struct gains an `Atoms` slice
/// carrying entries with synthetic namespaces:
///
/// - `Namespace: "rbac.permission"` for `has_permission X:Y:Z`.
/// - `Namespace: "rbac.role"` for `has_role X`.
/// - `Namespace: "predicate", Name: "authenticated"` for `authenticated`.
/// - `Namespace: "predicate", Name: "and|or|not"` markers for the
///   combinator structure, flattened so the runtime can walk the slice
///   linearly (OR-of-AND-of-atoms is the Policy.Atoms convention; the
///   `predicate.*` namespace marks combinator boundaries).
///
/// Runtime evaluation (in `runtime/go/lazuli`) reads these atoms and
/// dispatches to the generated `rbac.HasRole` / `rbac.HasPermission`
/// helpers via `ctx.User.Roles`. Until the runtime hook lands, the
/// atoms surface as metadata only — visible in the generated file,
/// audit logs, and reflection.
pub(super) fn format_policy_with_expr(
    policy: &PolicyRef,
    policy_expr: Option<&PolicyExpr>,
    policies: Option<&Policies>,
) -> String {
    // When a structured policy expression is present, prefer it: the
    // legacy single-atom `Atoms: [...]` rendering is subsumed by the
    // expanded form. The `Name` slot still echoes the raw author text
    // for diagnostics.
    if let Some(expr) = policy_expr {
        let atoms = render_policy_expr_atoms(expr);
        let name = policy_expr_display_name(expr);
        if atoms.is_empty() {
            return format!("lazuli.Policy{{Name: {:?}}},", name);
        }
        let inner = atoms.join(", ");
        return format!(
            "lazuli.Policy{{Name: {:?}, Atoms: []lazuli.PolicyAtom{{{inner}}}}},",
            name
        );
    }
    match policy {
        PolicyRef::Local(name) => {
            // WAR-RUNTIME-POLICY-01 — when the caller passes the
            // feature's `Policies` block, look up the category by name
            // and emit the resolved `Atoms` slice directly. Falls back
            // to the Name-only render when the name doesn't resolve
            // (analyzer should have caught that, but we degrade
            // gracefully) or when no `Policies` was threaded through.
            if let Some(p) = policies {
                if let Some(rendered) = format_local_policy(name, p) {
                    return rendered;
                }
            }
            format!(
                "lazuli.Policy{{Name: \"@policy.{}\"}},",
                escape_string(name)
            )
        }
        PolicyRef::Atom(atom) => {
            // Atom forms split on `.`: `@role.admin` → namespace=role,
            // name=admin. The analyzer strips the leading `@` before
            // landing the IR (`policy.create`, `role.admin`,
            // `scope.same_org`, `actor.system`), but tests construct
            // the IR by hand and may leave the `@` in place — strip
            // defensively.
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            // `policy.<name>` is the feature-local reference
            // `@policy.<name>` — resolves through the feature's
            // `policies` block to its atom list (WAR-RUNTIME-POLICY-01).
            // Falls back to Name-only render if no `Policies` available
            // or name not in catalog.
            if let Some(local) = stripped.strip_prefix("policy.") {
                if let Some(p) = policies {
                    if let Some(rendered) = format_local_policy(local, p) {
                        return rendered;
                    }
                }
                return format!(
                    "lazuli.Policy{{Name: \"@policy.{}\"}},",
                    escape_string(local)
                );
            }
            let mut parts = stripped.splitn(2, '.');
            let ns = parts.next().unwrap_or("");
            let nm = parts.next().unwrap_or("");
            format!(
                "lazuli.Policy{{Name: \"@{stripped}\", Atoms: []lazuli.PolicyAtom{{{{Namespace: \"{ns}\", Name: \"{nm}\"}}}}}},"
            )
        }
        PolicyRef::External { feature, name } => {
            format!("lazuli.Policy{{Name: \"{feature}.policy.{name}\"}},")
        }
        PolicyRef::Unresolved(raw) => format!("lazuli.Policy{{Name: \"{}\"}},", escape_string(raw)),
        PolicyRef::None => "lazuli.Policy{},".to_owned(),
    }
}

/// Render a `PolicyExpr` as a flat list of `lazuli.PolicyAtom{...}`
/// literal fragments. Atoms and predicates land as-is; combinators
/// (`and` / `or` / `not`) land as marker atoms with `Namespace:
/// "predicate"` so the runtime can reconstruct the tree shape.
///
/// Closed atom namespaces produced here:
///  - `rbac.role`        (from `has_role <name>`)
///  - `rbac.permission`  (from `has_permission <perm>`)
///  - `predicate` + Name `authenticated` | `and` | `or` | `not` | `(` | `)`
///  - plus the original `<ns>` for embedded `@<ns>.<name>` atoms
///    (`role`, `scope`, `actor`, etc.).
pub(super) fn render_policy_expr_atoms(expr: &PolicyExpr) -> Vec<String> {
    let mut out = Vec::new();
    walk_policy_expr_atoms(expr, &mut out);
    out
}

pub(super) fn walk_policy_expr_atoms(expr: &PolicyExpr, out: &mut Vec<String>) {
    match expr {
        PolicyExpr::Authenticated => {
            out.push("{Namespace: \"predicate\", Name: \"authenticated\"}".to_owned())
        }
        PolicyExpr::HasRole(name) => {
            out.push(format!("{{Namespace: \"rbac.role\", Name: {:?}}}", name))
        }
        PolicyExpr::HasPermission(perm) => out.push(format!(
            "{{Namespace: \"rbac.permission\", Name: {:?}}}",
            perm
        )),
        PolicyExpr::Atom(atom) => out.push(format!(
            "{{Namespace: {:?}, Name: {:?}}}",
            atom.namespace, atom.name
        )),
        PolicyExpr::And(terms) => {
            out.push("{Namespace: \"predicate\", Name: \"(\"}".to_owned());
            for (i, term) in terms.iter().enumerate() {
                if i > 0 {
                    out.push("{Namespace: \"predicate\", Name: \"and\"}".to_owned());
                }
                walk_policy_expr_atoms(term, out);
            }
            out.push("{Namespace: \"predicate\", Name: \")\"}".to_owned());
        }
        PolicyExpr::Or(terms) => {
            out.push("{Namespace: \"predicate\", Name: \"(\"}".to_owned());
            for (i, term) in terms.iter().enumerate() {
                if i > 0 {
                    out.push("{Namespace: \"predicate\", Name: \"or\"}".to_owned());
                }
                walk_policy_expr_atoms(term, out);
            }
            out.push("{Namespace: \"predicate\", Name: \")\"}".to_owned());
        }
        PolicyExpr::Not(inner) => {
            out.push("{Namespace: \"predicate\", Name: \"not\"}".to_owned());
            walk_policy_expr_atoms(inner, out);
        }
    }
}

/// Build a human-readable `Name:` for a structured policy expression,
/// reusing the closed surface syntax (`authenticated and has_role X`).
/// Mirrors the original source as faithfully as a tree-walk allows.
pub(super) fn policy_expr_display_name(expr: &PolicyExpr) -> String {
    let mut s = String::new();
    write_policy_expr_display(expr, &mut s, false);
    s
}

pub(super) fn write_policy_expr_display(expr: &PolicyExpr, out: &mut String, parenthesize: bool) {
    match expr {
        PolicyExpr::Authenticated => out.push_str("authenticated"),
        PolicyExpr::HasRole(name) => {
            out.push_str("has_role ");
            out.push_str(name);
        }
        PolicyExpr::HasPermission(perm) => {
            out.push_str("has_permission ");
            out.push_str(perm);
        }
        PolicyExpr::Atom(atom) => {
            out.push('@');
            out.push_str(&atom.namespace);
            out.push('.');
            out.push_str(&atom.name);
        }
        PolicyExpr::And(terms) => {
            if parenthesize {
                out.push('(');
            }
            for (i, t) in terms.iter().enumerate() {
                if i > 0 {
                    out.push_str(" and ");
                }
                write_policy_expr_display(t, out, true);
            }
            if parenthesize {
                out.push(')');
            }
        }
        PolicyExpr::Or(terms) => {
            if parenthesize {
                out.push('(');
            }
            for (i, t) in terms.iter().enumerate() {
                if i > 0 {
                    out.push_str(" or ");
                }
                write_policy_expr_display(t, out, true);
            }
            if parenthesize {
                out.push(')');
            }
        }
        PolicyExpr::Not(inner) => {
            out.push_str("not ");
            write_policy_expr_display(inner, out, true);
        }
    }
}
