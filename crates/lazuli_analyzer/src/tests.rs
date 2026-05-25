//! Analyzer unit tests.
//!
//! Extracted from `lib.rs` by R4-E. The parent declaration
//! `#[cfg(test)] mod tests;` gates this whole file.
//!
//! All test modules are kept as `mod NAME { ... }` siblings so raw-string
//! indentation inside test fixtures stays load-bearing-correct. The original
//! top-level `mod tests` from lib.rs is preserved here as `mod core`.

mod core {
    use lazuli_syntax::{parse_feature_skeletons, parse_lzx_document};

    use crate::auth::lower_auth_identity;
    use crate::query::parse_query_filter_line;
    use crate::resource::lower_validate_line;
    use crate::{
        AnalyzeError, lower_audit_block, lower_feature_skeleton, lower_lzx_document,
        lower_policy_atom_with_args, parse_cap_file_type, resolve_invalidates_targets,
        type_ref_from_syntax,
    };

    #[test]
    fn query_filter_line_lowers_dotted_path() {
        let filter = parse_query_filter_line("org_id = ctx.actor.org_id")
            .expect("dotted path filter parses");
        let ir::Predicate::Comparison { left, op, right } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        assert!(matches!(op, ir::CompareOp::Eq));
        assert_eq!(
            left,
            ir::Expr::Path(ir::Path::from_segments(["org_id".to_owned()]))
        );
        assert_eq!(
            right,
            ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "org_id".to_owned(),
            ]))
        );
        assert!(filter.when.is_none());
    }

    #[test]
    fn query_filter_line_lowers_bool_literal() {
        let filter = parse_query_filter_line("is_public = false").unwrap();
        let ir::Predicate::Comparison { right, .. } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        assert_eq!(right, ir::Expr::Boolean(false));
    }

    #[test]
    fn query_filter_line_lifts_bare_identifier_to_enum_literal() {
        // WAR-VOCAB-QUERY-ENUM-01 closure: `status = approved` must
        // lift `approved` to `Expr::Enum` so codegen emits a TEXT
        // const bind, NOT a runtime input lookup.
        let filter = parse_query_filter_line("status = approved").unwrap();
        let ir::Predicate::Comparison { right, .. } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        let literal = match right {
            ir::Expr::Enum(literal) => literal,
            other => panic!("expected Expr::Enum, got {other:?}"),
        };
        assert!(literal.type_name.is_none());
        assert_eq!(literal.variant, "approved");
    }

    #[test]
    fn query_filter_line_handles_inequality_operators() {
        let f1 = parse_query_filter_line("rating >= 4").unwrap();
        if let ir::Predicate::Comparison { op, .. } = f1.predicate {
            assert!(matches!(op, ir::CompareOp::Ge));
        } else {
            panic!("expected Comparison");
        }
        let f2 = parse_query_filter_line("status != cancelled").unwrap();
        if let ir::Predicate::Comparison { op, right, .. } = f2.predicate {
            assert!(matches!(op, ir::CompareOp::Ne));
            if let ir::Expr::Enum(literal) = right {
                assert_eq!(literal.variant, "cancelled");
            } else {
                panic!("expected Enum literal on RHS of !=");
            }
        } else {
            panic!("expected Comparison");
        }
    }

    #[test]
    fn query_filter_line_drops_blanks_and_comments() {
        assert!(parse_query_filter_line("").is_none());
        assert!(parse_query_filter_line("   ").is_none());
        assert!(parse_query_filter_line("# org_id = ctx.actor.org_id").is_none());
    }

    #[test]
    fn query_filter_line_lowers_quoted_string() {
        let filter = parse_query_filter_line("name = \"hello\"").unwrap();
        if let ir::Predicate::Comparison { right, .. } = filter.predicate {
            assert_eq!(right, ir::Expr::String("hello".to_owned()));
        } else {
            panic!("expected Comparison");
        }
    }

    #[test]
    fn query_filter_line_lowers_integer_and_nil() {
        let f1 = parse_query_filter_line("count >= 0").unwrap();
        if let ir::Predicate::Comparison { right, .. } = f1.predicate {
            assert_eq!(right, ir::Expr::Integer(0));
        } else {
            panic!("expected Comparison");
        }
        let f2 = parse_query_filter_line("deleted_at = nil").unwrap();
        if let ir::Predicate::Comparison { right, .. } = f2.predicate {
            assert_eq!(right, ir::Expr::Nil);
        } else {
            panic!("expected Comparison");
        }
    }

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
            parse_lzx_document(include_str!("../../../examples/customer-capsule.lzx")).unwrap();
        let surface =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.web.lzx")).unwrap();

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
        let source = include_str!("../../../examples/full-capsule/full-capsule.lzx");
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

    // -------------------------------------------------------------------------
    // Cut A — agent lowering (§4.4 snapshot tests)
    // -------------------------------------------------------------------------

    use lazuli_ir as ir;

    fn lower_first_agent(source: &str) -> ir::Agent {
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(features.len(), 1);
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        feature.agents.into_iter().next().expect("agent")
    }

    #[test]
    fn lower_agent_with_tools_resolves_to_ir() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer.query.by_id
      query.by_id
      command.archive
      @tool.web_search
      @tool.calendar.create_event
"#;
        let agent = lower_first_agent(source);

        assert_eq!(agent.feature, "customer");
        assert_eq!(agent.name, "triage");
        assert_eq!(agent.tools.len(), 5);

        match &agent.tools[0].reference {
            ir::QualifiedToolRef::CrossFeature {
                feature,
                kind,
                name,
            } => {
                assert_eq!(feature, "customer");
                assert_eq!(*kind, ir::ToolKind::QueryUnspecified);
                assert_eq!(name, "by_id");
            }
            other => panic!("expected CrossFeature, got {other:?}"),
        }
        match &agent.tools[1].reference {
            ir::QualifiedToolRef::Local { kind, name } => {
                assert_eq!(*kind, ir::ToolKind::QueryUnspecified);
                assert_eq!(name, "by_id");
            }
            other => panic!("expected Local, got {other:?}"),
        }
        match &agent.tools[2].reference {
            ir::QualifiedToolRef::Local { kind, name } => {
                assert_eq!(*kind, ir::ToolKind::Command);
                assert_eq!(name, "archive");
            }
            other => panic!("expected Local Command, got {other:?}"),
        }
        match &agent.tools[3].reference {
            ir::QualifiedToolRef::Adapter { dotted } => {
                assert_eq!(dotted, &vec!["web_search".to_owned()]);
            }
            other => panic!("expected Adapter, got {other:?}"),
        }
        match &agent.tools[4].reference {
            ir::QualifiedToolRef::Adapter { dotted } => {
                assert_eq!(
                    dotted,
                    &vec!["calendar".to_owned(), "create_event".to_owned()]
                );
            }
            other => panic!("expected Adapter dotted, got {other:?}"),
        }

        // Expand pass populates the resolved_* fields; lowering leaves them
        // None / empty.
        assert!(agent.tools.iter().all(|t| t.resolved_effect.is_none()));
        assert!(agent.tools.iter().all(|t| t.resolved_policy.is_none()));
        assert!(
            agent
                .tools
                .iter()
                .all(|t| t.resolved_pii_classes.is_empty())
        );
    }

    #[test]
    fn lower_agent_with_evals_resolves_to_ir() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case short_for_active
        requires customer.lifecycle_stage = active
        requires output contains "active"

      case redacts_email
        forbids output contains @semantic.Email

      case uses_lookup
        requires tools.calls includes customer.query.by_id
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.evals.len(), 3);

        // Case 0: Closed Comparison + Contains literal.
        let c0 = &agent.evals[0];
        assert_eq!(c0.name, "short_for_active");
        match &c0.assertions[0].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { left, op, right }) => {
                assert_eq!(*op, ir::CompareOp::Eq);
                match (left, right) {
                    (ir::Expr::Path(lhs), ir::Expr::Path(rhs)) => {
                        assert_eq!(lhs.segments, vec!["customer", "lifecycle_stage"]);
                        assert_eq!(rhs.segments, vec!["active"]);
                    }
                    other => panic!("unexpected Comparison sides: {other:?}"),
                }
            }
            other => panic!("expected Closed Comparison, got {other:?}"),
        }
        match &c0.assertions[1].predicate {
            ir::EvalPredicate::Contains { lhs, rhs } => {
                assert_eq!(lhs.segments, vec!["output"]);
                assert_eq!(rhs, &ir::EvalContainsRhs::Literal("active".to_owned()));
            }
            other => panic!("expected Contains literal, got {other:?}"),
        }

        // Case 1: Forbids + Contains semantic.
        let c1 = &agent.evals[1];
        assert_eq!(c1.assertions[0].kind, ir::EvalAssertionKind::Forbids);
        match &c1.assertions[0].predicate {
            ir::EvalPredicate::Contains { rhs, .. } => match rhs {
                ir::EvalContainsRhs::SemanticType(qn) => {
                    assert_eq!(qn.name, "@semantic.Email");
                }
                other => panic!("expected SemanticType, got {other:?}"),
            },
            other => panic!("expected Contains, got {other:?}"),
        }

        // Case 2: ToolsCalls includes a cross-feature target.
        let c2 = &agent.evals[2];
        match &c2.assertions[0].predicate {
            ir::EvalPredicate::ToolsCalls { op, target } => {
                assert_eq!(*op, ir::ToolsCallsOp::Includes);
                match target {
                    ir::QualifiedToolRef::CrossFeature { feature, name, .. } => {
                        assert_eq!(feature, "customer");
                        assert_eq!(name, "by_id");
                    }
                    other => panic!("expected CrossFeature target, got {other:?}"),
                }
            }
            other => panic!("expected ToolsCalls, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_with_discriminator_output_resolves() {
        let source = r#"
feature customer_support
  agent classify_intent
    input
      message: Text required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.output_kind, ir::AgentOutputKind::DiscriminatedEnum);
        match agent.output_discriminator.as_ref().unwrap() {
            ir::DiscriminatorRef::Enum(qn) => {
                assert_eq!(qn.name, "Intent");
                assert!(qn.feature.is_none());
            }
            other => panic!("expected Enum discriminator, got {other:?}"),
        }
        assert!(agent.output_type.is_none());
    }

    #[test]
    fn lower_agent_with_discriminated_record_resolves() {
        // Bare `output Action` lowers as Text + Some(output_type=Action).
        // The expand pass (Phase 5) promotes to DiscriminatedRecord when
        // it resolves `Action` to a record with a `discriminator` field.
        let source = r#"
feature customer
  agent extract_action
    input
      message: Text required
    policy @policy.read
    output Action
    model @llm.default
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.output_kind, ir::AgentOutputKind::Text);
        assert!(agent.output_discriminator.is_none());
        match agent.output_type.as_ref().unwrap() {
            ir::TypeRef::UserDefined(q) => {
                assert_eq!(q.name, "Action");
                assert!(q.feature.is_none());
            }
            other => panic!("expected UserDefined Action, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_evals_without_temperature_zero_is_marked_nondeterministic() {
        // Lowering doesn't fail; doctor's diagnostic
        // `eval_nondeterministic_warning` fires in Phase 3. Here we just
        // verify lowering captures `temperature` and `seed` so doctor can
        // inspect them.
        let source = r#"
feature customer
  agent flaky
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case nondeterministic
        requires output contains "x"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.temperature, Some(0.7));
        assert!(agent.seed.is_none());
        assert!(!agent.evals.is_empty());
        // Doctor will combine temperature + seed + evals.is_empty() to
        // emit `eval_nondeterministic_warning` in Phase 3.
    }

    #[test]
    fn lower_agent_propagates_safety_list_for_cut_a5_ready() {
        // Cut A allows 0..1 safety entries; Cut A.5 widens to a list.
        // The IR shape `safety: Vec<QualifiedName>` already supports the
        // wider form — this test pins the shape so A.5 lands by adding
        // a doctor diagnostic, not by changing IR.
        let source = r#"
feature customer
  agent guarded
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    safety @validator.pii_email_scrub, @validator.pii_ssn_scrub
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.safety.len(), 2);
        assert_eq!(agent.safety[0].name, "@validator.pii_email_scrub");
        assert_eq!(agent.safety[1].name, "@validator.pii_ssn_scrub");
    }

    #[test]
    fn lower_agent_ordered_compare_op_lowers_to_lt_le_gt_ge() {
        // Proposal §A3 admits ordered ops inside evals. Lowering parses
        // them; doctor's `eval_ordered_op_invalid_diagnostics` decides
        // whether the operand types are numeric.
        let source = r#"
feature customer
  agent ordered
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case bounded
        requires output.length <= 800
        requires output.length >= 1
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.evals.len(), 1);
        match &agent.evals[0].assertions[0].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { op, .. }) => {
                assert_eq!(*op, ir::CompareOp::Le);
            }
            other => panic!("expected Le Comparison, got {other:?}"),
        }
        match &agent.evals[0].assertions[1].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { op, .. }) => {
                assert_eq!(*op, ir::CompareOp::Ge);
            }
            other => panic!("expected Ge Comparison, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_invalid_tool_ref_errors() {
        // `@tool` (no dotted tail) is malformed; lowering returns
        // `AnalyzeError::InvalidToolRef`. Tool-string sanity checks fire
        // here so doctor can stay focused on cross-feature resolution.
        let source = r#"
feature customer
  agent broken
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      @tool.
"#;
        // Note: the parser already rejects `@tool.` (trailing dot leaves an
        // empty tail when split). We craft a slightly different shape so
        // the parser accepts and lowering rejects.
        let parsed = parse_feature_skeletons(source);
        match parsed {
            Err(_) => return, // parser caught it — equally valid
            Ok(features) => {
                let err = lower_feature_skeleton(&features[0]).unwrap_err();
                match err {
                    AnalyzeError::InvalidToolRef { .. } => {}
                    other => panic!("expected InvalidToolRef, got {other:?}"),
                }
            }
        }
    }

    #[test]
    fn lower_agent_golden_eval_lowers_to_ir() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case quality
        requires output contains "active"
        golden "./evals/summarize.jsonl" min_score 0.85
