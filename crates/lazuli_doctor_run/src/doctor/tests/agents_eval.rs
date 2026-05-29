    // Doctor agent / tool policy / discriminator / eval tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_rejects_tool_with_stricter_policy_than_agent() {
        // Agent declares `policy @policy.read` but invokes a `command`
        // whose policy is `@policy.delete` — the conservative lattice
        // ordering flags this as `agent_tool_policy_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    policy @policy.delete
    deletes Customer

  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    safety @validator.pii_scrub
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_tool_policy_diagnostics"),
            "expected agent_tool_policy_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_write_tool_without_safety() {
        // Same write-tool fan-in but with no `safety` declared — Cut A
        // requires safety as the write-tool guard (Q-impl-4 deferred
        // `idempotency by` to Cut B).
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    policy @policy.delete
    deletes Customer

  agent triage
    policy @policy.delete
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_tool_write_unguarded_diagnostics"),
            "expected agent_tool_write_unguarded_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_pii_tool_without_safety() {
        // Registry declares `@tool.web_search` with `pii_classes contact`
        // and the agent invokes it with no safety — emit
        // `agent_pii_unsafetied_warning`.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
registry
  tools
    tool web_search
      effect read
      pii_classes contact
      adapter @adapter.serp

feature customer
  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      @tool.web_search
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_pii_unsafetied_warning"),
            "expected agent_pii_unsafetied_warning; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_unknown_discriminator_target() {
        // No `enum Intent` is declared anywhere — emit
        // `agent_discriminator_target_invalid_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer_support
  agent classify_intent
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./p.md"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_discriminator_target_invalid_diagnostics"),
            "expected agent_discriminator_target_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_evals_without_determinism_pin() {
        // Agent has evals but no `temperature 0` and no `seed` — emit
        // `eval_nondeterministic_warning`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent flaky
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case smoke
        requires output contains "ok"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("eval_nondeterministic_warning"),
            "expected eval_nondeterministic_warning; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_registry_tool_missing_effect() {
        // `tool_registry_effect_required_diagnostics` is the only id
        // that fires off the registry-side IR. The parser collects a
        // defect for every `tool <name>` whose block omits `effect`.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
registry
  tools
    tool calendar_create_event
      adapter @adapter.google_calendar
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tool_registry_effect_required_diagnostics"),
            "expected tool_registry_effect_required_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_eval_ordered_op_on_non_numeric_operands() {
        // `requires customer.email < "x"` is an ordered op on text —
        // emit `eval_ordered_op_invalid_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent bounded
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case bad
        requires customer.email < "z@example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("eval_ordered_op_invalid_diagnostics"),
            "expected eval_ordered_op_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

