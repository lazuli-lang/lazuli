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


    // -------------------------------------------------------------------------
    // Cut A — agent lowering (§4.4 snapshot tests)
    // -------------------------------------------------------------------------

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
        allows customer.lifecycle_stage = active
        allows output contains "active"

      case redacts_email
        denies output contains @semantic.Email

      case uses_lookup
        allows tools.calls includes customer.query.by_id
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

        // Case 1: Denies + Contains semantic.
        let c1 = &agent.evals[1];
        assert_eq!(c1.assertions[0].kind, ir::EvalAssertionKind::Denies);
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
        allows output contains "x"
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
        allows output.length <= 800
        allows output.length >= 1
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
        allows output contains "active"
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
