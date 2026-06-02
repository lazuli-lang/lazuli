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

use lazuli_ir::{
    CompareOp, EvalPredicate, Expr, Policies, PolicyExpr, PolicyRef, Predicate,
    parse_conditional_policy_refs,
};

/// GAP-09 (command-level) — parse the verbatim `when` text of a command-level
/// conditional policy reference (`input.scope == "Production"`) directly into a
/// Go `&lazuli.PolicyWhen{...}` literal. Only the simple closed
/// `<input.path> <op> <literal>` comparison form is rendered (the same subset
/// `render_policy_when` emits and the runtime `PolicyWhen.holds` evaluates);
/// any richer / unparsed shape returns `None` so the atom degrades to
/// unconditional. Operator precedence: longest token first so `<=` / `>=` /
/// `!=` / `==` are not mis-split as `<` / `>` / `=`.
fn render_when_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    for (token, op_go) in [
        ("<=", "<="),
        (">=", ">="),
        ("!=", "!="),
        ("==", "="),
        ("<", "<"),
        (">", ">"),
    ] {
        if let Some(idx) = trimmed.find(token) {
            let (lhs, rhs) = trimmed.split_at(idx);
            let rhs = &rhs[token.len()..];
            let lhs = lhs.trim();
            let rhs = rhs.trim();
            if lhs.is_empty() || rhs.is_empty() {
                return None;
            }
            // LHS must be a dotted path (`input.scope`); a quoted / numeric LHS
            // is not an admissible input-value-predicate path.
            if lhs.starts_with('"') || lhs.chars().next()?.is_ascii_digit() {
                return None;
            }
            let value_lit = render_when_literal(rhs)?;
            return Some(format!(
                "&lazuli.PolicyWhen{{Path: {:?}, Op: {:?}, Value: {value_lit}}}",
                lhs, op_go
            ));
        }
    }
    None
}

/// Render the RHS literal of a `when` comparison (string / integer / boolean /
/// nil) as a Go value. Mirrors `render_when_value` but parses from raw text.
fn render_when_literal(rhs: &str) -> Option<String> {
    if let Some(inner) = rhs.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Some(format!("{:?}", inner));
    }
    match rhs {
        "true" | "false" => Some(rhs.to_owned()),
        "nil" => Some("nil".to_owned()),
        _ => {
            if let Ok(n) = rhs.parse::<i64>() {
                Some(format!("int64({n})"))
            } else {
                None
            }
        }
    }
}

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
/// Atom decomposition mirrors the canonical pilot's hand-written `patchPolicy()`
/// workaround (commit 700e95b): each `@<ns>.<name>` string in
/// `PolicyCategory.atoms` parses into `{Namespace, Name}`; when the
/// category carries 2+ atoms, the list is wrapped in infix form
/// `( A or B or C )` so the runtime's recursive-descent walker
/// (evalOr) consumes one operand at a time. A named-policy atom list has
/// OR semantics (see docs/audience-policy.md): `view: @role.ADMIN,
/// @role.MANAGER` admits a caller holding EITHER role — a single-role
/// user model can never satisfy an AND of two role atoms. AND-composition
/// is expressed structurally via `policy <expr>` / `PolicyExpr::And`, not
/// via the comma-separated category atom list. Closes WAR-RUNTIME-POLICY-01.
/// SECURITY (POLICY-REF-UNRESOLVED) — render a policy literal that always
/// DENIES. Emitted whenever a non-public, author-declared policy reference
/// cannot be resolved to its atom list at this codegen pass: a cross-feature
/// `PolicyRef::External` (the per-feature emitter has no view of the referenced
/// feature's `policies` block) or a `@policy.<name>` / `Local(<name>)` whose
/// category is absent from the feature.
///
/// Fail CLOSED, never open. The pre-fix code degraded these to a Name-only
/// `lazuli.Policy{Name: "..."}` with no `Atoms`; a command/query guarded by
/// such a reference then shipped EFFECTIVELY UNGUARDED at any runtime call
/// site that does not treat an empty atom list as deny (`Api.Invoke` runs no
/// `EvalPolicy` at all). We emit a single `{Namespace: "predicate", Name:
/// "deny"}` atom: the runtime's structured evaluator (`hasPredicateAtom` →
/// `evalExpr`) walks it as a leaf that `atomMatches` rejects, so the call is
/// denied (403). The doctor rule `POLICY-REF-UNRESOLVED-001` flags the same
/// situation at build time so the author fixes the reference rather than
/// shipping a permanently-denied command.
fn format_deny_policy(name: &str) -> String {
    format!(
        "lazuli.Policy{{Name: {:?}, Atoms: []lazuli.PolicyAtom{{{{Namespace: \"predicate\", Name: \"deny\"}}}}}},",
        name
    )
}

