    #[test]
    fn package_prefix_stripped_before_compare() {
        // accountgen.LoginInput on the handler side vs bare LoginInput
        // on the codegen side — must NOT fire after normalisation.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
func Login(ctx *lazuli.Ctx, input accountgen.LoginInput) (string, error) { return "", nil }
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[LoginInput, string]{ Name: "account.login" }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty(), "got: {:?}", findings);
    }

    #[test]
    fn handler_unreadable_emits_specific_finding() {
        // Function exists but signature shape is not the canonical
        // `(ctx, input) (output, error)` — should emit
        // HandlerSignatureUnreadable, NOT panic. Covers the
        // type-alias risk from the proposal §False-positive cases.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        // Three return values — not the canonical shape.
        let handler_src = r#"package accounthandlers
func Login(ctx *lazuli.Ctx, input LoginInput) (string, int, error) { return "", 0, nil }
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[LoginInput, string]{ Name: "account.login" }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].diff, Diff::HandlerSignatureUnreadable));
        assert!(findings[0].message().contains("could not parse"));
    }

    #[test]
    fn malformed_handler_does_not_panic() {
        // Truncated Go — handler `func Login(` with no matching `)`.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = "package accounthandlers\nfunc Login(ctx *lazuli.Ctx, input";
        let gen_src = r#"package accountgen
var login = lazuli.Command[LoginInput, string]{ Name: "account.login" }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        // Should produce a single Unreadable finding — no panic.
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].diff, Diff::HandlerSignatureUnreadable));
    }

    #[test]
    fn missing_codegen_file_silent() {
        // Handler exists. Codegen file does NOT. Sibling rule covers
        // this; we must short-circuit cleanly.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_dir = handler_path::handlers_dir(&app_root, "account");
        fs::create_dir_all(&handler_dir).unwrap();
        fs::write(handler_dir.join("login.go"), MATCHING_HANDLER).unwrap();
        // No gen file written.

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn missing_handler_file_silent_delegates_to_handler_missing_001() {
        // Codegen present, handler `.go` absent — HANDLER-MISSING-001
        // owns that finding; this rule MUST stay silent to avoid
        // double-fire.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let gen_dir = dist_root.join("go").join("account");
        fs::create_dir_all(&gen_dir).unwrap();
        fs::write(gen_dir.join("command.gen.go"), MATCHING_GEN).unwrap();

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn allow_comment_silences_finding() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(
            &lzi_path,
            "feature account\n# doctor:allow HANDLER-SIGNATURE-MISMATCH-001 — reason \"alias\"\n",
        );

        // Drift on disk — would fire without the opt-out.
        let handler_src = r#"package accounthandlers
func Login(ctx *lazuli.Ctx, input WrongInput) (string, error) { return "", nil }
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[LoginInput, string]{ Name: "account.login" }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn gen_file_with_no_matching_command_silent() {
        // Codegen file exists but contains no Command[...] block whose
        // Name: matches `account.login`. Sibling rule
        // (@correctness.migration_out_of_sync) owns that surface;
        // this rule MUST stay silent.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        // gen declares a DIFFERENT command name — login_with_google,
        // not login.
        let gen_src = r#"package accountgen
var loginWithGoogle = lazuli.Command[OtherInput, OtherOutput]{
    Name: "account.login_with_google",
}
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            MATCHING_HANDLER,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn function_export_renamed_emits_missing_handler() {
        // Handler file exists but author renamed `Login` to
        // `DoLogin` — the rule should surface MissingHandler so the
        // diagnostic is specific (not a confusing "signature drift" msg).
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
func DoLogin(ctx *lazuli.Ctx, input LoginInput) (string, error) { return "", nil }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            MATCHING_GEN,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].diff, Diff::MissingHandler));
    }

    #[test]
    fn pascal_case_basic() {
        assert_eq!(pascal_case("login_with_google"), "LoginWithGoogle");
        assert_eq!(pascal_case("login"), "Login");
        assert_eq!(pascal_case("verify_password_v2"), "VerifyPasswordV2");
    }

    #[test]
    fn extract_command_signature_finds_block() {
        let src = r#"package accountgen
var foo = lazuli.Command[Input1, Output1]{ Name: "account.foo" }
var bar = lazuli.Command[Input2, Output2]{
    Name: "account.bar",
}
"#;
        let sig = extract_command_signature(src, "account.bar").unwrap();
        assert_eq!(sig.input, "Input2");
        assert_eq!(sig.output, "Output2");
    }

    #[test]
    fn extract_handler_signature_strips_package_prefix() {
        let src = r#"package accounthandlers
func LoginWithGoogle(ctx *lazuli.Ctx, input accountgen.LoginResultWithGoogleInput) (struct{}, error) {
    return struct{}{}, nil
}
"#;
        let result = extract_handler_signature(src, "LoginWithGoogle");
        match result {
            HandlerExtractResult::Found(sig) => {
                assert_eq!(sig.input, "accountgen.LoginResultWithGoogleInput");
                assert_eq!(sig.output, "struct{}");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }
