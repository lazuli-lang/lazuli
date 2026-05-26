    // Inspect-CLI HTTP projection + symbol-render tests — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use std::path::Path;

    use crate::{ExpandSet, inspect_canonical_source, parse_expand_set, render_inspect_symbol_lazuli};

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
