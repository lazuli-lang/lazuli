//! Cell E4 — `query.lookup` emission. Walks a `LookupQuery` and emits
//! the typed args struct (params + lifted lookup keys), the
//! `lazuli.Query[Args, Resource]` value (with header + LookupBy +
//! filters), and the exported Go wrapper.
//!
//! Lookup args carry an extra synthesis step: each `by <path> = <expr>`
//! key whose RHS is a typed input (not just `ctx.*`) materializes a
//! slot on the args struct. `lookup_args` reconciles authored params
//! with synthesised key slots so the args struct stays the minimal
//! set the runtime needs.
//!
//! Wrapper shapes:
//! - Actor-keyed lookups (all `LookupBy` sources resolve from ctx)
//!   compile to `func <Pascal>(ctx *lazuli.Ctx) (R, error)` — no
//!   args struct in the signature because there's nothing to pass.
//!   This is the `conventions [me]` synth output for `lookup_my_*`.
//! - Every other lookup keeps the `(ctx, args)` signature.

use lazuli_ir::{BuiltinType, Expr, Feature, KeyClause, LookupQuery, Resource, TypeRef, TypedSlot};

use super::super::error_resolver::emit_operation_error_keys;
use super::super::module::EmitContext;
use super::super::patterns::{PATTERN_QUERY_PGX_LOOKUP, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::super::types::TypeCtx;
use super::args::{emit_args_struct, query_error_keys_var};
use super::filters::emit_lookup_by_with_filters;
use super::header::{
    emit_gate_annotations, emit_query_header, emit_scope_gaps, query_policy_denied_key_for_parts,
};
use super::util::{lower_camel, pascal_case, resource_for_query, write_section_banner};

/// Emit a single `query.lookup` (args struct + Query value + Go wrapper).
pub(super) fn emit_lookup_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &LookupQuery,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    let resource = resource_for_query(feature, &query.name);
    let resource_type = resource
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| "struct{}".to_owned());
    let resource_name_axis = resource
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| "Result".to_owned());
    // Wire registry key: `<feature>.<query_name>`. See list.rs for rationale.
    let qualified_name = format!("{}.{}", feature.name, query.name);
    let args_struct = lookup_args_struct_name(&query.name, &resource_name_axis);
    let var_name = lookup_var_name(&query.name, &resource_name_axis);
    let error_keys_var = query_error_keys_var(&var_name);
    let args = lookup_args(query, resource);

    write_section_banner(
        p,
        &[
            format!("Query: {qualified_name}"),
            format!("  query.lookup {}", query.name),
        ],
    );

    emit_args_struct(p, &args_struct, &args, ctx);
    p.blank();

    if let Some(key_ref) = query_policy_denied_key_for_parts(
        feature,
        &query.policy,
        query.policy_expr.as_ref(),
        query.policy_when_denied.as_ref(),
    ) {
        emit_operation_error_keys(p, &error_keys_var, &qualified_name, &feature.name, key_ref);
        p.blank();
    }

    emit_pattern_header(p, PATTERN_QUERY_PGX_LOOKUP);
    let line_directive_emitted = emit_ctx.emit_line_directive(p, query.span_ref);
    p.line(&format!(
        "var {var_name} = lazuli.Query[{args_struct}, {resource_type}]{{"
    ));
    p.indent();
    emit_query_header(
        p,
        feature,
        &qualified_name,
        resource,
        "lazuli.QueryLookup",
        emit_ctx,
        &query.name,
        query.span_ref,
        &query.policy,
        query.policy_expr.as_ref(),
        query.policy_when_denied.as_ref(),
        &error_keys_var,
    );
    emit_gate_annotations(p, emit_ctx.gates_for("query.lookup", &query.name));
    emit_scope_gaps(p, &query.scope, query.scope_override);
    emit_lookup_by_with_filters(
        p,
        feature,
        resource,
        &query.keys,
        &query.filters,
        query.owner_scope_sql.as_ref(),
    );
    p.dedent();
    p.line("}");
    emit_ctx.reset_line_directive(p, line_directive_emitted);
    p.blank();
    let actor_keyed = is_actor_keyed_lookup(query);
    emit_lookup_query_wrapper(
        p,
        &query.name,
        &var_name,
        &args_struct,
        &resource_type,
        actor_keyed,
    );
}

