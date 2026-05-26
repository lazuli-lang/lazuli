    // Inspect-CLI agent / tools / expose / events summary tests — split
    // from `crates/lazuli_cli/src/tests.rs`.

    use std::path::Path;

    use crate::{ExpandSet, inspect_canonical_source, parse_expand_set};

    #[test]
    fn inspect_summary_includes_agent_tools_evals_output_kind() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: ID required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 1
    prompt "./p.md"
    tools
      customer.query.lookup.by_id
      @tool.web_search
    evals
      case mentions_status
        requires output contains "active"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        // Agents are emitted regardless of expansion (always-on field).
        assert!(json.contains("\"name\":\"summarize\""));
        // tools[] now picks up indent-6 entries (canonical block form).
        assert!(
            json.contains("\"tools\":[\"customer.query.lookup.by_id\",\"@tool.web_search\"]"),
            "expected tools list in agent: {json}"
        );
        // evals[] carries the case names.
        assert!(
            json.contains("\"evals\":[\"mentions_status\"]"),
            "expected evals list in agent: {json}"
        );
        // output_kind + output_discriminator surface the discriminator
        // form.
        assert!(
            json.contains("\"output_kind\":\"discriminated_enum\""),
            "expected output_kind discriminated_enum: {json}"
        );
        assert!(
            json.contains("\"output_discriminator\":\"Intent\""),
            "expected output_discriminator Intent: {json}"
        );
        // eval_determinism is `pinned` because temperature 0 + seed 1.
        assert!(
            json.contains("\"eval_determinism\":\"pinned\""),
            "expected eval_determinism pinned: {json}"
        );
    }

    #[test]
    fn inspect_summary_marks_nondeterministic_eval_block() {
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
      case smoke
        requires output contains "ok"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"eval_determinism\":\"nondeterministic\""),
            "expected eval_determinism nondeterministic: {json}"
        );
        assert!(
            json.contains("\"output_kind\":\"stream\""),
            "expected output_kind stream: {json}"
        );
    }

    #[test]
    fn inspect_tools_projection_emits_per_agent_dispatch_graph() {
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
      query.lookup.by_id
      customer.command.archive
      @tool.web_search
"#;
        let mut expansions = ExpandSet::default();
        expansions.tools = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        // The new --expand=tools projection populates `features[].tools`.
        assert!(
            json.contains("\"agent\":\"triage\""),
            "expected agent entry: {json}"
        );
        // Local query.lookup categorised correctly.
        assert!(
            json.contains("\"reference\":\"query.lookup.by_id\",\"kind\":\"query.lookup\",\"scope\":\"local\",\"derived_effect\":\"read\""),
            "expected local query.lookup binding: {json}"
        );
        // Cross-feature command writes.
        assert!(
            json.contains("\"reference\":\"customer.command.archive\",\"kind\":\"command\",\"scope\":\"cross_feature\",\"derived_effect\":\"write\""),
            "expected cross-feature command binding: {json}"
        );
        // Adapter tool with unknown effect (registry resolves in doctor).
        assert!(
            json.contains("\"reference\":\"@tool.web_search\",\"kind\":\"adapter\",\"scope\":\"adapter\",\"derived_effect\":\"unknown\""),
            "expected adapter binding: {json}"
        );
    }

    #[test]
    fn inspect_expand_events_includes_built_in_trace_events() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let mut expansions = ExpandSet::default();
        expansions.events = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"built_in_trace_events\":[{\"name\":\"agent_run\""),
            "expected built_in_trace_events with agent_run: {json}"
        );
        assert!(
            json.contains("\"fires_per\":\"agent_dispatch\""),
            "expected fires_per agent_dispatch: {json}"
        );
        assert!(
            json.contains("\"name\":\"tokens_total\",\"type\":\"Integer\""),
            "expected canonical payload field tokens_total: {json}"
        );
    }

    #[test]
    fn inspect_built_in_trace_events_omitted_without_events_expand() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("built_in_trace_events"),
            "built_in_trace_events must be omitted without --expand=events: {json}"
        );
    }

    #[test]
    fn inspect_expand_expose_flag_parses() {
        let expansions = parse_expand_set("expose").unwrap();
        assert!(expansions.expose);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_summary_includes_agent_expose_http() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:id/summary"
      route id: Customer.ID
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"expose_http\":{\"method\":\"POST\""),
            "expected expose_http always-on summary: {json}"
        );
        assert!(json.contains("\"path\":\"/api/customers/:id/summary\""));
    }

    #[test]
    fn inspect_expose_projection_emits_unified_route_table() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:id/summary"
      route id: Customer.ID

  api list_customers
    method GET
    path "/api/customers"
    handler "./api/list.go"
"#;
        let mut expansions = ExpandSet::default();
        expansions.expose = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        assert!(
            json.contains("\"kind\":\"agent\",\"origin\":\"customer.agent.summarize\""),
            "expected agent expose entry: {json}"
        );
        assert!(
            json.contains("\"kind\":\"api\",\"origin\":\"customer.api.list_customers\""),
            "expected api expose entry: {json}"
        );
    }

    #[test]
    fn inspect_expose_projection_omitted_without_expand() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"origin\":\"customer.agent.summarize\""),
            "expose projection must be omitted without --expand=expose: {json}"
        );
    }

    #[test]
    fn inspect_tools_projection_omitted_without_expand() {
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
      query.lookup.by_id
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        // Without --expand=tools the new projection is omitted (skipped
        // by `Option::is_none`). The agent's plain tools list is still
        // emitted as part of the always-on agents block.
        assert!(
            !json.contains("\"reference\":\"query.lookup.by_id\""),
            "tools projection should not appear without --expand=tools: {json}"
        );
        assert!(
            json.contains("\"tools\":[\"query.lookup.by_id\"]"),
            "agent.tools list should still be present: {json}"
        );
    }
