    use lazuli_ir as ir;
    use crate::synthesize_conventions;
    use super::{empty_feature, host_resource, property_resource_with_axis, req_field, req_unique_field, user_qn, fk_field_with_axis};

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
            polymorphic_refs: Vec::new(),
            append_only: false,
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