"#;
        let agent = lower_first_agent(source);
        let case = &agent.evals[0];
        let golden = case.golden.as_ref().expect("golden");
        assert_eq!(golden.path, "./evals/summarize.jsonl");
        assert_eq!(golden.min_score, Some(0.85));
        // Assertions still present alongside the golden ref.
        assert_eq!(case.assertions.len(), 1);
    }

    #[test]
    fn lower_agent_with_expose_http_lowers_to_ir() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
      route customer_id: Customer.ID
      audience admin
      rate_limit "5 per minute per user"
"#;
        let agent = lower_first_agent(source);
        let expose = agent.expose_http.as_ref().expect("expose_http");
        assert_eq!(expose.method, ir::HttpMethod::Post);
        assert_eq!(expose.path, "/api/customers/:customer_id/summary");
        assert_eq!(expose.route_slots.len(), 1);
        assert_eq!(expose.route_slots[0].name, "customer_id");
        assert!(expose.route_slots[0].required);
        assert_eq!(expose.audience.as_deref(), Some("admin"));
        assert_eq!(
            expose.rate_limit_override.as_deref(),
            Some("5 per minute per user")
        );
    }

    #[test]
    fn lower_agent_without_expose_keeps_field_none() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert!(agent.expose_http.is_none());
    }

    // -------------------------------------------------------------------------
    // Phase L — `auth` block lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_auth_full_block_to_ir() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"

    oauth google
      adapter @adapter.google_oauth

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp

    sessions
      resource CustomerSession
      ttl "7 days"
      refresh false
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let auth = feature.auth.expect("auth lowered");

        assert_eq!(auth.identity.field.resource.name, "Customer");
        assert_eq!(auth.identity.field.field, "email");

        let password = auth.password.as_ref().expect("password");
        assert_eq!(password.algorithm, "argon2id");
        assert_eq!(password.hash, "@fn.hash_customer_password");
        assert_eq!(password.verify, "@fn.verify_customer_password");
        let rate_limit = password.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(rate_limit.default, "5 per 10 minutes");
        assert!(rate_limit.by_env.is_empty());

        let mfa = auth.mfa.as_ref().expect("mfa");
        assert_eq!(mfa.method, "totp");
        assert_eq!(mfa.enroll, "@fn.enroll_customer_totp");
        assert_eq!(mfa.verify, "@validator.verify_customer_totp");

        let sessions = auth.sessions.as_ref().expect("sessions");
        assert_eq!(sessions.resource.name, "CustomerSession");
        assert_eq!(sessions.ttl, "7 days");
        assert!(!sessions.refresh);
        assert!(sessions.access_ttl.is_none());
        assert!(sessions.rotation.is_none());

        assert_eq!(auth.oauth.len(), 1);
        assert_eq!(auth.oauth[0].provider, "google");
        assert_eq!(auth.oauth[0].adapter, "@adapter.google_oauth");
    }

    #[test]
    fn lower_auth_sessions_rotation_block_to_ir() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      access_ttl "15 minutes"
      rotation
        refresh_ttl "30 days"
        grace "30 seconds"
        theft_detection_action revoke_user
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let sessions = feature
            .auth
            .as_ref()
            .expect("auth lowered")
            .sessions
            .as_ref()
            .expect("sessions lowered");

        assert_eq!(sessions.access_ttl.as_deref(), Some("15 minutes"));
        let rotation = sessions.rotation.as_ref().expect("rotation lowered");
        assert_eq!(rotation.refresh_ttl.as_deref(), Some("30 days"));
        assert_eq!(rotation.grace.as_deref(), Some("30 seconds"));
        assert_eq!(
            rotation.theft_detection_action,
            Some(ir::TheftAction::RevokeUser)
        );
        assert!(rotation.span_ref.is_some());
    }

    #[test]
    fn lower_auth_sessions_empty_rotation_block_uses_ir_defaults_later() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      rotation
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let sessions = feature
            .auth
            .as_ref()
            .expect("auth lowered")
            .sessions
            .as_ref()
            .expect("sessions lowered");

        let rotation = sessions.rotation.as_ref().expect("rotation lowered");
        assert!(rotation.refresh_ttl.is_none());
        assert!(rotation.grace.is_none());
        assert!(rotation.theft_detection_action.is_none());
        assert_eq!(sessions.resolved_access_ttl(), "15 minutes");
        assert_eq!(sessions.resolved_refresh_ttl(), Some("30 days"));
        assert_eq!(sessions.resolved_rotation_grace(), Some("30 seconds"));
        assert_eq!(
            sessions.resolved_theft_action(),
            Some(ir::TheftAction::RevokeSessionFamily)
        );
    }

    #[test]
    fn lower_auth_sessions_without_legacy_refresh_keeps_rotation_none() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let sessions = feature
            .auth
            .as_ref()
            .expect("auth lowered")
            .sessions
            .as_ref()
            .expect("sessions lowered");

        assert!(!sessions.refresh);
        assert!(sessions.access_ttl.is_none());
        assert!(sessions.rotation.is_none());
    }

    #[test]
    fn lower_auth_identity_with_empty_field_errors() {
        // Parser would already reject `identity .email` because the
        // dot-qualified contract requires both segments; this test
        // documents the analyzer's defensive guard for any future
        // parser shape that lets a stray dot through.
        let identity = lazuli_syntax::AuthIdentity {
            field: "Customer.".to_owned(),
            public_contract: None,
            span: lazuli_syntax::Span::new(0, 9),
        };
        let err = lower_auth_identity(&identity).unwrap_err();
        match err {
            AnalyzeError::InvalidAuthIdentity { reference } => {
                assert_eq!(reference, "Customer.");
            }
            other => panic!("expected InvalidAuthIdentity, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 3 — job / webhook / notification / event_group lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_tier3_job_handler_full_block() {
        let source = r#"
feature customer
  job process_import
    trigger event customer_import_uploaded
    queue customer_imports
    tenant_from payload.org_id
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
      org_id = payload.org_id
    timeout "30s"
    handler "./jobs/process_import.go"
    emits customer_import_completed
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.jobs.len(), 1);
        let job = &feature.jobs[0];
        assert_eq!(job.name, "process_import");
        assert_eq!(job.queue.as_deref(), Some("customer_imports"));
        assert_eq!(job.timeout.as_deref(), Some("30s"));
        let tenant = job.tenant_from.as_ref().expect("tenant_from");
        assert_eq!(tenant.path.segments, vec!["payload", "org_id"]);
        let retry = job.retry.as_ref().expect("retry");
        assert_eq!(retry.count, 3);
        assert!(matches!(retry.backoff, ir::BackoffStrategy::Exponential));
        assert_eq!(job.external_calls.len(), 1);
        assert_eq!(job.external_calls[0].slot, "crm");
        assert_eq!(job.external_calls[0].op, "normalize_import_batch");
        assert_eq!(job.external_calls[0].args.len(), 2);
        match &job.body {
            ir::JobBody::Handler(h) => {
                assert_eq!(h.path.path, "./jobs/process_import.go");
            }
            other => panic!("expected Handler body, got {other:?}"),
        }
        assert_eq!(job.emits, vec!["customer_import_completed"]);
    }

    #[test]
    fn lower_tier3_job_declarative_carve_out() {
        let source = r#"
feature customer
  job recompute_score_after_invoice
    trigger event billing.invoice_paid
    tenant_from payload.org_id
    idempotency by envelope.id
    target query.by_id(id: payload.customer_id)
    let new_score = @fn.risk_score(target)
    updates Customer
      score = new_score
    emits customer_score_recomputed
      score = new_score
      reason = "invoice_paid"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.jobs.len(), 1);
        let job = &feature.jobs[0];
        match &job.body {
            ir::JobBody::Declarative(d) => {
                let target = d.target.as_ref().expect("target lifted");
                assert_eq!(target.query.name, "by_id");
                assert_eq!(d.lets.len(), 1);
                assert_eq!(d.lets[0].name, "new_score");
                match &d.effect {
                    ir::CommandEffect::Updates(u) => {
                        assert_eq!(u.resource.name, "Customer");
                        assert_eq!(u.assignments.len(), 1);
                        assert_eq!(u.assignments[0].field, "score");
                    }
                    other => panic!("expected Updates effect, got {other:?}"),
                }
            }
            other => panic!("expected Declarative body, got {other:?}"),
        }
    }

    #[test]
    fn lower_tier3_webhook_structured_verify() {
        let source = r#"
feature customer
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    handler "./integrations/upsert_customer_from_crm.go" returns Customer
    emits customer_webhook_received
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.webhooks.len(), 1);
        let webhook = &feature.webhooks[0];
        assert_eq!(webhook.route, "/webhooks/crm/customer-upsert");
        let verify = webhook
            .structured_verify
            .as_ref()
            .expect("structured verify");
        assert!(matches!(verify.scheme, ir::VerifyScheme::Hmac));
        assert_eq!(verify.algorithm, "sha256");
        assert_eq!(verify.secret_env, "CRM_WEBHOOK_SECRET");
        assert_eq!(verify.header, "X-CRM-Signature");
        let tenant = webhook.tenant_from.as_ref().expect("tenant_from");
        assert_eq!(tenant.path.segments, vec!["payload", "org_id"]);
        assert_eq!(
            webhook.handler.path,
            "./integrations/upsert_customer_from_crm.go"
        );
        assert_eq!(webhook.emits, vec!["customer_webhook_received"]);
    }

    #[test]
    fn lower_tier3_notification_full_block() {
        let source = r#"
feature customer_outreach
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    tenant_from payload.org_id
    idempotency by envelope.id
    retry 3 backoff exponential
    template "./outreach/welcome_email.mjml"
    policy @policy.notify
    emits welcome_email_sent
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.notifications.len(), 1);
        let n = &feature.notifications[0];
        assert_eq!(n.name, "welcome_email");
        assert_eq!(n.channels, vec!["email"]);
        assert_eq!(n.recipient, "target.email");
        assert_eq!(n.template, "./outreach/welcome_email.mjml");
        match &n.trigger {
            ir::JobTrigger::Event { event } => {
                assert_eq!(event.feature.as_deref(), Some("customer"));
                assert_eq!(event.name, "customer_activated");
            }
            other => panic!("expected Event trigger, got {other:?}"),
        }
        assert_eq!(n.emits, vec!["welcome_email_sent"]);
    }

    #[test]
    fn lower_tier3_event_group_payload_and_events() {
        let source = r#"
feature customer
  event_group customer_* on Customer
    payload
      customer_id = id
      org_id = org.id
    event created
    event activated
    event archived
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.event_groups.len(), 1);
        let group = &feature.event_groups[0];
        assert_eq!(group.pattern, "customer_*");
        assert_eq!(group.on_resource.as_deref(), Some("Customer"));
        assert_eq!(group.raw_payload.len(), 2);
        assert_eq!(
            group.events,
            vec![
                "created".to_owned(),
                "activated".to_owned(),
                "archived".to_owned()
            ]
        );
    }

    /// B5 framework gap 1 — per-event typed payload field bodies are
    /// lifted into `EventGroup.variants`. The legacy `events: Vec<String>`
    /// slot still holds the name list (back-compat), and each variant
    /// carries its `EventField`s, kind, and outbox flag.
    #[test]
    fn lower_event_group_lifts_per_event_typed_payload_fields() {
        let source = r#"
feature payments
  event_group charge_* on Charge
    payload
      charge_id = id
    event requested
      outbox guaranteed
      amount: @semantic.Money
      host_id: ID
    event confirmed
      outbox guaranteed
      amount: @semantic.Money
      provider_payment_id: Text
      paid_at: DateTime
    event.trace mp_status_received
      provider_status: Text
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let group = &feature.event_groups[0];
        assert_eq!(group.variants.len(), 3, "three variants under group");

        // Variant 0 — requested
        let requested = &group.variants[0];
        assert_eq!(requested.name, "requested");
        assert!(matches!(requested.kind, ir::EventVariantKind::Committed));
        assert!(requested.outbox.is_guaranteed());
        assert_eq!(requested.fields.len(), 2);
        assert_eq!(requested.fields[0].name, "amount");
        assert_eq!(requested.fields[1].name, "host_id");

        // Variant 1 — confirmed
        let confirmed = &group.variants[1];
        assert_eq!(confirmed.name, "confirmed");
        assert_eq!(confirmed.fields.len(), 3);
        let names: Vec<&str> = confirmed.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["amount", "provider_payment_id", "paid_at"]);

        // Variant 2 — trace
        let trace = &group.variants[2];
        assert_eq!(trace.name, "mp_status_received");
        assert!(matches!(trace.kind, ir::EventVariantKind::Trace));
        assert!(trace.outbox.is_none());
        assert_eq!(trace.fields.len(), 1);
        assert_eq!(trace.fields[0].name, "provider_status");
    }

    /// B5 framework gap 1 — `event foo` (no body) still parses and
    /// lowers cleanly. The variant comes through with an empty
    /// `fields` Vec so the legacy `Feature.events` lookup path stays
    /// in charge of the typed projection.
    #[test]
    fn lower_event_group_back_compat_empty_event_bodies() {
        let source = r#"
feature customer
  event_group customer_* on Customer
    payload
      customer_id = id
    event created
    event archived
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let group = &feature.event_groups[0];
        assert_eq!(group.variants.len(), 2);
        for variant in &group.variants {
            assert!(variant.fields.is_empty());
            assert!(matches!(variant.kind, ir::EventVariantKind::Committed));
        }
    }

    /// B5 framework gap 2 — `webhook ... emits foo when <predicate>`
    /// lifts the per-branch `when` clause into a typed `EmitPredicate`.
    #[test]
    fn lower_webhook_with_when_predicates_typed_lift() {
        let source = r#"
feature payments
  webhook mp_payment_event
    path "/webhooks/mp/payment"
    verify hmac sha256
      secret env.MERCADOPAGO_WEBHOOK_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_mp_payment_event
    emits charge_confirmed when payload.status = "approved"
    emits charge_failed when payload.status in ("rejected", "cancelled")
    emits mp_status_received
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let webhook = &feature.webhooks[0];
        assert_eq!(
            webhook.emits,
            vec![
                "charge_confirmed".to_owned(),
                "charge_failed".to_owned(),
                "mp_status_received".to_owned()
            ]
        );
        assert_eq!(webhook.emit_predicates.len(), 3);

        // [0] equals
        let approved = webhook.emit_predicates[0]
            .as_ref()
            .expect("first emit has predicate");
        match &approved.kind {
            ir::EmitPredicateKind::Equals { path, literal } => {
                assert_eq!(path, "payload.status");
                assert_eq!(literal, "approved");
            }
            other => panic!("expected Equals, got {:?}", other),
        }

        // [1] in
        let failed = webhook.emit_predicates[1]
            .as_ref()
            .expect("second emit has predicate");
        match &failed.kind {
            ir::EmitPredicateKind::In { path, literals } => {
                assert_eq!(path, "payload.status");
                assert_eq!(
                    literals,
                    &vec!["rejected".to_owned(), "cancelled".to_owned()]
                );
            }
            other => panic!("expected In, got {:?}", other),
        }

        // [2] no predicate (default branch)
        assert!(webhook.emit_predicates[2].is_none());
    }

    /// B5 framework gap 2 back-compat — the flat `emits foo` /
    /// `emits bar` shape (no predicates) leaves `emit_predicates`
    /// empty so the generated `WebhookContract` stays on the legacy
    /// `Emits []string{}` shape.
    #[test]
    fn lower_webhook_without_when_predicates_keeps_legacy_emits_shape() {
        let source = r#"
feature payments
  webhook mp_payment_event
    path "/webhooks/mp/payment"
    verify hmac sha256
      secret env.MERCADOPAGO_WEBHOOK_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_mp_payment_event
    emits charge_confirmed
    emits charge_failed
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let webhook = &feature.webhooks[0];
        assert_eq!(webhook.emits.len(), 2);
        assert!(
            webhook.emit_predicates.is_empty(),
            "no `when` clauses means no per-branch dispatch"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 2 — `@cap.File(...)` typing
    // -------------------------------------------------------------------------

    #[test]
    fn mfa_atom_with_args_lowers() {
        let atom = lower_policy_atom_with_args("@mfa.required(within:15m)");
        assert_eq!(atom.namespace, "mfa");
        assert_eq!(atom.name, "required");
        assert_eq!(atom.args.as_deref(), Some("within:15m"));
    }

    #[test]
    fn cap_pii_lowers() {
        let ty = type_ref_from_syntax("@cap.PII(class:contact,retention:90d,log_redact:true)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::PII(pii)) => {
                assert_eq!(pii.class, "contact");
                assert_eq!(pii.retention.as_deref(), Some("90d"));
                assert_eq!(pii.log_redact, Some(true));
            }
            other => panic!("expected Capability::PII, got {other:?}"),
        }
    }

    fn lower_field_line(line: &str) -> ir::Field {
        let source = format!(
            "feature account\n  domain\n    resource Customer\n      {}\n",
            line
        );
        let features = lazuli_syntax::parse_feature_skeletons(&source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        feature
            .resources
            .into_iter()
            .next()
            .expect("resource")
            .fields
            .into_iter()
            .next()
            .expect("field")
    }

    #[test]
    fn field_with_pii_decorator_stacks_with_semantic() {
        let line = "cpf: @semantic.BrazilianCPF optional unique @cap.PII(class:\"identity\")";
        let field = lower_field_line(line);
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::UserDefined(ref q) if q.name == "@semantic.BrazilianCPF"
        ));
        assert!(!field.required);
        assert!(field.unique);
        assert!(field.pii.is_some());
        assert_eq!(field.pii.as_ref().unwrap().class, "identity");
    }

    #[test]
    fn field_without_pii_decorator_has_none() {
        let field = lower_field_line("name: Text required");
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::Builtin(ir::BuiltinType::Text)
        ));
        assert!(field.required);
        assert!(field.pii.is_none());
    }

    #[test]
    fn owner_axis_on_fk_field_lowers_into_ir() {
        // `ir-resource-conventions-owner-scope` §7 — happy path: a
        // user-defined FK field (here `host: Host required`) is the
        // only legal carrier for `@owner_axis(through: <ident>)`.
        let field = lower_field_line("host: Host required @owner_axis(through: user)");
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::UserDefined(ref q) if q.name == "Host"
        ));
        let axis = field
            .owner_axis
            .as_ref()
            .expect("`@owner_axis(through: user)` must lower into ir::Field.owner_axis");
        assert_eq!(axis.through_column, "user");
    }

    #[test]
    fn owner_axis_on_primitive_field_emits_owner_axis_on_non_fk() {
        // `ir-resource-conventions-owner-scope` §11.1 —
        // `owner_axis_on_non_fk`. The annotation on a primitive field
        // (here `slug: Text`) is rejected at lowering: primitives carry
        // no ownership chain for the synth pass to walk.
        let source = "
feature catalog
  domain
    resource Property
      slug: Text @owner_axis(through: user)
";
        let features = lazuli_syntax::parse_feature_skeletons(source)
            .expect("parses (annotation is syntactic)");
        let err = lower_feature_skeleton(&features[0])
            .expect_err("lowering must reject @owner_axis on a non-FK field");
        match err {
            AnalyzeError::OwnerAxisOnNonFk { field, .. } => {
                assert_eq!(field, "slug");
            }
            other => panic!("expected OwnerAxisOnNonFk, got {other:?}"),
        }
    }

    #[test]
    fn field_with_pii_decorator_after_default_cleans_default() {
        let field = lower_field_line("name: Text required = anon @cap.PII(class:\"contact\")");
        assert_eq!(
            field.default,
            Some(ir::DefaultValue::EnumLiteral(ir::EnumLiteral {
                type_name: None,
                variant: "anon".to_owned(),
            }))
        );
        assert_eq!(field.pii.as_ref().unwrap().class, "contact");
    }

    #[test]
    fn audit_data_subject_lowers() {
        let spec = lower_audit_block("audit default\naudit data_subject user_id\n");
        assert_eq!(spec.subjects, vec!["default".to_owned()]);
        assert_eq!(spec.data_subject.as_deref(), Some("user_id"));
    }

    #[test]
    fn audit_before_after_lowers() {
        let spec = lower_audit_block("audit before, after\n");
        assert!(spec.record_before);
        assert!(spec.record_after);
    }

    #[test]
    fn audit_retain_lowers() {
        let spec = lower_audit_block("audit retain 90d\n");
        assert_eq!(spec.retain_for.as_deref(), Some("90d"));
    }

    #[test]
    fn validate_sanitize_html_lowers() {
        let constraints =
            lower_validate_line("validate sanitize_html(basic)").expect("valid profile");
        assert_eq!(
            constraints.sanitize_html,
            Some(ir::SanitizeHtmlProfile::Basic)
        );
    }

    #[test]
    fn validate_sanitize_html_rejects_unknown_profile() {
        let result = lower_validate_line("validate sanitize_html(unsafe)");
        assert!(matches!(
            result,
            Err(AnalyzeError::UnknownSanitizeHtmlProfile { .. })
        ));
    }

    #[test]
    fn validate_limits_lower() {
        let source = r#"
feature account
  domain
    resource Payload
      body: Json validate utf8_safe validate max_recursion:8 validate max_size:4096
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.utf8_safe, Some(true));
        assert_eq!(field.constraints.max_recursion, Some(8));
        assert_eq!(field.constraints.max_size, Some(4096));
    }

    #[test]
    fn validator_covers_pii_lowers() {
        let source = r#"
feature account
  domain
    resource Customer
      email: Text validator covers_pii
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.covers_pii.as_deref(), Some("covers_pii"));
    }

    #[test]
    fn command_route_token_kinds_lower() {
        let source = r#"
feature account
  command consume
    route opaque token: Text
    route signed_token
    returns Text
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let command = &feature.commands[0];
        assert_eq!(command.route[0].name, "token");
        assert_eq!(command.route[0].kind, ir::RouteSlotKind::OpaqueToken);
        assert_eq!(command.route[1].name, "signed_token");
        assert_eq!(command.route[1].kind, ir::RouteSlotKind::SignedToken);
    }

    #[test]
    fn cap_file_auto_photo_policy_lowers() {
        let cap = parse_cap_file_type(
            "@cap.File(max_size:5mb,accept:image/jpeg,auto_photo_policy:@policy.host_only) optional",
        )
        .expect("cap file parses");
        assert_eq!(cap.auto_photo_policy.as_deref(), Some("@policy.host_only"));
    }

    #[test]
    fn type_ref_from_syntax_lowers_full_cap_file() {
        let ty =
            type_ref_from_syntax("@cap.File(max_size:25mb,accept:text/csv,visibility:private)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::File(file)) => {
                assert_eq!(file.max_size.bytes, 25 * 1024 * 1024);
                assert!(matches!(file.max_size.literal, ir::FileSizeLiteral::Mb(25)));
                assert_eq!(file.accept.len(), 1);
                assert_eq!(file.accept[0].family, "text");
                assert_eq!(file.accept[0].subtype, "csv");
                assert_eq!(file.visibility, Some(ir::FileVisibility::Private));
                assert!(file.signed_ttl.is_none());
            }
            other => panic!("expected Capability::File, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_multi_mime_cap_file() {
        let ty = type_ref_from_syntax(
            "@cap.File(max_size:100mb,accept:text/csv|application/vnd.ms-excel,visibility:signed,signed_ttl:1h)",
        );
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::File(file)) => {
                assert_eq!(file.accept.len(), 2);
                assert_eq!(file.accept[1].family, "application");
                assert_eq!(file.accept[1].subtype, "vnd.ms-excel");
                assert_eq!(file.visibility, Some(ir::FileVisibility::Signed));
                assert_eq!(file.signed_ttl.as_deref(), Some("1h"));
            }
            other => panic!("expected Capability::File, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_list_of_builtin() {
        let ty = type_ref_from_syntax("list of Text");
        match ty {
            ir::TypeRef::Many(inner) => {
                assert!(matches!(
                    *inner,
                    ir::TypeRef::Builtin(ir::BuiltinType::Text)
                ));
            }
            other => panic!("expected Many(Text), got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_list_of_user_defined_with_trailing_decorator() {
        let ty = type_ref_from_syntax("list of Post @client.visible");
        match ty {
            ir::TypeRef::Many(inner) => match *inner {
                ir::TypeRef::UserDefined(qname) => assert_eq!(qname.name, "Post"),
                other => panic!("expected Many(Post), got Many({other:?})"),
            },
            other => panic!("expected Many(Post), got {other:?}"),
        }
    }

    // Wave 0 (ir-returns-list-2026-05-22): `list <X>` (no "of") is the
    // canonical authoring form, parity with `api.output list of <X>`
    // and with pilots that commented-out `# returns list of <X>` blocks.
    #[test]
    fn type_ref_from_syntax_lowers_bare_list_builtin() {
        let ty = type_ref_from_syntax("list Text");
        match ty {
            ir::TypeRef::Many(inner) => {
                assert!(matches!(
                    *inner,
                    ir::TypeRef::Builtin(ir::BuiltinType::Text)
                ));
            }
            other => panic!("expected Many(Text), got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_bare_list_user_defined() {
        let ty = type_ref_from_syntax("list ReservationCard");
        match ty {
            ir::TypeRef::Many(inner) => match *inner {
                ir::TypeRef::UserDefined(qname) => assert_eq!(qname.name, "ReservationCard"),
                other => panic!("expected Many(UserDefined), got Many({other:?})"),
            },
            other => panic!("expected Many(...), got {other:?}"),
        }
    }

    // Case-insensitive `List <X>` parity with legacy `List of <X>`.
    #[test]
    fn type_ref_from_syntax_lowers_capital_list() {
        let ty = type_ref_from_syntax("List Post");
        match ty {
            ir::TypeRef::Many(inner) => match *inner {
                ir::TypeRef::UserDefined(qname) => assert_eq!(qname.name, "Post"),
                other => panic!("expected Many(Post), got Many({other:?})"),
            },
            other => panic!("expected Many(Post), got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_falls_through_when_cap_file_missing_max_size() {
        // No `max_size` arg → falls through to UserDefined so the LSP
        // shape diagnostic remains the canonical authority.
        let ty = type_ref_from_syntax("@cap.File(accept:text/csv)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_falls_through_when_cap_file_malformed_size() {
        // `25xy` is not a recognised size literal.
        let ty = type_ref_from_syntax("@cap.File(max_size:25xy,accept:text/csv)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_lifts_cap_hashed_argon2id() {
        // Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:argon2id)`
        // now lowers into `CapabilityRef::Hashed(...)`.
        let ty = type_ref_from_syntax("@cap.Hashed(algorithm:argon2id)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::Hashed(h)) => {
                assert_eq!(h.algorithm, ir::HashAlgorithm::Argon2id);
            }
            other => panic!("expected Capability::Hashed, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lifts_cap_token_typed() {
        let ty = type_ref_from_syntax("@cap.Token(ttl:24h,single_use:true,store:hashed)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::Token(t)) => {
                assert_eq!(t.ttl, "24h");
                assert!(t.single_use);
                assert_eq!(t.store, ir::TokenStore::Hashed);
            }
            other => panic!("expected Capability::Token, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_falls_through_on_unknown_hash_algorithm() {
        // Closed catalog: unknown algo falls through to UserDefined so
        // the LSP can surface a shape diagnostic.
        let ty = type_ref_from_syntax("@cap.Hashed(algorithm:scrypt)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_lifts_semantic_currency() {
        let ty = type_ref_from_syntax("@semantic.Currency");
        assert!(matches!(
            ty,
            ir::TypeRef::Builtin(ir::BuiltinType::SemanticCurrency)
        ));
    }

    #[test]
    fn lower_feature_without_auth_keeps_field_none() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(feature.auth.is_none());
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4a — `defaults` lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_feature_defaults_full_block() {
        let source = r#"
feature customer
  defaults
    tenancy org
    timestamps
    policy_for jobs, webhooks: @actor.system
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(matches!(feature.defaults.tenancy, Some(ir::Tenancy::Org)));
        assert!(feature.defaults.timestamps);
        match feature.defaults.policy.as_ref().expect("policy") {
            ir::PolicyRef::Atom(atom) => assert_eq!(atom, "actor.system"),
            other => panic!("expected @actor.system atom, got {other:?}"),
        }
    }

    #[test]
    fn lower_feature_defaults_absent_keeps_default() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(feature.defaults.tenancy.is_none());
        assert!(!feature.defaults.timestamps);
        assert!(feature.defaults.policy.is_none());
    }

    #[test]
    fn lower_feature_defaults_custom_tenancy() {
        let source = r#"
feature pinned
  defaults
    tenancy workspace
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        match feature.defaults.tenancy.as_ref().expect("axis") {
            ir::Tenancy::Custom(axis) => assert_eq!(axis, "workspace"),
            other => panic!("expected custom axis, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4c — `resource` lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_feature_resource_lifts_retention_and_derived() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
      score: Integer = 0
      is_high_value: Boolean derived from score > 80
      has_many notes: CustomerNote inverse customer

      soft_delete
      retention 7y then anonymize
      validates @validator.tier_check
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.resources.len(), 1);
        let r = &feature.resources[0];
        assert_eq!(r.name, "Customer");
        assert!(r.soft_delete);
        let ret = r.retention.as_ref().expect("retention");
        assert_eq!(ret.duration, "7y");
        assert!(matches!(ret.action, ir::RetentionAction::Anonymize));
        let derived = r
            .fields
            .iter()
            .find(|f| f.name == "is_high_value")
            .expect("is_high_value");
        assert_eq!(derived.derived_from.as_deref(), Some("score > 80"));
        // validates @validator.tier_check projects onto `Resource.validate`
        // for single-entry authoring.
        assert!(r.validate.is_some());
    }

    #[test]
    fn lower_registry_tool_entry_with_effect_and_pii_classes() {
        // Pin the IR shape for `RegistryToolEntry`. The actual
        // registry.lzi parser lands in a later phase; this test
        // documents the contract that doctor's
        // `tool_registry_effect_required_diagnostics` will read.
        let entry = ir::RegistryToolEntry {
            name: "web_search".to_owned(),
            effect: ir::ToolEffect::Read,
            pii_classes: vec![ir::QualifiedName {
                feature: None,
                name: "@pii.contact".to_owned(),
            }],
            adapter: Some(ir::QualifiedName {
                feature: None,
                name: "@adapter.serp".to_owned(),
            }),
            span_ref: None,
        };

        let serialized = serde_json::to_value(&entry).unwrap();
        assert_eq!(serialized["name"], "web_search");
        assert_eq!(serialized["effect"], "read");
        assert_eq!(serialized["pii_classes"][0]["name"], "@pii.contact");
        assert_eq!(serialized["adapter"]["name"], "@adapter.serp");
    }

    // -------------------------------------------------------------------------
    // L0 #2 — design tokens lowering tests.
    // -------------------------------------------------------------------------

    use lazuli_syntax::parse_design_document;

    use crate::lower_design;

    fn lower_design_source(source: &str) -> ir::Design {
        let ast = parse_design_document(source).expect("parses");
        lower_design(&ast).expect("lowers")
    }

    #[test]
    fn lower_design_lifts_flat_color_as_base_state() {
        let source = "
design example
  color
    success \"#16a34a\"
";
        let design = lower_design_source(source);
        assert_eq!(design.name, "example");
        assert!(design.extends.is_none());
        assert_eq!(design.colors.len(), 1);
        let success = &design.colors[0];
        assert_eq!(success.name, "success");
        assert_eq!(success.states.len(), 1);
        assert_eq!(success.states[0].kind, ir::ColorStateKind::Base);
        assert_eq!(success.states[0].value, "#16a34a");
    }

    #[test]
    fn lower_design_lifts_sub_block_color_states() {
        let source = "
design example
  color
    primary
      base \"#7c3aed\"
      hover \"#6d28d9\"
      active \"#5b21b6\"
      foreground \"#ffffff\"
";
        let design = lower_design_source(source);
        let primary = &design.colors[0];
        assert_eq!(primary.states.len(), 4);
        let kinds: Vec<ir::ColorStateKind> = primary.states.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ir::ColorStateKind::Base,
                ir::ColorStateKind::Hover,
                ir::ColorStateKind::Active,
                ir::ColorStateKind::Foreground,
            ]
        );
    }

    #[test]
    fn lower_design_preserves_dark_suffix() {
        let source = "
design example
  color
    background
      base \"#ffffff\" dark \"#09090b\"
";
        let design = lower_design_source(source);
        let bg = &design.colors[0];
        assert_eq!(bg.states[0].value, "#ffffff");
        assert_eq!(bg.states[0].dark.as_deref(), Some("#09090b"));
    }

    #[test]
    fn lower_design_extends_rejected_with_cut_b_code() {
        let source = "
design alpha
  extends base
  color
    primary
      base \"#10b981\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("DESIGN-EXTENDS-CUT-B"),
            "expected DESIGN-EXTENDS-CUT-B, got: {msg}"
        );
        assert!(matches!(err, AnalyzeError::DesignExtendsCutB { .. }));
    }

    #[test]
    fn lower_design_multi_layer_shadow_rejected() {
        let source = "
design example
  shadow
    elevated \"0 1px 2px 0 rgb(0 0 0 / 0.05), 0 4px 6px -1px rgb(0 0 0 / 0.1)\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("DESIGN-SHADOW-MULTI-LAYER"),
            "expected DESIGN-SHADOW-MULTI-LAYER, got: {msg}"
        );
        assert!(matches!(
            err,
            AnalyzeError::DesignShadowMultiLayer { ref name } if name == "elevated"
        ));
    }

    #[test]
    fn lower_design_single_layer_shadow_with_inner_commas_passes() {
        // Commas inside `rgb(...)` are inner; they do NOT trigger the
        // multi-layer rejection. The closed grammar accepts single-layer
        // shadows whose inner color uses `rgb(r, g, b)` notation.
        let source = "
design example
  shadow
    base \"0 1px 3px 0 rgb(0, 0, 0, 0.1)\"
";
        let design = lower_design_source(source);
        assert_eq!(design.shadows.len(), 1);
        assert_eq!(design.shadows[0].value, "0 1px 3px 0 rgb(0, 0, 0, 0.1)");
    }

    #[test]
    fn lower_design_typography_full_round_trip() {
        let source = "
design example
  typography
    family
      sans \"Inter, system-ui, sans-serif\"
    scale
      base size 1rem, line_height 1.5rem
    weight
      medium 500
      bold 700
    tracking
      tight -0.025em
";
        let design = lower_design_source(source);
        assert_eq!(design.typography.families[0].name, "sans");
        assert_eq!(
            design.typography.families[0].value,
            "Inter, system-ui, sans-serif"
        );
        assert_eq!(design.typography.scale[0].size, "1rem");
        assert_eq!(design.typography.scale[0].line_height, "1.5rem");
        // u16 parse.
        assert_eq!(design.typography.weights[0].value, 500);
        assert_eq!(design.typography.weights[1].value, 700);
        // Tracking preserves text including negative.
        assert_eq!(design.typography.tracking[0].value, "-0.025em");
    }

    #[test]
    fn lower_design_z_values_parsed_as_i32() {
        let source = "
design example
  z
    docked 10
    modal 1300
    toast 1500
";
        let design = lower_design_source(source);
        assert_eq!(design.z_indices.len(), 3);
        assert_eq!(design.z_indices[0].value, 10);
        assert_eq!(design.z_indices[1].value, 1300);
        assert_eq!(design.z_indices[2].value, 1500);
    }

    #[test]
    fn lower_design_rejects_invalid_hex() {
        let source = "
design example
  color
    bogus \"not-a-hex\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        assert!(
            matches!(err, AnalyzeError::DesignColorHexInvalid { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn lower_design_rejects_unknown_color_state() {
        // Construct AST directly (parser surface uses kind=String, so an
        // unknown identifier passes parse but should fail lowering).
        use lazuli_syntax::{
            ColorStateAst, ColorTokenAst, DesignDeclAst, MotionAst, Span, TypographyAst,
        };

        let ast = DesignDeclAst {
            name: "example".to_owned(),
            extends: None,
            colors: vec![ColorTokenAst {
                name: "primary".to_owned(),
                states: vec![ColorStateAst {
                    kind: "disabled".to_owned(),
                    value: "#7c3aed".to_owned(),
                    dark: None,
                }],
                span: Span::new(0, 1),
            }],
            typography: TypographyAst::default(),
            spaces: Vec::new(),
            radii: Vec::new(),
            shadows: Vec::new(),
            motion: MotionAst::default(),
            breakpoints: Vec::new(),
            z_indices: Vec::new(),
            custom: Vec::new(),
            span: Span::new(0, 1),
        };
        let err = lower_design(&ast).unwrap_err();
        assert!(matches!(
            err,
            AnalyzeError::DesignColorStateUnknown { ref token, ref state }
                if token == "primary" && state == "disabled"
        ));
    }

    #[test]
    fn lower_design_full_example_round_trip() {
        let source = "
design example
  color
    primary
      base \"#7c3aed\"
      hover \"#6d28d9\"
      foreground \"#ffffff\"
    success \"#16a34a\"

  typography
    family
      sans \"Inter, system-ui, sans-serif\"
    scale
      base size 1rem, line_height 1.5rem

  space
    \"1\" 0.25rem
    \"4\" 1rem

  radius
    sm 0.125rem

  shadow
    base \"0 1px 3px 0 rgb(0 0 0 / 0.1)\"

  motion
    duration
      fast 150ms
    easing
      out \"cubic-bezier(0, 0, 0.2, 1)\"

  breakpoint
    sm 640px

  z
    modal 1300
";
        let design = lower_design_source(source);
        // Every group has at least one entry.
        assert!(!design.colors.is_empty());
        assert!(!design.typography.families.is_empty());
        assert!(!design.typography.scale.is_empty());
        assert!(!design.spaces.is_empty());
        assert!(!design.radii.is_empty());
        assert!(!design.shadows.is_empty());
        assert!(!design.motion.durations.is_empty());
        assert!(!design.motion.easings.is_empty());
        assert!(!design.breakpoints.is_empty());
        assert!(!design.z_indices.is_empty());
        // SpanRef preserved.
        assert!(design.span_ref.is_some());
        // Serializes round-trip cleanly.
        let json = serde_json::to_value(&design).unwrap();
        assert_eq!(json["name"], "example");
        assert_eq!(json["colors"][0]["name"], "primary");
        // States serialize with snake_case kind.
        assert_eq!(json["colors"][0]["states"][0]["kind"], "base");
        // ColorStateKind serializes as snake_case.
        assert_eq!(json["colors"][0]["states"][2]["kind"], "foreground");
    }

    // ── Z2 — `custom` 9th meta-group lowering ──────────────────────────────

    #[test]
    fn lower_design_lifts_custom_group_with_base_and_dark() {
        let source = r##"
design hostpoint
  custom
    chat-bubble-mine "#dcf8c6" dark "#005c4b"
    chat-bubble-other "#ffffff"
    map-marker-active "#ff5722"
"##;
        let design = lower_design_source(source);
        assert_eq!(design.custom.len(), 3);
        assert_eq!(design.custom[0].name, "chat-bubble-mine");
        assert_eq!(design.custom[0].base, "#dcf8c6");
        assert_eq!(design.custom[0].dark.as_deref(), Some("#005c4b"));
        assert_eq!(design.custom[1].dark, None);
        assert_eq!(design.custom[2].name, "map-marker-active");
    }

    #[test]
    fn lower_design_preserves_invalid_custom_hex_for_doctor() {
        // Analyzer is intentionally permissive on `custom` hex values —
        // doctor's `design-custom-invalid-value` rule does the proposal-
        // pending validation. See `docs/proposals/design-tokens-custom.md` §4.
        let source = r##"
design hostpoint
  custom
    oops "not-a-color"
    chat-bubble "#dcf8c6" dark "rgb(5,5,5)"
"##;
        let design = lower_design_source(source);
        assert_eq!(design.custom.len(), 2);
        assert_eq!(design.custom[0].base, "not-a-color");
        assert_eq!(design.custom[1].dark.as_deref(), Some("rgb(5,5,5)"));
    }

    // -------------------------------------------------------------------------
    // IR Error-Vocab (Cell PARSE-1) — analyzer lowering round-trip tests
    // for the three new IR slots populated by this cell:
    //   * `Command.policy_when_denied` ← `command.policy.when_denied`
    //   * `PolicyCategory.when_denied` ← `policies.<cat>.when_denied`
    //   * `Feature.errors` ← `errors` block (default + 4xx/5xx + messages)
    // -------------------------------------------------------------------------

    #[test]
    fn lower_command_policy_when_denied_populates_typed_ref() {
        let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied @translation.choose_role_signin_required
    input
      role_id: ID required
    returns User
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let command = feature
            .commands
            .iter()
            .find(|c| c.name == "choose_role")
            .expect("choose_role command");
        let key = command
            .policy_when_denied
            .as_ref()
            .expect("policy_when_denied lowered");
        assert_eq!(key.key, "choose_role_signin_required");
    }

    #[test]
    fn lower_policy_category_when_denied_populates_typed_ref() {
        let source = r#"
feature account
  policies
    authenticated: @scope.authenticated
      when_denied @translation.must_be_signed_in
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let authenticated = feature
            .policies
            .categories
            .iter()
            .find(|c| c.name == "authenticated")
            .expect("authenticated category");
        let key = authenticated
            .when_denied
            .as_ref()
            .expect("when_denied lowered");
        assert_eq!(key.key, "must_be_signed_in");
    }

    #[test]
    fn lower_feature_errors_populates_typed_block() {
        let source = r#"
feature account
  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

    policy_denied message @translation.account_signin_required
    validation_failed message @translation.account_invalid_input
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let errors = feature.errors.as_ref().expect("errors block lowered");
        assert_eq!(errors.default, Some(ir::ErrorExposureDefault::Hide));
        assert_eq!(errors.exposure_4xx, vec!["message", "code"]);
        assert_eq!(errors.exposure_5xx, vec!["code"]);
        assert_eq!(errors.messages.len(), 2);
        let policy_denied = errors
            .messages
            .iter()
            .find(|m| m.code == "policy_denied")
            .expect("policy_denied row");
        assert_eq!(policy_denied.message.key, "account_signin_required");
        let validation = errors
            .messages
            .iter()
            .find(|m| m.code == "validation_failed")
            .expect("validation_failed row");
        assert_eq!(validation.message.key, "account_invalid_input");
        // v1 leaves field_messages empty (reserved slot — proposal §3.4).
        assert!(errors.field_messages.is_empty());
    }

    #[test]
    fn lower_feature_without_errors_block_keeps_field_none() {
        let source = r#"
feature account
  command choose_role
    input
      role_id: ID required
    returns User
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(
            feature.errors.is_none(),
            "feature without `errors` block keeps `errors: None`"
        );
    }

    #[test]
    fn lower_feature_errors_default_expose_lowers_correctly() {
        let source = r#"
feature account
  errors
    default expose
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let errors = feature.errors.as_ref().expect("errors block lowered");
        assert_eq!(errors.default, Some(ir::ErrorExposureDefault::Expose));
        assert!(errors.exposure_4xx.is_empty());
        assert!(errors.exposure_5xx.is_empty());
        assert!(errors.messages.is_empty());
    }

    #[test]
    fn lower_feature_errors_redact_patterns_lowers() {
        let source = r#"
feature account
  errors
    error_redact "[0-9]{11}"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let errors = feature.errors.as_ref().expect("errors block lowered");
        assert_eq!(errors.redact_patterns, vec!["[0-9]{11}".to_owned()]);
    }

    #[test]
    fn lower_feature_errors_audience_exposure_lowers() {
        let source = r#"
feature account
  errors
    expose to @audience operator message, code
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let errors = feature.errors.as_ref().expect("errors block lowered");
        let rule = errors.audience_exposure.first().expect("audience exposure");
        assert_eq!(rule.audience.as_deref(), Some("operator"));
        assert_eq!(rule.fields, vec!["message".to_owned(), "code".to_owned()]);
    }
}

mod surface_lowering_tests {
    use crate::{AnalyzeError, lower_surface};
    use lazuli_ir as ir;
    use lazuli_syntax::parse_surface_document;

    fn parse(src: &str) -> ir::Surface {
        let ast = parse_surface_document(src).expect("parses");
        lower_surface(&ast).expect("lowers")
    }

    fn parse_requires(atom: &str) -> ir::PolicyAtom {
        let source = format!("surface slug web\n  audience admin\n    requires {atom}\n");
        let surface = parse(&source);
        surface.audiences[0].requires[0].clone()
    }

    #[test]
    fn lowers_minimal_surface() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, ir::SurfaceTarget::Web);
        assert_eq!(surface.audiences.len(), 1);
        assert_eq!(surface.audiences[0].views.len(), 1);
    }

    #[test]
    fn session_fresh_policy_atom_lowers() {
        let atom = parse_requires("@session.fresh(15m)");
        assert_eq!(atom.namespace, "session");
        assert_eq!(atom.name, "fresh");
        assert_eq!(atom.args.as_deref(), Some("15m"));
    }

    #[test]
    fn rate_budget_policy_atom_lowers() {
        let atom = parse_requires("@rate_budget.password_reset");
        assert_eq!(atom.namespace, "rate_budget");
        assert_eq!(atom.name, "password_reset");
        assert!(atom.args.is_none());
    }

    #[test]
    fn time_policy_atom_lowers() {
        let atom = parse_requires("@time.business_hours_brasilia(tz:America/Sao_Paulo)");
        assert_eq!(atom.namespace, "time");
        assert_eq!(atom.name, "business_hours_brasilia");
        assert_eq!(atom.args.as_deref(), Some("tz:America/Sao_Paulo"));
    }

    #[test]
    fn view_redacted_fields_lower() {
        let surface = parse(
            "surface customer web\n  audience admin\n    view create invite\n      submit customer.command.invite\n      fields email redacted\n",
        );
        let ir::View::Create(view) = &surface.audiences[0].views[0] else {
            panic!("expected create view");
        };
        assert_eq!(view.fields, vec!["email".to_owned()]);
        assert_eq!(view.redacted_fields, vec!["email".to_owned()]);
    }

    #[test]
    fn list_view_lowers_table_render_search_and_legacy_filter_names() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key, title\n      search key\n      filter title\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.render,
            ir::ListRender::Table {
                columns: vec!["key".into(), "title".into()]
            }
        );
        assert_eq!(
            view.search.as_ref().map(|search| &search.mode),
            Some(&ir::SearchMode::Columns {
                columns: vec!["key".into()]
            })
        );
        assert_eq!(view.filter.len(), 1);
        assert_eq!(view.filter[0].name, "title");
    }

    #[test]
    fn list_view_lowers_cells_render() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list cards\n      source item.query.search\n      cells @client.item_card\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.render,
            ir::ListRender::Cells {
                slot: "item_card".into()
            }
        );
    }

    #[test]
    fn lowers_filter_decl_block_to_typed_ir() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from query\n        tags: list of Text\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filter.len(), 2);
        assert_eq!(view.filter[0].name, "slug");
        assert_eq!(view.filter[0].type_ref, "Text");
        assert_eq!(view.filter[0].cardinality, ir::FilterCardinality::Single);
        assert!(view.filter[0].url_sync);
        assert_eq!(view.filter[1].cardinality, ir::FilterCardinality::Multi);
    }

    #[test]
    fn lowers_segmented_search_decl_bindings() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      search segmented\n        field slug binds filters.slug\n        field q binds source.search\n        free text into selection\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, ir::SearchMode::Segmented);
        assert_eq!(
            search.fields[0].binds_to,
            ir::BindingRef::Filter {
                name: "slug".into()
            }
        );
        assert_eq!(
            search.fields[1].binds_to,
            ir::BindingRef::SourceInput {
                name: "search".into()
            }
        );
        assert_eq!(
            search.free_text_target,
            Some(ir::BindingRef::SelectionScalar)
        );
    }

    #[test]
    fn lowers_drawer_subview() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer item_detail on select\n        source item.query.by_id\n        route key from selection\n        sections header, meta\n        cells owner @client.owner_card\n        actions update\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let drawer = view.drawer.as_ref().expect("drawer");
        assert_eq!(drawer.name, "item_detail");
        assert_eq!(drawer.trigger, ir::DrawerTrigger::Select);
        assert_eq!(drawer.source.name, "by_id");
        assert_eq!(drawer.route_binding.as_ref().unwrap().target, "key");
        assert_eq!(drawer.sections, vec!["header", "meta"]);
        assert_eq!(drawer.cells[0].slot, "owner_card");
        assert_eq!(drawer.actions[0].name, "update");
    }

    #[test]
    fn lowers_sort_selection_and_settings() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title, updated\n        default updated desc\n      selection multi\n      bulk_actions delete\n      settings\n        grid_size: Enum [sm, md] default sm\n          persist local\n        page_size: Int min 10 max 200 default 25\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let sort = view.sort.as_ref().expect("sort");
        assert_eq!(sort.allowed, vec!["title", "updated"]);
        assert_eq!(sort.default_dir, ir::SortDir::Desc);
        let selection = view.selection.as_ref().expect("selection");
        assert_eq!(selection.mode, ir::SelectionMode::Multi);
        assert_eq!(selection.bulk_actions[0].name, "delete");
        assert_eq!(view.settings.len(), 2);
        assert_eq!(
            view.settings[0].value_space,
            ir::SettingValueSpace::Enum {
                values: vec!["sm".into(), "md".into()]
            }
        );
        assert_eq!(view.settings[0].persistence, ir::SettingPersistence::Local);
        assert_eq!(
            view.settings[1].value_space,
            ir::SettingValueSpace::Int { min: 10, max: 200 }
        );
    }

    #[test]
    fn detail_view_lifts_route_params_and_sections() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.by_key\n      route key: Text from path\n      sections header, metadata\n",
        );
        let detail = match &surface.audiences[0].views[0] {
            ir::View::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.route.as_deref(), Some("/s/:key"));
        assert_eq!(detail.route_params.len(), 1);
        assert_eq!(detail.route_params[0].name, "key");
        assert_eq!(detail.route_params[0].type_ref, "Text");
        assert_eq!(detail.sections, vec!["header", "metadata"]);
    }

    #[test]
    fn create_view_lifts_submit_command_and_fields() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view create n at \"/s/new\"\n      submit slug.command.create\n      fields key, title\n",
        );
        let create = match &surface.audiences[0].views[0] {
            ir::View::Create(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(create.submit.feature, "slug");
        assert_eq!(create.submit.name, "create");
        assert_eq!(create.fields, vec!["key", "title"]);
    }

    #[test]
    fn create_view_lifts_on_success_to_ir() {
        let surface = parse(
            "surface host web\n  audience admin\n    view create edit_host\n      submit host.command.update_host_basic_details\n      fields title\n      on_success\n        back\n        flash success @translation.saved\n        invalidates query.lookup_my_host\n",
        );
        let create = match &surface.audiences[0].views[0] {
            ir::View::Create(v) => v,
            _ => unreachable!(),
        };
        let on_success = create.on_success.as_ref().expect("on_success");
        assert!(on_success.back);
        let flash = on_success.flash.as_ref().expect("flash");
        assert_eq!(flash.kind, "success");
        assert_eq!(flash.message_key.key, "saved");
        assert_eq!(on_success.invalidates.len(), 1);
        assert_eq!(
            on_success.invalidates[0].query.feature.as_deref(),
            Some("host")
        );
        assert_eq!(on_success.invalidates[0].query.name, "lookup_my_host");
    }

    #[test]
    fn requires_lifts_to_policy_atom() {
        let surface = parse(
            "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let req = &surface.audiences[0].requires[0];
        assert_eq!(req.namespace, "scope");
        assert_eq!(req.name, "workspace_admin");
    }

    #[test]
    fn query_ref_disambiguates_kind_via_prefix() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.lookup.by_key\n      route key: Text from path\n",
        );
        let detail = match &surface.audiences[0].views[0] {
            ir::View::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.source.feature, "slug");
        assert_eq!(detail.source.kind, ir::QueryKind::Lookup);
        assert_eq!(detail.source.name, "by_key");
    }

    #[test]
    fn query_ref_unqualified_defaults_to_list() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.source.kind, ir::QueryKind::List);
        assert_eq!(view.source.name, "mine");
    }

    #[test]
    fn actions_short_form_lifts_owning_feature() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions create, update\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions.len(), 2);
        for action in &view.actions {
            assert_eq!(action.feature, "slug");
        }
        assert_eq!(view.actions[0].name, "create");
        assert_eq!(view.actions[1].name, "update");
    }

    #[test]
    fn actions_qualified_form_keeps_explicit_feature() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions other.command.archive\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions[0].feature, "other");
        assert_eq!(view.actions[0].name, "archive");
    }

    #[test]
    fn cell_binding_lifts_to_ir_cell_binding() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns tags\n      cells tags @client.type_badge\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn route_param_orphan_error() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.by_key\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(
            err,
            AnalyzeError::LzxRouteParamMissingBinding { .. }
        ));
    }

    #[test]
    fn route_param_extra_without_placeholder_error() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view detail d at \"/s/x\"\n      source slug.query.by_key\n      route key: Text from path\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxRouteParamOrphan { .. }));
    }

    #[test]
    fn cell_slot_orphan_when_field_not_in_columns() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key, title\n      cells tags @client.type_badge\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxCellSlotOrphan { .. }));
    }

    #[test]
    fn bad_query_ref_rejected_at_lowering() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view list a\n      source bogus_thing\n      columns key\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxBadQueryRef { .. }));
    }

    #[test]
    fn lowers_full_section_13_1_fixture() {
        // Smoke: the proposal §13.1 fixture lowers cleanly end-to-end.
        let surface = parse(include_str!("../tests/fixtures/slug_web_section_13_1.lzx"));
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.audiences.len(), 2);
        assert_eq!(surface.audiences[0].views.len(), 3);
        let admin_list = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(admin_list.cells[0].slot, "type_badge");
        assert_eq!(admin_list.actions.len(), 3);
    }

    #[test]
    fn mobile_target_lowers_to_mobile_variant() {
        let surface = parse(
            "surface item mobile\n  audience kiosk\n    view list a\n      source item.query.mine\n      columns key\n",
        );
        assert_eq!(surface.target, ir::SurfaceTarget::Mobile);
    }

    #[test]
    fn span_ref_attached_after_lowering() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        assert!(surface.span_ref.is_some());
        assert!(surface.audiences[0].span_ref.is_some());
    }

    #[test]
    fn audience_view_count_preserves_source_order() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list b\n      source slug.query.mine\n      columns key\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let names: Vec<&str> = surface.audiences[0]
            .views
            .iter()
            .map(|v| v.name())
            .collect();
        assert_eq!(names, vec!["b", "a"]);
    }
}

