    use lazuli_ir as ir;

    use lazuli_syntax::{parse_feature_skeletons, parse_lzx_document};

    use crate::auth::lower_auth_identity;
    use crate::query::parse_query_filter_line;
    use crate::resource::lower_validate_line;
    use crate::{
        AnalyzeError, lower_audit_block, lower_feature_skeleton, lower_lzx_document,
        lower_policy_atom_with_args, parse_cap_file_type, resolve_invalidates_targets,
        type_ref_from_syntax,
    };

    fn lower_module_for_test(source: &str) -> lazuli_ir::Module {
        let skeletons = parse_feature_skeletons(source).expect("parses");
        let features = skeletons
            .iter()
            .map(lower_feature_skeleton)
            .collect::<Result<Vec<_>, _>>()
            .expect("lowers");
        lazuli_ir::Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    #[test]
    fn enum_metadata_lowers_to_ir_variant_fields() {
        let source = r#"
feature account
  domain
    enum Gender
      male: label @translation.gender_male, icon "user"
      prefer_not: label @translation.gender_prefer_not, hint @translation.gender_prefer_not_hint
"#;
        let module = lower_module_for_test(source);
        let variants = &module.features[0].enums[0].variants;

        assert_eq!(variants[0].name, "male");
        assert_eq!(variants[0].label_key.as_deref(), Some("gender_male"));
        assert_eq!(variants[0].hint_key, None);
        assert_eq!(variants[0].icon_key.as_deref(), Some("user"));

        assert_eq!(variants[1].name, "prefer_not");
        assert_eq!(variants[1].label_key.as_deref(), Some("gender_prefer_not"));
        assert_eq!(
            variants[1].hint_key.as_deref(),
            Some("gender_prefer_not_hint")
        );
        assert_eq!(variants[1].icon_key, None);
    }

    #[test]
    fn invalidates_same_feature_query_ref_lowers_to_current_feature() {
        let source = r#"
feature customer
  domain
    query.list list

  command save
    policy @policy.update
    updates Customer
    invalidates
      query.list
"#;
        let mut module = lower_module_for_test(source);
        resolve_invalidates_targets(&mut module).expect("invalidates target resolves");

        let query = &module.features[0].commands[0].invalidates[0].query;
        assert_eq!(query.feature.as_deref(), Some("customer"));
        assert_eq!(query.name, "list");
    }

    #[test]
    fn invalidates_cross_feature_query_ref_strips_query_marker() {
        let source = r#"
feature bar
  domain
    query.list baz

feature customer
  command save
    policy @policy.update
    updates Customer
    invalidates
      bar.query.baz
"#;
        let mut module = lower_module_for_test(source);
        resolve_invalidates_targets(&mut module).expect("invalidates target resolves");

        let customer = module
            .features
            .iter()
            .find(|feature| feature.name == "customer")
            .expect("customer feature");
        let query = &customer.commands[0].invalidates[0].query;
        assert_eq!(query.feature.as_deref(), Some("bar"));
        assert_eq!(query.name, "baz");
    }

    #[test]
    fn invalidates_unknown_target_reports_correctness_error() {
        let source = r#"
feature customer
  command save
    policy @policy.update
    updates Customer
    invalidates
      nope.query.x
"#;
        let mut module = lower_module_for_test(source);
        let err = resolve_invalidates_targets(&mut module).unwrap_err();

        assert_eq!(
            err.diagnostic_code(),
            Some("@correctness.unknown_invalidate_target")
        );
        match err {
            AnalyzeError::UnknownInvalidateTarget {
                cmd,
                target,
                target_feature,
            } => {
                assert_eq!(cmd, "save");
                assert_eq!(target, "nope.query.x");
                assert_eq!(target_feature, "nope");
            }
            other => panic!("expected UnknownInvalidateTarget, got {other:?}"),
        }
    }

    #[test]
    fn lowers_lzx_experience_and_surface_to_ir() {
        let experience =
            parse_lzx_document(include_str!("../../../../../examples/customer-capsule.lzx")).unwrap();
        let surface =
            parse_lzx_document(include_str!("../../../../../examples/customer-capsule.web.lzx")).unwrap();

        let experience_ir = lower_lzx_document(&experience);
        let surface_ir = lower_lzx_document(&surface);

        assert_eq!(experience_ir.experiences[0].name, "customer");
        assert_eq!(experience_ir.experiences[0].imports, vec!["customer"]);
        assert_eq!(
            experience_ir.experiences[0].views[0].actions[0].target,
            "customer.command.create"
        );
        assert_eq!(surface_ir.surfaces[0].experience, "customer");
        assert_eq!(
            surface_ir.surfaces[0].uses_experience.as_deref(),
            Some("customer")
        );
        assert_eq!(surface_ir.surfaces[0].audiences[0].name, "admin");
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].columns,
            vec!["name", "email", "status", "created_at"]
        );
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].search,
            vec!["name", "email"]
        );
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].cells,
            vec!["status @client.status_cell"]
        );
    }

    #[test]
    fn lowers_lzx_extension_slots_to_ir() {
        let source = r#"
experience customer_tags
  imports customer_tags, customer

  extends @anchor.customer_detail
    slot aside after activity_timeline
      block @client.tag_editor
      platforms web
      audience admin
"#;
        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);
        let extension = &module.experiences[0].extensions[0];

        assert_eq!(extension.anchor, "@anchor.customer_detail");
        assert_eq!(extension.slots.len(), 1);
        assert_eq!(extension.slots[0].name, "aside");
        assert_eq!(extension.slots[0].blocks, vec!["@client.tag_editor"]);
        assert_eq!(extension.slots[0].platforms, vec!["web"]);
        assert_eq!(extension.slots[0].audiences, vec!["admin"]);
        assert_eq!(
            extension.slots[0]
                .order
                .as_ref()
                .map(|order| (order.relation.as_str(), order.target.as_str())),
            Some(("after", "activity_timeline"))
        );
    }

    #[test]
    fn lowers_lzx_route_guards_to_ir_with_spans() {
        let source = r#"
app AcmeCRM
  actor_query "account.query.me"
  route_guard
    default_policy @scope.authenticated
    on_unauthenticated redirect "/sign-in"
    on_unauthorized redirect "/403"
    skeleton @client.route_guard_skeleton

route admin_home
  path "/admin"
  to customer.view.list
  surface customer web
  audience admin
  policy @policy.admin_only
    on_unauthenticated redirect "/sign-in"

experience customer
  view list
    policy @policy.admin_only
      on_unauthorized redirect "/"
    source customer.query.list

surface customer web
  uses experience customer

  audience admin
    policy @policy.admin_only
      on_unauthenticated redirect "/sign-in"
    view list Table
      policy @policy.admin_only
        on_unauthorized redirect "/"
      columns name
"#;

        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);
        let app = module.app.as_ref().unwrap();
        let defaults = app.route_guard.as_ref().unwrap();

        assert_eq!(app.actor_query.as_deref(), Some("account.query.me"));
        assert_eq!(
            defaults.default_policy.as_deref(),
            Some("@scope.authenticated")
        );
        assert_eq!(defaults.on_unauthenticated.as_deref(), Some("/sign-in"));
        assert_eq!(defaults.on_unauthorized.as_deref(), Some("/403"));
        assert_eq!(
            defaults.skeleton.as_deref(),
            Some("@client.route_guard_skeleton")
        );
        assert!(defaults.span_ref.is_some());

        let route_guard = module.routes[0].guard.as_ref().unwrap();
        assert_eq!(
            &route_guard.policy[..],
            vec!["@policy.admin_only".to_owned()].as_slice()
        );
        assert_eq!(route_guard.on_unauthenticated.as_deref(), Some("/sign-in"));
        assert!(route_guard.span_ref.is_some());

        let view_guard = module.experiences[0].views[0].guard.as_ref().unwrap();
        assert_eq!(
            &view_guard.policy[..],
            vec!["@policy.admin_only".to_owned()].as_slice()
        );
        assert_eq!(view_guard.on_unauthorized.as_deref(), Some("/"));
        assert!(view_guard.span_ref.is_some());

        let audience_guard = module.surfaces[0].audiences[0].guard.as_ref().unwrap();
        assert_eq!(
            audience_guard.on_unauthenticated.as_deref(),
            Some("/sign-in")
        );
        assert!(audience_guard.span_ref.is_some());

        let platform_guard = module.surfaces[0].audiences[0].views[0]
            .guard
            .as_ref()
            .unwrap();
        assert_eq!(platform_guard.on_unauthorized.as_deref(), Some("/"));
        assert!(platform_guard.span_ref.is_some());
    }

    #[test]
    fn full_capsule_lzx_route_guards_ir_json_round_trip_is_byte_identical() {
        let source = include_str!("../../../../../examples/full-capsule/full-capsule.lzx");
        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);
        let guard = module
            .experiences
            .iter()
            .find(|experience| experience.name == "customer_auth")
            .and_then(|experience| {
                experience
                    .views
                    .iter()
                    .find(|view| view.name == "enable_mfa")
            })
            .and_then(|view| view.guard.as_ref())
            .expect("full-capsule enable_mfa guard");

        assert_eq!(
            &guard.policy[..],
            vec!["@policy.update".to_owned()].as_slice()
        );
        assert_eq!(guard.on_unauthenticated.as_deref(), Some("/login"));

        let first = serde_json::to_string_pretty(&module).unwrap();
        let decoded: ir::ExperienceModule = serde_json::from_str(&first).unwrap();
        let second = serde_json::to_string_pretty(&decoded).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn lowers_lzx_app_manifest_and_routes_to_ir() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  targets
    backend go
    web react
  uses customer, billing

route customer_detail
  path "/customers/:id"
  route id: Customer.ID
  to customer.view.detail(id: route.id)
  surface customer web
  audience admin
"#;

        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);

        assert_eq!(module.app.as_ref().unwrap().name, "AcmeCRM");
        assert_eq!(
            module.app.as_ref().unwrap().targets,
            vec!["backend go", "web react"]
        );
        assert_eq!(module.routes[0].name, "customer_detail");
        // ir+codegen(ts) §2.1 typed route_params landed (commit fe4d3a1c):
        // `route id: Customer.ID` now lifts to `route_params`, not `routes`.
        assert_eq!(module.routes[0].routes, Vec::<String>::new());
        assert_eq!(module.routes[0].route_params.len(), 1);
        assert_eq!(module.routes[0].route_params[0].name, "id");
        assert_eq!(
            module.routes[0].to.as_deref(),
            Some("customer.view.detail(id: route.id)")
        );
    }
