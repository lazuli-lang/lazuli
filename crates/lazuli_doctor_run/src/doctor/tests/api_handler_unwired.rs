    // API-HANDLER-UNWIRED-001 end-to-end doctor tests (W2).
    //
    // Drives a real `.lzi` source through the full doctor package and
    // asserts the analyze-time diagnostic fires for a declared `api`. The
    // current Go codegen emits the `lazuli.Api[I,O]{...}` value + a
    // `RegisterApi(&var)` init but NEVER sets the `Handler` field and
    // never bridges the declared `handler @fn.<name>` to it. At runtime
    // the mount loop skips registrations with a nil Handler
    // (`HandlerChecker()` false), so the endpoint is never mounted → 404
    // for the whole api surface as-shipped. This is the real pauta
    // `api get_attachment_url` / `api me` DOA-endpoint class.
    //
    // The clean sibling declares no `api` (a plain `command` surface) and
    // must NOT fire — no false-positive on features that never declared an
    // HTTP endpoint.

    use super::test_support_packages::*;
    use crate::doctor::*;

    // An `api` declaring `handler @fn.<name>` — the bridge-gap case: the
    // DSL DID declare a handler, but codegen drops it, leaving a nil
    // `Handler` at runtime. Must fire.
    const API_WITH_FN_HANDLER_SRC: &str = r#"
feature attachments
  uses org

  domain
    resource Attachment
      tenancy org
      url: Url required
      timestamps

  api get_attachment_url
    method GET
    path "/attachments/{id}/url"
    output Attachment
    handler @fn.generate_signed_url
"#;

    // A feature with a `command` surface but NO `api` — the rule is
    // api-scoped, so it must NOT fire here.
    const NO_API_SRC: &str = r#"
feature attachments
  uses org

  domain
    resource Attachment
      tenancy org
      url: Url required
      timestamps

  command touch
    returns Boolean
"#;

    #[test]
    fn doctor_fires_on_api_with_fn_handler() {
        let package = package_from_sources(vec![("attachments.lzi", API_WITH_FN_HANDLER_SRC)]);
        let diagnostics = package.diagnostics();

        let hit = diagnostics
            .iter()
            .find(|d| d.code == "API-HANDLER-UNWIRED-001")
            .unwrap_or_else(|| {
                panic!(
                    "expected API-HANDLER-UNWIRED-001 to fire on `api get_attachment_url`, \
                     got: {diagnostics:#?}"
                )
            });
        // Profile-gated: `warning` under the harness's Strict profile
        // (the auto-bridge that fixes the wiring is a pending wave-3 item,
        // so prototype/strict warn while iterating); hard `error` only at
        // production / iron-hand.
        assert_eq!(hit.severity, DoctorSeverity::Warning);
        assert!(
            hit.message.contains("404"),
            "message should explain the 404: {}",
            hit.message
        );
        assert!(
            hit.message.contains("get_attachment_url"),
            "message should name the offending api: {}",
            hit.message
        );
        assert!(
            hit.message.contains("ValidateApiHandlers"),
            "message should name the fail-fast call: {}",
            hit.message
        );
    }

    #[test]
    fn doctor_does_not_fire_without_api() {
        let package = package_from_sources(vec![("attachments.lzi", NO_API_SRC)]);
        let diagnostics = package.diagnostics();

        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code == "API-HANDLER-UNWIRED-001"),
            "API-HANDLER-UNWIRED-001 must not fire on a feature with no `api`, got: {:#?}",
            diagnostics
                .iter()
                .filter(|d| d.code == "API-HANDLER-UNWIRED-001")
                .collect::<Vec<_>>()
        );
    }
