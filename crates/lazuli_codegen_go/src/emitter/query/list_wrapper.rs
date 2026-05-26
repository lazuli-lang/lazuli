//! Cell E4 — exported Go wrapper for `query.list` values. Lifted from
//! `list.rs` (wave R8-3) so the file's prod + tests stay ≤500 LOC. The
//! wrapper mirrors the command-side `Handle<Name>` convention so
//! Go-internal callers can invoke the list without going through the
//! HTTP router.

use super::super::patterns::{PATTERN_QUERY_PGX_LIST, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::util::pascal_case;

/// Emit an exported Go wrapper for a `query.list` value. Mirrors the
/// command-side `Handle<Name>` convention so Go-internal callers can
/// invoke the list without going through the HTTP router.
pub(super) fn emit_list_query_wrapper(
    p: &mut GoPrinter,
    query_name: &str,
    var_name: &str,
    args_struct: &str,
    resource_type: &str,
) {
    let func_name = pascal_case(query_name);
    emit_pattern_header(p, PATTERN_QUERY_PGX_LIST);
    p.line(&format!(
        "// {func_name} is the exported Go wrapper around the package-private"
    ));
    p.line(&format!(
        "// `{var_name}` value. Mirrors the command-side Handle<Name> shape"
    ));
    p.line("// so Go-internal callers (other handlers, helpers, tests) can");
    p.line("// invoke the list without going through the HTTP router.");
    p.line(&format!(
        "func {func_name}(ctx *lazuli.Ctx, args {args_struct}) ([]{resource_type}, error) {{"
    ));
    p.indent();
    p.line(&format!("return {var_name}.RunList(ctx, args)"));
    p.dedent();
    p.line("}");
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{base_feature, emit, field, resource};
    use lazuli_ir::{BuiltinType, ListQuery, PolicyRef, Query, TypeRef};

    /// Gap A — `query.list` values also carry an exported wrapper so
    /// Go-internal callers can drive the list through the runtime.
    #[test]
    fn list_query_emits_exported_go_wrapper() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![field("name", TypeRef::Builtin(BuiltinType::Text), true)],
        ));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: None,
            modifier: None,
            cache: None,
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
                "func List(ctx *lazuli.Ctx, args ListCustomersArgs) ([]Customer, error) {"
            ),
            "exported list wrapper missing:\n{out}"
        );
        assert!(
            out.contains("return listCustomers.RunList(ctx, args)"),
            "wrapper must delegate to RunList:\n{out}"
        );
    }
}