mod field_constraint_lowering_tests {
    use crate::AnalyzeError;
    use lazuli_syntax::parse_feature_skeletons;

    /// `length 120 min 100` — § 10.2 rejects `length + min`.
    #[test]
    fn length_plus_min_emits_constraint_conflict() {
        let source = r#"
feature post
  domain
    resource Post
      title: Text length 120 min 100
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::ConstraintConflict { field, combo }) => {
                assert_eq!(field, "title");
                assert_eq!(combo, "length+min");
            }
            other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
        }
    }

    /// `between 0 and 100 max 50` — §10.2 rejects `between + max`.
    #[test]
    fn between_plus_max_emits_constraint_conflict() {
        let source = r#"
feature score
  domain
    resource Score
      points: Integer between 0 and 100 max 50
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::ConstraintConflict { field, combo }) => {
                assert_eq!(field, "points");
                assert_eq!(combo, "between+max");
            }
            other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
        }
    }

    /// `in ["a", "b"] pattern "^a"` — §10.2 says use enum instead.
    #[test]
    fn in_plus_pattern_emits_constraint_conflict() {
        let source = r#"
feature acl
  domain
    resource Member
      role: Text in ["a", "b"] pattern "^a"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::ConstraintConflict { field, combo }) => {
                assert_eq!(field, "role");
                assert_eq!(combo, "in+pattern");
            }
            other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
        }
    }

    /// `Text required min 2 default ""` — §10.3 rejects empty default
    /// because the empty string has length 0 < 2.
    #[test]
    fn empty_default_violates_min_constraint() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text required min 2 = ""
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::DefaultViolatesConstraint { field, rule, .. }) => {
                assert_eq!(field, "handle");
                assert!(rule.starts_with("min="), "expected min rule, got {}", rule);
            }
            other => panic!("expected DefaultViolatesConstraint, got: {:?}", other.err()),
        }
    }

    /// Valid combination: `min N max M` (without between/length) passes.
    #[test]
    fn min_max_combination_passes_lowering() {
        let source = r#"
feature post
  domain
    resource Post
      title: Text required min 2 max 80
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = crate::lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.min, Some(2));
        assert_eq!(field.constraints.max, Some(80));
    }

    // -------------------------------------------------------------------------
    // Wave-B-CL4 — `inline_validator_range_invariant_001`
    // -------------------------------------------------------------------------

    /// `min 10 max 5` — N>M yields an empty domain.
    #[test]
    fn min_greater_than_max_emits_range_invariant() {
        let source = r#"
feature post
  domain
    resource Post
      score: Integer required min 10 max 5
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorRangeInvariant {
                field,
                rule,
                low,
                high,
            }) => {
                assert_eq!(field, "score");
                assert_eq!(rule, "min>max");
                assert_eq!(low, "10");
                assert_eq!(high, "5");
            }
            other => panic!(
                "expected InlineValidatorRangeInvariant, got: {:?}",
                other.err()
            ),
        }
    }

    /// `between 100 and 0` — A>B yields an empty domain.
    #[test]
    fn between_with_inverted_bounds_emits_range_invariant() {
        let source = r#"
feature score
  domain
    resource Score
      points: Integer required between 100 and 0
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorRangeInvariant {
                field,
                rule,
                low,
                high,
            }) => {
                assert_eq!(field, "points");
                assert_eq!(rule, "between");
                assert_eq!(low, "100");
                assert_eq!(high, "0");
            }
            other => panic!(
                "expected InlineValidatorRangeInvariant, got: {:?}",
                other.err()
            ),
        }
    }

    /// `min 5 max 5` — equal bounds are valid (single-value domain).
    #[test]
    fn min_equals_max_passes_range_invariant() {
        let source = r#"
feature post
  domain
    resource Post
      flag: Integer required min 5 max 5
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = crate::lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.min, Some(5));
        assert_eq!(field.constraints.max, Some(5));
    }

    // -------------------------------------------------------------------------
    // Wave-B-CL4 — `inline_validator_type_mismatch_001`
    // -------------------------------------------------------------------------

    /// `pattern "..."` on `Boolean` — §10.1 restricts `pattern` to Text.
    #[test]
    fn pattern_on_boolean_emits_type_mismatch() {
        let source = r#"
feature account
  domain
    resource Account
      enabled: Boolean pattern "^t"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorTypeMismatch {
                field,
                field_type,
                constraint,
                ..
            }) => {
                assert_eq!(field, "enabled");
                assert_eq!(field_type, "Boolean");
                assert_eq!(constraint, "pattern");
            }
            other => panic!(
                "expected InlineValidatorTypeMismatch, got: {:?}",
                other.err()
            ),
        }
    }

    /// `length N` on `Integer` — §10.1 restricts `length` to Text.
    #[test]
    fn length_on_integer_emits_type_mismatch() {
        let source = r#"
feature score
  domain
    resource Score
      points: Integer length 3
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorTypeMismatch {
                field,
                field_type,
                constraint,
                ..
            }) => {
                assert_eq!(field, "points");
                assert_eq!(field_type, "Integer");
                assert_eq!(constraint, "length");
            }
            other => panic!(
                "expected InlineValidatorTypeMismatch, got: {:?}",
                other.err()
            ),
        }
    }

    /// `between A and B` on `Text` — §10.1 restricts `between` to numerics.
    #[test]
    fn between_on_text_emits_type_mismatch() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text between 2 and 30
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorTypeMismatch {
                field,
                field_type,
                constraint,
                ..
            }) => {
                assert_eq!(field, "handle");
                assert_eq!(field_type, "Text");
                assert_eq!(constraint, "between");
            }
            other => panic!(
                "expected InlineValidatorTypeMismatch, got: {:?}",
                other.err()
            ),
        }
    }

    // -------------------------------------------------------------------------
    // Wave-B-CL4 — `inline_validator_pattern_compile_001`
    // -------------------------------------------------------------------------

    /// `pattern "[a"` — unbalanced character class.
    #[test]
    fn pattern_unbalanced_class_emits_compile_error() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "[a"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorPatternCompile {
                field,
                pattern,
                reason,
            }) => {
                assert_eq!(field, "handle");
                assert_eq!(pattern, "[a");
                assert!(reason.contains("unbalanced `[`"), "reason: {}", reason);
            }
            other => panic!(
                "expected InlineValidatorPatternCompile, got: {:?}",
                other.err()
            ),
        }
    }

    /// `pattern "^a("` — unbalanced group paren.
    #[test]
    fn pattern_unbalanced_paren_emits_compile_error() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^a("
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorPatternCompile {
                field,
                pattern,
                reason,
            }) => {
                assert_eq!(field, "handle");
                assert_eq!(pattern, "^a(");
                assert!(reason.contains("unbalanced `(`"), "reason: {}", reason);
            }
            other => panic!(
                "expected InlineValidatorPatternCompile, got: {:?}",
                other.err()
            ),
        }
    }

    /// `pattern "^a)"` — extra closing paren, no matching `(`.
    #[test]
    fn pattern_extra_closing_paren_emits_compile_error() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^a)"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = crate::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorPatternCompile {
                field,
                pattern,
                reason,
            }) => {
                assert_eq!(field, "handle");
                assert_eq!(pattern, "^a)");
                assert!(reason.contains("unbalanced `)`"), "reason: {}", reason);
            }
            other => panic!(
                "expected InlineValidatorPatternCompile, got: {:?}",
                other.err()
            ),
        }
    }

    /// Sanity: well-formed pattern passes.
    #[test]
    fn pattern_well_formed_passes() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^[a-z][a-z0-9-]{2,29}$"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = crate::lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(
            field.constraints.pattern.as_deref(),
            Some("^[a-z][a-z0-9-]{2,29}$")
        );
    }

    // -------------------------------------------------------------------------
    // Cross-feature contracts §5.4 — lowering of `uses [<feature>...] [version v<N>]`
    // populates parallel `uses` / `uses_spans` / `uses_versions` lists.
    // -------------------------------------------------------------------------

    #[test]
    fn lowers_uses_with_mixed_pins() {
        let source = r#"
feature billing
  uses account version v2
  uses notifications
  uses org, user version v1
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = crate::lower_feature_skeleton(&features[0]).expect("lowers");

        assert_eq!(
            feature.uses,
            vec![
                "account".to_owned(),
                "notifications".to_owned(),
                "org".to_owned(),
                "user".to_owned(),
            ]
        );
        assert_eq!(feature.uses_versions, vec![Some(2), None, Some(1), Some(1)]);
        assert_eq!(feature.uses_spans.len(), 4);
        // First two lines and last line have distinct spans.
        assert_ne!(feature.uses_spans[0], feature.uses_spans[1]);
        assert_ne!(feature.uses_spans[1], feature.uses_spans[2]);
        // Comma-list entries share the source line, hence the span.
        assert_eq!(feature.uses_spans[2], feature.uses_spans[3]);
    }

    #[test]
    fn auto_photo_synthesizes_4_commands_and_2_records() {
        // Inline a minimal feature skeleton with a per-user resource
        // carrying an optional @cap.File field. Expect synthesis to
        // populate feature.commands with 4 names ending in
        // _upload/_upload/_/url and feature.records with the 2
        // intent + display records.
        let source = r#"
feature photoshare
  defaults
    tenancy org

  uses org
  uses account

  policies
    photoshare_only: @scope.authenticated, @role.host
      when_denied @translation.x

  domain
    resource PhotoShare
      org: Org required
      user: User required unique
      avatar: @cap.File(max_size:5mb,accept:image/jpeg,visibility:signed,signed_ttl:1h) optional
      created_at: DateTime required
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = crate::lower_feature_skeleton(&features[0]).expect("lowering succeeds");

        let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
        assert!(
            cmd_names.contains(&"request_avatar_upload"),
            "request_avatar_upload missing; got {:?}",
            cmd_names
        );
        assert!(cmd_names.contains(&"confirm_avatar_upload"));
        assert!(cmd_names.contains(&"clear_avatar"));
        assert!(cmd_names.contains(&"get_avatar_url"));

        let record_names: Vec<&str> = feature.records.iter().map(|r| r.name.as_str()).collect();
        assert!(record_names.contains(&"AvatarUploadIntent"));
        assert!(record_names.contains(&"AvatarDisplayUrl"));

        // Marker must be set on synthesized commands.
        let req = feature
            .commands
            .iter()
            .find(|c| c.name == "request_avatar_upload")
            .unwrap();
        assert!(req.synthesized_from_cap_file.is_some());
    }
}

