    // Scaffold / template / case-helper tests — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        REGISTRY_TEMPLATE, app_template, default_module_name, pascal_case,
        pascal_case_project_name, scaffold_bare, scaffold_from_template, templates,
    };

    #[test]
    fn pascal_case_converts_project_names() {
        assert_eq!(pascal_case("my-app"), "MyApp");
        assert_eq!(pascal_case("acme_crm"), "AcmeCrm");
        assert_eq!(pascal_case("123-api"), "App123Api");
    }

    #[test]
    fn pascal_case_project_name_handles_kebab_and_snake() {
        assert_eq!(
            pascal_case_project_name(Path::new("my-app")).unwrap(),
            "MyApp"
        );
        assert_eq!(
            pascal_case_project_name(Path::new("acme_crm")).unwrap(),
            "AcmeCrm"
        );
    }

    #[test]
    fn default_module_name_derives_from_project_name() {
        assert_eq!(default_module_name(Path::new("my-app")), "lazuli/my-app");
        assert_eq!(
            default_module_name(Path::new("acme_crm")),
            "lazuli/acme-crm"
        );
        assert_eq!(default_module_name(Path::new("AcmeCRM")), "lazuli/acme-crm");
    }

    #[test]
    fn scaffold_bare_writes_minimal_files() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("lazuli-bare-test-{}-{suffix}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        let bare = root.join("bare-app");
        scaffold_bare(&bare, "BareApp").unwrap();
        assert_eq!(
            fs::read_to_string(bare.join("app.lzi")).unwrap(),
            app_template("BareApp")
        );
        assert_eq!(
            fs::read_to_string(bare.join("registry.lzi")).unwrap(),
            REGISTRY_TEMPLATE
        );
        assert!(bare.join("README.md").is_file());
        assert!(bare.join(".gitignore").is_file());
        assert!(bare.join("features").join(".gitkeep").is_file());
        assert!(!bare.join("Lazurite.toml").exists());
        assert!(!bare.join("features").join("account").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scaffold_from_template_substitutes_placeholders() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-substitute-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "github.com/me/myapp",
        )
        .unwrap();
        assert!(
            fs::read_to_string(root.join("app/app.lzi"))
                .unwrap()
                .contains("app MyApp")
        );
        assert!(
            fs::read_to_string(root.join("go.mod"))
                .unwrap()
                .contains("module github.com/me/myapp")
        );
        assert!(
            fs::read_to_string(root.join("README.md"))
                .unwrap()
                .contains("# MyApp")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scaffold_from_template_strips_tmpl_extension() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-extension-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "lazuli/my-app",
        )
        .unwrap();
        assert!(root.join("app/app.lzi").is_file());
        assert!(!root.join("app/app.lzi.tmpl").exists());
        assert!(root.join("app/features/account/account.lzi").is_file());
        assert!(!root.join("app/features/account/account.lzi.tmpl").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scaffold_from_template_smoke_tree_matches_expected() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-smoke-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "lazuli/my-app",
        )
        .unwrap();
        // Handler starter `.go` files are not scaffolded — the codegen
        // handler-stub emitter (`crates/lazuli_codegen_go/src/emitter/
        // handlers.rs`) lays them down in `dist/go/<feature>/<name>.go`
        // on first `lazuli generate go`. The scaffold owns `.lzi` /
        // `.lzx` / `.tmpl` (notification templates) / config; user Go
        // handlers materialise via the codegen path.
        for relative in [
            ".gitignore",
            "README.md",
            "app/app.lzi",
            "app/design.lzi",
            "go.mod",
            "go.work",
            "Lazurite.toml",
            "app/registry.lzi",
            "app/features/account/account.lzi",
            "app/features/account/templates/welcome.en-US",
            "app/features/account/templates/welcome.pt-BR",
            "i18n/common.en-US.json",
            "scripts/seed.sh",
            ".env.example",
            "docker-compose.yml",
            "scripts/bootstrap-storage.sh",
        ] {
            assert!(root.join(relative).is_file(), "missing {relative}");
        }

        // The bootstrap-storage script substitutes `{{app_slug}}` as a
        // bash-default fallback; the `.tmpl` suffix is stripped.
        let bootstrap = fs::read_to_string(root.join("scripts/bootstrap-storage.sh"))
            .expect("read bootstrap-storage.sh");
        assert!(
            bootstrap.contains(":-my_app"),
            "bootstrap-storage.sh should embed app_slug as a default: {bootstrap}"
        );
        let env_example = fs::read_to_string(root.join(".env.example")).expect("read .env.example");
        assert!(
            env_example.contains("S3_ENDPOINT="),
            ".env.example should declare S3_ENDPOINT"
        );
        let compose =
            fs::read_to_string(root.join("docker-compose.yml")).expect("read docker-compose.yml");
        assert!(
            compose.contains("MINIO_ROOT_USER_FILE: \"\""),
            "docker-compose.yml should clear MinIO _FILE defaults"
        );

        // Schema-drift gate: the freshly scaffolded tree must pass
        // `lazuli doctor` under the strict profile — the same sanity check
        // `lazuli new` runs (but swallows into a `warning:` line). Asserting
        // it here turns the swallowed check into a CI gate: a schema bump not
        // mirrored into the hardcoded `app.lzi.tmpl` `lazuli_version` pin (or
        // any template that drifts off the current grammar) starts failing
        // this test instead of silently shipping broken new projects.
        // `allow_version_mismatch = false` so the pin is actually checked.
        crate::doctor::doctor_command(
            &root,
            Some(lazuli_doctor_config::DoctorProfile::Strict),
            false,
            false,
        )
        .expect("freshly scaffolded project must pass `lazuli doctor` (strict)");

        let _ = fs::remove_dir_all(root);
    }
