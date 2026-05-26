    use crate::{CrudSynthDiagnostic, synthesize_conventions};
    use lazuli_ir as ir;

    /// Minimal `Feature` for testing — empty defaults, a single
    /// `authenticated` policy unless the test overrides.
    fn empty_feature(name: &str, with_authenticated: bool) -> ir::Feature {
        let policies = if with_authenticated {
            ir::Policies {
                categories: vec![ir::PolicyCategory {
                    name: "authenticated".to_owned(),
                    atoms: vec!["@scope.authenticated".to_owned()],
                    previous_names: Vec::new(),
                    when_denied: None,
                    when_denied_route: None,
                }],
                fields: Vec::new(),
                span_ref: None,
            }
        } else {
            ir::Policies::default()
        };
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
            policies,
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

    fn user_qn(name: &str) -> ir::TypeRef {
        ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        })
    }

    fn author_list_customers_query(policy: ir::PolicyRef) -> ir::Query {
        let mut query = crate::conventions::build_list_query("list_customers", "Customer");
        match &mut query {
            ir::Query::List(lq) => {
                lq.policy = policy;
            }
            other => panic!("expected list query helper to build List, got {other:?}"),
        }
        query
    }

    fn customer_resource() -> ir::Resource {
        // §8 worked example: feature customer, resource Customer.
        ir::Resource {
            name: "Customer".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                req_unique_field(
                    "email",
                    ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
                ),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
                req_field("status", user_qn("CustomerStatus")),
                req_field(
                    "created_at",
                    ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
                ),
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

    /// §8 worked example — synth produces exactly the 5 entries
    /// (3 commands + 2 queries) with the exact shapes per §5.2–§5.6.
    #[test]
    fn synth_produces_five_entries_for_customer_resource() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "expected no diagnostics for clean Customer, got {:?}",
            diags
        );

        // 3 commands appended: create / update / delete.
        let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            cmd_names,
            vec!["create_customer", "update_customer", "delete_customer"]
        );

        // 2 queries appended: lookup / list.
        let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
        assert_eq!(q_names, vec!["lookup_customer", "list_customers"]);

        // create_customer §5.2 shape — input has [email, name, status]
        // (org + created_at are Tenant/Auto, dropped).
        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_customer")
            .unwrap();
        assert!(matches!(create.kind, ir::CommandKind::Create));
        match &create.input {
            ir::CommandInput::Typed(slots) => {
                let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["email", "name", "status"]);
                // Required-on-resource fields stay required.
                assert!(slots.iter().all(|s| s.required));
            }
            other => panic!("expected Typed input, got {:?}", other),
        }
        match &create.effect {
            ir::CommandEffect::Creates(e) => assert_eq!(e.resource.name, "Customer"),
            other => panic!("expected Creates effect, got {:?}", other),
        }
        let create_rate_limit = create.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(create_rate_limit.default, "100 per 10 minutes per ip");
        assert!(create_rate_limit.by_env.is_empty());
        assert!(matches!(&create.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
        assert!(create.audit.is_some());

        // update_customer §5.3 — every field becomes optional in input,
        // route id: ID present, effect Updates Customer.
        let update = feature
            .commands
            .iter()
            .find(|c| c.name == "update_customer")
            .unwrap();
        assert!(matches!(update.kind, ir::CommandKind::Update));
        assert_eq!(update.route.len(), 1);
        assert_eq!(update.route[0].name, "id");
        assert!(matches!(
            update.route[0].type_ref,
            ir::TypeRef::Builtin(ir::BuiltinType::Id)
        ));
        match &update.input {
            ir::CommandInput::Typed(slots) => {
                let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["email", "name", "status"]);
                // All slots optional per §5.3.
                assert!(slots.iter().all(|s| !s.required));
            }
            other => panic!("expected Typed input, got {:?}", other),
        }
        match &update.effect {
            ir::CommandEffect::Updates(e) => assert_eq!(e.resource.name, "Customer"),
            other => panic!("expected Updates effect, got {:?}", other),
        }

        // delete_customer §5.4 — no input, route id, Deletes effect.
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_customer")
            .unwrap();
        assert!(matches!(delete.kind, ir::CommandKind::Delete));
        assert_eq!(delete.route.len(), 1);
        assert_eq!(delete.route[0].name, "id");
        assert!(matches!(delete.input, ir::CommandInput::Empty));
        match &delete.effect {
            ir::CommandEffect::Deletes(e) => assert_eq!(e.resource.name, "Customer"),
            other => panic!("expected Deletes effect, got {:?}", other),
        }

        // lookup_customer §5.5 — Lookup with key id, policy authenticated.
        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_customer")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["id".to_owned()]);
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }

        // list_customers §5.6 — List with limit+offset params, paginate 50.
        let list = feature
            .queries
            .iter()
            .find(|q| q.name() == "list_customers")
            .unwrap();
        match list {
            ir::Query::List(lq) => {
                let pnames: Vec<&str> = lq.params.iter().map(|p| p.name.as_str()).collect();
                assert_eq!(pnames, vec!["limit", "offset"]);
                assert!(lq.params.iter().all(|p| !p.required));
                assert_eq!(lq.paginate, Some(50));
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
            }
            other => panic!("expected List query, got {:?}", other),
        }
    }

    /// §5.2 / §5.3 binding axis — both the synthesized create_<R> and
    /// update_<R> commands must carry one `<field> = input.<field>`
    /// assignment per input slot, mirroring what the author would have
    /// written by hand. Without these the Go codegen emits an empty
    /// `lazuli.Bindings{}` body and every dispatch tripped the runtime
    /// guard "updates effect requires Bind bindings" (PG 500 at first
    /// call). Regression for the 2026-05-22 hostpoint /settings save
    /// outage; pairs with `create_<R>` having the same gap.
    #[test]
    fn synth_create_and_update_populate_assignments_from_input() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "unexpected synth diagnostics: {diags:?}");

        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_customer")
            .expect("create_customer must synth");
        let create_assignments = match &create.effect {
            ir::CommandEffect::Creates(e) => &e.assignments,
            other => panic!("expected Creates effect, got {:?}", other),
        };
        let create_fields: Vec<&str> = create_assignments
            .iter()
            .map(|a| a.field.as_str())
            .collect();
        assert_eq!(
            create_fields,
            vec!["email", "name", "status"],
            "create assignments must mirror input slots in order"
        );
        for a in create_assignments {
            match &a.value {
                ir::Expr::Path(p) => assert_eq!(
                    p.segments,
                    vec!["input".to_owned(), a.field.clone()],
                    "create assignment value must be `input.<field>`"
                ),
                other => panic!("create assignment value not a Path: {:?}", other),
            }
        }

        let update = feature
            .commands
            .iter()
            .find(|c| c.name == "update_customer")
            .expect("update_customer must synth");
        let update_assignments = match &update.effect {
            ir::CommandEffect::Updates(e) => &e.assignments,
            other => panic!("expected Updates effect, got {:?}", other),
        };
        let update_fields: Vec<&str> = update_assignments
            .iter()
            .map(|a| a.field.as_str())
            .collect();
        assert_eq!(
            update_fields,
            vec!["email", "name", "status"],
            "update assignments must mirror input slots in order"
        );
        for a in update_assignments {
            match &a.value {
                ir::Expr::Path(p) => assert_eq!(
                    p.segments,
                    vec!["input".to_owned(), a.field.clone()],
                    "update assignment value must be `input.<field>`"
                ),
                other => panic!("update assignment value not a Path: {:?}", other),
            }
        }
    }

    /// §9 worked override — author wrote `update_customer`; other 4
    /// still synthesize; no warning emitted.
    #[test]
    fn author_override_skips_just_that_name() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());

        // Author's update_customer: matches canonical input + Updates
        // Customer (so no signature_mismatch diagnostic should fire).
        let author_update = ir::Command {
            name: "update_customer".to_owned(),
            public_contract: None,
            kind: ir::CommandKind::Update,
            route: vec![ir::RouteSlot {
                name: "id".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
                from: None,
                kind: ir::RouteSlotKind::Plain,
            }],
            input: ir::CommandInput::Typed(vec![
                ir::TypedSlot {
                    name: "email".to_owned(),
                    type_ref: ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                    validate_skip: false,
                },
                ir::TypedSlot {
                    name: "name".to_owned(),
                    type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                    validate_skip: false,
                },
                ir::TypedSlot {
                    name: "status".to_owned(),
                    type_ref: ir::TypeRef::UserDefined(ir::QualifiedName {
                        feature: None,
                        name: "CustomerStatus".to_owned(),
                    }),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                    validate_skip: false,
                },
            ]),
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::Updates(ir::UpdateEffect {
                resource: ir::QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                },
                assignments: Vec::new(),
            }),
            policy: ir::PolicyRef::Local("customer_admin".to_owned()),
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
            handler: None,
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
            triggers: Vec::new(),
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
        };
        feature.commands.push(author_update);

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "matching-signature author override should not emit a diagnostic, got {:?}",
            diags
        );

        let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
        assert!(cmd_names.contains(&"create_customer"));
        assert!(cmd_names.contains(&"delete_customer"));
        // update_customer present, but appears exactly once (the author's).
        let update_count = cmd_names
            .iter()
            .filter(|n| **n == "update_customer")
            .count();
        assert_eq!(update_count, 1, "update_customer must not be duplicated");

        // The remaining update_customer is the author's — its policy is
        // `customer_admin`, not `authenticated`.
        let update = feature
            .commands
            .iter()
            .find(|c| c.name == "update_customer")
            .unwrap();
        assert!(matches!(&update.policy, ir::PolicyRef::Local(p) if p == "customer_admin"));

        let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
        assert!(q_names.contains(&"lookup_customer"));
        assert!(q_names.contains(&"list_customers"));
    }

    #[test]
    fn fx1_crud_without_author_query_emits_catalog_queries() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

        let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
        assert_eq!(q_names, vec!["lookup_customer", "list_customers"]);
    }

    #[test]
    fn fx1_crud_author_list_query_silences_synth() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());
        feature
            .queries
            .push(author_list_customers_query(ir::PolicyRef::Local(
                "authenticated".to_owned(),
            )));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

        let list_count = feature
            .queries
            .iter()
            .filter(|q| q.name() == "list_customers")
            .count();
        assert_eq!(
            list_count, 1,
            "author list_customers must not be duplicated"
        );
        assert_eq!(
            feature.synth_origins.get("list_customers"),
            Some(&ir::ConventionOrigin::AuthorOverride(
                ir::ConventionRef::Crud
            ))
        );
    }

    #[test]
    fn fx1_crud_author_list_query_policy_mismatch_warns_and_silences() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());
        feature
            .queries
            .push(author_list_customers_query(ir::PolicyRef::Local(
                "customer_admin".to_owned(),
            )));

        let diags = synthesize_conventions(&mut feature);
        let mismatch = diags
            .iter()
            .find(|d| {
                matches!(
                    d,
                    CrudSynthDiagnostic::SignatureMismatch { resource, synth_name, .. }
                        if resource == "Customer" && synth_name == "list_customers"
                )
            })
            .expect("expected SignatureMismatch for list_customers policy divergence");
        assert_eq!(
            mismatch.diagnostic_code(),
            "@correctness.crud_synth_author_signature_mismatch"
        );
        assert_eq!(mismatch.severity(), "warning");

        let lists: Vec<&ir::Query> = feature
            .queries
            .iter()
            .filter(|q| q.name() == "list_customers")
            .collect();
        assert_eq!(
            lists.len(),
            1,
            "author list_customers must not be duplicated"
        );
        match lists[0] {
            ir::Query::List(lq) => {
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "customer_admin"));
            }
            other => panic!("expected List query, got {other:?}"),
        }
    }

    #[test]
    fn fx1_without_crud_author_list_query_has_no_synth_collision() {
        let mut feature = empty_feature("customer", true);
        let mut resource = customer_resource();
        resource.conventions = Vec::new();
        feature.resources.push(resource);
        feature
            .queries
            .push(author_list_customers_query(ir::PolicyRef::Local(
                "authenticated".to_owned(),
            )));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert!(feature.commands.is_empty());
        assert_eq!(feature.queries.len(), 1);
        assert_eq!(feature.queries[0].name(), "list_customers");
    }

    /// §5.7 edge — resource with `user: User required unique` places
    /// both `org` and `user` in the Tenant group (neither lands in
    /// input).
    #[test]
    fn user_unique_resource_drops_user_from_inputs() {
        let mut feature = empty_feature("photoshare", true);
        feature.resources.push(ir::Resource {
            name: "PhotoShare".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("caption", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
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
        });

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "no diagnostics expected, got {:?}", diags);

        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_photo_share")
            .unwrap();
        match &create.input {
            ir::CommandInput::Typed(slots) => {
                let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
                // org + user are Tenant; only caption remains.
                assert_eq!(names, vec!["caption"]);
            }
            other => panic!("expected Typed input, got {:?}", other),
        }
    }

    /// §5.7 edge — resource without a lifecycle block has no discriminator
    /// to drop. A field named like a discriminator on another resource
    /// stays in input. Verifies the discriminator-skip is gated on
    /// `resource.lifecycle` being `Some`.
    #[test]
    fn resource_without_lifecycle_keeps_status_field() {
        let mut feature = empty_feature("customer", true);
        // Customer above has `status` field; it has NO lifecycle block,
        // so `status` should land in create / update input.
        feature.resources.push(customer_resource());
        let _ = synthesize_conventions(&mut feature);

        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_customer")
            .unwrap();
        let names: Vec<&str> = match &create.input {
            ir::CommandInput::Typed(slots) => slots.iter().map(|s| s.name.as_str()).collect(),
            other => panic!("expected Typed input, got {:?}", other),
        };
        assert!(names.contains(&"status"));
    }

    /// §11 — `crud_synth_no_required_fields` fires when every required
    /// field is Tenant or Auto. Build a resource with only `org`,
    /// `id`, `created_at` (all Tenant/Auto).
    #[test]
    fn empty_required_emits_no_required_fields_diagnostic() {
        let mut feature = empty_feature("ledger", true);
        feature.resources.push(ir::Resource {
            name: "Ledger".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("id", ir::TypeRef::Builtin(ir::BuiltinType::Id)),
                req_field("org", user_qn("Org")),
                req_field(
                    "created_at",
                    ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
                ),
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
        });

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags
                .iter()
                .any(|d| matches!(d, CrudSynthDiagnostic::NoRequiredFields { resource } if resource == "Ledger")),
            "expected NoRequiredFields for Ledger, got {:?}",
            diags
        );
    }

    /// §11 — `crud_synth_policy_not_found` fires when the feature has
    /// no `authenticated` policy. Synth still produces entries with the
    /// canonical PolicyRef; Cell C4 surfaces the diagnostic to the
    /// author.
    #[test]
    fn missing_authenticated_policy_emits_diagnostic() {
        let mut feature = empty_feature("customer", false); // no authenticated
        feature.resources.push(customer_resource());

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags
                .iter()
                .any(|d| matches!(d, CrudSynthDiagnostic::PolicyNotFound { resource } if resource == "Customer")),
            "expected PolicyNotFound for Customer, got {:?}",
            diags
        );
    }

    /// §11 — `crud_synth_signature_mismatch` fires when author wrote
    /// `update_customer` with a non-canonical input list (e.g., extra
    /// field).
    #[test]
    fn diverging_author_signature_emits_mismatch_diagnostic() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());
        // Author wrote update_customer with extra `notes` field — diverges.
        feature.commands.push(ir::Command {
            name: "update_customer".to_owned(),
            public_contract: None,
            kind: ir::CommandKind::Update,
            route: vec![ir::RouteSlot {
                name: "id".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
                from: None,
                kind: ir::RouteSlotKind::Plain,
            }],
            input: ir::CommandInput::Typed(vec![
                ir::TypedSlot {
                    name: "name".to_owned(),
                    type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                    validate_skip: false,
                },
                ir::TypedSlot {
                    name: "notes".to_owned(),
                    type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                    validate_skip: false,
                },
            ]),
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::Updates(ir::UpdateEffect {
                resource: ir::QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                },
                assignments: Vec::new(),
            }),
            policy: ir::PolicyRef::Local("customer_admin".to_owned()),
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
            handler: None,
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
            triggers: Vec::new(),
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
        });

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                CrudSynthDiagnostic::SignatureMismatch { resource, synth_name, .. }
                    if resource == "Customer" && synth_name == "update_customer"
            )),
            "expected SignatureMismatch for update_customer, got {:?}",
            diags
        );
    }

    /// Resource without `conventions [crud]` is a no-op for the synth.
    #[test]
    fn resource_without_conventions_is_no_op() {
        let mut feature = empty_feature("customer", true);
        let mut r = customer_resource();
        r.conventions = Vec::new();
        feature.resources.push(r);

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty());
        assert!(feature.commands.is_empty());
        assert!(feature.queries.is_empty());
    }
