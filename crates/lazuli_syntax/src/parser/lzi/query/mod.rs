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
mod compose;
mod list;
mod lookup;
mod sql;

use super::super::common::{SourceLine, line_error};
use super::super::error::ParseError;

use crate::ast::QueryDecl;

use compose::parse_query_compose_decl;
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
    if let Some(rest) = trimmed.strip_prefix("query.compose ") {
        return parse_query_compose_decl(lines, start, rest);
    }
    Err(line_error(
        header,
        "query header must be `query.list <name>`, `query.lookup <name> by ...`, `query.sql <name>`, `query.view <name>`, or `query.compose <name>`",
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

    // =========================================================================
    // query.compose — composite-read primitive (W1).
    // `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §3.1.
    // =========================================================================

    #[test]
    fn query_compose_full_block_parses_into_typed_ast() {
        use crate::{
            ComposeAggFnDecl, ComposeProjectionSourceDecl, ComposeSubselectKindDecl,
            ComposeSubselectPredOp,
        };
        let source = r#"
feature trust
  domain
    query.compose my_pending_reviews_as_traveler
      from ServiceTransaction
      join service_transaction.property as p
      join h.user as host_user optional

      subselect already_reviewed = exists Review
        related_by review.transaction
        where author = ctx.user.id AND subject_kind = "traveler_to_host"
        negate
      subselect revenue_cents = aggregate sum total_amount_cents of ServiceTransaction
        related_by service_transaction.property
        filter status in [paid, completed]
      subselect last_msg = latest body of ChatMessage
        related_by chat_message.chat
        order created_at desc

      select
        transaction_id = self.id
        property_name  = p.name
        host_name      = host_user.name
        already_reviewed = already_reviewed

      filters
        status = "completed"
      key self.id = params.tx_id
      order completed_at desc
      paginate 50
      policy @policy.read
      returns PendingReviewRow
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features[0].queries.len(), 1);
        let q = match &features[0].queries[0] {
            crate::QueryDecl::Compose(q) => q,
            other => panic!("expected query.compose, got {other:?}"),
        };
        assert_eq!(q.name, "my_pending_reviews_as_traveler");
        assert_eq!(q.root, "ServiceTransaction");
        assert_eq!(q.policy.as_deref(), Some("@policy.read"));
        assert_eq!(q.returns.as_deref(), Some("PendingReviewRow"));
        assert_eq!(q.paginate, Some(50));
        assert_eq!(q.key.as_deref(), Some("self.id = params.tx_id"));
        assert_eq!(q.order, vec!["completed_at desc"]);
        assert_eq!(q.filters, vec!["status = \"completed\""]);

        // joins: 2, with alias + optional flags.
        assert_eq!(q.joins.len(), 2);
        assert_eq!(q.joins[0].path, "service_transaction.property");
        assert_eq!(q.joins[0].alias.as_deref(), Some("p"));
        assert!(!q.joins[0].nullable);
        assert_eq!(q.joins[1].path, "h.user");
        assert_eq!(q.joins[1].alias.as_deref(), Some("host_user"));
        assert!(q.joins[1].nullable);

        // projections: 4, three source kinds present.
        assert_eq!(q.projections.len(), 4);
        assert_eq!(q.projections[0].name, "transaction_id");
        assert!(matches!(
            &q.projections[0].source,
            ComposeProjectionSourceDecl::SelfColumn(c) if c == "id"
        ));
        assert!(matches!(
            &q.projections[1].source,
            ComposeProjectionSourceDecl::Joined { alias, column } if alias == "p" && column == "name"
        ));
        assert!(matches!(
            &q.projections[3].source,
            ComposeProjectionSourceDecl::Subselect(s) if s == "already_reviewed"
        ));

        // subselects: closed catalog (exists+negate, aggregate sum+filter+in, latest).
        assert_eq!(q.subselects.len(), 3);
        let s0 = &q.subselects[0];
        assert_eq!(s0.name, "already_reviewed");
        assert!(matches!(
            &s0.kind,
            ComposeSubselectKindDecl::Exists { resource } if resource == "Review"
        ));
        assert!(s0.negate);
        assert_eq!(s0.related_by.as_deref(), Some("review.transaction"));
        assert_eq!(s0.where_pred.len(), 2);
        assert_eq!(s0.where_pred[0].left, "author");
        assert!(matches!(&s0.where_pred[0].op, ComposeSubselectPredOp::Eq(r) if r == "ctx.user.id"));

        let s1 = &q.subselects[1];
        assert!(matches!(
            &s1.kind,
            ComposeSubselectKindDecl::Aggregate { func, column, resource }
                if *func == ComposeAggFnDecl::Sum && column == "total_amount_cents" && resource == "ServiceTransaction"
        ));
        // The `in [paid, completed]` literal-set form parses into an `In` op.
        assert_eq!(s1.filter_pred.len(), 1);
        assert!(matches!(
            &s1.filter_pred[0].op,
            ComposeSubselectPredOp::In(items) if items == &vec!["paid".to_string(), "completed".to_string()]
        ));

        let s2 = &q.subselects[2];
        assert!(matches!(
            &s2.kind,
            ComposeSubselectKindDecl::Latest { column, resource }
                if column == "body" && resource == "ChatMessage"
        ));
        assert_eq!(s2.order, vec!["created_at desc"]);
    }

    #[test]
    fn query_compose_minimal_self_only_parses() {
        let source = r#"
feature catalog
  domain
    query.compose only_self
      from Property
      select
        property_id = self.id
      policy @policy.read
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let q = match &features[0].queries[0] {
            crate::QueryDecl::Compose(q) => q,
            other => panic!("expected query.compose, got {other:?}"),
        };
        assert_eq!(q.root, "Property");
        assert_eq!(q.projections.len(), 1);
        assert!(q.joins.is_empty());
        assert!(q.subselects.is_empty());
        assert!(q.key.is_none());
    }

    // ── Negative tests: the §3.1 closure levers the PARSER enforces ──

    #[test]
    fn query_compose_rejects_in_subselect_rhs() {
        // `in ( subselect )` is the correlated-subquery backdoor — rejected.
        let source = r#"
feature trust
  domain
    query.compose bad
      from Review
      subselect x = exists Review
        related_by review.transaction
        where author in (select id from users)
      select
        id = self.id
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("literal-set") || message.contains("in (subselect)"),
            "expected `in (subselect)` rejection, got: {message}"
        );
    }

    #[test]
    fn query_compose_rejects_in_params_rhs() {
        // `in params.x` is the dynamic-set backdoor — rejected.
        let source = r#"
feature trust
  domain
    query.compose bad
      from Review
      subselect x = count Review
        related_by review.transaction
        where status in params.allowed
      select
        id = self.id
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("literal-set") || message.contains("params"),
            "expected `in params.x` rejection, got: {message}"
        );
    }

    #[test]
    fn query_compose_rejects_unknown_subselect_kind() {
        // Only count/exists/latest/aggregate are admitted.
        let source = r#"
feature trust
  domain
    query.compose bad
      from Review
      subselect x = histogram Review
        related_by review.transaction
      select
        id = self.id
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("closed catalog"), "got: {message}");
    }

    #[test]
    fn query_compose_rejects_unknown_aggregate_fn() {
        // Only sum/avg/min/max/count_distinct are admitted.
        let source = r#"
feature trust
  domain
    query.compose bad
      from Review
      subselect x = aggregate median rating of Review
        related_by review.transaction
      select
        id = self.id
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("sum"), "got: {message}");
    }

    #[test]
    fn query_compose_requires_from_root() {
        let source = r#"
feature trust
  domain
    query.compose bad
      select
        id = self.id
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(format!("{err}").contains("from"), "expected `from` requirement");
    }

    #[test]
    fn query_compose_requires_select() {
        let source = r#"
feature trust
  domain
    query.compose bad
      from Review
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            format!("{err}").contains("select"),
            "expected `select` requirement"
        );
    }

    #[test]
    fn query_compose_rejects_second_from_root() {
        let source = r#"
feature trust
  domain
    query.compose bad
      from Review
      from Transaction
      select
        id = self.id
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            format!("{err}").contains("exactly one"),
            "expected single-root rejection"
        );
    }
}
