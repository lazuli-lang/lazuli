//! Command / Query identifier resolution per §6 of the lzx-integration
//! codegen proposal.
//!
//! Encodes the verb-prefix conventions (`createSlug`, `listMineSlugs`,
//! `lookupSlugByKey`) plus the resource-plural de-duplication logic
//! that keeps `conventions [crud, me]` synth output free of double
//! prefixes (e.g. `listListTravelersTravelers`).

use crate::pluralize;

use super::casing::{lower_camel, pascal_case, pascal_tokens};
use super::ir::{CommandRef, QueryKind, QueryRef};

/// Resolve a `CommandRef` to its imported SDK identifier per the
/// convention in §6.1 / §6.3: verb + PascalResource. For example
/// `slug.command.create` → `createSlug`; `slug.command.update_email` →
/// `updateSlug` is wrong (the full `update_email` short name is split
/// on `_` so the suffix words pascalize). Matches `runtime::
/// command_var_name`.
pub(crate) fn command_ident(cmd: &CommandRef) -> String {
    let resource_pascal = pascal_case(&cmd.feature);
    let mut parts = cmd.name.split('_');
    let verb = parts.next().unwrap_or("");
    let modifier_words: Vec<&str> = parts.collect();
    let mut out = verb.to_ascii_lowercase();
    out.push_str(&resource_pascal);
    for w in modifier_words {
        out.push_str(&pascal_case(w));
    }
    out
}

/// The short tail of a command name — used as the KEY in the
/// `actions: { create: createSlug, ... }` map. `slug.command.create` →
/// `create`; `slug.command.update_email` → `updateEmail`.
pub(crate) fn command_action_key(cmd: &CommandRef) -> String {
    lower_camel(&cmd.name)
}

/// Resolve a `QueryRef` to its imported SDK identifier per §6 examples.
/// - List: `list<PascalShort><PluralFeature>` (e.g. `slug.query.mine`
///   → `listMineSlugs`). If `short` is exactly `list`, the short is
///   dropped (`slug.query.list` → `listSlugs`). Resource suffixes already
///   present in `short` are normalized instead of duplicated.
/// - Lookup: `lookup<PascalFeature>By<PascalShort>` — a leading `by_`
///   is stripped from `short` first, so `by_key` → `Key`. If `short`
///   has no `by_` prefix, the entire short pascal-cases.
/// - Sql: same convention as List for now (raw-SQL queries are rare
///   inside `.lzx` and surface a similar shape).
///
/// Verb-prefix dedup: when the author-side or synth-side query name
/// itself already starts with `lookup_` / `list_` (e.g. `lookup_my_host`,
/// `list_travelers` from `conventions [crud, me]`), the legacy
/// `<verb><Resource>By<PascalShort>` / `list<PascalShort><PluralResource>`
/// shape would double-prefix the verb. In that case we drop the
/// `<Resource>By` / duplicated resource suffix entirely and emit just
/// `<verb><RestPascal>`: `lookup_my_host` → `lookupMyHost`,
/// `list_travelers` → `listTravelers`. Empty/trivial rest falls back
/// to the legacy shape (defensive).
pub(crate) fn query_ident(q: &QueryRef) -> String {
    let resource_pascal = pascal_case(&q.feature);
    match q.kind {
        QueryKind::List | QueryKind::Sql | QueryKind::View => {
            let short = q.name.as_str();
            let resource_plural = pluralize(&resource_pascal);
            if short.eq_ignore_ascii_case("list") {
                format!("list{resource_plural}")
            } else if let Some(rest) = strip_verb_prefix(short, "list_") {
                list_prefixed_ident(rest, &resource_pascal, &resource_plural)
            } else {
                let short_pascal = pascal_case(short);
                format!(
                    "list{}",
                    list_subject_pascal(&short_pascal, &resource_pascal, &resource_plural)
                )
            }
        }
        QueryKind::Lookup => {
            if let Some(rest) = strip_verb_prefix(&q.name, "lookup_") {
                format!("lookup{}", pascal_case(rest))
            } else {
                let stripped = q.name.strip_prefix("by_").unwrap_or(&q.name);
                format!("lookup{}By{}", resource_pascal, pascal_case(stripped))
            }
        }
    }
}

