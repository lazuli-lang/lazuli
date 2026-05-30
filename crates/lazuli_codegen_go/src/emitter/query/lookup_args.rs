//! Cell E4 — args struct synthesis for `query.lookup`. Lifted from
//! `lookup.rs` (wave R8-3) so the file's prod + tests stay ≤500 LOC.
//!
//! Lookup args carry an extra synthesis step: each `by <path> = <expr>`
//! key whose RHS is a typed input (not just `ctx.*`) materializes a
//! slot on the args struct. `lookup_args` reconciles authored params
//! with synthesised key slots so the args struct stays the minimal
//! set the runtime needs.

use lazuli_ir::{BuiltinType, Expr, KeyClause, LookupQuery, Resource, TypeRef, TypedSlot};

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
    if let Some(resource) = resource
        && let Some(head) = key.path.segments.first()
        && let Some(field) = resource.fields.iter().find(|field| &field.name == head)
    {
        return field.type_ref.clone();
    }
    TypeRef::Builtin(BuiltinType::Text)
}
