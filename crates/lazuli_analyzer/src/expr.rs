//! Expression and policy-atom lowering — the leaf utility slot.
//!
//! ## Why this slot exists
//!
//! The analyzer's lowering pipeline needs a single canonical mechanical
//! projection from freeform expression / policy-atom / qualified-name
//! source text onto the `lazuli_ir::Expr`, `lazuli_ir::Path`, and
//! `lazuli_ir::Policy*` shapes. Every other lowering slice (command,
//! workflow, feature, surface) calls into these helpers, so pulling
//! them into a sibling module keeps the domain slices focused on
//! what's distinct about *their* lowering — not the shared "string →
//! IR atom" tail.
//!
//! ## What lives here
//!
//! * `lower_raw_expr` / `expr_from_text` — promote a literal token to
//!   the smallest expression kind that fits (`String`, `Integer`,
//!   `Boolean`, `Nil`, `FnCall`, `Path`). `lower_raw_expr` also
//!   handles the v1 `@fn.<name>(<args>)` extension-fn invocation.
//! * `lower_path_string` — split a `"."`-delimited path token onto
//!   `ir::Path { segments }`. Strips leading argument tail when the
//!   surface form embeds a comma (used by `target` expressions).
//! * `lower_qualified_name` — split a `<feature>.<name>` reference
//!   onto `ir::QualifiedName`. Bare names lower to feature-less form.
//! * `lower_policy_atom` — coerce `@<rest>` to `PolicyRef::Atom`,
//!   anything else to `PolicyRef::Local`. Closed-namespace catalog is
//!   enforced upstream (LSP / doctor).
//! * `lower_policy_expr` — recursive lowering of the structured
//!   `PolicyExprAst` (parser already validated keyword catalog).
//! * `lower_translation_key_ref` — verbatim `@translation.<key>`
//!   reference lowering. Cross-feature resolution lives in doctor.
//!
//! ## What does NOT live here
//!
//! Anything that consults `ir::Feature`, `ir::Module`, or knows about
//! a specific dialect (command, query, workflow) — those slices own
//! their own lowering. Helpers here must be pure mechanical text →
//! IR projections with no domain context beyond the line they're
//! reading.
//!
//! Source AST shapes: `lazuli_syntax::PolicyExprAst`,
//! `lazuli_syntax::TranslationKeyRefAst`. Destination IR shapes:
//! `lazuli_ir::Expr`, `lazuli_ir::Path`, `lazuli_ir::PolicyExpr`,
//! `lazuli_ir::PolicyAtom`, `lazuli_ir::PolicyRef`,
//! `lazuli_ir::QualifiedName`, `lazuli_ir::TranslationKeyRef`.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::helpers::span_of;

