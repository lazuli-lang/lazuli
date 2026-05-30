//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

#[test]
fn agent_tools_accepts_canonical_block() {
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
      customer.query.lookup.by_id
      query.by_id
      command.archive
      @tool.web_search
      @tool.calendar.create_event
"#;
    let diagnostics = diagnostics_for(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        !codes.iter().any(|c| c == "agent_tools_diagnostics"),
        "canonical tool block should not produce agent_tools_diagnostics; got: {codes:?}"
    );
}

#[test]
fn agent_rejects_non_llm_model_reference() {
    let source = r#"
feature customer
  agent summarize_customer
    input
      prompt: Text required
    policy @policy.read
    output stream Text
    model gpt-4
    prompt "./prompts/summarize_customer.md"
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("must be a `@llm.<name>` reference"))
    );
}

#[test]
fn agent_tools_rejects_unknown_kind_segment() {
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
      customer.script.run_unsafe
"#;
    let diagnostics = diagnostics_for(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes.iter().any(|c| c == "agent_tools_diagnostics"),
        "expected agent_tools_diagnostics for unknown kind; got: {codes:?}"
    );
}

#[test]
fn agent_tools_rejects_empty_segment() {
    // `customer..by_id` has an empty segment — must be rejected.
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
      customer..by_id
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "agent_tools_diagnostics"),
        "expected agent_tools_diagnostics for empty segment"
    );
}

#[test]
fn agent_evals_accepts_case_with_requires_forbids() {
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
      case redacts_email
        allows customer.email = "ada@example.com"
        denies output contains @semantic.Email
"#;
    let diagnostics = diagnostics_for(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        !codes.iter().any(|c| c == "agent_evals_diagnostics"),
        "canonical evals block should not produce agent_evals_diagnostics; got: {codes:?}"
    );
    assert!(
        !codes.iter().any(|c| c == "eval_nondeterministic_warning"),
        "agent pinned at temperature 0 + seed 1 must not warn nondeterministic; got: {codes:?}"
    );
}

#[test]
fn agent_evals_rejects_given_expect_legacy_vocabulary() {
    let source = r#"
feature customer
  agent legacy
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      given a_case
        expect output contains "ok"
"#;
    let diagnostics = diagnostics_for(source);
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("`given` is legacy")),
        "expected `given` legacy diagnostic; got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("`expect` is legacy")),
        "expected `expect` legacy diagnostic; got: {messages:?}"
    );
}

#[test]
fn agent_discriminator_rejects_when_marker_outside_record() {
    // Field `tag: Status discriminator` declared inside `agent
    // input` instead of a record — must be rejected.
    let source = r#"
feature customer
  agent classify
    input
      message: Text required
      tag: Status discriminator
    policy @policy.read
    output discriminator Intent
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "agent_discriminator_diagnostics"),
        "expected agent_discriminator_diagnostics when marker appears outside record"
    );
}

#[test]
fn agent_evals_warns_without_temperature_zero_seed() {
    // Agent has an evals block but `temperature 0.7` (non-zero) and
    // no `seed` — must emit `eval_nondeterministic_warning`.
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
        allows output contains "ok"
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "eval_nondeterministic_warning"),
        "expected eval_nondeterministic_warning"
    );
}

#[test]
fn agent_expose_local_path_conflict_caught() {
    // Two agents in the same file declare the same (method, path).
    let source = r#"
feature customer
  agent first
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:id"
      route id: ID

  agent second
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./q.md"
    expose http
      method POST
      path "/api/x/:other"
      route other: ID
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "agent_expose_path_conflict_local_diagnostics"),
        "expected local path conflict; got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn agent_expose_slot_unbound_caught() {
    let source = r#"
feature customer
  agent broken
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:customer_id"
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "agent_expose_slot_unbound_diagnostics"),
        "expected slot_unbound"
    );
}

#[test]
fn agent_expose_slot_must_use_route_caught_with_input_slot_collision() {
    let source = r#"
feature customer
  agent broken
    input
      customer_id: ID required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:customer_id"
"#;
    let diagnostics = diagnostics_for(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes
            .iter()
            .any(|c| c == "agent_expose_slot_must_use_route_diagnostics"),
        "expected slot_must_use_route; got: {codes:?}"
    );
}

