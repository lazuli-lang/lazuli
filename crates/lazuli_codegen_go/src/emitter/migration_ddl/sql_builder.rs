//! Low-level SQL string builders shared by every DDL sub-emitter.
//!
//! This file owns the lexical concerns of Postgres DDL emission:
//! identifier quoting, reserved-word detection, the snake_case slug
//! that table/index names use, and the `-- comment` body escaper.
//!
//! Everything here is intentionally `pub(super)` so siblings in the
//! `migration_ddl` module can compose them, but no shape leaks out of
//! the `migration_ddl` namespace. Cross-emitter helpers (e.g. for the
//! Go side of codegen) live elsewhere — the SQL dialect knobs are
//! local to this module.
//!
//! Dialects covered: Postgres 14+ (the only target Lazuli runtime
//! supports today). The reserved-word list mirrors the Postgres docs
//! plus a small set of ANSI keywords most likely to collide with
//! resource field names (`user`, `from`, `to`, `select`, …).
//!
//! See `mod.rs` for the dispatch entry point.

use lazuli_ir::{BuiltinType, Feature, Resource, Tenancy, TypeRef};

pub(super) fn effective_tenancy(feature: &Feature, resource: &Resource) -> Tenancy {
    resource
        .tenancy
        .clone()
        .or_else(|| feature.defaults.tenancy.clone())
        .unwrap_or(Tenancy::None)
}

pub(super) fn uses_timestamps(feature: &Feature, resource: &Resource) -> bool {
    resource.timestamps.unwrap_or(feature.defaults.timestamps)
}

pub(super) fn resource_uses_postgis(resource: &Resource) -> bool {
    resource
        .fields
        .iter()
        .any(|field| super::sql_column::pg_type_for(&field.type_ref).uses_postgis)
}

pub(super) fn is_direct_geo_point(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Builtin(BuiltinType::SemanticGeoPoint))
}

pub(super) fn sql_ident(raw: &str) -> String {
    if is_plain_sql_ident(raw) && !is_sql_reserved_word(raw) {
        raw.to_owned()
    } else {
        quote_ident(raw)
    }
}

pub(super) fn is_plain_sql_ident(raw: &str) -> bool {
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_lowercase()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

/// SQL reserved words that need to be quoted when used as identifiers.
/// Covers Postgres + ANSI SQL keywords most likely to collide with
/// resource field names (`user`, `from`, `to`, `select`, …).
///
/// Per Postgres docs, reserved words can be used as column names ONLY
/// if quoted. Lazuli prefers quoting over rejection so user-friendly
/// names like `User.user` (the FK column from `user: User required`)
/// remain expressible.
pub(super) fn is_sql_reserved_word(raw: &str) -> bool {
    matches!(
        raw,
        "all"
            | "analyse"
            | "analyze"
            | "and"
            | "any"
            | "array"
            | "as"
            | "asc"
            | "asymmetric"
            | "authorization"
            | "binary"
            | "both"
            | "case"
            | "cast"
            | "check"
            | "collate"
            | "collation"
            | "column"
            | "concurrently"
            | "constraint"
            | "create"
            | "cross"
            | "current_catalog"
            | "current_date"
            | "current_role"
            | "current_schema"
            | "current_time"
            | "current_timestamp"
            | "current_user"
            | "default"
            | "deferrable"
            | "desc"
            | "distinct"
            | "do"
            | "else"
            | "end"
            | "except"
            | "false"
            | "fetch"
            | "for"
            | "foreign"
            | "freeze"
            | "from"
            | "full"
            | "grant"
            | "group"
            | "having"
            | "ilike"
            | "in"
            | "initially"
            | "inner"
            | "intersect"
            | "into"
            | "is"
            | "isnull"
            | "join"
            | "lateral"
            | "leading"
            | "left"
            | "like"
            | "limit"
            | "localtime"
            | "localtimestamp"
            | "natural"
            | "not"
            | "notnull"
            | "null"
            | "offset"
            | "on"
            | "only"
            | "or"
            | "order"
            | "outer"
            | "overlaps"
            | "placing"
            | "primary"
            | "references"
            | "returning"
            | "right"
            | "select"
            | "session_user"
            | "similar"
            | "some"
            | "symmetric"
            | "system_user"
            | "table"
            | "tablesample"
            | "then"
            | "to"
            | "trailing"
            | "true"
            | "union"
            | "unique"
            | "user"
            | "using"
            | "variadic"
            | "verbose"
            | "when"
            | "where"
            | "window"
            | "with"
    )
}

pub(super) fn quote_ident(raw: &str) -> String {
    format!("\"{}\"", raw.replace('"', "\"\""))
}

pub(super) fn comment_value(raw: &str) -> String {
    raw.replace(['\n', '\r'], " ")
}

pub(super) fn lower_snake(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_is_lower_or_digit = false;
    let mut prev_is_sep = false;

    for ch in raw.chars() {
        if ch == '-' || ch == ' ' || ch == '.' || ch == '/' || ch == '\\' {
            if !out.is_empty() && !prev_is_sep {
                out.push('_');
            }
            prev_is_lower_or_digit = false;
            prev_is_sep = true;
            continue;
        }

        if ch == '_' {
            if !out.is_empty() && !prev_is_sep {
                out.push('_');
            }
            prev_is_lower_or_digit = false;
            prev_is_sep = true;
            continue;
        }

        if ch.is_ascii_uppercase() {
            if prev_is_lower_or_digit && !prev_is_sep {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = false;
            prev_is_sep = false;
            continue;
        }

        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            prev_is_sep = false;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }

    out
}

pub(super) fn feature_declares_resource(
    module: &lazuli_ir::Module,
    feature_name: &str,
    resource_name: &str,
) -> bool {
    module.features.iter().any(|feature| {
        feature.name == feature_name
            && feature
                .resources
                .iter()
                .any(|resource| resource.name == resource_name)
    })
}
