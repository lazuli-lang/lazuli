    // Inspect-CLI auth + storage projection tests — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use std::path::Path;

    use crate::{ExpandSet, inspect_canonical_source};

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
    // cookie-sessions-child — `--expand=security` session-cookie envelope
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_security_projects_session_cookie_envelope() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        name "lazuli_session"
        same_site strict
        secure true
        http_only true
        domain ".example.com"
        path "/"
"#;
        let mut expansions = ExpandSet::default();
        expansions.security = true;
        let report = inspect_canonical_source(source, Path::new("customer_auth.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let cookie = &json["features"][0]["security"]["session_cookie"];
        assert!(
            !cookie.is_null(),
            "session_cookie envelope should be present under --expand=security: {json}"
        );
        assert_eq!(cookie["resource"], "CustomerSession");
        assert_eq!(cookie["name"], "lazuli_session");
        assert_eq!(cookie["same_site"], "strict");
        assert_eq!(cookie["secure"], true);
        assert_eq!(cookie["http_only"], true);
        assert_eq!(cookie["domain"], ".example.com");
        assert_eq!(cookie["path"], "/");
        assert_eq!(cookie["origin"]["feature"], "customer_auth");
    }

    #[test]
    fn inspect_security_session_cookie_omits_absent_axes() {
        // A partial cookie (only same_site) projects only that axis; absent
        // axes serialize nothing, signalling "runtime keeps its default".
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        same_site lax
"#;
        let mut expansions = ExpandSet::default();
        expansions.security = true;
        let report = inspect_canonical_source(source, Path::new("customer_auth.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let cookie = &json["features"][0]["security"]["session_cookie"];
        assert_eq!(cookie["same_site"], "lax");
        assert!(cookie["name"].is_null(), "absent name must not serialize");
        assert!(cookie["secure"].is_null(), "absent secure must not serialize");
        assert!(cookie["domain"].is_null(), "absent domain must not serialize");
    }

    #[test]
    fn inspect_security_session_cookie_absent_when_no_cookie_block() {
        // sessions without a cookie child → no envelope (runtime keeps the
        // hardcoded literals); other security bands still present.
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
"#;
        let mut expansions = ExpandSet::default();
        expansions.security = true;
        let report = inspect_canonical_source(source, Path::new("customer_auth.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let security = &json["features"][0]["security"];
        assert!(!security.is_null(), "security envelope present: {json}");
        assert!(
            security["session_cookie"].is_null(),
            "session_cookie must be absent when no cookie block: {security}"
        );
    }

    #[test]
    fn inspect_security_session_cookie_omitted_without_expand() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        same_site strict
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer_auth.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("session_cookie"),
            "session_cookie must be absent without --expand=security: {json}"
        );
    }
