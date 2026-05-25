//! Cell E4 — typed-args struct emission, query argument type
//! resolution, `QueryCache` lowering, and the import registration
//! pass that scans every query before banner emit.
//!
//! These helpers run BEFORE the per-kind emitters: `emit_query_file`
//! calls `register_imports_for_query` once for each query so the
//! `ImportSet` collects every package referenced from the query's
//! params / returns / cache TTL before the banner is painted.
//!
//! Boundary note: `query_arg_type_ref` is a local-scope
//! `CrossFeatureIndex.owner` lookup — it lifts bare `Foo` typerefs to
//! their owning feature (`other.Foo`) so the args struct's Go
//! declaration imports the right `<module>/other` package. The runtime
//! does not see this — it's a codegen-time projection.

use lazuli_ir::{CacheTtl, CacheTtlLiteral, Feature, Query, QueryCache, TypeRef, TypedSlot};

use super::super::imports::ImportSet;
use super::super::printer::GoPrinter;
use super::super::types::{self, TypeCtx};
use super::util::{escape_string, pascal_case, resource_for_query};

/// `<name>ErrorKeys`. The runtime reads this slice via
/// `Query.ErrorKeys` when a policy denial fires; if no
/// `policy_when_denied` was authored the slice is omitted entirely.
pub(super) fn query_error_keys_var(var_name: &str) -> String {
    format!("{var_name}ErrorKeys")
}

/// Emit `type <ArgsName> struct { ... }` for a query's typed
/// arguments. Optional slots gain a `*` pointer prefix and
/// `omitempty` JSON suffix; required slots are non-pointer.
pub(super) fn emit_args_struct(
    p: &mut GoPrinter,
    name: &str,
    slots: &[TypedSlot],
    ctx: &TypeCtx<'_>,
) {
    p.line(&format!("type {name} struct {{"));
    p.indent();
    let mut rows: Vec<(String, String, String)> = Vec::with_capacity(slots.len());
    for slot in slots {
        let type_ref = query_arg_type_ref(&slot.type_ref, ctx);
        let (go_type, _import) = types::go_type_for(&type_ref, ctx);
        let optional = !slot.required;
        let final_type = if optional {
            format!("*{}", go_type)
        } else {
            go_type
        };
        let json_suffix = if optional {
            format!("{},omitempty", slot.name)
        } else {
            slot.name.clone()
        };
        let tag = format!("`json:\"{}\"`", json_suffix);
        rows.push((pascal_case(&slot.name), final_type, tag));
    }
    let row_refs: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(n, t, g)| (n.as_str(), t.as_str(), g.as_str()))
        .collect();
    p.aligned_struct_rows(&row_refs);
    p.dedent();
    p.line("}");
}

/// Lift a bare `Unresolved("Foo")` to `UserDefined { feature: Some(<owner>),
/// name: "Foo" }` when the cross-feature index can name an owner.
/// Keeps the args struct's Go type qualified with its package so
/// `register_imports_for_type` can attach the import path.
pub(super) fn query_arg_type_ref(type_ref: &TypeRef, ctx: &TypeCtx<'_>) -> TypeRef {
    match type_ref {
        TypeRef::Unresolved(name) => match ctx.cross_index.owner(name) {
            Some(owner) => TypeRef::UserDefined(lazuli_ir::QualifiedName {
                feature: Some(owner.to_owned()),
                name: name.clone(),
            }),
            None => type_ref.clone(),
        },
        TypeRef::Many(inner) => TypeRef::Many(Box::new(query_arg_type_ref(inner, ctx))),
        _ => type_ref.clone(),
    }
}

/// Emit `Cache: &lazuli.CacheSpec{ Key, TTL, ... }`. Quoted TTLs
/// (prose like "until next midnight") aren't expressible as
/// `time.Duration` so we emit a `TTL: 0` placeholder + a TODO
/// pointing at the runtime gap.
pub(super) fn emit_cache(p: &mut GoPrinter, cache: &QueryCache) {
    p.line("Cache: &lazuli.CacheSpec{");
    p.indent();
    p.line(&format!("Key: \"{}\",", escape_string(&cache.key)));
    match &cache.ttl {
        CacheTtl::Literal(ttl) => p.line(&format!("TTL: {},", format_cache_ttl(*ttl))),
        CacheTtl::Quoted(prose) => {
            p.line("TTL: 0,");
            p.line(&format!(
                "// TODO(runtime): quoted QueryCache.ttl \"{}\" has no CacheSpec prose slot.",
                escape_string(prose)
            ));
        }
    }
    if !cache.tags.is_empty() {
        p.line(&format!(
            "// TODO(runtime): QueryCache.tags not yet in Lazuli Go lib: {}.",
            cache.tags.join(", ")
        ));
    }
    if let Some(namespace) = &cache.namespace {
        p.line(&format!(
            "// TODO(runtime): QueryCache.namespace not yet in Lazuli Go lib: {}.",
            escape_string(namespace)
        ));
    }
    p.dedent();
    p.line("},");
}

/// Does this cache spec reference a `time.Duration` literal? Used by
/// `register_imports_for_query` to decide whether to add `"time"` to
/// the package's import set.
pub(super) fn cache_uses_time(cache: &QueryCache) -> bool {
    matches!(cache.ttl, CacheTtl::Literal(_))
}

/// Render a `CacheTtlLiteral` as a Go `time.Duration` expression. Days
/// become `<n*24> * time.Hour` because the `time` package has no `Day`
/// constant.
pub(super) fn format_cache_ttl(ttl: CacheTtlLiteral) -> String {
    match ttl {
        CacheTtlLiteral::Seconds(n) => format!("{n} * time.Second"),
        CacheTtlLiteral::Minutes(n) => format!("{n} * time.Minute"),
        CacheTtlLiteral::Hours(n) => format!("{n} * time.Hour"),
        CacheTtlLiteral::Days(n) => format!("{} * time.Hour", n.saturating_mul(24)),
    }
}

/// Walk every typed surface on a `Query` (params, lookup keys,
/// SQL returns, cache TTL) and register the resulting Go package
/// imports onto the shared `ImportSet`.
pub(super) fn register_imports_for_query(
    query: &Query,
    feature: &Feature,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    match query {
        Query::List(q) => {
            for slot in &q.params {
                register_imports_for_query_arg_type(&slot.type_ref, ctx, imports);
            }
            if q.cache.as_ref().map(cache_uses_time).unwrap_or(false) {
                imports.add("time");
            }
        }
        Query::Lookup(q) => {
            let resource = resource_for_query(feature, &q.name);
            for slot in super::lookup::lookup_args(q, resource) {
                register_imports_for_query_arg_type(&slot.type_ref, ctx, imports);
            }
        }
        Query::Sql(q) => {
            for slot in &q.params {
                register_imports_for_query_arg_type(&slot.type_ref, ctx, imports);
            }
            register_imports_for_type(&q.returns, ctx, imports);
            if q.cache.as_ref().map(cache_uses_time).unwrap_or(false) {
                imports.add("time");
            }
        }
    }
}

fn register_imports_for_query_arg_type(
    type_ref: &TypeRef,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    let resolved = query_arg_type_ref(type_ref, ctx);
    register_imports_for_type(&resolved, ctx, imports);
}

fn register_imports_for_type(type_ref: &TypeRef, ctx: &TypeCtx<'_>, imports: &mut ImportSet) {
    let (_go, import) = types::go_type_for(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let TypeRef::Many(inner) = type_ref {
        register_imports_for_type(inner, ctx, imports);
    }
}