/// Built-in policy names that resolve WITHOUT a declared `policies` category:
/// `public` (anonymous access) and `authenticated` (any signed-in user). The
/// CRUD/me-mode conventions synth default to `@policy.authenticated`, and pilots
/// write `@policy.public` for marketing/catalog reads, both without declaring a
/// category — they map directly to the closed `@scope.*` runtime atoms. Returns
/// the resolved `lazuli.Policy{...}` literal, or `None` for a non-built-in name
/// (which then routes through category resolution / the deny fallback).
fn format_builtin_policy(name: &str) -> Option<String> {
    let scope = match name {
        "public" => "public",
        "authenticated" => "authenticated",
        _ => return None,
    };
    Some(format!(
        "lazuli.Policy{{Name: \"@policy.{name}\", Atoms: []lazuli.PolicyAtom{{{{Namespace: \"scope\", Name: \"{scope}\"}}}}}},"
    ))
}

pub(super) fn format_local_policy(name: &str, policies: &Policies) -> Option<String> {
    let category = policies.categories.iter().find(|c| c.name == name)?;
    let render_atom = |atom: &String| -> String {
        let stripped = atom.strip_prefix('@').unwrap_or(atom);
        let mut parts = stripped.splitn(2, '.');
        let ns = parts.next().unwrap_or("");
        let nm = parts.next().unwrap_or("");
        format!("{{Namespace: \"{ns}\", Name: \"{nm}\"}}")
    };
    // GAP-09 — render an atom with an attached input-value predicate
    // (`When`). The atom evaluates as a normal OR-atom but the runtime
    // `EvalPolicyInput` skips it unless the predicate holds against the
    // request input. Mirrors `render_atom` plus a trailing `When:` field.
    let render_conditional = |atom: &str, when: &EvalPredicate| -> Option<String> {
        let stripped = atom.strip_prefix('@').unwrap_or(atom);
        let mut parts = stripped.splitn(2, '.');
        let ns = parts.next().unwrap_or("");
        let nm = parts.next().unwrap_or("");
        let guard = render_policy_when(when)?;
        Some(format!(
            "{{Namespace: \"{ns}\", Name: \"{nm}\", When: {guard}}}"
        ))
    };
    let mut atom_literals: Vec<String> = Vec::new();
    if category.atoms.len() >= 2 {
        atom_literals.push("{Namespace: \"predicate\", Name: \"(\"}".to_owned());
        for (i, atom) in category.atoms.iter().enumerate() {
            if i > 0 {
                atom_literals.push("{Namespace: \"predicate\", Name: \"or\"}".to_owned());
            }
            atom_literals.push(render_atom(atom));
        }
        atom_literals.push("{Namespace: \"predicate\", Name: \")\"}".to_owned());
    } else {
        for atom in &category.atoms {
            atom_literals.push(render_atom(atom));
        }
    }
    // GAP-09 — predicate-gated atoms join the OR list as flat atoms
    // carrying their own `When` guard, consistent with the unconditional
    // 2+-atom `( ... or ... )` OR-wrapping above: each predicate atom is an
    // independent OR-branch ("admin when prod" OR "manager when media").
    // A guard that cannot be rendered (richer/unparsed predicate)
    // degrades to an unconditional atom — fail-open is avoided because the
    // analyzer + doctor reject unparseable `when` shapes upstream.
    for ca in &category.conditional_atoms {
        if let Some(rendered) = render_conditional(&ca.atom, &ca.when) {
            atom_literals.push(rendered);
        } else {
            atom_literals.push(render_atom(&ca.atom));
        }
    }
    let inner = atom_literals.join(", ");
    Some(format!(
        "lazuli.Policy{{Name: \"@policy.{}\", Atoms: []lazuli.PolicyAtom{{{inner}}}}},",
        escape_string(name)
    ))
}