mod conventions_unknown_diagnostic_tests {
    //! ir-resource-conventions-crud Cell C1 — tests for the
    //! `conventions_unknown` diagnostic plumbing. Cell C2 (parser)
    //! will be the actual emit site; here we lock the suggestion
    //! helper + the error formatting so the parser's emission shape
    //! is stable before it lands.

    use crate::{AnalyzeError, CONVENTION_CATALOG, conventions_unknown_suggestion};

    #[test]
    fn catalog_contains_crud_and_me_today() {
        // crud §4.2 + me §4.2 — closed catalog is `{ crud, me }`.
        // Any further addition is an IR change requiring a proposal;
        // this test fails on accidental growth.
        assert_eq!(CONVENTION_CATALOG, &["crud", "me"]);
    }

    #[test]
    fn suggestion_for_single_char_typo_returns_crud() {
        // §4.3 names this exact case verbatim: `conventions [crd]`
        // suggests `crud` (single-character Levenshtein).
        assert_eq!(conventions_unknown_suggestion("crd"), Some("crud"));
    }

    #[test]
    fn suggestion_for_extra_char_typo_returns_crud() {
        // `crude` and `cruds` are also distance-1 from `crud`.
        assert_eq!(conventions_unknown_suggestion("crude"), Some("crud"));
        assert_eq!(conventions_unknown_suggestion("cruds"), Some("crud"));
    }

