
    use lazuli_ir::{
        Auth, AuthIdentity, AuthSessions, BuiltinType, Defaults, EnumLiteral, Feature, Field,
        FieldConstraints, FieldRef, ListQuery, Path as IrPath, Policies, PolicyRef, Predicate,
        QualifiedName, Query, Resource, SpanRef, TypeRef,
    };

    use super::*;

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields: vec![
                mk_field("id", TypeRef::Builtin(BuiltinType::Id)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    /// `expires_at <op> ctx.now`.
    fn temporal_filter(op: CompareOp) -> Filter {
        Filter {
            predicate: Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["expires_at"])),
                op,
                right: Expr::Path(IrPath::from_segments(["ctx", "now"])),
            },
            when: None,
        }
    }

    /// A non-temporal equality filter (`customer.id = params.customer_id`).
    fn owner_filter() -> Filter {
        Filter {
            predicate: Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["customer", "id"])),
                op: CompareOp::Eq,
                right: Expr::Path(IrPath::from_segments(["params", "customer_id"])),
            },
            when: None,
        }
    }

    fn list_query(name: &str, modifier: Option<&str>, filters: Vec<Filter>) -> Query {
        Query::List(ListQuery {
            name: name.to_owned(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters,
            order: vec![],
            paginate: None,
            modifier: modifier.map(str::to_owned),
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: Some(SpanRef { start: 42, end: 99 }),
            owner_scope_sql: None,
        })
    }

    fn feature_with(
        session_resource: &str,
        resources: Vec<Resource>,
        queries: Vec<Query>,
    ) -> Feature {
        Feature {
            name: "customer_auth".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries,
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            auth: Some(Auth {
                identity: AuthIdentity {
                    field: FieldRef {
                        resource: qn(session_resource),
                        field: "email".to_owned(),
                    },
                    public_contract: None,
                },
                password: None,
                sessions: Some(AuthSessions {
                    resource: qn(session_resource),
                    ttl: "7 days".to_owned(),
                    refresh: false,
                    extra_columns: vec![],
                    access_ttl: None,
                    rotation: None,
                    cookie: None,
                }),
                mfa: None,
                oauth: vec![],
                span_ref: None,
            }),
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_fires_when_session_query_lacks_temporal_bound() {
        // Ported from the LSP negative case: a session-listing query
        // under a NON-`active_sessions` name with no temporal filter.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query(
                "live_tokens",
                Some("@query_modifier.active_session_scope"),
                vec![owner_filter()],
            )],
        );
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert_eq!(Finding::CODE, "session_query_temporal_validity_001");
        assert_eq!(Finding::KEBAB_CODE, "session-query-temporal-validity");
        assert_eq!(findings[0].query, "live_tokens");
        assert_eq!(findings[0].session_resource, "UserSession");
        assert_eq!(findings[0].offset, Some(42));
        assert!(findings[0].message().contains("live_tokens"));
        assert!(findings[0].message().contains("UserSession"));
        assert!(findings[0].message().contains("expires_at > ctx.now"));
    }

    #[test]
    fn positive_fires_when_only_modifier_present() {
        // A modifier alone is NOT sufficient evidence (canonical
        // semantics §"Active sessions"); the IR carries no `guarantees`
        // contract on the opaque modifier name.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query(
                "active_sessions",
                Some("@query_modifier.active_session_scope"),
                vec![],
            )],
        );
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "modifier alone must still fire: {findings:?}"
        );
    }

    #[test]
    fn negative_clean_with_explicit_gt_filter() {
        // Mirrors examples/production-grade/features/auth/auth.lzi:74 —
        // explicit `expires_at > ctx.now`, no modifier.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query(
                "active_sessions",
                None,
                vec![owner_filter(), temporal_filter(CompareOp::Gt)],
            )],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_clean_with_ge_filter() {
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query(
                "active_sessions",
                None,
                vec![temporal_filter(CompareOp::Ge)],
            )],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_clean_with_modifier_and_filter() {
        // Mirrors examples/user-auth.lzi:58 + full-capsule.lzi:585 —
        // modifier present AND explicit filter present.
        let feature = feature_with(
            "Session",
            vec![mk_resource("Session")],
            vec![list_query(
                "active_sessions",
                Some("@query_modifier.active_session_scope"),
                vec![owner_filter(), temporal_filter(CompareOp::Gt)],
            )],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_temporal_bound_inside_and_conjunction() {
        let and = Filter {
            predicate: Predicate::And(vec![
                owner_filter().predicate,
                temporal_filter(CompareOp::Gt).predicate,
            ]),
            when: None,
        };
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![and])],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn positive_or_branch_does_not_prove_validity() {
        // An `Or` can take the branch that omits the bound, so it does
        // not guarantee every row is unexpired.
        let or = Filter {
            predicate: Predicate::Or(vec![
                owner_filter().predicate,
                temporal_filter(CompareOp::Gt).predicate,
            ]),
            when: None,
        };
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![or])],
        );
        assert_eq!(check(&feature, Path::new("auth.lzi")).len(), 1);
    }

    #[test]
    fn negative_no_sessions_block_does_not_fire() {
        let mut feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![])],
        );
        // Strip the sessions binding — no session axis to enforce.
        if let Some(auth) = feature.auth.as_mut() {
            auth.sessions = None;
        }
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_no_auth_block_does_not_fire() {
        let mut feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![])],
        );
        feature.auth = None;
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_session_resource_not_declared_locally_does_not_fire() {
        // auth_sessions_resource_unknown_001 owns the "binding names a
        // missing resource" case; this rule stays silent so the two do
        // not double-fire.
        let feature = feature_with(
            "MissingSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![])],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_non_session_query_is_out_of_scope() {
        // A list query that scores onto a NON-session resource is not
        // checked. Multi-resource feature so the scorer is active.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession"), mk_resource("AuditLog")],
            vec![list_query("audit_logs", None, vec![])],
        );
        assert!(
            check(&feature, Path::new("auth.lzi")).is_empty(),
            "audit_logs targets AuditLog, not the session resource"
        );
    }

    #[test]
    fn positive_name_agnostic_multi_resource_scores_session() {
        // Multi-resource feature; a non-`active_sessions` query name that
        // scores onto the session resource still fires.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession"), mk_resource("AuditLog")],
            vec![list_query("user_sessions", None, vec![owner_filter()])],
        );
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert_eq!(findings[0].query, "user_sessions");
    }

    #[test]
    fn edge_enum_rhs_is_not_ctx_now() {
        // Guard against a false negative: `expires_at > Status.active`
        // (nonsense, but exercises the RHS gate) must not count.
        let weird = Filter {
            predicate: Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["expires_at"])),
                op: CompareOp::Gt,
                right: Expr::Enum(EnumLiteral {
                    type_name: None,
                    variant: "now".to_owned(),
                }),
            },
            when: None,
        };
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![weird])],
        );
        assert_eq!(check(&feature, Path::new("auth.lzi")).len(), 1);
    }
