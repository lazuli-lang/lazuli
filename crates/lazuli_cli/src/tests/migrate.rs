    // Migrate / go-work CLI tests — split from `crates/lazuli_cli/src/tests.rs`
    // by the R10-A pass. Indent preserved at the original (4-space) level so
    // raw-string fixture content is byte-identical.

    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::Parser;
    use tempfile::TempDir;

    use crate::go_work_io::add_missing_go_work_use_entries;
    use crate::{Cli, Commands, MigrateCommand, write_go_work_preserving_entries};

    #[test]
    fn go_work_preserve_adds_dist_go_without_dropping_runtime() {
        let original = "go 1.26.0\n\nuse (\n\t.\n\tc:/Users/lucas/lazuli/runtime/go\n)\n";
        let generated = "go 1.26.0\n\nuse (\n\t.\n\t./dist/go\n)\n";
        let updated = add_missing_go_work_use_entries(
            original,
            &crate::go_work_io::extract_go_work_use_entries(generated),
        );

        assert!(updated.contains("\t.\n"));
        assert!(updated.contains("\t./dist/go\n"));
        assert!(updated.contains("\tc:/Users/lucas/lazuli/runtime/go\n"));
        assert_eq!(updated.matches("./dist/go").count(), 1);
    }

    #[test]
    fn go_work_preserve_creates_missing_file_from_generated_contents() {
        let root = TempDir::new().unwrap();
        let generated = "go 1.26.0\n\nuse (\n\t.\n\t./dist/go\n)\n";

        write_go_work_preserving_entries(root.path(), generated).unwrap();

        let written = fs::read_to_string(root.path().join("go.work")).unwrap();
        assert_eq!(written, generated);
    }

    #[test]
    fn migrate_action_up_parses_target_flag() {
        let cli = Cli::try_parse_from([
            "lazuli",
            "migrate",
            "up",
            "--target",
            "20260513_001_account_user",
            "--yes",
        ])
        .unwrap();

        let Commands::Migrate {
            sub: MigrateCommand::Up { target, yes: true },
        } = cli.command
        else {
            panic!("expected migrate up command");
        };
        assert_eq!(target.as_deref(), Some("20260513_001_account_user"));
    }

    #[test]
    fn migrate_dsl_parses_from_to_and_dry_run() {
        let cli = Cli::try_parse_from([
            "lazuli",
            "migrate",
            "dsl",
            "--from",
            "v0.11",
            "--to",
            "v0.12",
            "--dry-run",
        ])
        .unwrap();
        let Commands::Migrate {
            sub:
                MigrateCommand::Dsl {
                    from,
                    to,
                    dry_run,
                    path,
                },
        } = cli.command
        else {
            panic!("expected migrate dsl command");
        };
        assert_eq!(from, "v0.11");
        assert_eq!(to, "v0.12");
        assert!(dry_run);
        assert!(path.is_none());
    }

    #[test]
    fn migrate_dsl_bootstrap_recipe_rewrites_real_source_end_to_end() {
        // End-to-end: stand up a tempdir project with the bootstrap
        // recipe (mirrored from
        // `migrations/recipes/v0.11-to-v0.12/00-rename-validates-resource.md`)
        // plus a real-shaped .lzi file using the legacy
        // `validates resource @validator.X` form. After
        // `run_migrate_dsl`, the file must (a) reflect the modern
        // form (b) parse cleanly via
        // `lazuli_syntax::parse_feature_skeletons` (c) survive a
        // second migrate pass as a no-op.
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root: PathBuf = std::env::temp_dir().join(format!(
            "lazuli-migrate-dsl-e2e-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        let bootstrap = "---\n\
                         name: rename-validates-resource-keyword\n\
                         applies_to: .lzi\n\
                         match: |\n\
                         \x20\x20${indent:ws}validates resource @validator.${ref}\n\
                         replace: |\n\
                         \x20\x20${indent}validates @validator.${ref}\n\
                         description: Tier-4 cleanup.\n\
                         ---\n";
        fs::write(
            recipe_dir.join("00-rename-validates-resource.md"),
            bootstrap,
        )
        .unwrap();

        let feature_dir = root.join("features/customer");
        fs::create_dir_all(&feature_dir).unwrap();
        let original = "feature customer\n\
                        \x20\x20resource Customer\n\
                        \x20\x20\x20\x20name: Text\n\
                        \x20\x20\x20\x20validates resource @validator.row_check\n";
        let lzi_path = feature_dir.join("customer.lzi");
        fs::write(&lzi_path, original).unwrap();

        let report = crate::migrate::dsl::run_migrate_dsl(&root, "v0.11", "v0.12", false)
            .expect("migrate dsl");
        assert_eq!(report.changed.len(), 1, "report = {report:?}");
        assert!(report.rolled_back.is_empty(), "report = {report:?}");
        let after = fs::read_to_string(&lzi_path).unwrap();
        assert!(after.contains("validates @validator.row_check"));
        assert!(!after.contains("validates resource"));

        // Survives a sanity reparse via the canonical feature-skeleton parser.
        lazuli_syntax::parse_feature_skeletons(&after).expect("reparse rewritten .lzi");

        // Second pass is a no-op: no legacy form left to match.
        let report2 = crate::migrate::dsl::run_migrate_dsl(&root, "v0.11", "v0.12", false)
            .expect("second migrate dsl");
        assert!(report2.changed.is_empty());

        let _ = fs::remove_dir_all(&root);
    }
