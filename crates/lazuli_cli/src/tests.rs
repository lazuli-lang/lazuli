//! `lazuli_cli` test suite — pulled from main.rs's `#[cfg(test)] mod tests`
//! block as part of the W4.5 R2 split. Kept as `mod tests { ... }` so the
//! inner string-literal content (raw and non-raw) preserves its original
//! indentation; de-indenting would corrupt the multi-line .lzi fixture
//! strings the tests assert against.

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::Parser;
    use tempfile::TempDir;

    use crate::cli_args::{DesignExportTarget, DesignImportFormat};
use crate::go_work_io::add_missing_go_work_use_entries;
use crate::{
        Cli, Commands, DesignCommand, ExpandSet,
        GenerateKind, MigrateCommand, REGISTRY_TEMPLATE,
        app_template, default_module_name, emit_feature_barrel_ts, emit_feature_react_hooks_ts,
        emit_feature_sdk_ts, expand_canonical_source, inspect_canonical_source, inspect_json_value,
        new_command, parse_expand_set, pascal_case, pascal_case_project_name,
        render_inspect_symbol_lazuli, scaffold_bare, scaffold_from_template, templates,
        write_go_work_preserving_entries,
    };

    // NOTE: tests for `query_ident` / `strip_query_verb_prefix` (the
    // verb-prefix dedup added alongside the Hostpoint bug fix) cannot
    // live here because the `lazuli_cli` test binary currently fails to
    // compile on this branch's base (pre-existing `doctor::lzx::ir_stub`
    // field mismatches, unrelated to this change — see `cargo test -p
    // lazuli_cli` baseline). The behaviour is covered by the matching
    // tests in `lazuli_codegen_ts::lzx::tests` (the helper logic is
    // identical and was factored to mirror the CLI's local copy).

    mod migrate {
        include!("tests/migrate.rs");
    }

    mod test_support {
        include!("tests/test_support.rs");
    }
    use test_support::*;

    mod codegen_ts_enums {
        include!("tests/codegen_ts_enums.rs");
    }

    mod codegen_ts_command_sdk {
        include!("tests/codegen_ts_command_sdk.rs");
    }

    mod codegen_ts_react_hooks {
        include!("tests/codegen_ts_react_hooks.rs");
    }

    mod codegen_ts_query_sdk {
        include!("tests/codegen_ts_query_sdk.rs");
    }

    mod codegen_ts_plugin_semantic {
        include!("tests/codegen_ts_plugin_semantic.rs");
    }

    mod dispatch {
        include!("tests/dispatch.rs");
    }

    mod in_place {
        include!("tests/in_place.rs");
    }

    mod scaffold {
        include!("tests/scaffold.rs");
    }

    mod inspect_expand_basic {
        include!("tests/inspect_expand_basic.rs");
    }

    mod inspect_manifest_json {
        include!("tests/inspect_manifest_json.rs");
    }

    mod inspect_expand_projections {
        include!("tests/inspect_expand_projections.rs");
    }

    mod inspect_summary_agent {
        include!("tests/inspect_summary_agent.rs");
    }

    // -------------------------------------------------------------------------
    // Phase L — `--expand=auth` projection coverage
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_auth_projection_emits_full_block() {
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
        let mut expansions = ExpandSet::default();
        expansions.auth = true;
        let report = inspect_canonical_source(source, Path::new("customer_auth.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let auth = &json["features"][0]["auth"];
        assert!(!auth.is_null(), "auth projection should be present: {json}");
        assert_eq!(auth["origin"]["feature"], "customer_auth");
        assert_eq!(auth["identity"]["field"], "Customer.email");
        assert_eq!(auth["identity"]["resource"], "Customer");
        assert_eq!(auth["identity"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["password"]["algorithm"], "argon2id");
        assert_eq!(auth["password"]["hash"], "@fn.hash_customer_password");
        assert_eq!(auth["password"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["mfa"]["method"], "totp");
        assert_eq!(auth["mfa"]["enroll"], "@fn.enroll_customer_totp");
        assert_eq!(auth["mfa"]["verify"], "@validator.verify_customer_totp");
        assert_eq!(auth["mfa"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["sessions"]["ttl"], "7 days");
        assert_eq!(auth["sessions"]["refresh"], false);
        assert_eq!(auth["sessions"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["oauth"][0]["provider"], "google");
        assert_eq!(auth["oauth"][0]["origin"]["feature"], "customer_auth");
    }

    #[test]
    fn inspect_auth_projection_omitted_without_expand() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer_auth.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"auth\":{"),
            "auth projection must be absent without --expand=auth: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 2 — `--expand=storage` projection coverage
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_storage_projection_emits_resource_field_capability() {
        let source = r#"
feature customer_import
  domain
    resource CustomerImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv) required
      uploaded_by: User required
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer_import.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let storage = &json["features"][0]["storage"];
        assert!(
            !storage.is_null(),
            "storage projection should be present: {json}"
        );
        let field = &storage["fields"][0];
        assert_eq!(field["resource"], "CustomerImportBatch");
        assert_eq!(field["field"], "file");
        assert_eq!(
            field["file_capability"]["max_size"]["bytes"],
            25 * 1024 * 1024
        );
        assert_eq!(field["file_capability"]["max_size"]["literal"], "25mb");
        assert_eq!(field["file_capability"]["accept"][0]["family"], "text");
        assert_eq!(field["file_capability"]["accept"][0]["subtype"], "csv");
    }

    #[test]
    fn inspect_storage_projection_emits_api_output_capability() {
        let source = r#"
feature customer
  api customer_export
    method GET
    path "/api/customers/export"
    output @cap.File(max_size:100mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/export.go"
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let output = &json["features"][0]["storage"]["api_outputs"][0];
        assert_eq!(output["api"], "customer_export");
        assert_eq!(output["file_capability"]["max_size"]["literal"], "100mb");
        assert_eq!(output["file_capability"]["visibility"], "signed");
        assert_eq!(output["file_capability"]["signed_ttl"], "1h");
    }

    #[test]
    fn inspect_storage_projection_omitted_without_expand() {
        let source = r#"
feature customer_import
  domain
    resource CustomerImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv) required
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("customer_import.lzi"),
            ExpandSet::default(),
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"storage\":{"),
            "storage projection must be absent without --expand=storage: {json}"
        );
    }

    #[test]
    fn inspect_storage_projection_absent_when_feature_has_no_cap_file() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        // No @cap.File authored → field omitted entirely.
        assert!(json["features"][0]["storage"].is_null());
    }

    #[test]
    fn inspect_auth_projection_absent_when_feature_has_no_auth() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let mut expansions = ExpandSet::default();
        expansions.auth = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        // No auth block authored → field omitted (None serialises away).
        assert!(json["features"][0]["auth"].is_null());
    }

    // -------------------------------------------------------------------------
    // Roadmap §1.2 — `--expand=http` projection coverage. The unified
    // `http` slot at the report root surfaces cookie + proxy + limits
    // with `origin` metadata only when the flag is set. The typed
    // blocks still serialize on `app` either way.
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_http_flag_parses() {
        let expansions = parse_expand_set("http").unwrap();
        assert!(expansions.http);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_http_projection_surfaces_cookie_proxy_limits_with_flag() {
        let source = r#"
app MyApp
  cookie
    default
      signed true
      secure true
      http_only true
      same_site strict
      max_age "7d"
    session
      same_site lax

  proxy
    trusted 10.0.0.0/8, 172.16.0.0/12
    real_ip_header X-Forwarded-For
    forwarded_proto_header X-Forwarded-Proto

  limits
    body_size "10mb"
    header_size "16kb"
    timeout "30s"
"#;
        let mut expansions = ExpandSet::default();
        expansions.http = true;
        let report = inspect_canonical_source(source, Path::new("app.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let http = &json["http"];
        assert!(!http.is_null(), "http projection should be present: {json}");
        assert_eq!(http["origin"]["app"], "MyApp");
        // Cookie block.
        assert_eq!(http["cookie"]["profiles"][0]["name"], "default");
        assert_eq!(http["cookie"]["profiles"][0]["signed"], true);
        assert_eq!(http["cookie"]["profiles"][0]["same_site"], "strict");
        assert_eq!(http["cookie"]["profiles"][0]["max_age"], "7d");
        assert_eq!(http["cookie"]["profiles"][1]["name"], "session");
        assert_eq!(http["cookie"]["profiles"][1]["same_site"], "lax");
        // Proxy block.
        assert_eq!(http["proxy"]["trusted"][0], "10.0.0.0/8");
        assert_eq!(http["proxy"]["trusted"][1], "172.16.0.0/12");
        assert_eq!(http["proxy"]["real_ip_header"], "X-Forwarded-For");
        assert_eq!(http["proxy"]["forwarded_proto_header"], "X-Forwarded-Proto");
        // Limits block.
        assert_eq!(http["limits"]["body_size"], "10mb");
        assert_eq!(http["limits"]["header_size"], "16kb");
        assert_eq!(http["limits"]["timeout"], "30s");
        // Per-block origin envelope.
        assert_eq!(http["cookie"]["origin"]["app"], "MyApp");
        assert_eq!(http["proxy"]["origin"]["app"], "MyApp");
        assert_eq!(http["limits"]["origin"]["app"], "MyApp");
    }

    #[test]
    fn inspect_http_projection_omitted_without_expand() {
        let source = r#"
app MyApp
  cookie
    default
      same_site strict

  limits
    body_size "10mb"
"#;
        let report = inspect_canonical_source(source, Path::new("app.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        // The unified `http` slot at the report root is absent without
        // the flag — Option<Value>::None skips the serde key.
        assert!(
            !json.contains("\"http\":{"),
            "http projection must be absent without --expand=http: {json}"
        );
        // But the typed blocks still serialize on `app`.
        assert!(
            json.contains("\"cookie\":"),
            "cookie still surfaces on AppManifest: {json}"
        );
        assert!(
            json.contains("\"limits\":"),
            "limits still surfaces on AppManifest: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // `--format=lazuli` for `lazuli inspect <symbol>` (next-checklist
    // follow-up from lsp-symbol-origin v0.2; closes the deferred item).
    // -------------------------------------------------------------------------

    #[test]
    fn render_inspect_symbol_lazuli_found_emits_human_readable_one_liner() {
        let output = serde_json::json!({
            "symbol": "Customer",
            "feature": "account",
            "defined_in": {
                "source": "file",
                "file": "features/account/account.lzi",
                "line": 42,
                "column": 3,
                "kind": "resource",
            },
            "imported_via": null,
            "type": "resource",
            "previous_names": [],
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("Customer"),
            "rendered should name the symbol:\n{rendered}"
        );
        assert!(
            rendered.contains("account"),
            "rendered should name the feature:\n{rendered}"
        );
        assert!(
            rendered.contains("features/account/account.lzi:42"),
            "rendered should anchor the source location:\n{rendered}"
        );
        assert!(
            rendered.contains("(resource)"),
            "rendered should name the symbol kind:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_with_previous_names() {
        let output = serde_json::json!({
            "symbol": "Customer",
            "feature": "account",
            "defined_in": {
                "source": "file",
                "file": "x.lzi",
                "line": 10,
                "column": 1,
                "kind": "resource",
            },
            "imported_via": null,
            "type": "resource",
            "previous_names": ["Client", "User"],
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("previously:"),
            "rendered should announce previously: trailer:\n{rendered}"
        );
        assert!(
            rendered.contains("Client") && rendered.contains("User"),
            "rendered should list both previous names:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_not_found_emits_code_and_message() {
        let output = serde_json::json!({
            "error": {
                "code": "SYMBOL_NOT_FOUND",
                "message": "no declaration named `Foo` in any feature of this project",
            }
        });
        let rendered = render_inspect_symbol_lazuli("Foo", &output);
        assert!(
            rendered.starts_with("SYMBOL_NOT_FOUND:"),
            "rendered should lead with the error code:\n{rendered}"
        );
        assert!(
            rendered.contains("Foo"),
            "rendered should echo the missing symbol:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_ambiguous_lists_candidates() {
        let output = serde_json::json!({
            "error": {
                "code": "AMBIGUOUS_SYMBOL",
                "message": "`Customer` is declared in multiple features",
                "candidates": ["account.Customer", "billing.Customer"],
            }
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("AMBIGUOUS_SYMBOL"),
            "rendered should lead with the error code:\n{rendered}"
        );
        assert!(
            rendered.contains("- account.Customer"),
            "rendered should list candidate as bullet:\n{rendered}"
        );
        assert!(
            rendered.contains("- billing.Customer"),
            "rendered should list every candidate:\n{rendered}"
        );
    }
}
