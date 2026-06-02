//! `defineQuery<Args, Result>(name)` emitter + naming heuristics.
//!
//! Per the post-B1 wire contract (`docs/runtime/protocol.md`), the
//! registry key for a query is `<feature>.<short_name>` — the
//! historical `.query.` infix was dropped because the `/q/` HTTP
//! prefix already disambiguates query vs. command.
//!
//! The bulk of this module is verb-prefix / resource-suffix de-dup:
//! synth and author names often carry the verb and resource tail
//! already (`list_hosts`, `lookup_my_host`, `pending_basic_details_
//! hosts`), and the canonical export keeps that intent exactly once
//! while pluralizing the resource via the shared TS pluralizer. The
//! emitter also writes `/** @deprecated */` aliases for the legacy
//! shapes so downstream code can migrate without losing a green build.

use std::fmt::Write;

use lazuli_codegen_spec::{QueryKind, RuntimeArg, RuntimeFeature, RuntimeQuery};

use super::header::write_section_banner;
use super::naming::{field_kind_ts, lower_camel, lower_camel_wire_lowered, pascal_case};
use crate::pluralize;

pub(super) fn write_query(s: &mut String, feature: &RuntimeFeature, query: &RuntimeQuery) {
    // Runtime spec invariant: every feature has at least one resource.
    let Some(resource) = feature.resources.first() else {
        return;
    };
    let resource_pascal = pascal_case(&resource.name);
    // Wire registry key: `<feature>.<query_name>` (cell B1 dropped `.query.` infix).
    let qualified_name = format!("{}.{}", feature.name, query.short_name);

    // Verb-prefix and resource-suffix dedup: synth and author names often
    // already carry the verb/resource tail (`list_hosts`, `lookup_my_host`,
    // `pending_basic_details_hosts`). The canonical export keeps that intent
    // once, and pluralizes the resource via the shared TS pluralizer.
    let (args_iface, var_name, return_type) = match query.kind {
        QueryKind::List => (
            list_args_iface_name(&resource_pascal),
            list_query_var_name(&query.short_name, &resource_pascal),
            format!("{}[]", resource_pascal),
        ),
        QueryKind::Lookup => (
            format!("{}{}Args", resource_pascal, pascal_case(&query.short_name)),
            lookup_query_var_name(&resource.name, &query.short_name, &resource_pascal),
            resource_pascal.clone(),
        ),
    };
    let legacy_args_iface =
        legacy_query_args_iface_name(query.kind, &query.short_name, &resource_pascal);
    let legacy_var_name = legacy_query_var_name(
        query.kind,
        &resource.name,
        &query.short_name,
        &resource_pascal,
    );

    write_section_banner(s, &[format!("Query: {qualified_name}")]);

    writeln!(s, "export interface {args_iface} {{").ok();
    for arg in &query.args {
        write_query_arg(s, arg);
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
    if legacy_args_iface != args_iface {
        write_deprecated_type_alias(s, &legacy_args_iface, &args_iface);
    }

    writeln!(
        s,
        "export const {var_name} = defineQuery<{args_iface}, {return_type}>("
    )
    .ok();
    writeln!(s, "  \"{qualified_name}\",").ok();
    writeln!(s, ");").ok();
    writeln!(s).ok();
    if legacy_var_name != var_name {
        write_deprecated_const_alias(s, &legacy_var_name, &var_name);
    }
}

fn list_args_iface_name(resource_pascal: &str) -> String {
    format!("List{}Args", pluralize(resource_pascal))
}

fn legacy_query_args_iface_name(
    kind: QueryKind,
    query_short_name: &str,
    resource_pascal: &str,
) -> String {
    match kind {
        QueryKind::List => format!("List{}sArgs", resource_pascal),
        QueryKind::Lookup => format!("{}{}Args", resource_pascal, pascal_case(query_short_name)),
    }
}

fn list_query_var_name(query_short_name: &str, resource_pascal: &str) -> String {
    let resource_plural = pluralize(resource_pascal);
    if let Some(rest) = crate::lzx::strip_verb_prefix(query_short_name, "list_") {
        return list_prefixed_name(rest, resource_pascal, &resource_plural);
    }

    if query_short_name.eq_ignore_ascii_case("list") {
        return format!("list{resource_plural}");
    }

    let short_pascal = pascal_case(query_short_name);
    format!(
        "list{}",
        list_subject_pascal(&short_pascal, resource_pascal, &resource_plural)
    )
}

fn lookup_query_var_name(
    resource_name: &str,
    query_short_name: &str,
    resource_pascal: &str,
) -> String {
    if let Some(rest) = crate::lzx::strip_verb_prefix(query_short_name, "lookup_") {
        return format!("lookup{}", pascal_case(rest));
    }

    if query_short_name.starts_with("by_") {
        return format!(
            "{}{}",
            lower_camel(resource_name),
            pascal_case(query_short_name)
        );
    }

    let short_pascal = pascal_case(query_short_name);
    if ends_with_resource_subject(&short_pascal, resource_pascal, &pluralize(resource_pascal)) {
        format!("lookup{short_pascal}")
    } else {
        format!(
            "{}{}",
            lower_camel(resource_name),
            pascal_case(query_short_name)
        )
    }
}

fn legacy_query_var_name(
    kind: QueryKind,
    resource_name: &str,
    query_short_name: &str,
    resource_pascal: &str,
) -> String {
    match kind {
        QueryKind::List => {
            if let Some(rest) = crate::lzx::strip_verb_prefix(query_short_name, "list_") {
                format!("list{}", pascal_case(rest))
            } else {
                format!("list{}s", resource_pascal)
            }
        }
        QueryKind::Lookup => {
            if let Some(rest) = crate::lzx::strip_verb_prefix(query_short_name, "lookup_") {
                format!("lookup{}", pascal_case(rest))
            } else {
                format!(
                    "{}{}",
                    lower_camel(resource_name),
                    pascal_case(query_short_name)
                )
            }
        }
    }
}

fn list_prefixed_name(rest: &str, resource_pascal: &str, resource_plural: &str) -> String {
    let rest_pascal = pascal_case(rest);
    format!(
        "list{}",
        list_subject_pascal(&rest_pascal, resource_pascal, resource_plural)
    )
}

fn list_subject_pascal(short_pascal: &str, resource_pascal: &str, resource_plural: &str) -> String {
    let legacy_plural = format!("{resource_pascal}s");
    if short_pascal == resource_plural || short_pascal.ends_with(resource_plural) {
        short_pascal.to_owned()
    } else if short_pascal == legacy_plural {
        resource_plural.to_owned()
    } else if let Some(stem) = short_pascal.strip_suffix(&legacy_plural) {
        format!("{stem}{resource_plural}")
    } else if short_pascal == resource_pascal {
        resource_plural.to_owned()
    } else if let Some(stem) = short_pascal.strip_suffix(resource_pascal) {
        format!("{stem}{resource_plural}")
    } else if let Some(cleaned) = remove_embedded_resource_plural(short_pascal, resource_pascal) {
        format!("{cleaned}{resource_plural}")
    } else {
        format!("{short_pascal}{resource_plural}")
    }
}

fn remove_embedded_resource_plural(short_pascal: &str, resource_pascal: &str) -> Option<String> {
    let tokens = pascal_tokens(short_pascal);
    let resource_tokens = pascal_tokens(resource_pascal);
    let resource_last = resource_tokens.last()?;
    let resource_last_plural = pluralize(resource_last);
    let legacy_resource_last_plural = format!("{resource_last}s");
    let mut remove = vec![false; tokens.len()];

    for (index, token) in tokens.iter().enumerate() {
        if token == &resource_last_plural || token == &legacy_resource_last_plural {
            remove[index] = true;
        }
    }
    if !remove.iter().any(|remove| *remove) {
        return None;
    }

    for (index, token) in tokens.iter().enumerate() {
        let adjacent_removed =
            (index > 0 && remove[index - 1]) || remove.get(index + 1).copied().unwrap_or(false);
        if token == "As" && adjacent_removed {
            remove[index] = true;
        }
    }

    let cleaned = tokens
        .iter()
        .zip(remove)
        .filter_map(|(token, remove)| (!remove).then_some(token.as_str()))
        .collect::<String>();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn pascal_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut start = 0;
    for (index, ch) in value.char_indices().skip(1) {
        if ch.is_ascii_uppercase() {
            tokens.push(value[start..index].to_owned());
            start = index;
        }
    }
    tokens.push(value[start..].to_owned());
    tokens.retain(|token| !token.is_empty());
    tokens
}

fn ends_with_resource_subject(
    short_pascal: &str,
    resource_pascal: &str,
    resource_plural: &str,
) -> bool {
    let legacy_plural = format!("{resource_pascal}s");
    short_pascal == resource_pascal
        || short_pascal == resource_plural
        || short_pascal == legacy_plural
        || short_pascal.ends_with(resource_plural)
        || short_pascal.ends_with(&legacy_plural)
        || short_pascal.ends_with(resource_pascal)
}

fn write_deprecated_type_alias(s: &mut String, old_name: &str, new_name: &str) {
    writeln!(s, "/** @deprecated use `{new_name}` */").ok();
    writeln!(s, "export type {old_name} = {new_name};").ok();
    writeln!(s).ok();
}

fn write_deprecated_const_alias(s: &mut String, old_name: &str, new_name: &str) {
    writeln!(s, "/** @deprecated use `{new_name}` */").ok();
    writeln!(s, "export const {old_name} = {new_name};").ok();
    writeln!(s).ok();
}

fn write_query_arg(s: &mut String, arg: &RuntimeArg) {
    // Query args mirror command inputs: the Go runtime emits the json
    // tag lower-cased (`query.rs` `.to_ascii_lowercase()`), so the SDK
    // key must lower-case-then-camel to stay wire-aligned.
    let key = if arg.field_name == "ID" {
        "id".to_owned()
    } else {
        lower_camel_wire_lowered(&arg.field_name)
    };
    let suffix = if arg.optional { "?" } else { "" };
    writeln!(s, "  {key}{suffix}: {ty};", ty = field_kind_ts(arg.kind)).ok();
}
