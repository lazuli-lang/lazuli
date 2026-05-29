    // Doctor CORS wildcard / origin + approval role/timeout/required-children tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_rejects_cors_wildcard_with_credentials() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  cors
    allow_origins production "https://app.example.com"
    allow_origins local "*"
    allow_credentials true
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_credentials_wildcard_conflict_diagnostics"),
            "expected cors_credentials_wildcard_conflict_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_cors_origin_not_in_urls() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"

  cors
    allow_origins production "https://stranger.example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_origin_undocumented_diagnostics"),
            "expected cors_origin_undocumented_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_cors_origin_matching_declared_url() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"

  cors
    allow_origins production "https://app.example.com"
    allow_credentials true
    max_age "1h"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes_set = codes(&diagnostics);
        for code in [
            "cors_unknown_environment_diagnostics",
            "cors_credentials_wildcard_conflict_diagnostics",
            "cors_origin_undocumented_diagnostics",
        ] {
            assert!(
                !codes_set.contains(code),
                "well-formed CORS must not produce {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn doctor_rejects_approval_with_unknown_role() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    policy @policy.delete
    approval
      required_when target.tier = enterprise
      by @role.nonexistent
      timeout "24h"
      then deny
    deletes Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        // W4 GAP-06 — the single-approver `by` form lifts to a 1-element
        // chain; the unknown-role check now reports under the unified
        // APPROVAL-CHAIN-ORDER-001 code.
        assert!(
            codes(&diagnostics).contains("APPROVAL-CHAIN-ORDER-001"),
            "expected APPROVAL-CHAIN-ORDER-001; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_approval_with_malformed_timeout() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    approval
      by @role.admin
      timeout "soon"
      then deny
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("approval_timeout_invalid_diagnostics"),
            "expected approval_timeout_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_approval_satisfies_write_tool_guard_without_agent_safety() {
        // Agent dispatches a write tool whose target command carries
        // `approval` — the guard is satisfied even though the agent
        // has no `safety` declaration.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin
    read: @scope.same_org

  command archive
    policy @policy.delete
    approval
      by @role.admin
      timeout "24h"
      then deny
    deletes Customer

  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_tool_write_unguarded_diagnostics"),
            "approval on target command must satisfy the write-tool guard; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_approval_missing_required_children() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    approval
      by @role.admin
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("approval_contract_diagnostics"),
            "expected approval_contract_diagnostics for missing children; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

