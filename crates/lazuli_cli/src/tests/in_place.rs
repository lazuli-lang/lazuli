    // `lazuli new --in-place` flow tests — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use std::fs;

    use tempfile::TempDir;

    use crate::new_command;

    #[test]
    fn in_place_appends_manifest_block() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[lazuli]"));
        assert!(manifest.contains("[frontends.web]"));
        assert!(manifest.contains("target = \"tanstack-vite\""));
        assert!(manifest.contains("source = \"app/web\""));
    }

    #[test]
    fn in_place_preserves_existing_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("app/web")).unwrap();
        fs::write(
            root.join("app/web/tailwind.config.ts"),
            "// custom tailwind\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("app/web/tailwind.config.ts")).unwrap(),
            "// custom tailwind\n"
        );
    }

    #[test]
    fn in_place_writes_missing_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        assert!(root.join("app/web/index.html").is_file());
        assert!(root.join("app/web/main.tsx").is_file());
        assert!(root.join("app/web/shell/root.tsx").is_file());
        assert!(root.join("app/web/shell/layout.tsx").is_file());
        assert!(root.join("app/web/theme/theme_provider.tsx").is_file());
        assert!(root.join("app/web/theme/globals.css").is_file());
        assert!(root.join("app/web/tailwind.config.ts").is_file());
        assert!(root.join("app/web/tsconfig.json").is_file());
        assert!(root.join("app/web/vite.config.ts").is_file());
    }

    #[test]
    fn in_place_without_manifest_errors() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        let err = new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("no Lazurite project in")
                && err
                    .to_string()
                    .contains("run without --in-place to scaffold a new project"),
            "{err:#}"
        );
    }

    #[test]
    fn in_place_merges_package_json() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("app/web")).unwrap();
        fs::write(
            root.join("app/web/package.json"),
            r#"{
  "name": "custom-app",
  "dependencies": {
    "left-pad": "1.3.0",
    "react": "18.0.0"
  },
  "devDependencies": {
    "custom-dev-tool": "0.1.0"
  }
}
"#,
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        let package_json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("app/web/package.json")).unwrap())
                .unwrap();
        assert_eq!(package_json["name"], "custom-app");
        assert_eq!(package_json["dependencies"]["left-pad"], "1.3.0");
        assert_eq!(package_json["dependencies"]["react"], "18.0.0");
        assert_eq!(package_json["devDependencies"]["custom-dev-tool"], "0.1.0");
        assert!(package_json["dependencies"]["@tanstack/react-query"].is_string());
        assert!(package_json["dependencies"]["@lazuli/runtime"].is_string());
        assert!(package_json["devDependencies"]["vite"].is_string());
    }
