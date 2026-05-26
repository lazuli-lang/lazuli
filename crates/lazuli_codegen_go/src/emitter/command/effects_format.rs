//! Binding-source formatters for `effects.rs` — lower individual
//! `Expr` values inside a `Creates` / `Updates` / `Deletes` assignment
//! into the matching Lazuli Go lib `From*` source constructor:
//!
//! - `format_binding_row` — composes one `"<col>": <FromX>(...),` row
//!   per `Assignment`.
//! - `format_binding_source` — `Expr` dispatcher (Path / String /
//!   Integer / Boolean / Enum / Nil / FnCall).
//! - `format_path_source` — `Path` segments lowering
//!   (`input.X` → `FromInput` / `FromInputOptional`, `ctx.X` →
//!   `FromCtx`, `target.X` → `FromTarget`, `route.X` → `FromInput`).
//!
//! Lifted out of `effects.rs` (wave R8-2c) so the parent file stays
//! under the ≤500-LOC gold standard. These three functions form a
//! coherent layer of the production code — the Effect emitter calls
//! `format_binding_row` for each Assignment, and the row formatter
//! delegates downward through `format_binding_source` →
//! `format_path_source`. Moving the whole layer keeps the call graph
//! readable (single arrow from `effects.rs` into this file).
//!
//! Visibility: `pub(super)` so the three Effect emitters in
//! `effects.rs` (`emit_creates_effect`, `emit_updates_effect`,
//! `emit_deletes_effect`) can call them. No external (cross-emitter)
//! caller — the parallel `query/filters.rs` carries its own local
//! `format_path_source` for the filter axis.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{Assignment, Expr};

use super::{escape_string, format_expr, pascal_case};

pub(super) fn format_binding_row(
    assignment: &Assignment,
    let_bindings: &BTreeMap<&str, &Expr>,
    optional_inputs: &BTreeSet<&str>,
) -> String {
    let column = assignment.field.to_ascii_lowercase();
    let value_repr = format_binding_source(&assignment.value, let_bindings, optional_inputs);
    format!("\"{column}\": {value_repr},")
}

pub(super) fn format_binding_source(
    expr: &Expr,
    let_bindings: &BTreeMap<&str, &Expr>,
    optional_inputs: &BTreeSet<&str>,
) -> String {
    match expr {
        Expr::Path(path) => format_path_source(&path.segments, let_bindings, optional_inputs),
        Expr::String(s) => format!("lazuli.FromConst(\"{}\")", escape_string(s)),
        Expr::Integer(n) => format!("lazuli.FromConst({n})"),
        Expr::Boolean(b) => format!("lazuli.FromConst({b})"),
        Expr::Enum(literal) => {
            let qualifier = literal
                .type_name
                .as_ref()
                .map(|q| pascal_case(&q.name))
                .unwrap_or_default();
            if qualifier.is_empty() {
                format!("lazuli.FromConst(\"{}\")", literal.variant)
            } else {
                format!(
                    "lazuli.FromConst({}{})",
                    qualifier,
                    pascal_case(&literal.variant)
                )
            }
        }
        Expr::Nil => "lazuli.FromConst(nil)".to_owned(),
        // WAR-VOCAB-CREATES-FN-CALL-01 closure — `@fn.<name>(<arg>...)`
        // in a creates/updates binding emits a `lazuli.FromFn` source
        // that the runtime resolves by looking up the user-registered
        // BindingFn (via `lazuli.RegisterBindingFn`), resolving the
        // arg sources first, then invoking the fn with the resolved
        // args. Host apps register the fn at boot:
        //   lazuli.RegisterBindingFn("hash_password", func(ctx, args ...any) (any, error) {...})
        Expr::FnCall(call) => {
            let arg_sources: Vec<String> = call
                .args
                .iter()
                .map(|a| format_binding_source(a, let_bindings, optional_inputs))
                .collect();
            let args_arr = if arg_sources.is_empty() {
                "nil".to_owned()
            } else {
                format!("[]lazuli.Source{{{}}}", arg_sources.join(", "))
            };
            format!(
                "lazuli.FromFn(\"{}\", {})",
                escape_string(&call.name.name),
                args_arr
            )
        }
    }
}

/// Classify a `Path` (e.g. `input.name`, `ctx.user`, `route.id`) into
/// the matching Lazuli Go lib source constructor. `optional_inputs`
/// names the input slots whose Go type is `*T` (optional); for those,
/// `input.<X>` is rendered as `lazuli.FromInputOptional` so the runtime
/// can skip the column when the wire payload omits the field.
pub(super) fn format_path_source(
    segments: &[String],
    let_bindings: &BTreeMap<&str, &Expr>,
    optional_inputs: &BTreeSet<&str>,
) -> String {
    if let [name] = segments {
        if let Some(target_expr) = let_bindings.get(name.as_str()) {
            return format!(
                "lazuli.FromConst(\"{}\") /* let {} = {} */",
                escape_string(name),
                name,
                format_expr(target_expr)
            );
        }
    }

    let head = segments.first().map(|s| s.as_str()).unwrap_or("");
    let tail = if segments.len() > 1 {
        segments[1..].join(".")
    } else {
        String::new()
    };
    match head {
        "input" => {
            // For nested paths like `input.address.city`, only the
            // top-level slot's optionality determines skip-on-nil — the
            // runtime's `readPath` returns nil for any nil pointer
            // encountered en route, so the same FromInputOptional
            // contract applies. The set is keyed by the top-level slot
            // name, which is `tail` for `[input, X]` and the first
            // component for deeper paths.
            let first_segment = tail.split('.').next().unwrap_or(tail.as_str());
            if optional_inputs.contains(first_segment) {
                format!("lazuli.FromInputOptional(\"{tail}\")")
            } else {
                format!("lazuli.FromInput(\"{tail}\")")
            }
        }
        "ctx" => format!("lazuli.FromCtx(\"{tail}\")"),
        "target" => format!("lazuli.FromTarget(\"{tail}\")"),
        "route" => format!("lazuli.FromInput(\"{tail}\")"),
        _ => {
            // Fallback: surface as a constant string so the output
            // remains Go-valid. Cell I4 will upgrade this to a hard
            // diagnostic for unresolved binding sources.
            format!("lazuli.FromConst(\"{}\")", segments.join("."))
        }
    }
}
