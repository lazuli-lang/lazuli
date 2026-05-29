    // Doctor expose_http path collision/audience + cap_file capability tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_rejects_expose_http_path_colliding_cross_feature_with_api() {
        // Agent in `customer` exposes the same (method, path) as an
        // `api` block in `customer_outreach`. Cross-feature collision
        // fires `agent_expose_path_conflict_cross_feature_diagnostics`.
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
"#,
            ),
            (
                "customer_outreach.lzi",
                r#"
feature customer_outreach
  api customer_summary_stream
    method POST
    path "/api/customers/:id/summary"
    output Text
    policy @scope.public
    handler "./x.go"
"#,
            ),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_expose_path_conflict_cross_feature_diagnostics"),
            "expected agent_expose_path_conflict_cross_feature_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_unknown_audience_on_expose_http() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent restricted
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x"
      audience nonexistent_audience
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_expose_audience_unknown_diagnostics"),
            "expected agent_expose_audience_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_audience_declared_in_surface() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  agent admin_only
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/admin/x"
      audience admin
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
"#,
            ),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_expose_audience_unknown_diagnostics"),
            "audience declared in .lzx must be honored; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // ---------------------------------------------------------------------
    // Row 30 — Storage bucket cycle: 5 typed `@cap.File` diagnostics.
    // ---------------------------------------------------------------------

    #[test]
    fn doctor_emits_cap_file_visibility_undeclared() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_export
  domain
    resource Export
      id: ID required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_undeclared"),
            "expected cap_file_visibility_undeclared on api output; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_skips_visibility_undeclared_on_resource_field() {
        // Resource fields default `visibility` to private; the
        // diagnostic only fires on api outputs.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_field
  domain
    resource Export
      file: @cap.File(max_size:10mb,accept:text/csv) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("cap_file_visibility_undeclared"),
            "resource fields default to private; should not emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_accept_input_output_mismatch() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_pipeline
  domain
    resource ImportBatch
      file: @cap.File(max_size:25mb,accept:application/json,visibility:private) required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_accept_input_output_mismatch"),
            "expected cap_file_accept_input_output_mismatch; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_overlapping_accept_lists() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_pipeline_ok
  domain
    resource ImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv,visibility:private) required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("cap_file_accept_input_output_mismatch"),
            "overlapping accept lists should not emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_visibility_signed_ttl_mismatch_when_ttl_missing() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_signed
  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_signed_ttl_mismatch"),
            "signed visibility without signed_ttl must emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_visibility_signed_ttl_mismatch_when_ttl_with_private() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_private_ttl
  domain
    resource Export
      file: @cap.File(max_size:10mb,accept:text/csv,visibility:private,signed_ttl:1h) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_signed_ttl_mismatch"),
            "private visibility with signed_ttl must emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_size_unit_invalid() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_size
  domain
    resource Export
      blob: @cap.File(max_size:large,accept:text/csv) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_size_unit_invalid"),
            "expected cap_file_size_unit_invalid; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_mime_family_unknown() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_mime
  domain
    resource Export
      blob: @cap.File(max_size:10mb,accept:gibberish/csv,visibility:private) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_mime_family_unknown"),
            "expected cap_file_mime_family_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