/// Strip `prefix` from `name` and return `Some(rest)` iff the remainder
/// would produce a non-empty PascalCase segment. Returns `None` when
/// the name has no such prefix, when stripping leaves an empty string,
/// or when the remainder pascalizes to empty (e.g. `lookup_` alone).
/// Callers fall back to the legacy hook shape on `None`.
pub(crate) fn strip_verb_prefix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(prefix)?;
    if rest.is_empty() {
        return None;
    }
    if pascal_case(rest).is_empty() {
        return None;
    }
    Some(rest)
}

fn list_prefixed_ident(rest: &str, resource_pascal: &str, resource_plural: &str) -> String {
    let rest_pascal = pascal_case(rest);
    format!(
        "list{}",
        list_subject_pascal(&rest_pascal, resource_pascal, resource_plural)
    )
}

fn list_subject_pascal(
    short_pascal: &str,
    resource_pascal: &str,
    resource_plural: &str,
) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ident_handles_simple_and_compound_short_names() {
        let create = CommandRef {
            feature: "slug".to_owned(),
            name: "create".to_owned(),
        };
        assert_eq!(command_ident(&create), "createSlug");

        let update_email = CommandRef {
            feature: "customer".to_owned(),
            name: "update_email".to_owned(),
        };
        assert_eq!(command_ident(&update_email), "updateCustomerEmail");
    }

    #[test]
    fn query_ident_list_includes_short_name() {
        let mine = QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::List,
            name: "mine".to_owned(),
        };
        assert_eq!(query_ident(&mine), "listMineSlugs");

        let plain_list = QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::List,
            name: "list".to_owned(),
        };
        assert_eq!(query_ident(&plain_list), "listSlugs");
    }

    #[test]
    fn query_ident_list_pluralizes_resource_subject() {
        let custom_service_categories = QueryRef {
            feature: "category".to_owned(),
            kind: QueryKind::List,
            name: "custom_service".to_owned(),
        };
        assert_eq!(
            query_ident(&custom_service_categories),
            "listCustomServiceCategories"
        );
        let legacy_custom_service_categories = QueryRef {
            feature: "custom_service_category".to_owned(),
            kind: QueryKind::List,
            name: "list_custom_service_categorys".to_owned(),
        };
        assert_eq!(
            query_ident(&legacy_custom_service_categories),
            "listCustomServiceCategories"
        );

        let properties = QueryRef {
            feature: "property".to_owned(),
            kind: QueryKind::List,
            name: "list".to_owned(),
        };
        assert_eq!(query_ident(&properties), "listProperties");

        let payments = QueryRef {
            feature: "payments".to_owned(),
            kind: QueryKind::List,
            name: "list".to_owned(),
        };
        assert_eq!(query_ident(&payments), "listPayments");

        let host_transactions = QueryRef {
            feature: "service_transaction".to_owned(),
            kind: QueryKind::List,
            name: "mine_transactions_as_host".to_owned(),
        };
        assert_eq!(
            query_ident(&host_transactions),
            "listMineHostServiceTransactions"
        );
    }

    #[test]
    fn query_ident_lookup_strips_by_prefix() {
        let by_key = QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::Lookup,
            name: "by_key".to_owned(),
        };
        assert_eq!(query_ident(&by_key), "lookupSlugByKey");
    }

    // -----------------------------------------------------------------------
    // Verb-prefix dedup — covers Hostpoint bug where `conventions [crud, me]`
    // synth produced `lookupHostByLookupMyHost` / `listListTravelersTravelers`
    // because the synth's `lookup_my_<r>` / `list_<r>s` query names already
    // carry the verb and got double-prefixed by the legacy hook-name shape.
    // -----------------------------------------------------------------------

    #[test]
    fn query_ident_lookup_drops_resource_by_when_lookup_prefix_present() {
        // me-synth case: `lookup_my_host` (resource = Host) → `lookupMyHost`.
        let lookup_my_host = QueryRef {
            feature: "host".to_owned(),
            kind: QueryKind::Lookup,
            name: "lookup_my_host".to_owned(),
        };
        assert_eq!(query_ident(&lookup_my_host), "lookupMyHost");

        // crud-synth case: `lookup_traveler` (singular resource) → `lookupTraveler`.
        let lookup_traveler = QueryRef {
            feature: "traveler".to_owned(),
            kind: QueryKind::Lookup,
            name: "lookup_traveler".to_owned(),
        };
        assert_eq!(query_ident(&lookup_traveler), "lookupTraveler");

        // Authored convention: `lookup_active_users` (resource = User)
        // → `lookupActiveUsers`. Verifies the dedup also covers
        // hand-rolled queries that follow the synth naming convention.
        let lookup_active_users = QueryRef {
            feature: "user".to_owned(),
            kind: QueryKind::Lookup,
            name: "lookup_active_users".to_owned(),
        };
        assert_eq!(query_ident(&lookup_active_users), "lookupActiveUsers");
    }

    #[test]
    fn query_ident_list_drops_resource_s_when_list_prefix_present() {
        // crud-synth case: `list_travelers` (plural) → `listTravelers`.
        let list_travelers = QueryRef {
            feature: "traveler".to_owned(),
            kind: QueryKind::List,
            name: "list_travelers".to_owned(),
        };
        assert_eq!(query_ident(&list_travelers), "listTravelers");

        // crud-synth case: `list_customers` → `listCustomers`.
        let list_customers = QueryRef {
            feature: "customer".to_owned(),
            kind: QueryKind::List,
            name: "list_customers".to_owned(),
        };
        assert_eq!(query_ident(&list_customers), "listCustomers");
    }

    #[test]
    fn query_ident_list_dedups_resource_suffix_without_list_prefix() {
        let pending_basic_details_hosts = QueryRef {
            feature: "host".to_owned(),
            kind: QueryKind::List,
            name: "pending_basic_details_hosts".to_owned(),
        };
        assert_eq!(
            query_ident(&pending_basic_details_hosts),
            "listPendingBasicDetailsHosts"
        );
    }

    #[test]
    fn query_ident_preserves_legacy_shape_when_no_verb_prefix() {
        // Author-written non-prefix query: `by_email` keeps the legacy
        // `lookup<R>By<X>` shape (with `by_` stripped per existing rule).
        let by_email = QueryRef {
            feature: "user".to_owned(),
            kind: QueryKind::Lookup,
            name: "by_email".to_owned(),
        };
        assert_eq!(query_ident(&by_email), "lookupUserByEmail");

        // Author-written list query: `mine` keeps the
        // `list<PascalShort><PluralResource>` shape.
        let mine = QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::List,
            name: "mine".to_owned(),
        };
        assert_eq!(query_ident(&mine), "listMineSlugs");
    }

    #[test]
    fn query_ident_falls_back_when_verb_prefix_strip_leaves_empty() {
        // Edge: `lookup` alone (no underscore) — no `lookup_` prefix to
        // strip, so legacy `lookup<R>By<Pascal>` shape applies. Verifies
        // the dedup doesn't accidentally trigger on the bare verb.
        let bare_lookup = QueryRef {
            feature: "user".to_owned(),
            kind: QueryKind::Lookup,
            name: "lookup".to_owned(),
        };
        assert_eq!(query_ident(&bare_lookup), "lookupUserByLookup");

        // Edge: `lookup_` (trailing underscore, empty rest) — defensive
        // fallback. `pascal_case("")` is empty so we keep legacy shape.
        let trailing_underscore = QueryRef {
            feature: "user".to_owned(),
            kind: QueryKind::Lookup,
            name: "lookup_".to_owned(),
        };
        // `pascal_case("lookup_")` is `"Lookup"`, so legacy shape is
        // `lookupUserByLookup`.
        assert_eq!(query_ident(&trailing_underscore), "lookupUserByLookup");
    }

    #[test]
    fn strip_verb_prefix_helper_handles_edge_cases() {
        assert_eq!(
            strip_verb_prefix("lookup_my_host", "lookup_"),
            Some("my_host")
        );
        assert_eq!(strip_verb_prefix("list_travelers", "list_"), Some("travelers"));
        // No prefix match.
        assert_eq!(strip_verb_prefix("by_email", "lookup_"), None);
        assert_eq!(strip_verb_prefix("mine", "list_"), None);
        // Empty rest after strip — defensive fallback.
        assert_eq!(strip_verb_prefix("lookup_", "lookup_"), None);
        assert_eq!(strip_verb_prefix("list_", "list_"), None);
        // Bare verb without underscore — no prefix match.
        assert_eq!(strip_verb_prefix("lookup", "lookup_"), None);
        assert_eq!(strip_verb_prefix("list", "list_"), None);
    }
}