/// Lower a `@translation.<key>` reference verbatim. Resolution
/// against the per-feature `translation` block lives in doctor; the
/// IR slot only carries the literal key plus source span. Supports
/// the cross-feature `<feature>.@translation.<key>` form (parser
/// keeps the surface token single-shape — feature segment is encoded
/// in the key text).
pub(crate) fn lower_translation_key_ref(
    decl: &syntax::TranslationKeyRefAst,
) -> ir::TranslationKeyRef {
    ir::TranslationKeyRef {
        key: decl.key.clone(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// Split `<feature>.<name>` onto a typed `ir::QualifiedName`. Bare
/// identifiers (no `.`) become feature-less qualified names.
pub(crate) fn lower_qualified_name(text: &str) -> ir::QualifiedName {
    let trimmed = text.trim();
    if let Some((feature, name)) = trimmed.split_once('.') {
        ir::QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        }
    } else {
        ir::QualifiedName {
            feature: None,
            name: trimmed.to_owned(),
        }
    }
}

/// Capture a freeform expression as a typed `ir::Expr`. Handles the
/// five literal shapes (string / integer / bool / nil / path) plus
/// the v1 `@fn.<name>(<arg>...)` extension-fn invocation form (closes
/// WAR-VOCAB-CREATES-FN-CALL-01).
pub(crate) fn lower_raw_expr(text: &str) -> ir::Expr {
    let trimmed = text.trim();
    if let Some(unquoted) = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return ir::Expr::String(unquoted.to_owned());
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return ir::Expr::Integer(n);
    }
    match trimmed {
        "true" => return ir::Expr::Boolean(true),
        "false" => return ir::Expr::Boolean(false),
        "nil" => return ir::Expr::Nil,
        _ => {}
    }
    // `@fn.<name>(<arg>, ...)` — extension-fn invocation. Args are
    // recursively lowered as expressions; nested fn calls are
    // permitted at the IR level (codegen emits a TODO comment for
    // unsupported nesting today).
    if let Some(fn_call) = parse_fn_call_expr(trimmed) {
        return ir::Expr::FnCall(fn_call);
    }
    let segments = trimmed.split('.').map(|s| s.trim().to_owned()).collect();
    ir::Expr::Path(ir::Path { segments })
}

fn parse_fn_call_expr(text: &str) -> Option<ir::FnCallExpr> {
    let rest = text.strip_prefix("@fn.")?;
    // Bare reference form: `@fn.<name>` with NO call parens lowers to a
    // ZERO-ARG fn source (e.g. `tenant_id = @fn.placeholder_tenant_id`).
    // The runtime resolves it identically to the with-parens form — the
    // registered BindingFn is invoked with just `ctx` and no resolved
    // args. Without this branch the bare reference fell through to
    // `Expr::Path(["@fn", "name"])` and codegen mis-lowered it to a
    // literal `FromConst("@fn.name")` string (the WAR signup `tenant_id`
    // bug: a bigint column receiving a garbage string at INSERT).
    let Some(paren_idx) = rest.find('(') else {
        let name = rest.trim();
        // A bare `@fn.` (no name) or a dotted tail (`@fn.a.b`) is not a
        // zero-arg fn reference — leave it for the Path fallback.
        if name.is_empty() || name.contains('.') {
            return None;
        }
        return Some(ir::FnCallExpr {
            name: ir::QualifiedName {
                feature: None,
                name: name.to_owned(),
            },
            args: Vec::new(),
        });
    };
    if !rest.ends_with(')') {
        return None;
    }
    let (name_text, after) = rest.split_at(paren_idx);
    let inside = &after[1..after.len() - 1];
    let name = name_text.trim();
    if name.is_empty() {
        return None;
    }
    let args = if inside.trim().is_empty() {
        Vec::new()
    } else {
        split_fn_call_args(inside)
            .into_iter()
            .map(|arg| lower_raw_expr(&arg))
            .collect()
    };
    Some(ir::FnCallExpr {
        name: ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        },
        args,
    })
}

fn split_fn_call_args(input: &str) -> Vec<String> {
    // Splits on commas that live at paren-depth 0 outside double-quoted
    // strings. Nested fn-call args therefore stay grouped.
    let mut out = Vec::new();
    let mut depth: usize = 0;
    let mut in_quote = false;
    let mut start = 0usize;
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_quote = !in_quote;
        } else if !in_quote {
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth = depth.saturating_sub(1);
            } else if b == b',' && depth == 0 {
                let part = input[start..i].trim();
                if !part.is_empty() {
                    out.push(part.to_owned());
                }
                start = i + 1;
            }
        }
        i += 1;
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_owned());
    }
    out
}

