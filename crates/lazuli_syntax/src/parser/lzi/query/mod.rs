//! Feature-level `query.*` block parsers.
//!
//! Lazuli queries come in four canonical shapes, chosen by the
//! header keyword:
//!
//! | Header              | Body                                                            | IR shape         |
//! |---------------------|-----------------------------------------------------------------|------------------|
//! | `query.lookup <n>`  | `by <field>: <Type>` inline OR `params`/`filters` block         | `LookupQueryDecl`|
//! | `query.list <n>`    | `policy`, `params`, `filters`, `search`, `cache`, `order`, ...  | `ListQueryDecl`  |
//! | `query.sql <n>`     | `returns <Type>` + `sql "./<path>.sql"`                         | `SqlQueryDecl`   |
//! | `query.view <n>`    | `returns <Type>` + `source @file.<name>.sql`                    | `SqlQueryDecl`   |
//!
//! Every shape supports the closed catalog of cross-cutting children
//! that an LLM expects when authoring a read path:
//!
//! - `policy <expr>` — single source of truth for read authorization.
//! - `params <name>: <Type>` — inputs the caller supplies; share the
//!   inline-constraint catalog (min/max/in/pattern) with command
//!   inputs and resource fields via `extract_field_constraints`
//!   (`pub(super)` in `mod.rs`) and `split_command_input_modifiers`
//!   (`pub(super)` in `command.rs`).
//! - `scope` / `scope override` — tenant boundary; override is opt-in
//!   and demands a `reason "..."` clause.
//! - `filters`, `order`, `paginate`, `cache`, `search` — list-only
//!   ergonomics, gated by header keyword.
//!
//! The dispatch entry `parse_query_decl` is `pub(super)` so the
//! feature-skeleton walker in `mod.rs` keeps a single call site.

mod blocks;
mod list;
mod lookup;
mod sql;

use super::super::common::{SourceLine, line_error};
use super::super::error::ParseError;

use crate::ast::QueryDecl;

use list::parse_query_list_decl;
use lookup::parse_query_lookup_decl;
use sql::{parse_query_sql_decl, parse_query_view_decl};

pub(super) fn parse_query_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    if let Some(rest) = trimmed.strip_prefix("query.lookup ") {
        return parse_query_lookup_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.list ") {
        return parse_query_list_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.sql ") {
        return parse_query_sql_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.view ") {
        return parse_query_view_decl(lines, start, rest);
    }
    Err(line_error(
        header,
        "query header must be `query.list <name>`, `query.lookup <name> by ...`, `query.sql <name>`, or `query.view <name>`",
    ))
}


// =============================================================================
// Phase L Tier 4d — `query` block parser slice tests.
// =============================================================================
#[cfg(test)]
mod query_parser_tests {
    use super::super::parse_feature_skeletons;

    #[test]
    fn query_list_full_block_parses() {
        let source = r#"
feature customer
  domain
    query.list list
      modifier @query_modifier.query_scope_modifier

      params
        lifecycle_stage: CustomerStatus optional
        search: Text optional

      filters
        lifecycle_stage when params.lifecycle_stage

      search params.search over name, email
        mode contains

      cache
        key customer.list(params)
        ttl "5 minutes"

      paginate 50
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features[0].queries.len(), 1);
        match &features[0].queries[0] {
            crate::QueryDecl::List(q) => {
                assert_eq!(q.name, "list");
                assert_eq!(
                    q.modifier.as_deref(),
                    Some("@query_modifier.query_scope_modifier")
                );
                assert_eq!(q.params.len(), 2);
                assert_eq!(q.filters.len(), 1);
                let search = q.search.as_ref().expect("search");
                assert_eq!(search.fields, vec!["name", "email"]);
                assert_eq!(search.mode.as_deref(), Some("contains"));
                assert_eq!(q.paginate, Some(50));
            }
            other => panic!("expected query.list, got {other:?}"),
        }
    }

    #[test]
    fn query_lookup_inline_parses() {
        let source = r#"
feature customer
  domain
    query.lookup by_id by id: ID
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::Lookup(l) => {
                assert_eq!(l.name, "by_id");
                assert_eq!(l.keys.len(), 1);
                assert_eq!(l.keys[0].name, "id");
                assert_eq!(l.keys[0].type_text, "ID");
            }
            other => panic!("expected query.lookup, got {other:?}"),
        }
    }

    #[test]
    fn query_sql_parses() {
        let source = r#"
feature customer
  domain
    query.sql lifetime_value
      returns CustomerLtv[]
      scope
        org = ctx.user.org
      sql "./queries/customer_lifetime_value.sql"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::Sql(s) => {
                assert_eq!(s.kind, crate::SqlQueryKind::Sql);
                assert_eq!(s.name, "lifetime_value");
                assert_eq!(s.returns, "CustomerLtv[]");
                assert_eq!(s.sql_path, "./queries/customer_lifetime_value.sql");
                assert_eq!(s.scope_lines.len(), 1);
            }
            other => panic!("expected query.sql, got {other:?}"),
        }
    }

    #[test]
    fn query_view_parses_file_source_and_list_returns() {
        let source = r#"
feature host
  domain
    query.view host_home_view
      policy @policy.host_only
      returns list of HostHomeRow
      source @file.host_home_view.sql
      params
        user_id: ID required
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::Sql(s) => {
                assert_eq!(s.kind, crate::SqlQueryKind::View);
                assert_eq!(s.name, "host_home_view");
                assert_eq!(s.policy.as_deref(), Some("@policy.host_only"));
                assert_eq!(s.returns, "list of HostHomeRow");
                assert_eq!(s.sql_path, "@file.host_home_view.sql");
                assert_eq!(s.params.len(), 1);
                assert_eq!(s.params[0].name, "user_id");
            }
            other => panic!("expected query.view, got {other:?}"),
        }
    }

    #[test]
    fn query_view_parses_scalar_returns_and_scope() {
        let source = r#"
feature host
  domain
    query.view property_detail_view
      returns PropertyDetailRow
      source @file.property_detail_view.sql
      scope
        org = ctx.actor.org_id
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::Sql(s) => {
                assert_eq!(s.kind, crate::SqlQueryKind::View);
                assert_eq!(s.name, "property_detail_view");
                assert_eq!(s.returns, "PropertyDetailRow");
                assert_eq!(s.sql_path, "@file.property_detail_view.sql");
                assert_eq!(s.scope_lines, vec!["org = ctx.actor.org_id"]);
                assert!(s.params.is_empty());
            }
            other => panic!("expected query.view, got {other:?}"),
        }
    }
}