/// GAP-09 (command-level) — resolve a *conditional / comma-list* policy
/// reference of the form
/// `@policy.<a> [when <pred>], @policy.<b> [when <pred>] ...` into a single
/// `lazuli.Policy{...}` whose `Atoms` slice is an OR over each referenced
/// category, with every atom of a `when`-gated reference carrying the
/// reference's `When` guard.
///
/// This is the path the pre-6068b856 codegen LACKED: the whole conditional
/// comma-string landed as one opaque `PolicyRef::Atom`/`Local` name,
/// `format_local_policy` found no category with that literal name, and
/// (post-fix) the deny-fallback fired — a false-positive that DENIED a
/// legitimate command. Here we parse the GAP-09 atoms first, resolve each
/// `@policy.<name>` against the feature's categories, and emit the resolved
/// role/scope atoms gated by their `when` predicate.
///
/// Returns:
/// - `Some(Some(literal))` — the payload IS the conditional/comma form AND
///   every referenced category resolves → the rendered `lazuli.Policy{...}`.
/// - `Some(None)` — the payload IS the conditional/comma form but at least
///   one referenced category does NOT resolve → the caller must fail CLOSED
///   (deny). Security preserved: a genuinely-unresolvable ref still denies.
/// - `None` — the payload is NOT the conditional/comma form; the caller
///   continues with its normal single-ref resolution path (unaffected).
fn format_conditional_policy(raw: &str, policies: &Policies) -> Option<Option<String>> {
    let refs = parse_conditional_policy_refs(raw)?;
    // OR over each referenced category; each category's own atoms are OR'd
    // internally (and wrapped in `( ... )` when 2+), all gated by the
    // reference's optional `when` guard.
    let mut groups: Vec<String> = Vec::with_capacity(refs.len());
    for r in &refs {
        // Built-ins (`public` / `authenticated`) are resolvable without a
        // category but produce a single `@scope.*` atom — gate it too.
        // Parse the verbatim `when` text into a Go `&lazuli.PolicyWhen{...}`
        // literal. A `when` that does not parse to the simple closed
        // comparison form degrades to an UNCONDITIONAL atom — same fail-open
        // avoidance as `format_local_policy`'s `render_conditional` (the
        // analyzer + doctor reject unparseable `when` shapes upstream).
        let when_lit: Option<String> = r.when.as_deref().and_then(render_when_text);
        let atoms: Vec<(String, String)> = match builtin_scope_atom(&r.name) {
            Some(scope) => vec![("scope".to_owned(), scope.to_owned())],
            None => {
                let category = policies.categories.iter().find(|c| c.name == r.name)?;
                let mut out = Vec::new();
                for atom in &category.atoms {
                    out.push(split_ns_name(atom));
                }
                // A category may itself carry conditional atoms (rare in the
                // command-ref context); fold them in unconditionally — their
                // own predicate is independent of the command `when`.
                for ca in &category.conditional_atoms {
                    out.push(split_ns_name(&ca.atom));
                }
                if out.is_empty() {
                    // A category with NO atoms resolves to nothing enforceable
                    // — treat as unresolvable (fail closed).
                    return Some(None);
                }
                out
            }
        };
        let mut rendered: Vec<String> = Vec::with_capacity(atoms.len());
        for (ns, nm) in &atoms {
            rendered.push(match &when_lit {
                Some(w) => format!("{{Namespace: {ns:?}, Name: {nm:?}, When: {w}}}"),
                None => format!("{{Namespace: {ns:?}, Name: {nm:?}}}"),
            });
        }
        let group = if rendered.len() >= 2 {
            let mut g = vec!["{Namespace: \"predicate\", Name: \"(\"}".to_owned()];
            for (i, a) in rendered.into_iter().enumerate() {
                if i > 0 {
                    g.push("{Namespace: \"predicate\", Name: \"or\"}".to_owned());
                }
                g.push(a);
            }
            g.push("{Namespace: \"predicate\", Name: \")\"}".to_owned());
            g.join(", ")
        } else {
            rendered.join(", ")
        };
        groups.push(group);
    }
    // Join the per-reference groups with `or`. A `predicate.or` marker
    // between every group makes the runtime treat the whole list as the
    // structured (recursive-descent) form, so `When` guards are honored.
    let mut atom_list: Vec<String> = Vec::new();
    for (i, g) in groups.into_iter().enumerate() {
        if i > 0 {
            atom_list.push("{Namespace: \"predicate\", Name: \"or\"}".to_owned());
        }
        atom_list.push(g);
    }
    let inner = atom_list.join(", ");
    Some(Some(format!(
        "lazuli.Policy{{Name: {:?}, Atoms: []lazuli.PolicyAtom{{{inner}}}}},",
        raw
    )))
}

