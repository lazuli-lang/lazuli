//! Cell E4 — exported Go wrapper for `query.lookup` values. Lifted from
//! `lookup.rs` (wave R8-3) so the file's prod + tests stay ≤500 LOC.
//!
//! Two wrapper shapes share this file:
//! - Actor-keyed lookups (all `LookupBy` sources resolve from ctx)
//!   compile to `func <Pascal>(ctx *lazuli.Ctx) (R, error)` — no
//!   args struct in the signature because there's nothing to pass.
//!   This is the `conventions [me]` synth output for `lookup_my_*`.
//! - Every other lookup keeps the `(ctx, args)` signature.

use lazuli_ir::{Expr, LookupQuery};

use super::super::patterns::{PATTERN_QUERY_PGX_LOOKUP, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::util::pascal_case;

/// Emit an exported Go wrapper that lets Go-internal callers invoke a
/// `query.lookup` value directly instead of going through the HTTP
/// router. Mirrors `command.rs`'s `Handle<Name>` wrapper convention —
/// the wrapper is byte-equivalent to manually writing
/// `lookupMyHost.RunLookup(ctx, LookupMyHostArgs{...})`, just discoverable
/// at the package level.
pub(super) fn emit_lookup_query_wrapper(
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
pub(super) fn is_actor_keyed_lookup(query: &LookupQuery) -> bool {
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

#[cfg(test)]
mod tests {
    use super::super::test_support::{base_feature, emit, field, qname, resource};
    use lazuli_ir::{
        BuiltinType, Expr, KeyClause, LookupQuery, PolicyRef, Query, Tenancy, TypeRef,
    };

    /// Gap A — every emitted `query.lookup` value carries an exported
    /// Go wrapper so Go-internal callers (other handlers, helpers,
    /// tests) can invoke the lookup without going through the HTTP
    /// router. Wrapper shape mirrors commands' `Handle<Name>` (here:
    /// PascalCase of the query name).
    #[test]
    fn lookup_query_emits_exported_go_wrapper() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![field(
                "email",
                TypeRef::Builtin(BuiltinType::SemanticEmail),
                true,
            )],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "by_email".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![KeyClause {
                path: lazuli_ir::Path::from_segments(["email"]),
                equals: Expr::Path(lazuli_ir::Path::from_segments(["email"])),
            }],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(
                "func ByEmail(ctx *lazuli.Ctx, args CustomerByEmailArgs) (Customer, error) {"
            ),
            "exported lookup wrapper missing:\n{out}"
        );
        assert!(
            out.contains("return customerByEmail.RunLookup(ctx, args)"),
            "wrapper must delegate to RunLookup:\n{out}"
        );
    }

    /// Gap A — `lookup_my_*` queries authored by the `conventions [me]`
    /// synth carry no params and resolve every LookupBy from ctx. The
    /// wrapper drops the `args` parameter so callers write
    /// `LookupMyHost(ctx)`; the args literal is zero-constructed inline.
    #[test]
    fn actor_keyed_lookup_query_emits_args_less_go_wrapper() {
        let mut feature = base_feature("host");
        feature.defaults.tenancy = Some(Tenancy::Org);
        feature.resources.push(resource("Org", Vec::new()));
        feature.resources.push(resource("User", Vec::new()));
        feature.resources.push(resource(
            "Host",
            vec![field("user", TypeRef::UserDefined(qname("User")), true)],
        ));
        feature.queries.push(Query::Lookup(LookupQuery {
            name: "lookup_my_host".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: vec![
                KeyClause {
                    path: lazuli_ir::Path::from_segments(["org"]),
                    equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "org_id"])),
                },
                KeyClause {
                    path: lazuli_ir::Path::from_segments(["user"]),
                    equals: Expr::Path(lazuli_ir::Path::from_segments(["ctx", "actor", "user_id"])),
                },
            ],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("func LookupMyHost(ctx *lazuli.Ctx) (Host, error) {"),
            "actor-keyed wrapper signature must drop args:\n{out}"
        );
        assert!(
            out.contains("return lookupMyHost.RunLookup(ctx, LookupMyHostArgs{})"),
            "actor-keyed wrapper must zero-construct args inline:\n{out}"
        );
    }
}
