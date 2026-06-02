
    use super::*;

    fn handler_file(source: &str) -> GoHandlerSourceFile {
        GoHandlerSourceFile {
            feature_name: "auth".to_owned(),
            bucket: "handlers".to_owned(),
            relative_path: PathBuf::from("features/auth/handlers/login.go"),
            absolute_path: PathBuf::from("/abs/features/auth/handlers/login.go"),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_test: false,
        }
    }

    #[test]
    fn errors_new_inside_function_fires() {
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n    return errors.New(\"bad\")\n}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 4);
        assert_eq!(findings[0].construct, "errors.New(...)");
    }

    #[test]
    fn package_level_var_err_sentinel_is_silent() {
        let f = handler_file(
            "package handlers\n\nvar ErrNotFound = errors.New(\"not found\")\n\nfunc Login() error { return ErrNotFound }\n",
        );
        assert!(check(&[f]).is_empty(), "var Err... sentinels are allowed");
    }

    #[test]
    fn var_block_err_sentinels_are_silent() {
        let f = handler_file(
            "package handlers\n\nvar (\n    ErrNotFound = errors.New(\"not found\")\n    ErrLocked   = errors.New(\"locked\")\n)\n",
        );
        assert!(
            check(&[f]).is_empty(),
            "var (...) block sentinels are allowed"
        );
    }

    #[test]
    fn fmt_errorf_pure_string_fires() {
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n    return fmt.Errorf(\"could not foo\")\n}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct, "fmt.Errorf(...)");
    }

    #[test]
    fn fmt_errorf_with_percent_w_does_not_fire() {
        // %w means wrapping — handled by HANDLER-ERROR-WRAP-001.
        let f = handler_file(
            "package handlers\n\nfunc Login(err error) error {\n    return fmt.Errorf(\"wrap: %w\", err)\n}\n",
        );
        assert!(check(&[f]).is_empty(), "%w wrapping is not flagged here");
    }

    #[test]
    fn fmt_errorf_with_percent_v_does_not_fire_here() {
        // %v is handled by HANDLER-ERROR-WRAP-001.
        let f = handler_file(
            "package handlers\n\nfunc Login(err error) error {\n    return fmt.Errorf(\"wrap: %v\", err)\n}\n",
        );
        assert!(
            check(&[f]).is_empty(),
            "%v formatting deferred to wrap rule"
        );
    }

    #[test]
    fn errors_new_inside_block_comment_is_silent() {
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n  /* old impl:\n  return errors.New(\"bad\")\n  */\n  return nil\n}\n",
        );
        assert!(
            check(&[f]).is_empty(),
            "errors.New inside /* */ is not flagged"
        );
    }

    #[test]
    fn errors_new_inside_line_comment_is_silent() {
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n  // return errors.New(\"bad\")\n  return nil\n}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn errors_new_inside_string_literal_is_silent() {
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n  log.Print(\"see errors.New(...)\")\n  return nil\n}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn multiple_findings_per_file() {
        let f = handler_file(
            "package handlers\n\nfunc A() error { return errors.New(\"a\") }\nfunc B() error { return fmt.Errorf(\"b\") }\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn test_file_is_skipped() {
        let mut f = handler_file(
            "package handlers\n\nfunc TestX(t *T) {\n  if false { return errors.New(\"bad\") }\n}\n",
        );
        f.is_test = true;
        assert!(check(&[f]).is_empty(), "test file is silent");
    }

    #[test]
    fn multibyte_em_dash_in_go_raw_string_before_errors_new_does_not_panic() {
        // Live pauta crash: an em-dash (`—`, 3 UTF-8 bytes) inside a Go
        // raw string (backtick) sits in non-`"`-quoted code as far as the
        // double-quote string tracker is concerned, so the scan loop's
        // `if !in_str { no_line_comment[i..]... }` branch runs with `i`
        // landing mid-`—` and the old byte-walk panicked with
        // "byte index N is not a char boundary; it is inside '—'".
        // The finding on the following line must still fire.
        let f = handler_file(
            "package handlers\n\nfunc Q() error {\n    q := `note — keep`\n    _ = q\n    return errors.New(\"bad\")\n}\n",
        );
        let findings = check(&[f]);
        assert!(
            findings.iter().any(|x| x.construct == "errors.New(...)"),
            "errors.New after a backtick raw string with em-dash still flagged"
        );
    }

    #[test]
    fn multibyte_em_dash_in_line_comment_before_errors_new_does_not_panic() {
        // Em-dash in a `//` comment (stripped) then errors.New next line.
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n    // note — see below\n    return errors.New(\"bad\")\n}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1, "em-dash in comment must not suppress finding");
        assert_eq!(findings[0].line, 5);
        assert_eq!(findings[0].construct, "errors.New(...)");
    }

    #[test]
    fn multibyte_em_dash_in_same_string_arg_does_not_panic() {
        // The em-dash lives in the error literal itself — the panic site
        // is the string-literal scan inside `fmt.Errorf(`.
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n    return fmt.Errorf(\"could not foo — bar\")\n}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct, "fmt.Errorf(...)");
    }

    #[test]
    fn multibyte_in_sql_doctor_allow_comment_does_not_panic() {
        // A `-- doctor:allow` SQL comment carrying an em-dash inside a Go
        // raw string (backtick) — the exact live shape. We only assert
        // no panic; the rule may or may not fire depending on heuristics,
        // but it must complete.
        let f = handler_file(
            "package handlers\n\nfunc Q() error {\n    q := `SELECT 1 -- doctor:allow — keep`\n    _ = q\n    return errors.New(\"x\")\n}\n",
        );
        // Must not panic. errors.New on a later line still fires.
        let findings = check(&[f]);
        assert!(
            findings.iter().any(|x| x.construct == "errors.New(...)"),
            "errors.New after the multibyte SQL comment still flagged"
        );
    }

    #[test]
    fn multibyte_accent_and_emoji_in_block_comment_does_not_panic() {
        // Accented letters (2 bytes) and an emoji (4 bytes) inside a
        // /* */ block comment must be stripped without panicking or
        // corrupting the surviving code's byte offsets.
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n  /* açúcar 🎉 stuff */ return errors.New(\"bad\")\n}\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1, "code after multibyte block comment still scanned");
        assert_eq!(findings[0].construct, "errors.New(...)");
    }

    #[test]
    fn multibyte_in_string_literal_keeps_string_state_correct() {
        // An em-dash inside a benign string literal that itself contains
        // the text `errors.New(` must stay silent (string-literal
        // suppression) and not panic.
        let f = handler_file(
            "package handlers\n\nfunc Login() error {\n  log.Print(\"see — errors.New(...) docs\")\n  return nil\n}\n",
        );
        assert!(
            check(&[f]).is_empty(),
            "errors.New inside a multibyte string literal is silent"
        );
    }

    #[test]
    fn message_includes_construct_and_path() {
        let f = handler_file("package handlers\n\nfunc A() error { return errors.New(\"x\") }\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("errors.New"));
        assert!(msg.contains("login.go"));
    }