    #[test]
    fn suggestion_for_typo_resolves_to_me() {
        // `ir-resource-conventions-me.md` cell M1: typos distance-1
        // from `me` resolve to `me`. `m` (deletion), `mee`/`mes`
        // (insertion / substitution). Locks the nearest-match
        // behaviour now that the catalog has a second entry.
        assert_eq!(conventions_unknown_suggestion("m"), Some("me"));
        assert_eq!(conventions_unknown_suggestion("mee"), Some("me"));
        assert_eq!(conventions_unknown_suggestion("mes"), Some("me"));
    }

    #[test]
    fn suggestion_for_far_typo_returns_none() {
        // Distance 2+ from every catalog entry — no suggestion is
        // better than a misleading one.
        assert_eq!(conventions_unknown_suggestion("workflow"), None);
        assert_eq!(conventions_unknown_suggestion("xyz"), None);
        assert_eq!(conventions_unknown_suggestion(""), None);
    }

    #[test]
    fn suggestion_for_exact_match_returns_self() {
        // Defensive: if the parser somehow calls this with a known
        // identifier, the helper still resolves rather than failing.
        // (The parser shouldn't reach this path — exact matches don't
        // hit the unknown diagnostic — but the helper is total.)
        assert_eq!(conventions_unknown_suggestion("crud"), Some("crud"));
    }

