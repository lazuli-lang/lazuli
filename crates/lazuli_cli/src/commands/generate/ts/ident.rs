//! TypeScript identifier shaping.
//!
//! Carved out of `ts/mod.rs` as part of Wave R6-5 (Rails-style refactor).
//! Pure string-shaping helpers — no IR walking, no module traversal —
//! that produce the public identifiers Lazuli's TS SDK exposes for
//! every command, query, and input interface:
//!
//! - **Command identifiers** ([`command_ident`],
//!   [`command_export_ident`], [`command_input_iface`]): camelCase
//!   verb + feature dedup (WAR-CODEGEN-TS-02 — `save_host_basic_details`
//!   in feature `host` lowers to `saveHostBasicDetails`, not
//!   `saveHostHostBasicDetails`).
//! - **Query identifiers** ([`query_ident`], [`legacy_query_ident`]):
//!   `list<Resource>s` / `lookup<Resource>By<Key>` / `search<Resource>Fulltext`
//!   shape, with the `conventions [crud, me]` dedup so
//!   `list_travelers` doesn't collapse to `listListTravelersTravelers`.
//! - **List subject shaping** ([`list_subject_pascal`],
//!   [`list_prefixed_ident`], [`remove_embedded_resource_plural`]):
//!   the per-resource pluralisation rules that keep
//!   `list<Subject>` legible across all observed pilot vocab.
//! - **Token splitter** ([`pascal_tokens`]): canonical PascalCase
//!   → `Vec<token>` split used by every subject-shaping rule above.
//! - **Verb-prefix strip** ([`strip_query_verb_prefix`]): conservative
//!   `lookup_`/`list_` strip; falls back to legacy hook shape on
//!   empty remainder.

use crate::casing::pascal_case;

use super::types::{command_is_pure_read, find_resource, is_resource_ref};

pub(crate) fn command_ident(feature: &str, command_name: &str) -> String {
    let resource_pascal = pascal_case(feature);
    let feature_lc = feature.to_ascii_lowercase();
    let mut parts = command_name.split('_');
    let verb = parts.next().unwrap_or("");
    let mut out = verb.to_ascii_lowercase();
    out.push_str(&resource_pascal);
    // Closes WAR-CODEGEN-TS-02: when the command name already contains
    // the feature name as a token (e.g. `save_host_basic_details` in
    // feature `host`), skip the duplicate token so we get
    // `saveHostBasicDetails` instead of `saveHostHostBasicDetails`.
    let mut skipped_dup = false;
    for word in parts {
        if !skipped_dup && word.eq_ignore_ascii_case(&feature_lc) {
            skipped_dup = true;
            continue;
        }
        out.push_str(&pascal_case(word));
    }
    out
}

