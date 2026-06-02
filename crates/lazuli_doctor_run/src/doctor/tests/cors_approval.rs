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
    fn doctor_flags_cors_wildcard_in_production_as_error() {
        // CORS-WILDCARD-PROD-001 — a bare `"*"` origin in a production-
        // targeted environment fires (error). Compile-time companion to the
        // runtime `Mux()` boot refusal.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  cors
    allow_origins production "*"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hit = diagnostics
            .iter()
            .find(|d| d.code == "CORS-WILDCARD-PROD-001")
            .unwrap_or_else(|| {
                panic!(
                    "expected CORS-WILDCARD-PROD-001; got {:?}",
                    diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
                )
            });
        assert_eq!(
            hit.severity,
            DoctorSeverity::Error,
            "production wildcard must be an error (runtime refuses to boot)"
        );
    }

    #[test]
    fn doctor_warns_cors_wildcard_in_local_only() {
        // The same `"*"` under `local` is a warning, not an error — mirrors
        // the runtime's dev `slog.Warn` (the runtime allows `"*"` in dev).
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  cors
    allow_origins local "*"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hit = diagnostics
            .iter()
            .find(|d| d.code == "CORS-WILDCARD-PROD-001")
            .unwrap_or_else(|| {
                panic!(
                    "expected CORS-WILDCARD-PROD-001; got {:?}",
                    diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
                )
            });
        assert_eq!(
            hit.severity,
            DoctorSeverity::Warning,
            "dev/local wildcard is a warning, not an error"
        );
    }

    #[test]
    fn doctor_no_false_positive_on_explicit_origins() {
        // Explicit origins everywhere → CORS-WILDCARD-PROD-001 must NOT fire.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"
    web local "http://localhost:5173"

  cors
    allow_origins production "https://app.example.com"
    allow_origins local "http://localhost:5173"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("CORS-WILDCARD-PROD-001"),
            "explicit origins must not fire CORS-WILDCARD-PROD-001; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_no_false_positive_when_no_cors_contract() {
        // App with NO `cors` block (like pauta) → graceful no-op.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("CORS-WILDCARD-PROD-001"),
            "absent cors contract must be a no-op; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
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
    fn doctor_accepts_approval_chain_sequential() {
        // F2/W4 GAP-06 — `chain [@role.manager, @role.admin] sequential`
        // with both roles declared and a valid `then escalate` must pass
        // clean: neither the file-local presence walker nor the IR-layer
        // chain-order check should fire.
        let package = package_from_sources(vec![(
            "approvals.lzi",
            r#"
feature approvals
  policies
    delete: @role.manager, @role.admin

  command approve_job
    policy @policy.delete
    approval
      required_when target.tier = enterprise
      chain [@role.manager, @role.admin] sequential
      timeout "24h"
      then escalate
    deletes Job
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes_set = codes(&diagnostics);
        for code in [
            "approval_contract_diagnostics",
            "APPROVAL-CHAIN-ORDER-001",
            "approval_timeout_invalid_diagnostics",
        ] {
            assert!(
                !codes_set.contains(code),
                "well-formed approval chain must not produce {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn doctor_rejects_approval_chain_with_unknown_role() {
        // F2 — the chain form must still flag an approver that no policy
        // declares, under the unified APPROVAL-CHAIN-ORDER-001 code.
        let package = package_from_sources(vec![(
            "approvals.lzi",
            r#"
feature approvals
  policies
    delete: @role.manager

  command approve_job
    policy @policy.delete
    approval
      chain [@role.manager, @role.ghost] sequential
      timeout "24h"
      then escalate
    deletes Job
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("APPROVAL-CHAIN-ORDER-001"),
            "expected APPROVAL-CHAIN-ORDER-001 for unknown chain role; got {:?}",
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