    #[test]
    fn error_message_includes_suggestion_when_present() {
        let err = AnalyzeError::ConventionsUnknown {
            resource: "Customer".to_owned(),
            identifier: "crd".to_owned(),
            suggestion: Some("crud".to_owned()),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("CONVENTIONS-UNKNOWN"),
            "missing diagnostic code: {msg}"
        );
        assert!(msg.contains("`Customer`"), "missing resource name: {msg}");
        assert!(msg.contains("`crd`"), "missing offending identifier: {msg}");
        assert!(
            msg.contains("did you mean `crud`?"),
            "missing suggestion clause: {msg}"
        );
    }

    #[test]
    fn error_message_omits_suggestion_clause_when_none() {
        let err = AnalyzeError::ConventionsUnknown {
            resource: "Customer".to_owned(),
            identifier: "workflow".to_owned(),
            suggestion: None,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("CONVENTIONS-UNKNOWN"),
            "missing diagnostic code: {msg}"
        );
        assert!(msg.contains("`workflow`"));
        assert!(
            !msg.contains("did you mean"),
            "should not invent a suggestion when none was found: {msg}"
        );
    }
}

mod conventions_crud_synth_tests {
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
        let mut query = crate::build_list_query("list_customers", "Customer");
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
}

mod conventions_me_synth_tests {
    use crate::{ConventionSynthDiagnostic, synthesize_conventions};
    use lazuli_ir as ir;

