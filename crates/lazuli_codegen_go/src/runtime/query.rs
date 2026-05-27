//! Query block emission for the runtime-form Go emitter. Includes the
//! args struct, the `lazuli.Query` value literal, filter / search /
//! lookup / cache shapes. See `super::emit_feature_go` for the
//! orchestrator.

use std::fmt::Write;

use lazuli_codegen_spec::{QueryKind, RuntimeFeature, RuntimeQuery};

use super::{
    field_kind_go, lower_camel, pascal_case, write_aligned_struct_rows, write_policy,
    write_section_banner,
};

pub(super) fn write_query(s: &mut String, feature: &RuntimeFeature, query: &RuntimeQuery) {
    // Runtime spec invariant: every feature has at least one resource. If
    // somehow absent, skip query emission (nothing to bind to).
    let Some(resource) = feature.resources.first() else { return };
    let resource_pascal = pascal_case(&resource.name);
    let resource_var = format!("{}Resource", lower_camel(&resource.name));
    // Wire registry key: `<feature>.<query_name>` (cell B1 dropped `.query.` infix).
    let qualified_name = format!("{}.{}", feature.name, query.short_name);

    let (args_struct, var_name, return_type) = match query.kind {
        QueryKind::List => (
            format!("List{}sArgs", resource_pascal),
            format!("list{}s", resource_pascal),
            resource_pascal.clone(),
        ),
        QueryKind::Lookup => (
            format!("{}{}Args", resource_pascal, pascal_case(&query.short_name)),
            format!(
                "{}{}",
                lower_camel(&resource.name),
                pascal_case(&query.short_name)
            ),
            resource_pascal.clone(),
        ),
    };

    write_section_banner(
        s,
        &[
            format!("Query: {qualified_name}"),
            format!(
                "  query.{} {}",
                query_kind_word(query.kind),
                query.short_name
            ),
        ],
    );

    // Args struct.
    writeln!(s, "type {args_struct} struct {{").ok();
    let rows: Vec<(String, String, String)> = query
        .args
        .iter()
        .map(|arg| {
            let go_type = field_kind_go(arg.kind);
            let final_type = if arg.optional {
                format!("*{go_type}")
            } else {
                go_type.to_string()
            };
            let json_tag = if arg.optional {
                format!(
                    "`json:\"{},omitempty\"`",
                    arg.field_name.to_ascii_lowercase()
                )
            } else {
                format!("`json:\"{}\"`", arg.field_name.to_ascii_lowercase())
            };
            (arg.field_name.clone(), final_type, json_tag)
        })
        .collect();
    write_aligned_struct_rows(s, &rows);
    writeln!(s, "}}").ok();
    writeln!(s).ok();

    // Query var.
    writeln!(
        s,
        "var {var_name} = lazuli.Query[{args_struct}, {return_type}]{{"
    )
    .ok();
    writeln!(s, "\tName:     \"{qualified_name}\",").ok();
    writeln!(s, "\tResource: &{resource_var},").ok();
    let kind_const = match query.kind {
        QueryKind::List => "lazuli.QueryList",
        QueryKind::Lookup => "lazuli.QueryLookup",
    };
    writeln!(s, "\tKind:     {kind_const},").ok();
    write_policy(s, "\t", &query.policy_name, &query.policy_atoms);

    if !query.filters.is_empty() {
        writeln!(s, "\tFilters: []lazuli.FilterRule{{").ok();
        for f in &query.filters {
            writeln!(
                s,
                "\t\t{{Column: \"{}\", When: lazuli.FromInput(\"{}\")}},",
                f.column, f.when_input
            )
            .ok();
        }
        writeln!(s, "\t}},").ok();
    }
    if let Some(search) = &query.search {
        writeln!(s, "\tSearch: &lazuli.SearchSpec{{").ok();
        writeln!(
            s,
            "\t\tSource: lazuli.FromInput(\"{}\"),",
            search.source_input
        )
        .ok();
        let cols: Vec<String> = search.over.iter().map(|c| format!("\"{c}\"")).collect();
        writeln!(s, "\t\tOver:   []string{{{}}},", cols.join(", ")).ok();
        writeln!(s, "\t\tMode:   lazuli.SearchContains,").ok();
        writeln!(s, "\t}},").ok();
    }
    if !query.lookup_by.is_empty() {
        writeln!(s, "\tLookupBy: []lazuli.LookupKey{{").ok();
        for k in &query.lookup_by {
            writeln!(
                s,
                "\t\t{{Column: \"{}\", Source: lazuli.FromInput(\"{}\")}},",
                k.column, k.source_input
            )
            .ok();
        }
        writeln!(s, "\t}},").ok();
    }
    if query.paginate > 0 {
        writeln!(s, "\tPaginate: {},", query.paginate).ok();
    }
    if let Some(cache) = &query.cache {
        writeln!(s, "\tCache: &lazuli.CacheSpec{{").ok();
        writeln!(s, "\t\tKey: \"{}\",", cache.key).ok();
        writeln!(s, "\t\tTTL: {},", cache.ttl).ok();
        writeln!(s, "\t}},").ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

pub(super) fn query_kind_word(kind: QueryKind) -> &'static str {
    match kind {
        QueryKind::List => "list",
        QueryKind::Lookup => "lookup",
    }
}
