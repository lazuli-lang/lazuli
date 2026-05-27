    use lazuli_ir as ir;
    use crate::{ConventionSynthDiagnostic, build_owner_scope_cte_prefix_for_test, build_owner_scope_where_for_test, synthesize_conventions};
    use super::{empty_feature, host_resource, property_resource_with_axis, req_field, req_unique_field, user_qn, fk_field_with_axis};

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
            derived_from: None,
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
