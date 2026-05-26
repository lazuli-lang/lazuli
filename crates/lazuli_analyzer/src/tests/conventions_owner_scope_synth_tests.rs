    use crate::{
        ConventionSynthDiagnostic, build_owner_scope_cte_prefix_for_test,
        build_owner_scope_where_for_test, synthesize_conventions,
    };
    use lazuli_ir as ir;

    fn empty_feature(name: &str) -> ir::Feature {
        ir::Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: ir::Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: ir::Policies {
                categories: vec![ir::PolicyCategory {
                    name: "authenticated".to_owned(),
                    atoms: vec!["@scope.authenticated".to_owned()],
                    previous_names: Vec::new(),
                    when_denied: None,
                    when_denied_route: None,
                }],
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn req_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
        ir::Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn req_unique_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
        ir::Field {
            unique: true,
            ..req_field(name, type_ref)
        }
    }

    /// Build an FK field annotated with `@owner_axis(through: <col>)`.
    fn fk_field_with_axis(name: &str, target: &str, through: &str) -> ir::Field {
        let mut f = req_field(
            name,
            ir::TypeRef::UserDefined(ir::QualifiedName {
                feature: None,
                name: target.to_owned(),
            }),
        );
        f.owner_axis = Some(ir::OwnerAxis {
            through_column: through.to_owned(),
        });
        f
    }

    fn user_qn(name: &str) -> ir::TypeRef {
        ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        })
    }

    /// Build the Hostpoint pilot's `Host` resource (the FK target with
    /// the `user: User required unique` actor key). Used to back the
    /// owner-chain in fixtures.
    fn host_resource() -> ir::Resource {
        ir::Resource {
            name: "Host".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
        }
    }

    /// Build the trigger pilot's `Property` resource — owner-scoped via
    /// `host: Host required @owner_axis(through: user)`.
    fn property_resource_with_axis() -> ir::Resource {
        ir::Resource {
            name: "Property".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                fk_field_with_axis("host", "Host", "user"),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud],
            lifecycle_routes: None,
        }
    }

    /// §8.1 — owner-scope mode emits a chain WHERE predicate on
    /// `delete_<r>`. The synthesized command carries `owner_scope_sql`
    /// with the `host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)`
    /// fragment — the same shape the trigger pilot's pre-absorption
    /// `delete_property.go` (§1.1) used.
    #[test]
    fn owner_scope_delete_emits_chain_where_predicate() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        feature.resources.push(property_resource_with_axis());

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "owner-scope delete_property should not emit diagnostics, got {:?}",
            diags
        );

        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .expect("synth emits delete_property");
        let scope = delete
            .owner_scope_sql
            .as_ref()
            .expect("delete_property carries owner_scope_sql");
        assert_eq!(scope.field_name, "host");
        assert_eq!(scope.fk_target, "Host");
        assert_eq!(scope.through_column, "user");
        assert_eq!(
            scope.where_predicate,
            r#"host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)"#
        );
        // DELETE doesn't need the CTE prefix — only CREATE does.
        assert!(scope.cte_owner_check.is_none(), "DELETE carries no CTE");
    }

    /// §8.2 / §8.3 / §8.4 — owner-scope mode emits the same WHERE
    /// fragment on UPDATE, LOOKUP, and LIST. Single test asserts all
    /// three because the predicate is composed by the unified
    /// builder; per-shape divergence would surface here.
    #[test]
    fn owner_scope_update_lookup_list_emit_chain_where_predicate() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        feature.resources.push(property_resource_with_axis());

        let _ = synthesize_conventions(&mut feature);

        let expected = r#"host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)"#;

        let update = feature
            .commands
            .iter()
            .find(|c| c.name == "update_property")
            .expect("synth emits update_property");
        assert_eq!(
            update
                .owner_scope_sql
                .as_ref()
                .map(|s| s.where_predicate.as_str()),
            Some(expected)
        );

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_property")
            .expect("synth emits lookup_property");
        let lookup_scope = match lookup {
            ir::Query::Lookup(lq) => lq.owner_scope_sql.as_ref(),
            _ => panic!("expected Lookup variant"),
        };
        assert_eq!(
            lookup_scope.map(|s| s.where_predicate.as_str()),
            Some(expected),
        );

        let list = feature
            .queries
            .iter()
            .find(|q| q.name() == "list_propertys")
            .expect("synth emits list_propertys");
        let list_scope = match list {
            ir::Query::List(lq) => lq.owner_scope_sql.as_ref(),
            _ => panic!("expected List variant"),
        };
        assert_eq!(
            list_scope.map(|s| s.where_predicate.as_str()),
            Some(expected),
        );
    }

    /// §8.5.A — `create_<r>` synth emits the CTE-INSERT prefix in the
    /// `cte_owner_check` slot. RULE-VOCAB-03 affirmation: one SQL
    /// statement (CTE-wrapped INSERT), no procedural sequencing.
    #[test]
    fn owner_scope_create_emits_cte_owner_check_prefix() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        feature.resources.push(property_resource_with_axis());

        let _ = synthesize_conventions(&mut feature);

        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_property")
            .expect("synth emits create_property");
        let scope = create
            .owner_scope_sql
            .as_ref()
            .expect("create_property carries owner_scope_sql");
        let cte = scope
            .cte_owner_check
            .as_ref()
            .expect("create_property carries cte_owner_check prefix");
        assert_eq!(
            cte,
            r#"WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $host AND "user" = ctx.User.ID)"#
        );
    }

    /// §6.1 composition — `[crud, me]` + `@owner_axis` propagates the
    /// chain WHERE to `lookup_my_<r>`. This is the core composability
    /// claim (§5.3 / proposal §6.2): one annotation, all bundles see
    /// it. The fixture uses a `Profile` resource that is NOT user-keyed
    /// (no `user: User required unique`) so the `me` mode falls back to
    /// the owner-axis route via `host`.
    ///
    /// We exercise the lookup_my path with an `org_keyed` me mode (the
    /// `Profile` has `org` but no direct `user` field) — the chain
    /// WHERE adds the ownership filter on top of the actor-keyed
    /// shape, exactly per §6.1's "compose, don't replace" rule.
    #[test]
    fn composition_crud_and_me_with_owner_axis_propagates_chain_to_lookup_my() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        let profile = ir::Resource {
            name: "Profile".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                fk_field_with_axis("host", "Host", "user"),
                req_field("bio", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud, ir::ConventionRef::Me],
            lifecycle_routes: None,
        };
        // Sanity: not user-keyed (no `user: User required unique`).
        profile
            .fields
            .iter()
            .for_each(|f| assert_ne!(f.name, "user"));
        feature.resources.push(profile);

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "composition + @owner_axis should not emit diagnostics, got {:?}",
            diags
        );

        // lookup_my_profile is emitted (me §5.3 OrgKeyed route — Profile
        // has `org`, no `user`). The owner-scope synth ALSO attached its
        // chain predicate.
        let lookup_my = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_profile")
            .expect("composition emits lookup_my_profile");
        let scope = match lookup_my {
            ir::Query::Lookup(lq) => lq
                .owner_scope_sql
                .as_ref()
                .expect("lookup_my_profile carries owner_scope_sql"),
            _ => panic!("expected Lookup variant"),
        };
        assert_eq!(scope.field_name, "host");
        assert_eq!(scope.fk_target, "Host");
        assert_eq!(
            scope.where_predicate,
            r#"host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)"#
        );

        // Plus the 5 crud entries all carry the same scope (spot-check
        // delete_profile to confirm cross-bundle uniformity).
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_profile")
            .expect("composition emits delete_profile");
        assert!(delete.owner_scope_sql.is_some());
    }

    /// §11.1 `owner_axis_unknown_through` — annotation names a column
    /// that doesn't exist on the FK target. Suggestion field is
    /// populated when a nearest match exists.
    #[test]
    fn diagnostic_owner_axis_unknown_through() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        // Property with `@owner_axis(through: usr)` — typo: `usr` not
        // `user`. Nearest-match should suggest `user`.
        let mut property = property_resource_with_axis();
        // Replace the host field's owner_axis with the typo'd column.
        for f in property.fields.iter_mut() {
            if f.name == "host" {
                f.owner_axis = Some(ir::OwnerAxis {
                    through_column: "usr".to_owned(),
                });
            }
        }
        feature.resources.push(property);

        let diags = synthesize_conventions(&mut feature);
        let found = diags.iter().find_map(|d| match d {
            ConventionSynthDiagnostic::OwnerAxisUnknownThrough {
                resource,
                field,
                through,
                fk_target,
                suggestion,
            } if resource == "Property" && field == "host" => {
                Some((through.clone(), fk_target.clone(), suggestion.clone()))
            }
            _ => None,
        });
        let (through, fk_target, suggestion) =
            found.expect("expected OwnerAxisUnknownThrough diagnostic");
        assert_eq!(through, "usr");
        assert_eq!(fk_target, "Host");
        assert_eq!(suggestion, Some("user".to_owned()));

        // Synth fell back to tenant-only — owner_scope_sql NOT attached.
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .expect("synth still emits delete_property");
        assert!(
            delete.owner_scope_sql.is_none(),
            "unresolved @owner_axis must not produce SQL fragments"
        );
    }

    /// §11.1 `owner_axis_through_not_user_keyed` — the resolved
    /// `through:` column on the FK target is not typed as `User`.
    /// Warning severity (proposal §11.1) — chain still emits so author
    /// can hand-correct.
    #[test]
    fn diagnostic_owner_axis_through_not_user_keyed() {
        let mut feature = empty_feature("catalog");

        // Host with a `manager: Text required` (not a User type).
        let mut host = host_resource();
        host.fields = vec![
            req_field("org", user_qn("Org")),
            req_field("manager", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
        ];
        feature.resources.push(host);

        // Property with `@owner_axis(through: manager)` — `manager`
        // exists on Host but is Text-typed, not User-typed.
        let mut property = property_resource_with_axis();
        for f in property.fields.iter_mut() {
            if f.name == "host" {
                f.owner_axis = Some(ir::OwnerAxis {
                    through_column: "manager".to_owned(),
                });
            }
        }
        feature.resources.push(property);

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed {
                    resource,
                    field,
                    through,
                    fk_target,
                } if resource == "Property"
                    && field == "host"
                    && through == "manager"
                    && fk_target == "Host"
            )),
            "expected OwnerAxisThroughNotUserKeyed diagnostic, got {:?}",
            diags
        );

        // Warning, not error — the chain SQL is still emitted so the
        // author can hand-fix the chain.
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .expect("synth still emits delete_property");
        let scope = delete
            .owner_scope_sql
            .as_ref()
            .expect("warning-level diag still attaches scope");
        assert!(scope.where_predicate.contains("manager"));
    }

    /// §11.1 `owner_axis_collides_with_unique_user` — resource has BOTH
    /// `user: User required unique` AND `@owner_axis(through: <col>)`
    /// on another field. Synth surfaces a warning and skips the
    /// owner-axis emission (user-keyed mode already provides
    /// ownership; §11.1 mitigation).
    #[test]
    fn diagnostic_owner_axis_collides_with_unique_user() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        // Property with BOTH `user: User required unique` AND
        // `host: Host required @owner_axis(through: user)`. The two
        // are mutually redundant.
        let property = ir::Resource {
            name: "Property".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                fk_field_with_axis("host", "Host", "user"),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud],
            lifecycle_routes: None,
        };
        feature.resources.push(property);

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser {
                    resource,
                    field,
                } if resource == "Property" && field == "host"
            )),
            "expected OwnerAxisCollidesWithUniqueUser diagnostic, got {:?}",
            diags
        );

        // Owner-axis SQL must NOT be attached — user-keyed mode wins,
        // the existing tenant categorization handles ownership via
        // the `user: User required unique` field.
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .expect("synth still emits delete_property");
        assert!(
            delete.owner_scope_sql.is_none(),
            "user-unique + @owner_axis must not double-restrict"
        );
    }

    /// §9 override semantics — author writes `command delete_<r>` with
    /// their own handler; synth skips just that name, no diagnostic.
    /// The author's command is untouched (no `owner_scope_sql`
    /// attached — the synth doesn't mutate author-written commands).
    #[test]
    fn override_with_handler_skips_synth_and_does_not_attach_scope() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        feature.resources.push(property_resource_with_axis());

        // Author-written `delete_property` — bare canonical shape so
        // the existing signature-match logic passes; the analyzer
        // simply records `AuthorOverride(Crud)` and skips the synth.
        feature.commands.push(ir::Command {
            name: "delete_property".to_owned(),
            public_contract: None,
            kind: ir::CommandKind::Delete,
            route: vec![ir::RouteSlot {
                name: "id".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
                from: None,
                kind: ir::RouteSlotKind::Plain,
            }],
            input: ir::CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::Deletes(ir::DeleteEffect {
                resource: ir::QualifiedName {
                    feature: None,
                    name: "Property".to_owned(),
                },
            }),
            policy: ir::PolicyRef::Local("host_only".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: Some(ir::HandlerRef {
                namespace: "fn".to_owned(),
                name: "delete_property".to_owned(),
                span_ref: None,
            }),
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
            triggers: Vec::new(),
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
        });

        let diags = synthesize_conventions(&mut feature);
        // No diagnostic — override is first-class per §9 / RULE-VOCAB-02.
        assert!(
            !diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::OwnerAxisUnknownThrough { .. }
                    | ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed { .. }
                    | ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser { .. }
                    | ConventionSynthDiagnostic::SignatureMismatch { .. }
            )),
            "override should not emit owner-axis OR signature-mismatch diagnostics, got {:?}",
            diags
        );

        // Exactly one `delete_property` — the author's, with policy
        // `host_only`, handler set, NO `owner_scope_sql`.
        let count = feature
            .commands
            .iter()
            .filter(|c| c.name == "delete_property")
            .count();
        assert_eq!(count, 1, "delete_property must not be duplicated");
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .unwrap();
        assert!(matches!(&delete.policy, ir::PolicyRef::Local(p) if p == "host_only"));
        assert!(delete.handler.is_some(), "author's handler preserved");
        assert!(
            delete.owner_scope_sql.is_none(),
            "synth must not mutate author-written delete_property",
        );
        // §11 — synth_origins records AuthorOverride(Crud).
        assert_eq!(
            feature.synth_origins.get("delete_property"),
            Some(&ir::ConventionOrigin::AuthorOverride(
                ir::ConventionRef::Crud
            )),
        );

        // Other 4 crud entries still synth WITH owner-scope.
        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_property")
            .expect("create still synthesized");
        assert!(create.owner_scope_sql.is_some());
    }

    /// Direct-call builder sanity — `build_owner_scope_where_for_test`
    /// and `build_owner_scope_cte_prefix_for_test` round-trip the SQL.
    /// Anchors the function-level surface in case downstream cells
    /// invoke the builders directly (O3 inspect / LSP hover).
    #[test]
    fn builder_functions_round_trip_canonical_sql() {
        // §7.3 — WHERE predicate shape.
        assert_eq!(
            build_owner_scope_where_for_test("host", "Host", "user"),
            r#"host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)"#,
        );
        // §8.5.A — CTE prefix shape.
        assert_eq!(
            build_owner_scope_cte_prefix_for_test("host", "Host", "user"),
            r#"WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $host AND "user" = ctx.User.ID)"#,
        );
    }
