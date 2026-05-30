
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "lazuli-migrate-dsl-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn bootstrap_recipe() -> &'static str {
        "---\n\
         name: rename-validates-resource-keyword\n\
         applies_to: .lzi\n\
         match: |\n\
         \x20\x20${indent:ws}validates resource @validator.${ref}\n\
         replace: |\n\
         \x20\x20${indent}validates @validator.${ref}\n\
         description: Tier-4 follow-up retired the resource axis.\n\
         ---\n\
         # human prose\n"
    }

    #[test]
    fn parses_bootstrap_recipe_frontmatter() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("00-bootstrap.md")).unwrap();
        assert_eq!(recipe.name, "rename-validates-resource-keyword");
        assert_eq!(recipe.applies_to, AppliesTo::Lzi);
        assert_eq!(recipe.match_pattern.len(), 3);
        match &recipe.match_pattern[0] {
            PatternToken::Whitespace(name) => assert_eq!(name, "indent"),
            other => panic!("expected ws marker, got {other:?}"),
        }
        match &recipe.match_pattern[1] {
            PatternToken::Literal(lit) => assert_eq!(lit, "validates resource @validator."),
            other => panic!("expected literal, got {other:?}"),
        }
        match &recipe.match_pattern[2] {
            PatternToken::Token(name) => assert_eq!(name, "ref"),
            other => panic!("expected token marker, got {other:?}"),
        }
    }

    #[test]
    fn rejects_recipe_with_missing_frontmatter_close() {
        let raw = "---\nname: x\napplies_to: .lzi\nmatch: |\n  foo\nreplace: |\n  bar\n";
        let err = parse_recipe(raw, Path::new("bad.md")).unwrap_err();
        assert!(err.contains("not closed"), "got: {err}");
    }

    #[test]
    fn rejects_replace_referencing_undefined_slot() {
        let raw = "---\nname: bad\napplies_to: .lzi\nmatch: |\n  foo ${a}\nreplace: |\n  baz ${nope}\n---\n";
        let err = parse_recipe(raw, Path::new("bad.md")).unwrap_err();
        assert!(err.contains("not defined in match pattern"), "got: {err}");
    }

    #[test]
    fn rejects_replace_marker_with_type_suffix() {
        let raw = "---\nname: bad\napplies_to: .lzi\nmatch: |\n  foo ${a:ws}\nreplace: |\n  ${a:ws}\n---\n";
        let err = parse_recipe(raw, Path::new("bad.md")).unwrap_err();
        assert!(err.contains("must not carry a type suffix"), "got: {err}");
    }

    #[test]
    fn rejects_unknown_marker_type() {
        let raw =
            "---\nname: bad\napplies_to: .lzi\nmatch: |\n  ${a:weird}\nreplace: |\n  ${a}\n---\n";
        let err = parse_recipe(raw, Path::new("bad.md")).unwrap_err();
        assert!(err.contains("unknown marker type"), "got: {err}");
    }

    #[test]
    fn match_line_captures_ws_and_token() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("0.md")).unwrap();
        let line = "      validates resource @validator.tier_check";
        let caps = match_line(line, &recipe.match_pattern).expect("must match");
        assert_eq!(caps[0], ("indent".to_owned(), "      ".to_owned()));
        assert_eq!(caps[1], ("ref".to_owned(), "tier_check".to_owned()));
    }

    #[test]
    fn match_line_rejects_when_keyword_missing() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("0.md")).unwrap();
        assert!(
            match_line(
                "      validates @validator.tier_check",
                &recipe.match_pattern
            )
            .is_none()
        );
    }

    #[test]
    fn apply_recipe_rewrites_matching_lines_only() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("0.md")).unwrap();
        let src = "feature customer\n\
                   \x20\x20resource Customer\n\
                   \x20\x20\x20\x20# legacy form below\n\
                   \x20\x20\x20\x20validates resource @validator.row_check\n\
                   \x20\x20\x20\x20\x20\x20validates resource @validator.tier_check\n\
                   \x20\x20\x20\x20validates @validator.already_modern\n";
        let out = apply_recipe(src, &recipe);
        assert!(
            out.contains("    validates @validator.row_check"),
            "output: {out}"
        );
        assert!(out.contains("      validates @validator.tier_check"));
        assert!(out.contains("    validates @validator.already_modern"));
        assert!(!out.contains("validates resource"));
        // Non-matching lines untouched (headers, etc.).
        assert!(out.starts_with("feature customer\n"));
    }

    #[test]
    fn apply_recipe_preserves_trailing_newline_state() {
        let recipe = parse_recipe(bootstrap_recipe(), Path::new("0.md")).unwrap();
        let with_nl = "  validates resource @validator.x\n";
        let without_nl = "  validates resource @validator.x";
        assert!(apply_recipe(with_nl, &recipe).ends_with('\n'));
        assert!(!apply_recipe(without_nl, &recipe).ends_with('\n'));
    }

    #[test]
    fn walk_lazuli_sources_skips_dist_and_target() {
        let root = temp_dir("walk");
        fs::create_dir_all(root.join("features/customer")).unwrap();
        fs::create_dir_all(root.join("dist/go")).unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::write(root.join("app.lzi"), "app A\n").unwrap();
        fs::write(root.join("features/customer/customer.lzi"), "").unwrap();
        fs::write(root.join("features/customer/customer.web.lzx"), "").unwrap();
        fs::write(root.join("dist/go/should-skip.lzi"), "").unwrap();
        fs::write(root.join("target/debug/should-skip.lzi"), "").unwrap();
        fs::write(root.join("features/customer/notes.txt"), "").unwrap();

        let mut found = walk_lazuli_sources(&root).unwrap();
        found.sort();
        let names: Vec<_> = found
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(names.contains(&"app.lzi".to_owned()));
        assert!(names.contains(&"features/customer/customer.lzi".to_owned()));
        assert!(names.contains(&"features/customer/customer.web.lzx".to_owned()));
        for n in &names {
            assert!(!n.contains("dist/"), "should skip dist: {n}");
            assert!(!n.contains("target/"), "should skip target: {n}");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_errors_on_missing_recipe_dir() {
        let root = temp_dir("no-recipes");
        let err = run_migrate_dsl(&root, "v0.11", "v0.12", false).unwrap_err();
        assert!(err.to_string().contains("no DSL recipes"), "got: {}", err);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_dry_run_does_not_write() {
        let root = temp_dir("dry-run");
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        fs::write(recipe_dir.join("00-bootstrap.md"), bootstrap_recipe()).unwrap();

        let lzi_path = root.join("customer.lzi");
        let original = "feature customer\n\
                        \x20\x20resource Customer\n\
                        \x20\x20\x20\x20validates resource @validator.row_check\n";
        fs::write(&lzi_path, original).unwrap();

        let report = run_migrate_dsl(&root, "v0.11", "v0.12", true).unwrap();
        assert_eq!(report.changed.len(), 0, "report = {report:?}");
        assert_eq!(report.dry_run_changes.len(), 1, "report = {report:?}");
        assert_eq!(fs::read_to_string(&lzi_path).unwrap(), original);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_writes_when_not_dry_run() {
        let root = temp_dir("write");
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        fs::write(recipe_dir.join("00-bootstrap.md"), bootstrap_recipe()).unwrap();

        let lzi_path = root.join("customer.lzi");
        fs::write(
            &lzi_path,
            "feature customer\n\
             \x20\x20resource Customer\n\
             \x20\x20\x20\x20validates resource @validator.row_check\n",
        )
        .unwrap();

        let report = run_migrate_dsl(&root, "v0.11", "v0.12", false).unwrap();
        assert_eq!(report.changed.len(), 1, "report = {report:?}");
        let after = fs::read_to_string(&lzi_path).unwrap();
        assert!(after.contains("validates @validator.row_check"));
        assert!(!after.contains("validates resource"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_rolls_back_on_parse_failure() {
        let root = temp_dir("rollback");
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        // A pathological recipe that mangles field declarations
        // inside a `resource` body into bogus content the Lazuli
        // parser rejects. `bogus_keyword` matches no resource-child
        // verb and carries no colon, so it falls through to the
        // "resource children are..." error and trips the rollback.
        let bad = "---\n\
                   name: corrupt-resource-field\n\
                   applies_to: .lzi\n\
                   match: |\n\
                   \x20\x20${indent:ws}name: ${ty}\n\
                   replace: |\n\
                   \x20\x20${indent}bogus_keyword\n\
                   ---\n";
        fs::write(recipe_dir.join("00-break.md"), bad).unwrap();

        let lzi_path = root.join("broken.lzi");
        let original = "feature demo\n  resource Demo\n    name: Text\n";
        fs::write(&lzi_path, original).unwrap();

        let report = run_migrate_dsl(&root, "v0.11", "v0.12", false).unwrap();
        assert_eq!(
            report.rolled_back.len(),
            1,
            "expected rollback, got report = {:?}",
            report
        );
        assert!(
            report.rolled_back[0]
                .1
                .contains("post-transform parse failure")
        );
        // Source file untouched on disk.
        assert_eq!(fs::read_to_string(&lzi_path).unwrap(), original);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_migrate_dsl_walks_multi_file_project() {
        let root = temp_dir("multi");
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        fs::write(recipe_dir.join("00-bootstrap.md"), bootstrap_recipe()).unwrap();

        let feature_dir = root.join("features/customer");
        fs::create_dir_all(&feature_dir).unwrap();
        let one = "feature one\n  resource One\n    validates resource @validator.in_one\n";
        let two = "feature two\n  resource Two\n    validates resource @validator.in_two\n";
        let lzx_legacy = "  validates resource @validator.in_lzx\n";
        fs::write(root.join("one.lzi"), one).unwrap();
        fs::write(feature_dir.join("customer.lzi"), two).unwrap();
        // .lzx file should be skipped by an `.lzi`-only recipe.
        fs::write(feature_dir.join("customer.lzx"), lzx_legacy).unwrap();

        let report = run_migrate_dsl(&root, "v0.11", "v0.12", false).unwrap();
        assert_eq!(report.changed.len(), 2, "report = {report:?}");
        assert!(
            fs::read_to_string(root.join("one.lzi"))
                .unwrap()
                .contains("validates @validator.in_one")
        );
        assert!(
            fs::read_to_string(feature_dir.join("customer.lzi"))
                .unwrap()
                .contains("validates @validator.in_two")
        );
        // .lzx untouched.
        assert_eq!(
            fs::read_to_string(feature_dir.join("customer.lzx")).unwrap(),
            lzx_legacy
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn render_report_dry_run_emits_diff() {
        let diff = DslDiff {
            file: PathBuf::from("a.lzi"),
            before: "  validates resource @validator.x\n".to_owned(),
            after: "  validates @validator.x\n".to_owned(),
        };
        let report = DslReport {
            dry_run_changes: vec![diff],
            recipes_applied: vec!["bootstrap".to_owned()],
            ..DslReport::default()
        };
        let out = render_report(&report, true);
        assert!(out.contains("would change"));
        assert!(out.contains("validates resource"));
        assert!(out.contains("validates @validator.x"));
    }

    #[test]
    fn pattern_token_with_following_literal_backtracks() {
        // ${name} should consume only up to where the next literal
        // starts matching, not greedily to whitespace.
        let raw =
            "---\nname: t\napplies_to: .lzi\nmatch: |\n  ${a}/${b}\nreplace: |\n  ${b}/${a}\n---\n";
        let recipe = parse_recipe(raw, Path::new("t.md")).unwrap();
        let caps = match_line("alpha/beta", &recipe.match_pattern).unwrap();
        assert!(caps.iter().any(|(k, v)| k == "a" && v == "alpha"));
        assert!(caps.iter().any(|(k, v)| k == "b" && v == "beta"));
    }
