    // Doctor codegen-wrap + pattern-draft-stale rule tests — split from
    // `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;

    use super::test_support_core::*;
    use crate::doctor::{check_codegen_wrap_001, check_pattern_draft_stale_001_at};

    #[test]
    fn codegen_wrap_001_fires_on_field_error_literal_in_bucket() {
        let root = temp_project_root("codegen-wrap-fires");
        write_file(
            &root.join("runtime/go/lazuli/auth/password.go"),
            "package auth\n\nfunc f() error { return &lazuli.FieldError{} }\n",
        );

        let diagnostics = check_codegen_wrap_001(&root);
        fs::remove_dir_all(&root).ok();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "CODEGEN-WRAP-001");
        assert_eq!(diagnostics[0].line, 3);
    }

    #[test]
    fn codegen_wrap_001_ignores_top_level_runtime_files() {
        let root = temp_project_root("codegen-wrap-top-level");
        write_file(
            &root.join("runtime/go/lazuli/error_field.go"),
            "package lazuli\n\nvar _ = lazuli.FieldError{}\n",
        );

        let diagnostics = check_codegen_wrap_001(&root);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn codegen_wrap_001_ignores_gen_files() {
        let root = temp_project_root("codegen-wrap-gen");
        write_file(
            &root.join("runtime/go/lazuli/auth/password.gen.go"),
            "package auth\n\nvar _ = lazuli.FieldError{}\n",
        );

        let diagnostics = check_codegen_wrap_001(&root);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn codegen_wrap_001_ignores_test_files() {
        let root = temp_project_root("codegen-wrap-test");
        write_file(
            &root.join("runtime/go/lazuli/auth/password_test.go"),
            "package auth\n\nvar _ = lazuli.FieldError{}\n",
        );

        let diagnostics = check_codegen_wrap_001(&root);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn pattern_draft_stale_001_skips_when_no_drafts() {
        let root = temp_project_root("pattern-draft-no-drafts");
        write_file(
            &root.join("crates/lazuli_codegen_go/src/emitter/patterns.rs"),
            "pub const PATTERN_COMMAND: (&str, &str) = (\"command\", \"v1\");\n",
        );

        let diagnostics = check_pattern_draft_stale_001_at(&root, 1_800_000_000);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn pattern_draft_stale_001_skips_when_recent() {
        let root = temp_project_root("pattern-draft-recent");
        let pattern_file = root.join("crates/lazuli_codegen_go/src/emitter/patterns.rs");
        write_file(
            &pattern_file,
            "pub const PATTERN_COMMAND: (&str, &str) = (\"command\", \"draft\");\n",
        );
        let recent = 1_800_000_000_u64;
        if !init_git_repo_with_commit(&root, recent) {
            fs::remove_dir_all(&root).ok();
            return;
        }

        let diagnostics = check_pattern_draft_stale_001_at(&root, recent + 60);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }
