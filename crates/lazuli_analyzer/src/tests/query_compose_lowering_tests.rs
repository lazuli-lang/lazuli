    //! `query.compose` W2 — analyzer-lowering tests.
    //!
    //! Exercises the four W2 obligations from
    //! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §5:
    //!
    //!   1. FK-path resolution — `join <fk.path>` resolves against the
    //!      in-feature relation graph; an unresolved first segment is a
    //!      clean `AnalyzeError`, not a panic (W3 `COMPOSE-JOIN-PATH-001`).
    //!   2. Projection-source resolution — `self.col` / `<alias>.col` /
    //!      `<subselect>` lower to the closed `ProjectionSource`; undeclared
    //!      alias / subselect is a clean error (W3 `COMPOSE-PROJECTION-SOURCE-001`).
    //!   3. Subselect resolution — closed kind + resource `TypeRef` +
    //!      `related_by` Fk-path + lowered `where`/`filter`; missing
    //!      `related_by` is a clean error (W3 `COMPOSE-SUBSELECT-RELATION-001`).
    //!   4. Scope inheritance — the load-bearing property. Tenant +
    //!      soft-delete scope is INHERITED (generated), recorded via
    //!      `scope_origin: Inherited`, never author-supplied. `scope override`
    //!      flips it to `Overridden`.
    //!
    //! The 4-space outer indent is the analyzer test-tree convention (a
    //! co-located module body under `#[cfg(test)] mod tests;`), preserved so
    //! raw-string `.lzi` fixtures keep their structural indentation.

    use lazuli_ir as ir;
    use lazuli_syntax::parse_feature_skeletons;

    use crate::{AnalyzeError, lower_feature_skeleton};

    /// A self-contained messaging feature whose `query.compose chat_inbox`
    /// mirrors proposal §4.1: a root `Chat` (org-tenant), an `optional` FK
    /// join to the counterpart, a `count` + `latest` subselect over
    /// `ChatMessage`, and a `self`/`alias`/`subselect` projection mix. Every
    /// FK is declared in-feature so resolution actually resolves.
    fn chat_inbox_source() -> &'static str {
        r#"
feature messaging
  domain
    resource Chat
      org: Org required
      counterpart: User
      deleted_at: DateTime optional
    resource ChatMessage
      chat: Chat required
      body: Text required
      created_at: DateTime required
      read_at: DateTime optional
      sender: User required
    record ChatInboxRow
      chat_id: ID
      counterpart_name: Text
      last_message_preview: Text optional
      unread_count: Integer

    query.compose chat_inbox
      from Chat
      join chat.counterpart as cp optional
      subselect last_message_preview = latest body of ChatMessage
        related_by chat_message.chat
        order created_at desc
      subselect unread_count = count ChatMessage
        related_by chat_message.chat
        where read_at = nil AND sender != ctx.user.id
      select
        chat_id = self.id
        counterpart_name = cp.name
        last_message_preview = last_message_preview
        unread_count = unread_count
      scope
        participants has ctx.user.id
      order last_message_at desc
      policy @policy.read
      returns ChatInboxRow
"#
    }

    /// Lower the single feature in `source` and pull out its first
    /// `query.compose` as a resolved `ir::ComposeQuery`.
    fn lower_first_compose(source: &str) -> ir::ComposeQuery {
        let features = parse_feature_skeletons(source).expect("feature parses");
        assert_eq!(features.len(), 1, "fixture declares exactly one feature");
        let feature = lower_feature_skeleton(&features[0]).expect("feature lowers");
        feature
            .queries
            .into_iter()
            .find_map(|q| match q {
                ir::Query::Compose(c) => Some(c),
                _ => None,
            })
            .expect("feature declares a query.compose")
    }

    fn user_defined(name: &str) -> ir::TypeRef {
        ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        })
    }

    // =========================================================================
    // 1. FK-path resolution
    // =========================================================================

    #[test]
    fn lowers_root_and_resolved_join_fk_path() {
        let compose = lower_first_compose(chat_inbox_source());

        // `from Chat` lifts to a UserDefined root.
        assert_eq!(compose.root, user_defined("Chat"));

        // The single `join chat.counterpart as cp optional` resolved its
        // FK path (against Chat.counterpart, a declared FK field), defaulted
        // nothing (alias authored), and recorded LEFT-JOIN nullability.
        assert_eq!(compose.joins.len(), 1);
        let join = &compose.joins[0];
        assert_eq!(join.path.segments, vec!["chat", "counterpart"]);
        assert_eq!(join.alias, "cp");
        assert!(join.nullable, "`optional` ⇒ LEFT JOIN ⇒ nullable");
    }

    #[test]
    fn join_alias_defaults_to_last_path_segment_when_omitted() {
        // No `as <alias>` on the join → alias defaults to the last FK-path
        // segment (`counterpart`), so projections can still reference it.
        let source = r#"
feature messaging
  domain
    resource Chat
      org: Org required
      counterpart: User
    query.compose chat_inbox
      from Chat
      join chat.counterpart
      select
        counterpart_name = counterpart.name
"#;
        let compose = lower_first_compose(source);
        assert_eq!(compose.joins[0].alias, "counterpart");
        assert!(!compose.joins[0].nullable, "INNER JOIN by default");
        // And the projection resolved against the defaulted alias.
        assert!(matches!(
            &compose.projections[0].source,
            ir::ProjectionSource::Joined(alias, col) if alias == "counterpart" && col == "name"
        ));
    }

    #[test]
    fn unresolved_join_fk_path_is_a_clean_error_not_a_panic() {
        // `join chat.bogus` — `bogus` is not a declared FK relation on the
        // in-feature root `Chat`. W2 must surface a clean
        // COMPOSE-JOIN-PATH-001 carrier (W3's data), not panic.
        let source = r#"
feature messaging
  domain
    resource Chat
      org: Org required
      counterpart: User
    query.compose chat_inbox
      from Chat
      join chat.bogus as b
      select
        chat_id = self.id
"#;
        let features = parse_feature_skeletons(source).expect("feature parses");
        let err = lower_feature_skeleton(&features[0]).expect_err("join must not resolve");
        match err {
            AnalyzeError::ComposeJoinPathUnresolved {
                query,
                path,
                segment,
                on_resource,
            } => {
                assert_eq!(query, "chat_inbox");
                assert_eq!(path, "chat.bogus");
                assert_eq!(segment, "bogus");
                assert_eq!(on_resource, "Chat");
            }
            other => panic!("expected ComposeJoinPathUnresolved, got {other:?}"),
        }
    }

    #[test]
    fn cross_feature_root_join_is_trusted_not_rejected() {
        // When the root resource is NOT resolvable in-feature (cross-feature
        // or undeclared), the analyzer trusts the join path — full
        // resolution is deferred to doctor (Module context). This mirrors
        // the owner-scope synth's documented cross-feature deferral.
        let source = r#"
feature messaging
  domain
    query.compose chat_inbox
      from Chat
      join chat.counterpart as cp
      select
        chat_id = self.id
"#;
        let compose = lower_first_compose(source);
        // Path still recorded verbatim; no rejection.
        assert_eq!(compose.joins[0].path.segments, vec!["chat", "counterpart"]);
        assert_eq!(compose.root, user_defined("Chat"));
    }

    // =========================================================================
    // 2. Projection-source resolution
    // =========================================================================

    #[test]
    fn lowers_all_three_projection_source_kinds() {
        let compose = lower_first_compose(chat_inbox_source());
        let by_name = |n: &str| {
            compose
                .projections
                .iter()
                .find(|p| p.name == n)
                .unwrap_or_else(|| panic!("projection {n} present"))
                .source
                .clone()
        };
        // self.id → SelfCol
        assert_eq!(by_name("chat_id"), ir::ProjectionSource::SelfCol("id".into()));
        // cp.name → Joined(alias, col) against the declared join alias
        assert_eq!(
            by_name("counterpart_name"),
            ir::ProjectionSource::Joined("cp".into(), "name".into())
        );
        // bare `unread_count` → Subselect against a declared subselect
        assert_eq!(
            by_name("unread_count"),
            ir::ProjectionSource::Subselect("unread_count".into())
        );
    }

    #[test]
    fn projection_against_undeclared_alias_is_a_clean_error() {
        // `nope.name` — `nope` is not a declared join alias.
        let source = r#"
feature messaging
  domain
    resource Chat
      org: Org required
      counterpart: User
    query.compose chat_inbox
      from Chat
      join chat.counterpart as cp
      select
        bad = nope.name
"#;
        let features = parse_feature_skeletons(source).expect("feature parses");
        let err = lower_feature_skeleton(&features[0]).expect_err("alias must not resolve");
        match err {
            AnalyzeError::ComposeProjectionSourceUnresolved {
                query,
                field,
                source_text,
                kind,
            } => {
                assert_eq!(query, "chat_inbox");
                assert_eq!(field, "bad");
                assert_eq!(source_text, "nope.name");
                assert_eq!(kind, "join alias");
            }
            other => panic!("expected ComposeProjectionSourceUnresolved, got {other:?}"),
        }
    }

    #[test]
    fn projection_against_undeclared_subselect_is_a_clean_error() {
        // bare `ghost_count` references no declared subselect.
        let source = r#"
feature messaging
  domain
    resource Chat
      org: Org required
    query.compose chat_inbox
      from Chat
      select
        bad = ghost_count
"#;
        let features = parse_feature_skeletons(source).expect("feature parses");
        let err = lower_feature_skeleton(&features[0]).expect_err("subselect must not resolve");
        match err {
            AnalyzeError::ComposeProjectionSourceUnresolved {
                field,
                source_text,
                kind,
                ..
            } => {
                assert_eq!(field, "bad");
                assert_eq!(source_text, "ghost_count");
                assert_eq!(kind, "subselect");
            }
            other => panic!("expected ComposeProjectionSourceUnresolved, got {other:?}"),
        }
    }

    // =========================================================================
    // 3. Subselect resolution (kind + resource + related_by + predicates)
    // =========================================================================

    #[test]
    fn lowers_subselect_kinds_resources_and_correlation() {
        let compose = lower_first_compose(chat_inbox_source());
        assert_eq!(compose.subselects.len(), 2);

        let latest = compose
            .subselects
            .iter()
            .find(|s| s.name == "last_message_preview")
            .expect("latest subselect present");
        match &latest.kind {
            ir::SubselectKind::Latest { column, resource } => {
                assert_eq!(column, "body");
                assert_eq!(resource, &user_defined("ChatMessage"));
            }
            other => panic!("expected Latest kind, got {other:?}"),
        }
        // related_by resolved to the FK path; order lowered for `latest`.
        assert_eq!(
            latest.related_by.segments,
            vec!["chat_message", "chat"]
        );
        assert_eq!(latest.order.len(), 1);
        assert_eq!(latest.order[0].field, "created_at");
        assert!(matches!(latest.order[0].direction, ir::OrderDir::Desc));

        let count = compose
            .subselects
            .iter()
            .find(|s| s.name == "unread_count")
            .expect("count subselect present");
        assert!(matches!(&count.kind, ir::SubselectKind::Count(r) if r == &user_defined("ChatMessage")));

        // The `where read_at = nil AND sender != ctx.user.id` anti-leak
        // predicate is on the surface and folded into a closed AND tree.
        assert_eq!(count.where_pred.len(), 1, "AND-folded into one predicate");
        let ir::Predicate::And(arms) = &count.where_pred[0] else {
            panic!("expected AND-folded where predicate, got {:?}", count.where_pred[0]);
        };
        assert_eq!(arms.len(), 2);
        // Second arm carries the load-bearing `sender != ctx.user.id` clause —
        // a model can't drop it without it being structurally visible.
        assert!(matches!(
            &arms[1],
            ir::Predicate::Comparison { op: ir::CompareOp::Ne, .. }
        ));
    }

    #[test]
    fn aggregate_filter_with_literal_set_lowers_to_or_of_equalities() {
        // Proposal §4.3 — `aggregate sum ... filter status in [paid, completed]`.
        // The literal-set `in [...]` (the only set form) lowers to an OR of
        // equalities over the closed predicate sublanguage.
        let source = r#"
feature catalog
  domain
    resource Property
      org: Org required
    resource ServiceTransaction
      property: Property required
      total_amount_cents: Integer required
      status: Text required
    query.compose property_kpis
      from Property
      subselect revenue_cents = aggregate sum total_amount_cents of ServiceTransaction
        related_by service_transaction.property
        filter status in [paid, completed]
      select
        property_id = self.id
        revenue_cents = revenue_cents
"#;
        let compose = lower_first_compose(source);
        let agg = &compose.subselects[0];
        match &agg.kind {
            ir::SubselectKind::Aggregate { func, column, resource } => {
                assert!(matches!(func, ir::AggFn::Sum));
                assert_eq!(column, "total_amount_cents");
                assert_eq!(resource, &user_defined("ServiceTransaction"));
            }
            other => panic!("expected Aggregate, got {other:?}"),
        }
        assert_eq!(agg.filter_pred.len(), 1);
        let ir::Predicate::Or(arms) = &agg.filter_pred[0] else {
            panic!("expected OR-of-equalities for literal set, got {:?}", agg.filter_pred[0]);
        };
        assert_eq!(arms.len(), 2, "two literal-set members → two equality arms");
    }

    #[test]
    fn subselect_missing_related_by_is_a_clean_error() {
        // A `count` subselect with no `related_by` cannot be correlated —
        // W2 rejects cleanly (W3 COMPOSE-SUBSELECT-RELATION-001 data).
        let source = r#"
feature messaging
  domain
    resource Chat
      org: Org required
    resource ChatMessage
      chat: Chat required
    query.compose chat_inbox
      from Chat
      subselect unread_count = count ChatMessage
        where read_at = nil
      select
        chat_id = self.id
        unread_count = unread_count
"#;
        let features = parse_feature_skeletons(source).expect("feature parses");
        let err = lower_feature_skeleton(&features[0]).expect_err("missing related_by must error");
        match err {
            AnalyzeError::ComposeSubselectMissingRelation { query, name } => {
                assert_eq!(query, "chat_inbox");
                assert_eq!(name, "unread_count");
            }
            other => panic!("expected ComposeSubselectMissingRelation, got {other:?}"),
        }
    }

    #[test]
    fn negate_on_exists_lowers_to_anti_join_flag() {
        // Proposal §4.2 — `subselect already_reviewed = exists Review ... negate`
        // is the S3 anti-join. `negate` must set the IR Exists.negate flag.
        let source = r#"
feature trust
  domain
    resource ServiceTransaction
      org: Org required
    resource Review
      transaction: ServiceTransaction required
      author: User required
    query.compose my_pending_reviews
      from ServiceTransaction
      subselect already_reviewed = exists Review
        related_by review.transaction
        where author = ctx.user.id
        negate
      select
        transaction_id = self.id
        already_reviewed = already_reviewed
"#;
        let compose = lower_first_compose(source);
        let sub = &compose.subselects[0];
        match &sub.kind {
            ir::SubselectKind::Exists { resource, negate } => {
                assert_eq!(resource, &user_defined("Review"));
                assert!(*negate, "`negate` ⇒ NOT EXISTS anti-join");
            }
            other => panic!("expected Exists, got {other:?}"),
        }
    }

    // =========================================================================
    // 4. Scope inheritance — the verifiability-critical property
    // =========================================================================

    #[test]
    fn scope_is_inherited_by_default_not_author_supplied() {
        // THE load-bearing assertion: an author who writes NO tenant
        // predicate still gets `scope_origin: Inherited` (codegen GENERATES
        // the tenant + soft-delete predicate from `effective_tenancy`). The
        // author's `scope participants has ctx.user.id` is a LOCAL safety
        // predicate layered ON TOP — it is NOT the tenant scope.
        let compose = lower_first_compose(chat_inbox_source());

        assert_eq!(
            compose.scope_origin,
            ir::ComposeScopeOrigin::Inherited,
            "tenant/soft-delete scope must be inherited (generated), never dropped"
        );
        assert!(
            !compose.scope_override,
            "default compose does not override inherited scope"
        );
        // The author's local safety predicate is present and DISTINCT from
        // the (generated, not-in-IR) tenant predicate — no `org_id` literal
        // was lowered from source.
        assert_eq!(compose.scope.len(), 1, "one local safety predicate");
        let lowered = format!("{:?}", compose.scope);
        assert!(
            !lowered.contains("org_id"),
            "tenant predicate must NOT be author-supplied in the local scope: {lowered}"
        );
    }

    #[test]
    fn scope_override_flips_origin_to_overridden() {
        // `scope override` (with reason) is the explicit, doctor-gated
        // opt-out. It flips `scope_origin` to `Overridden` and sets the flag.
        let source = r#"
feature messaging
  domain
    resource Chat
      org: Org required
    query.compose all_chats
      from Chat
      select
        chat_id = self.id
      scope override
        reason "admin cross-tenant audit console"
      policy @policy.admin
"#;
        let compose = lower_first_compose(source);
        assert!(compose.scope_override, "scope override flag set");
        assert_eq!(
            compose.scope_origin,
            ir::ComposeScopeOrigin::Overridden,
            "explicit override is recorded as Overridden origin"
        );
    }

    // =========================================================================
    // Generated record + key (single-row) contract
    // =========================================================================

    #[test]
    fn returns_record_defaults_to_pascal_row_when_omitted() {
        // No `returns` → generated record name `<PascalName>Row`.
        let source = r#"
feature messaging
  domain
    resource Chat
      org: Org required
    query.compose chat_inbox
      from Chat
      select
        chat_id = self.id
"#;
        let compose = lower_first_compose(source);
        assert_eq!(compose.returns, user_defined("ChatInboxRow"));
    }

    #[test]
    fn key_clause_makes_a_single_row_read() {
        // §3.2 #6 — `key self.id = params.property_id` ⇒ single-row read.
        let source = r#"
feature catalog
  domain
    resource Property
      org: Org required
    query.compose property_kpis
      from Property
      params
        property_id: ID required
      select
        property_id = self.id
      key self.id = params.property_id
"#;
        let compose = lower_first_compose(source);
        let key = compose.key.expect("key clause lowered");
        assert_eq!(key.path.segments, vec!["self", "id"]);
        assert_eq!(
            key.equals,
            ir::Expr::Path(ir::Path::from_segments(["params", "property_id"]))
        );
    }
