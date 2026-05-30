//! Cell E4 - `Query` kind emission. Walks every `Query` declared on a
//! feature and emits typed args structs plus `lazuli.Query[A, R]`
//! values into `<feature>/query.gen.go`.
//!
//! Proposal references:
//! - section 3.3 - `Query.List` / `Query.Lookup` / `Query.Sql` value shape.
//! - section 11 - boundary discipline: every typed arg / SQL return value
//!   flows through `types::go_type_for` so imports stay centralised.
//!
//! Determinism: queries are sorted by `(name, kind)` before emission.
//! Query child vectors (`params`, `filters`, `order`, `keys`) preserve
//! IR order because that order mirrors source order and is meaningful
//! for SQL predicates / order clauses.

use lazuli_ir::{Feature, Query};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::module::EmitContext;
use super::printer::GoPrinter;
use super::types::TypeCtx;

mod util;
use util::query_kind_rank;
pub(super) use util::resource_for_query;

mod args;
use args::register_imports_for_query;

mod header;
use header::{query_callable_kind, query_policy_denied_key};

mod filters;

mod sql;
use sql::emit_sql_query;

mod list;
use list::emit_list_query;
pub(super) use list::list_var_name;

mod list_wrapper;

mod lookup;
use lookup::emit_lookup_query;
pub(super) use lookup::lookup_var_name;

mod lookup_args;
mod lookup_wrapper;

mod compose;
use compose::emit_compose_query;

#[cfg(test)]
mod test_support;

/// Emit `<feature>/query.gen.go` for a feature, or `None` when the
/// feature declares no queries.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_query_file("billing.lzi", &feature, "demo", &cross_index, &emit_ctx);
/// ```
pub fn emit_query_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    emit_ctx: &EmitContext<'_>,
) -> Option<String> {
    if feature.queries.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();

    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    let mut queries: Vec<&Query> = feature.queries.iter().collect();
    queries.sort_by(|a, b| {
        a.name()
            .cmp(b.name())
            .then_with(|| query_kind_rank(a).cmp(&query_kind_rank(b)))
    });

    imports.add("context");
    imports.add("lazuli.dev/runtime/lazuli");
    for query in &queries {
        register_imports_for_query(query, feature, &type_ctx, &mut imports);
    }
    // PG.C.2 — gated queries import `billing` (GateRef / GateBehind /
    // GateQuota) and `<module>/plan` (the package-wide Catalog;
    // unused here but kept in scope for handler-authored
    // `billing.CheckFeature(ctx, plan.Catalog, ...)` calls).
    let any_gated = queries.iter().any(|q| {
        !emit_ctx
            .gates_for(query_callable_kind(q), q.name())
            .is_empty()
    });
    if any_gated {
        imports.add("lazuli.dev/runtime/lazuli/billing");
        imports.add(&format!("{module_name}/plan"));
    }
    if queries
        .iter()
        .any(|q| query_policy_denied_key(feature, q).is_some())
    {
        imports.add("lazuli.dev/runtime/lazuli/i18n");
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for query in &queries {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_query(&mut p, feature, query, &type_ctx, emit_ctx);
    }

    Some(p.finish())
}

fn emit_query(
    p: &mut GoPrinter,
    feature: &Feature,
    query: &Query,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    match query {
        Query::List(q) => emit_list_query(p, feature, q, ctx, emit_ctx),
        Query::Lookup(q) => emit_lookup_query(p, feature, q, ctx, emit_ctx),
        Query::Sql(q) => emit_sql_query(p, feature, q, ctx, emit_ctx),
        // query.compose (W5): lower the composite read to ONE parameterized
        // SELECT embedded in query.gen.go (SQLText on the shared QueryView
        // runtime path) + the generated return-record struct.
        Query::Compose(q) => emit_compose_query(p, feature, q, ctx, emit_ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::{base_feature, emit, module_with_features, resource};
    use lazuli_ir::{ListQuery, PolicyRef};

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn deterministic_across_runs_and_sorted_by_name() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource("Customer", Vec::new()));
        feature.queries.push(Query::List(ListQuery {
            name: "zebra".to_owned(),
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
        feature.queries.push(Query::List(ListQuery {
            name: "alpha".to_owned(),
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

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);
        let alpha_pos = a.find("Query: customer.alpha").expect("alpha banner");
        let zebra_pos = a.find("Query: customer.zebra").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }

    #[test]
    fn gated_query_emits_real_prelude_field_and_billing_imports() {
        // PG.C.2 — gated queries lift the wave-4 comment annotation
        // into a real `Prelude: []billing.GateRef{...}` field that
        // `RunList` / `RunLookup` consult via `lazuli.RunPrelude`.
        let mut feature = base_feature("billing");
        feature.resources.push(resource("Invoice", Vec::new()));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: Some(50),
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let mut gates: std::collections::BTreeMap<String, Vec<lazuli_ir::Gate>> =
            std::collections::BTreeMap::new();
        gates.insert(
            "billing/query.list:list".to_owned(),
            vec![
                lazuli_ir::Gate::Behind {
                    feature: "list_invoices".to_owned(),
                },
                lazuli_ir::Gate::Quota {
                    limit: "queries_per_month".to_owned(),
                },
            ],
        );
        let module = module_with_features(vec![feature]);
        let cross_index = CrossFeatureIndex::build(&module);
        let emit_ctx =
            EmitContext::for_feature(None, "billing-app", "billing", "billing/query.gen.go")
                .with_gates(Some(&gates));
        let out = emit_query_file(
            "examples/billing.lzi",
            &module.features[0],
            "billing-app",
            &cross_index,
            &emit_ctx,
        )
        .expect("must emit");

        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "billing import missing:\n{out}"
        );
        assert!(
            out.contains("\"billing-app/plan\""),
            "plan import missing:\n{out}"
        );
        assert!(
            out.contains("Prelude: []billing.GateRef{"),
            "Prelude field missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateBehind, Name: \"list_invoices\"},"),
            "behind-gate row missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateQuota, Name: \"queries_per_month\"},"),
            "quota-gate row missing:\n{out}"
        );
    }

    #[test]
    fn ungated_query_emits_no_prelude_or_billing_import() {
        // PG.C.2 backward-compat — queries without gates emit
        // byte-equivalent wave-3 output.
        let mut feature = base_feature("customer");
        feature.resources.push(resource("Customer", Vec::new()));
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
            !out.contains("Prelude:"),
            "no Prelude when no gates:\n{out}"
        );
        assert!(
            !out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "no billing import when no gates:\n{out}"
        );
    }

}

#[cfg(test)]
mod feature_emit {
    use super::*;
    use super::test_support::{base_feature, emit, field, resource, slot};
    use lazuli_ir::{BuiltinType, ListQuery, PolicyRef, TypeRef};

    #[test]
    fn entry_point_emits_representative_query_file_shape() {
        let mut feature = base_feature("customer");
        feature.resources.push(resource(
            "Customer",
            vec![field("name", TypeRef::Builtin(BuiltinType::Text), true)],
        ));
        feature.queries.push(Query::List(ListQuery {
            name: "list".to_owned(),
            public_contract: None,
            params: vec![slot("search", TypeRef::Builtin(BuiltinType::Text), false)],
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: Some(25),
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let out = emit(&feature).expect("query feature entry point must emit non-empty output");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customergen"));
        assert!(out.contains("type ListCustomersArgs struct {"));
        assert!(out.contains("Search *string `json:\"search,omitempty\"`"));
        assert!(out.contains("var listCustomers = lazuli.Query[ListCustomersArgs, Customer]{"));
        assert!(out.contains("Paginate: 25,"));
    }

}