/// Lower a `"."`-delimited path expression onto `ir::Path`. Strips
/// leading argument tail when the source token embeds a comma (the
/// `target <Resource>.<field>, key: <ref>` shape used by command
/// effects).
pub(crate) fn lower_path_string(text: &str) -> ir::Path {
    ir::Path {
        segments: text
            .split(',')
            .next()
            .unwrap_or(text)
            .split('.')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

/// Promote a literal token to the smallest expression kind that
/// fits. Stricter sibling of [`lower_raw_expr`] used in contexts
/// where `@fn.` invocations are not permitted (the v1 `target`
/// expression slot is the canonical caller).
pub(crate) fn expr_from_text(text: &str) -> ir::Expr {
    let text = text.trim();
    if let Some(stripped) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return ir::Expr::String(stripped.to_owned());
    }
    if let Ok(n) = text.parse::<i64>() {
        return ir::Expr::Integer(n);
    }
    match text {
        "true" => return ir::Expr::Boolean(true),
        "false" => return ir::Expr::Boolean(false),
        "nil" => return ir::Expr::Nil,
        _ => {}
    }
    ir::Expr::Path(ir::Path::from_segments(text.split('.').map(str::to_owned)))
}

/// Coerce a policy-atom token to the `PolicyRef` discriminator:
/// `@<rest>` → `Atom(rest)`; bare identifier → `Local(name)`. Closed
/// namespace catalog is enforced upstream by LSP and doctor.
pub(crate) fn lower_policy_atom(atom: &str) -> ir::PolicyRef {
    if let Some(rest) = atom.strip_prefix('@') {
        ir::PolicyRef::Atom(rest.to_owned())
    } else {
        ir::PolicyRef::Local(atom.to_owned())
    }
}

/// RB.S6 — lower the structured AST policy expression (parser
/// already validated permission-ref shape + closed keyword catalog).
/// The lowering is purely structural; catalog cross-checks (role /
/// perm existence) live in doctor (`RBAC-ROLE-UNDECLARED-001` /
/// `RBAC-PERM-UNDECLARED-001`).
pub(crate) fn lower_policy_expr(expr: &syntax::PolicyExprAst) -> ir::PolicyExpr {
    match expr {
        syntax::PolicyExprAst::Authenticated => ir::PolicyExpr::Authenticated,
        syntax::PolicyExprAst::HasRole(name) => ir::PolicyExpr::HasRole(name.clone()),
        syntax::PolicyExprAst::HasPermission(perm) => ir::PolicyExpr::HasPermission(perm.clone()),
        syntax::PolicyExprAst::Atom(atom) => ir::PolicyExpr::Atom(ir::PolicyAtom {
            namespace: atom.namespace.clone(),
            name: atom.name.clone(),
            args: atom.args.clone(),
        }),
        syntax::PolicyExprAst::And(terms) => {
            ir::PolicyExpr::And(terms.iter().map(lower_policy_expr).collect())
        }
        syntax::PolicyExprAst::Or(terms) => {
            ir::PolicyExpr::Or(terms.iter().map(lower_policy_expr).collect())
        }
        syntax::PolicyExprAst::Not(inner) => {
            ir::PolicyExpr::Not(Box::new(lower_policy_expr(inner)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_fn_ref_no_parens_lowers_to_zero_arg_fn_call() {
        // WAR signup `tenant_id = @fn.placeholder_tenant_id` — the bare
        // `@fn.<name>` reference (no call parens) must lower to a
        // ZERO-ARG `FnCall`, NOT a `Path`/string const. Mirrors the
        // with-parens path so codegen emits `lazuli.FromFn`.
        let expr = lower_raw_expr("@fn.placeholder_tenant_id");
        match expr {
            ir::Expr::FnCall(call) => {
                assert_eq!(call.name.name, "placeholder_tenant_id");
                assert!(call.name.feature.is_none());
                assert!(call.args.is_empty(), "bare ref carries no args");
            }
            other => panic!("expected FnCall, got {other:?}"),
        }
    }

    #[test]
    fn fn_call_with_parens_still_lowers_to_fn_call() {
        // The with-parens form must stay correct (regression guard).
        let expr = lower_raw_expr("@fn.hash_password(input.password)");
        match expr {
            ir::Expr::FnCall(call) => {
                assert_eq!(call.name.name, "hash_password");
                assert_eq!(call.args.len(), 1);
            }
            other => panic!("expected FnCall, got {other:?}"),
        }
    }

    #[test]
    fn empty_parens_fn_call_lowers_to_zero_arg_fn_call() {
        let expr = lower_raw_expr("@fn.now()");
        match expr {
            ir::Expr::FnCall(call) => {
                assert_eq!(call.name.name, "now");
                assert!(call.args.is_empty());
            }
            other => panic!("expected FnCall, got {other:?}"),
        }
    }

    #[test]
    fn bare_fn_with_dotted_tail_is_not_a_fn_call() {
        // `@fn.a.b` is ambiguous (not a flat fn name) — leave it as a
        // Path so the existing fallback/diagnostics handle it.
        let expr = lower_raw_expr("@fn.a.b");
        assert!(
            matches!(expr, ir::Expr::Path(_)),
            "dotted @fn tail should fall through to Path, got {expr:?}"
        );
    }
}
