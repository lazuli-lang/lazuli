    // API-HANDLER-UNWIRED-001 end-to-end doctor tests.
    //
    // Drives a real `.lzi` source through the full doctor package. As of
    // the wave-3 codegen bridge, the Go emitter wires every well-formed
    // api's declared `handler @fn.<name>` (or convention `./api/<name>.go`)
    // into the runtime `Handler` field via `HandlerFromRegistry`, so the
    // route mounts and the endpoint serves. The rule must therefore stay
    // QUIET on a normally-lowered api with a declared handler (the real
    // pauta `api me` / `api get_attachment_url` case — now fixed), and on
    // a feature with no `api` at all.

    use super::test_support_packages::*;
    use crate::doctor::*;

    // An `api` declaring `handler @fn.<name>` — codegen now bridges this
    // into the runtime `Handler`, so the rule must NOT fire.
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
    fn doctor_quiet_on_api_with_bridged_fn_handler() {
        let package = package_from_sources(vec![("attachments.lzi", API_WITH_FN_HANDLER_SRC)]);
        let diagnostics = package.diagnostics();

        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code == "API-HANDLER-UNWIRED-001"),
            "API-HANDLER-UNWIRED-001 must NOT fire on a well-formed api whose handler is \
             bridged by codegen, got: {:#?}",
            diagnostics
                .iter()
                .filter(|d| d.code == "API-HANDLER-UNWIRED-001")
                .collect::<Vec<_>>()
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