/// Split an `@<ns>.<name>` (or `<ns>.<name>`) atom string into its
/// `(namespace, name)` parts. Mirrors `format_local_policy::render_atom`.
fn split_ns_name(atom: &str) -> (String, String) {
    let stripped = atom.strip_prefix('@').unwrap_or(atom);
    let mut parts = stripped.splitn(2, '.');
    let ns = parts.next().unwrap_or("").to_owned();
    let nm = parts.next().unwrap_or("").to_owned();
    (ns, nm)
}

/// Built-in policy names resolvable without a declared category. Returns the
/// `@scope.*` atom name (`public` → `public`, `authenticated` →
/// `authenticated`), or `None` for a non-built-in name. Mirrors
/// `format_builtin_policy` but returns just the scope token for reuse inside
/// the conditional-ref resolver.
fn builtin_scope_atom(name: &str) -> Option<&'static str> {
    match name {
        "public" => Some("public"),
        "authenticated" => Some("authenticated"),
        _ => None,
    }
}

/// GAP-09 — render an `EvalPredicate` as a Go `&lazuli.PolicyWhen{...}`
/// literal for the `When` slot on a `lazuli.PolicyAtom`. Only the simple
/// closed `<input.path> <op> <literal>` comparison form is rendered;
/// richer shapes (`And`/`Or`/`Has`/`Contains`/`ToolsCalls`/`Unparsed`)
/// return `None` so the caller degrades the atom to unconditional. The
/// `Path` is the dotted field path (`input.scope` → `"input.scope"`),
/// `Op` is the comparison token, and `Value` is the Go literal.
fn render_policy_when(when: &EvalPredicate) -> Option<String> {
    let EvalPredicate::Closed(Predicate::Comparison { left, op, right }) = when else {
        return None;
    };
    // The author writes `input.<field> <op> <literal>`; the path is on the
    // left and the literal on the right. Accept the symmetric form too.
    let (path, value) = match (left, right) {
        (Expr::Path(p), v) => (p, v),
        (v, Expr::Path(p)) => (p, v),
        _ => return None,
    };
    let path_str = path.segments.join(".");
    let value_lit = render_when_value(value)?;
    let op_str = match op {
        CompareOp::Eq => "=",
        CompareOp::Ne => "!=",
        CompareOp::Lt => "<",
        CompareOp::Le => "<=",
        CompareOp::Gt => ">",
        CompareOp::Ge => ">=",
    };
    Some(format!(
        "&lazuli.PolicyWhen{{Path: {:?}, Op: {:?}, Value: {value_lit}}}",
        path_str, op_str
    ))
}

