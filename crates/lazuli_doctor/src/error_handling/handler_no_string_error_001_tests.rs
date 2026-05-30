
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
    fn message_includes_construct_and_path() {
        let f = handler_file("package handlers\n\nfunc A() error { return errors.New(\"x\") }\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("errors.New"));
        assert!(msg.contains("login.go"));
    }