#[test]
fn agent_expose_method_get_streaming_warns() {
    let source = r#"
feature customer
  agent flaky
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method GET
      path "/api/customers/:id/summary"
      route id: ID
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "agent_expose_method_streaming_mismatch_warning"),
        "expected method/streaming warning"
    );
}

#[test]
fn agent_expose_well_formed_emits_nothing() {
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
"#;
    let diagnostics = diagnostics_for(source);
    let codes = diagnostic_codes(&diagnostics);
    for code in [
        "agent_expose_path_conflict_local_diagnostics",
        "agent_expose_slot_unbound_diagnostics",
        "agent_expose_slot_must_use_route_diagnostics",
        "agent_expose_method_streaming_mismatch_warning",
    ] {
        assert!(
            !codes.iter().any(|c| c == code),
            "well-formed expose should not produce {code}; got: {codes:?}"
        );
    }
}

#[test]
fn cors_rejects_unknown_child() {
    let source = r#"
app MyApp
  cors
    allow_methods GET, POST
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "cors_contract_diagnostics"),
        "expected cors_contract_diagnostics for unknown child `allow_methods`"
    );
}

#[test]
fn cors_rejects_allow_origins_without_origins() {
    let source = r#"
app MyApp
  cors
    allow_origins production
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "cors_contract_diagnostics"),
        "expected cors_contract_diagnostics for missing origins"
    );
}

#[test]
fn cors_rejects_invalid_allow_credentials() {
    let source = r#"
app MyApp
  cors
    allow_credentials yes
"#;
    let diagnostics = diagnostics_for(source);
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("allow_credentials yes")),
        "expected diagnostic about invalid allow_credentials value; got {messages:?}"
    );
}

#[test]
fn cors_well_formed_emits_nothing() {
    let source = r#"
app MyApp
  cors
    allow_origins production "https://app.example.com", "https://*.example.com"
    allow_origins local "*"
    allow_credentials true
    max_age "1h"
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        !diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "cors_contract_diagnostics"),
        "well-formed cors must not produce cors_contract_diagnostics"
    );
}

#[test]
fn approval_rejects_missing_required_children() {
    let source = r#"
feature customer
  command archive
    approval
      by @role.admin
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "approval_contract_diagnostics"),
        "expected approval_contract_diagnostics for missing timeout/then"
    );
}

#[test]
fn approval_rejects_unknown_then_action() {
    // F2b — the closed `then` catalog is `deny | allow | escalate`
    // (matches the parser/IR). `proceed` was a stale legacy synonym
    // the walker once accepted; it is now rejected.
    let source = r#"
feature customer
  command archive
    approval
      by @role.admin
      timeout "24h"
      then proceed
"#;
    let diagnostics = diagnostics_for(source);
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("`approval then proceed`")),
        "expected diagnostic about invalid then value; got: {messages:?}"
    );
}

#[test]
fn approval_accepts_escalate_then_action() {
    // F2b — `escalate` is a valid `then` resolution (parser lowers it
    // to `ApprovalThen::Escalate`); the file-local walker must not flag it.
    let source = r#"
feature customer
  command archive
    approval
      by @role.admin
      timeout "24h"
      then escalate
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        !diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "approval_contract_diagnostics"),
        "valid `then escalate` must not produce approval_contract_diagnostics"
    );
}

#[test]
fn approval_accepts_chain_sequential_without_by() {
    // F2/W4 GAP-06 — a non-empty `chain [...] sequential` satisfies the
    // approver requirement; the file-local walker must not demand `by`.
    let source = r#"
feature customer
  command archive
    approval
      chain [@role.manager, @role.admin] sequential
      timeout "24h"
      then escalate
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        !diagnostic_codes(&diagnostics)
            .iter()
            .any(|c| c == "approval_contract_diagnostics"),
        "chain form must satisfy the approver requirement; got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