/// Emit an exported Go wrapper that lets Go-internal callers invoke a
/// `query.lookup` value directly instead of going through the HTTP
/// router. Mirrors `command.rs`'s `Handle<Name>` wrapper convention —
/// the wrapper is byte-equivalent to manually writing
/// `lookupMyHost.RunLookup(ctx, LookupMyHostArgs{...})`, just discoverable
/// at the package level.
///
/// Two shapes:
/// - `lookup_my_*` queries with no `params` (singleton-per-actor)
///   compile to `func <Pascal>(ctx *lazuli.Ctx) (R, error)`. The args
///   value is constructed inline as the zero literal; runtime resolves
///   every `LookupBy` source from ctx.
/// - Every other lookup query keeps the `(ctx, args)` signature so
///   route params / typed inputs reach `RunLookup` verbatim.
fn emit_lookup_query_wrapper(
    p: &mut GoPrinter,
    query_name: &str,
    var_name: &str,
    args_struct: &str,
    resource_type: &str,
    actor_keyed: bool,
) {
    let func_name = pascal_case(query_name);
    emit_pattern_header(p, PATTERN_QUERY_PGX_LOOKUP);
    if actor_keyed {
        p.line(&format!(
            "// {func_name} is the exported Go wrapper around the package-private"
        ));
        p.line(&format!(
            "// `{var_name}` value. Callers invoke {func_name}(ctx) without"
        ));
        p.line("// passing an args struct — the actor identity drives the LookupBy");
        p.line("// keys via ctx-sourced bindings.");
        p.line(&format!(
            "func {func_name}(ctx *lazuli.Ctx) ({resource_type}, error) {{"
        ));
        p.indent();
        p.line(&format!(
            "return {var_name}.RunLookup(ctx, {args_struct}{{}})"
        ));
        p.dedent();
        p.line("}");
    } else {
        p.line(&format!(
            "// {func_name} is the exported Go wrapper around the package-private"
        ));
        p.line(&format!(
            "// `{var_name}` value. Mirrors the command-side Handle<Name> shape"
        ));
        p.line("// so Go-internal callers (other handlers, helpers, tests) can");
        p.line("// invoke the lookup without going through the HTTP router.");
        p.line(&format!(
            "func {func_name}(ctx *lazuli.Ctx, args {args_struct}) ({resource_type}, error) {{"
        ));
        p.indent();
        p.line(&format!("return {var_name}.RunLookup(ctx, args)"));
        p.dedent();
        p.line("}");
    }
}

/// A `query.lookup` is actor-keyed when every `LookupBy` source resolves
/// from ctx (no route params or typed inputs needed). This is the shape
/// the `conventions [me]` synth produces and the only case where the
/// emitted wrapper can drop the `args` parameter — for every other
/// lookup, the caller MUST pass an args struct so route / input keys
/// reach the runtime.
fn is_actor_keyed_lookup(query: &LookupQuery) -> bool {
    if !query.params.is_empty() {
        return false;
    }
    if query.keys.is_empty() {
        return false;
    }
    query.keys.iter().all(|key| match &key.equals {
        Expr::Path(path) => path
            .segments
            .first()
            .map(|s| s.as_str() == "ctx")
            .unwrap_or(false),
        _ => false,
    })
}

/// Reconcile authored `params` with synthesised key slots: for each
/// `by <path> = <expr>` key whose argument name doesn't already exist
/// in `params`, add a typed slot to the args struct. Used by
/// `register_imports_for_query` and `emit_lookup_query`.
pub(in crate::emitter::query) fn lookup_args(
    query: &LookupQuery,
    resource: Option<&Resource>,
) -> Vec<TypedSlot> {
    let mut args = query.params.clone();
    for key in &query.keys {
        let name = lookup_arg_name(key);
        if args.iter().any(|slot| slot.name == name) {
            continue;
        }
        args.push(TypedSlot {
            name,
            type_ref: infer_lookup_type(key, resource),
            required: true,
            constraints: lazuli_ir::FieldConstraints::default(),
            validate_skip: false,
        });
    }
    args
}

/// Pull the lookup arg name from a `KeyClause`. `by host = params.host_id`
/// becomes `host_id`; `by user.id = ctx.actor.user_id` becomes
/// `actor_user_id`. The args name shape matches what the runtime
/// expects on `FromInput(...)` sources.
fn lookup_arg_name(key: &KeyClause) -> String {
    match &key.equals {
        Expr::Path(path) => {
            let segments = match path.segments.first().map(|s| s.as_str()) {
                Some("params" | "input" | "route") => &path.segments[1..],
                _ => path.segments.as_slice(),
            };
            if segments.is_empty() {
                key.path
                    .segments
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "value".to_owned())
            } else {
                segments.join("_")
            }
        }
        _ => key
            .path
            .segments
            .last()
            .cloned()
            .unwrap_or_else(|| "value".to_owned()),
    }
}

/// Infer the Go type for a synthesised lookup arg. `id`-suffixed paths
/// resolve to `Id`; otherwise we follow the first segment to the
/// resource's matching field and copy its `TypeRef`. Falls back to
/// `Text` for free-form keys.
fn infer_lookup_type(key: &KeyClause, resource: Option<&Resource>) -> TypeRef {
    if key.path.segments.last().map(|s| s == "id").unwrap_or(false) {
        return TypeRef::Builtin(BuiltinType::Id);
    }
    if let Some(resource) = resource {
        if let Some(head) = key.path.segments.first() {
            if let Some(field) = resource.fields.iter().find(|field| &field.name == head) {
                return field.type_ref.clone();
            }
        }
    }
    TypeRef::Builtin(BuiltinType::Text)
}

fn lookup_args_struct_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name.starts_with("by_") || query_name == "by" {
        format!("{}{}Args", resource_pascal, pascal_case(query_name))
    } else {
        format!("{}Args", pascal_case(query_name))
    }
}

/// Package-private var name for a `query.lookup`. The naming axis
/// mirrors `list_var_name` — `by_*` queries get a resource prefix
/// (`hostBySlug`) because the verb already encodes "look up by";
/// every other query keeps its bare lower-camel name.
pub(in crate::emitter) fn lookup_var_name(query_name: &str, resource_pascal: &str) -> String {
    if query_name.starts_with("by_") || query_name == "by" {
        format!(
            "{}{}",
            lower_camel(resource_pascal),
            pascal_case(query_name)
        )
    } else {
        lower_camel(query_name)
    }
}