pub(crate) fn command_export_ident(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> String {
    if command_is_pure_read(command) {
        if let Some(resource_pascal) = command_return_resource_pascal(command, module) {
            let resource_plural = lazuli_codegen_ts::pluralize(&resource_pascal);
            if command.name.eq_ignore_ascii_case("list") {
                return format!("list{resource_plural}");
            }
            if let Some(rest) = strip_query_verb_prefix(&command.name, "list_") {
                let rest_pascal = pascal_case(rest);
                return format!(
                    "list{}",
                    list_subject_pascal(&rest_pascal, &resource_pascal, &resource_plural)
                );
            }
        }
    }

    command_ident(&feature.name, &command.name)
}

fn command_return_resource_pascal(
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Option<String> {
    let lazuli_ir::CommandEffect::Returns(effect) = &command.effect else {
        return None;
    };
    resource_pascal_from_return_type(&effect.return_type, module)
}

fn resource_pascal_from_return_type(
    type_ref: &lazuli_ir::TypeRef,
    module: &lazuli_ir::Module,
) -> Option<String> {
    match type_ref {
        lazuli_ir::TypeRef::Many(inner) => resource_pascal_from_return_type(inner, module),
        lazuli_ir::TypeRef::UserDefined(name) if is_resource_ref(type_ref, module) => {
            find_resource(module, name).map(|resource| pascal_case(&resource.name))
        }
        _ => None,
    }
}

pub(crate) fn query_ident(
    _feature: &str,
    resource_pascal: &str,
    kind: lazuli_ir::QueryKind,
    query_name: &str,
) -> String {
    match kind {
        lazuli_ir::QueryKind::List | lazuli_ir::QueryKind::Sql | lazuli_ir::QueryKind::View => {
            let resource_plural = lazuli_codegen_ts::pluralize(resource_pascal);
            if query_name.eq_ignore_ascii_case("list") {
                format!("list{resource_plural}")
            } else if query_name.eq_ignore_ascii_case("fulltext") {
                format!("search{resource_plural}Fulltext")
            } else if let Some(rest) = strip_query_verb_prefix(query_name, "list_") {
                // `conventions [crud]` synth produces `list_<resource>s`;
                // without the dedup the legacy shape would emit
                // `listListTravelersTravelers` from `list_travelers`.
                list_prefixed_ident(rest, resource_pascal, &resource_plural)
            } else {
                let short_pascal = pascal_case(query_name);
                format!(
                    "list{}",
                    list_subject_pascal(&short_pascal, resource_pascal, &resource_plural)
                )
            }
        }
        lazuli_ir::QueryKind::Lookup => {
            if let Some(rest) = strip_query_verb_prefix(query_name, "lookup_") {
                // `conventions [crud, me]` synth produces `lookup_<r>` and
                // `lookup_my_<r>`; without the dedup the legacy
                // `lookup<R>By<X>` shape would emit
                // `lookupHostByLookupMyHost` from `lookup_my_host`.
                format!("lookup{}", pascal_case(rest))
            } else {
                let stripped = query_name.strip_prefix("by_").unwrap_or(query_name);
                format!("lookup{}By{}", resource_pascal, pascal_case(stripped))
            }
        }
    }
}

pub(crate) fn legacy_query_ident(
    feature: &str,
    kind: lazuli_ir::QueryKind,
    query_name: &str,
) -> String {
    let resource_pascal = pascal_case(feature);
    match kind {
        lazuli_ir::QueryKind::List | lazuli_ir::QueryKind::Sql | lazuli_ir::QueryKind::View => {
            if query_name.eq_ignore_ascii_case("list") {
                format!("list{}s", resource_pascal)
            } else if query_name.eq_ignore_ascii_case("fulltext") {
                format!("search{}sFulltext", resource_pascal)
            } else if let Some(rest) = strip_query_verb_prefix(query_name, "list_") {
                format!("list{}", pascal_case(rest))
            } else {
                format!("list{}{}s", pascal_case(query_name), resource_pascal)
            }
        }
        lazuli_ir::QueryKind::Lookup => {
            if let Some(rest) = strip_query_verb_prefix(query_name, "lookup_") {
                format!("lookup{}", pascal_case(rest))
            } else {
                let stripped = query_name.strip_prefix("by_").unwrap_or(query_name);
                format!("lookup{}By{}", resource_pascal, pascal_case(stripped))
            }
        }
    }
}

fn list_prefixed_ident(rest: &str, resource_pascal: &str, resource_plural: &str) -> String {
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
    let resource_last_plural = lazuli_codegen_ts::pluralize(resource_last);
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

/// Strip a verb prefix (`lookup_` / `list_`) from a query name, returning
/// `Some(rest)` only when the remainder pascal-cases to a non-empty
/// segment. Returns `None` for bare prefix (`lookup_`), missing prefix,
/// or empty/whitespace remainder — callers fall back to the legacy hook
/// shape. Mirrors `lazuli_codegen_ts::lzx::strip_verb_prefix`; duplicated
/// here to keep the CLI's identifier-casing rules self-contained.
fn strip_query_verb_prefix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(prefix)?;
    if rest.is_empty() {
        return None;
    }
    if pascal_case(rest).is_empty() {
        return None;
    }
    Some(rest)
}

pub(crate) fn command_input_iface(command_name: &str, feature_pascal: &str) -> String {
    let feature_lc = feature_pascal.to_ascii_lowercase();
    let mut parts = command_name.split('_');
    let verb = parts.next().unwrap_or("");
    let mut out = pascal_case(verb);
    out.push_str(feature_pascal);
    // Mirror command_ident's WAR-CODEGEN-TS-02 dedup so the *Input
    // interface name matches the command identifier shape.
    let mut skipped_dup = false;
    for word in parts {
        if !skipped_dup && word.eq_ignore_ascii_case(&feature_lc) {
            skipped_dup = true;
            continue;
        }
        out.push_str(&pascal_case(word));
    }
    out.push_str("Input");
    out
}