    /// Minimal `Feature` with a single `authenticated` policy.
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

    fn user_qn(name: &str) -> ir::TypeRef {
        ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        })
    }

    /// Build a minimal Resource with `conventions [me]`.
    fn me_resource(name: &str, fields: Vec<ir::Field>) -> ir::Resource {
        ir::Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields,
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
            conventions: vec![ir::ConventionRef::Me],
            lifecycle_routes: None,
        }
    }

    /// me §5.3 row 1 — `user_keyed`: resource has `user: User required
    /// unique` + `org: Org required`. Emits SELECT with
    /// `WHERE org = ctx.User.OrgID AND "user" = ctx.User.ID`.
    #[test]
    fn user_keyed_mode_emits_org_and_user_key_clauses() {
        let mut feature = empty_feature("host");
        feature.resources.push(me_resource(
            "Host",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

        let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
        assert_eq!(q_names, vec!["lookup_my_host"]);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_host")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                // Route-less + param-less per §5.2.
                assert!(
                    lq.params.is_empty(),
                    "expected no params, got {:?}",
                    lq.params
                );
                // Two key clauses: org + user.
                assert_eq!(lq.keys.len(), 2);
                assert_eq!(lq.keys[0].path.segments, vec!["org".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "org_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path for org, got {:?}", other),
                }
                assert_eq!(lq.keys[1].path.segments, vec!["user".to_owned()]);
                match &lq.keys[1].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path for user, got {:?}", other),
                }
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }

        // §11 inspect surface — synth_origins records Synthesized(Me).
        assert_eq!(
            feature.synth_origins.get("lookup_my_host"),
            Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me))
        );
    }

    /// me §5.3 row 2 — `user_keyed_no_org`: `user: User required` and
    /// no `org` field. Emits SELECT with `WHERE "user" = ctx.User.ID`.
    #[test]
    fn user_keyed_no_org_mode_emits_user_only_key_clause() {
        let mut feature = empty_feature("profile");
        feature.resources.push(me_resource(
            "Profile",
            vec![
                req_unique_field("user", user_qn("User")),
                req_field("bio", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_profile")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                // Single key clause on `user`.
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["user".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

    /// me §5.3 row 3 — `org_keyed`: resource has `org: Org required`
    /// AND no `user: User required` field. Emits SELECT with
    /// `WHERE org_id = ctx.User.OrgID`.
    #[test]
    fn org_keyed_mode_emits_org_only_key_clause() {
        let mut feature = empty_feature("settings");
        feature.resources.push(me_resource(
            "OrgSettings",
            vec![
                req_field("org", user_qn("Org")),
                req_field("theme", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_org_settings")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["org".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "org_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

    /// me §5.3 row 4 — `self_keyed`: the resource IS the User table.
    /// Emits SELECT with `WHERE id = ctx.User.ID`.
    #[test]
    fn self_keyed_mode_emits_id_key_clause_for_user_resource() {
        let mut feature = empty_feature("account");
        // resource User — no `user` field needed; the row IS the actor.
        feature.resources.push(me_resource(
            "User",
            vec![
                req_unique_field(
                    "email",
                    ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
                ),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_user")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["id".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

    /// me §6 — author wrote `query lookup_my_customer`; synth skips
    /// that name, records `AuthorOverride(Me)` in `synth_origins`. No
    /// duplicate query, no diagnostic when the signature matches.
    #[test]
    fn author_override_skips_synth_and_records_origin() {
        let mut feature = empty_feature("customer");
        feature.resources.push(me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        // Author wrote their own `lookup_my_customer` query (e.g.,
        // with a role-gated policy) — canonical-matching shape (no
        // params, Lookup variant).
        feature.queries.push(ir::Query::Lookup(ir::LookupQuery {
            name: "lookup_my_customer".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: ir::PolicyRef::Local("customer_admin".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "expected no diagnostics for matching override, got {:?}",
            diags
        );

        // Exactly one `lookup_my_customer` — the author's.
        let count = feature
            .queries
            .iter()
            .filter(|q| q.name() == "lookup_my_customer")
            .count();
        assert_eq!(count, 1);

        // Author's policy preserved (not overwritten by synth).
        let q = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_customer")
            .unwrap();
        match q {
            ir::Query::Lookup(lq) => {
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "customer_admin"));
            }
            other => panic!("expected Lookup, got {:?}", other),
        }

        // §11 — synth_origins records `AuthorOverride(Me)`.
        assert_eq!(
            feature.synth_origins.get("lookup_my_customer"),
            Some(&ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me))
        );
    }

    /// me §6.1 — `conventions [crud, me]` composes cleanly: 5 from
    /// crud + 1 from me = 6 entries, no naming collisions. All 6
    /// names appear in `synth_origins`.
    #[test]
    fn conventions_crud_and_me_compose_to_six_entries() {
        let mut feature = empty_feature("customer");
        let mut r = me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        );
        // Declare both bundles.
        r.conventions = vec![ir::ConventionRef::Crud, ir::ConventionRef::Me];
        feature.resources.push(r);

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        // 3 crud commands + 0 me commands.
        let cmd_names: std::collections::BTreeSet<String> =
            feature.commands.iter().map(|c| c.name.clone()).collect();
        assert!(cmd_names.contains("create_customer"));
        assert!(cmd_names.contains("update_customer"));
        assert!(cmd_names.contains("delete_customer"));
        assert_eq!(cmd_names.len(), 3, "got commands: {:?}", cmd_names);

        // 2 crud queries + 1 me query.
        let q_names: std::collections::BTreeSet<String> = feature
            .queries
            .iter()
            .map(|q| q.name().to_owned())
            .collect();
        assert!(q_names.contains("lookup_customer"));
        assert!(q_names.contains("list_customers"));
        assert!(q_names.contains("lookup_my_customer"));
        assert_eq!(q_names.len(), 3, "got queries: {:?}", q_names);

        // §11 inspect — synth_origins has 6 entries: 5 crud + 1 me.
        assert_eq!(
            feature.synth_origins.len(),
            6,
            "expected 6 synth_origins entries, got {:?}",
            feature.synth_origins
        );
        // Spot-check the 5 crud entries.
        for name in [
            "create_customer",
            "update_customer",
            "delete_customer",
            "lookup_customer",
            "list_customers",
        ] {
            assert_eq!(
                feature.synth_origins.get(name),
                Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud)),
                "expected Synthesized(Crud) for `{}`",
                name
            );
        }
        // And the 1 me entry.
        assert_eq!(
            feature.synth_origins.get("lookup_my_customer"),
            Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me))
        );
    }

    /// me §11.1 — `me_synth_no_actor_resolution` fires when the
    /// resource has neither `user` nor `org` and is not named `User`.
    /// No synth emitted for that resource.
    #[test]
    fn no_actor_resolution_diagnostic_when_no_user_no_org_not_user() {
        let mut feature = empty_feature("audit");
        feature.resources.push(me_resource(
            "AuditNote",
            vec![req_field(
                "note",
                ir::TypeRef::Builtin(ir::BuiltinType::Text),
            )],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::MeNoActorResolution { resource }
                    if resource == "AuditNote"
            )),
            "expected MeNoActorResolution for AuditNote, got {:?}",
            diags
        );

        // No `lookup_my_audit_note` synthesized.
        assert!(
            feature
                .queries
                .iter()
                .all(|q| q.name() != "lookup_my_audit_note"),
            "synth should skip the resource entirely on no actor axis"
        );
        // No entry in synth_origins.
        assert!(!feature.synth_origins.contains_key("lookup_my_audit_note"));
    }

    /// me §11.1 — `me_synth_signature_mismatch` fires when the author
    /// wrote a divergent shape (e.g., a `Query::List` named
    /// `lookup_my_<r>`; or a Lookup with non-empty params).
    #[test]
    fn divergent_author_signature_emits_mismatch_diagnostic() {
        let mut feature = empty_feature("traveler");
        feature.resources.push(me_resource(
            "Traveler",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
            ],
        ));

        // Author wrote a Lookup with non-empty params — diverges from
        // the canonical route-less + param-less shape.
        feature.queries.push(ir::Query::Lookup(ir::LookupQuery {
            name: "lookup_my_traveler".to_owned(),
            public_contract: None,
            params: vec![ir::TypedSlot {
                name: "extra".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            }],
            keys: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::MeSignatureMismatch { resource, synth_name, .. }
                    if resource == "Traveler" && synth_name == "lookup_my_traveler"
            )),
            "expected MeSignatureMismatch for lookup_my_traveler, got {:?}",
            diags
        );

        // §6 — synth still records AuthorOverride(Me) so inspect can
        // render the override annotation.
        assert_eq!(
            feature.synth_origins.get("lookup_my_traveler"),
            Some(&ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me))
        );
    }

    /// Sanity — resource without `conventions [me]` is a no-op for the
    /// `me` half of the synth (existing crud-no-op test covers the
    /// joint path; this one anchors the bundle-isolation property).
    #[test]
    fn resource_without_me_convention_is_no_op() {
        let mut feature = empty_feature("customer");
        let mut r = me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
            ],
        );
        r.conventions = Vec::new();
        feature.resources.push(r);

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty());
        assert!(feature.queries.is_empty());
        assert!(feature.synth_origins.is_empty());
    }
}

mod conventions_owner_scope_synth_tests {
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
}