/// Render the RHS literal of a policy `when` comparison as a Go value.
/// String / integer / boolean / nil only — paths and enum literals are
/// not admissible on the value side of an input-value predicate.
fn render_when_value(expr: &Expr) -> Option<String> {
    match expr {
        Expr::String(s) => Some(format!("{:?}", s)),
        Expr::Integer(n) => Some(format!("int64({n})")),
        Expr::Boolean(b) => Some(b.to_string()),
        Expr::Nil => Some("nil".to_owned()),
        _ => None,
    }
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
            if let Some(p) = policies
                && let Some(rendered) = format_local_policy(name, p)
            {
                return rendered;
            }
            // Built-in `public` / `authenticated` resolve without a declared
            // category (closed `@scope.*` atoms).
            if let Some(builtin) = format_builtin_policy(name) {
                return builtin;
            }
            // GAP-09 (command-level) — the `name` may be the whole conditional
            // comma form `@policy.a when <p>, @policy.b when <p>`; resolve each
            // atom against the feature categories BEFORE the deny-fallback.
            if let Some(p) = policies
                && let Some(resolved) = format_conditional_policy(name, p)
            {
                match resolved {
                    Some(rendered) => return rendered,
                    // Conditional form but a referenced category is unresolvable
                    // — fall through to fail CLOSED.
                    None => return format_deny_policy(name),
                }
            }
            // SECURITY: an author-declared `@policy.<name>` that resolves to no
            // category is unenforceable here — fail CLOSED (deny), never emit a
            // Name-only empty-atoms policy that silently allows.
            format_deny_policy(&format!("@policy.{}", escape_string(name)))
        }
        PolicyRef::Atom(atom) => {
            // Atom forms split on `.`: `@role.admin` → namespace=role,
            // name=admin. The analyzer strips the leading `@` before
            // landing the IR (`policy.create`, `role.admin`,
            // `scope.same_org`, `actor.system`), but tests construct
            // the IR by hand and may leave the `@` in place — strip
            // defensively.
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            // GAP-09 (command-level) — the conditional / comma-list form
            // `@policy.a when <p>, @policy.b when <p>` lands here as ONE opaque
            // atom string (the parser stores the whole `policy` payload, and
            // `lower_policy_atom` wraps it in `PolicyRef::Atom`). Resolve each
            // `@policy.<name>` against the feature categories and emit the
            // gated atoms BEFORE the single-name `policy.` branch / deny
            // fallback. Operates on the full `atom` (entries keep their own
            // `@policy.`/`policy.` prefix). This is the regression fix: the
            // pre-6068b856 path degraded this to a Name-only (unguarded) emit;
            // 6068b856's deny-fallback then wrongly DENIED it.
            if let Some(p) = policies
                && let Some(resolved) = format_conditional_policy(atom, p)
            {
                match resolved {
                    Some(rendered) => return rendered,
                    None => return format_deny_policy(atom),
                }
            }
            // `policy.<name>` is the feature-local reference
            // `@policy.<name>` — resolves through the feature's
            // `policies` block to its atom list (WAR-RUNTIME-POLICY-01).
            // Falls back to Name-only render if no `Policies` available
            // or name not in catalog.
            if let Some(local) = stripped.strip_prefix("policy.") {
                if let Some(p) = policies
                    && let Some(rendered) = format_local_policy(local, p)
                {
                    return rendered;
                }
                // Built-in `public` / `authenticated` resolve without a
                // declared category.
                if let Some(builtin) = format_builtin_policy(local) {
                    return builtin;
                }
                // SECURITY: unresolvable `@policy.<name>` atom — fail CLOSED.
                return format_deny_policy(&format!("@policy.{}", escape_string(local)));
            }
            let mut parts = stripped.splitn(2, '.');
            let ns = parts.next().unwrap_or("");
            let nm = parts.next().unwrap_or("");
            format!(
                "lazuli.Policy{{Name: \"@{stripped}\", Atoms: []lazuli.PolicyAtom{{{{Namespace: \"{ns}\", Name: \"{nm}\"}}}}}},"
            )
        }
        PolicyRef::External { feature, name } => {
            // SECURITY (POLICY-REF-UNRESOLVED): a cross-feature policy reference
            // cannot be resolved to its atom list in this per-feature pass.
            // Fail CLOSED — emit a deny atom rather than a Name-only empty-atoms
            // policy that would ship the command effectively unguarded.
            format_deny_policy(&format!("{feature}.policy.{name}"))
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

#[cfg(test)]
mod tests {
    include!("policy_tests.rs");
}
