    // Doctor manifest + plugin + env + frontend + migration tests — split
    // from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_core::*;
    use super::test_support_packages::*;
    use crate::doctor::*;
    use crate::doctor::aggregators::env_manifest::{
        dedupe_env_contract_diagnostics, manifest_required_diagnostics,
    };

    #[test]
    fn doctor_manifest_required_skipped_when_no_plugin_refs() {
        let package = package_from_sources(vec![("app.lzi", "app NoManifest\n")]);
        let diagnostics = package.diagnostics();

        assert!(
            !codes(&diagnostics).contains("MANIFEST-REQUIRED-001"),
            "MANIFEST-REQUIRED-001 should not fire without @lazuli/plugin-* refs"
        );
    }

    #[test]
    fn doctor_runs_on_project_without_manifest() {
        let root =
            std::env::temp_dir().join(format!("lazuli-doctor-no-manifest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp doctor project");
        fs::write(root.join("app.lzi"), "app NoManifest\n").expect("write app.lzi");

        let result = doctor_command(&root, SecurityProfile::Strict, false, false);
        let _ = fs::remove_dir_all(&root);

        result.expect("doctor should pass without Lazurite.toml when no @lazuli/plugin-* refs");
    }

    #[test]
    fn doctor_passes_full_capsule_without_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/full-capsule");
        doctor_command(&root, SecurityProfile::Strict, false, false)
            .expect("full-capsule should pass without Lazurite.toml");
    }

    #[test]
    fn doctor_passes_auth_roundtrip_without_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/auth-roundtrip");
        doctor_command(&root, SecurityProfile::Strict, false, false)
            .expect("auth-roundtrip should pass without Lazurite.toml");
    }

    #[test]
    fn doctor_emits_manifest_required_when_lzi_refs_plugin_without_manifest() {
        let root = std::env::temp_dir().join(format!(
            "lazuli-doctor-manifest-required-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp doctor project");
        fs::write(
            root.join("app.lzi"),
            r#"
feature billing
  command charge
    policy @lazuli/plugin-payments
"#,
        )
        .expect("write app.lzi");

        let result = doctor_command(&root, SecurityProfile::Strict, false, false);
        let _ = fs::remove_dir_all(&root);

        let error = result.expect_err("doctor should fail when @plugin refs lack Lazurite.toml");
        assert!(
            error.to_string().contains("failed Lazuli doctor checks"),
            "unexpected error: {error:?}"
        );
    }

    /// Regression for the R1.C real-world sweep — `lazuli doctor file.lzi`
    /// (single-file invocation) must not emit `MANIFEST-REQUIRED-001`.
    /// The previous behavior treated the file's parent directory as the
    /// project root, scanned every sibling `.lzi`, and reported a phantom
    /// `Lazurite.toml` even when the target file itself had no plugin refs.
    #[test]
    fn doctor_skips_manifest_required_on_single_file_invocation() {
        let root =
            std::env::temp_dir().join(format!("lazuli-doctor-single-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp doctor project");

        // Sibling file in the parent dir uses @lazuli/plugin-payments, but we
        // are NOT going to invoke the doctor on it.
        fs::write(
            root.join("sibling.lzi"),
            r#"
feature billing
  command charge
    policy @lazuli/plugin-payments
"#,
        )
        .expect("write sibling.lzi");

        // The file we DO invoke the doctor on has no plugin refs.
        let target = root.join("clean.lzi");
        fs::write(
            &target,
            r#"
feature greetings
  query.list hello
    title "Hello"
"#,
        )
        .expect("write clean.lzi");

        let package = DoctorPackage::load(&target, SecurityProfile::Strict)
            .expect("load single-file package");
        let diagnostics = package.diagnostics();
        let _ = fs::remove_dir_all(&root);

        assert!(
            !codes(&diagnostics).contains("MANIFEST-REQUIRED-001"),
            "MANIFEST-REQUIRED-001 should not fire on single-file invocations; got: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    /// Regression for the R1.C real-world sweep — the LSP fires both
    /// `app-env-contract` and `env-schema-contract` on the same line of
    /// a `registry.env` block when the env declaration shape is invalid.
    /// The doctor layer dedupes them in favor of `env-schema-contract`.
    #[test]
    fn doctor_dedupes_env_contract_on_same_line() {
        // Invalid env declaration (missing `required|optional`) under
        // `registry.env.group <name>` triggers both `app-env-contract`
        // (via `validate_app_env_line`) and `env-schema-contract` in
        // the LSP. The doctor must collapse them into one diagnostic.
        // Sanity-check the upstream double-emission first via the LSP
        // directly so this test fails loudly if the LSP wiring changes.
        let source = r#"registry
  env
    group storage
      server S3_ENDPOINT: Text
"#;
        let lsp_diagnostics =
            lazuli_lsp::diagnostics_for_source_with_profile(source, SecurityProfile::Strict);
        let lsp_codes: Vec<String> = lsp_diagnostics
            .iter()
            .map(|d| DoctorDiagnostic::from_lsp(PathBuf::from("registry.lzi"), d).code)
            .collect();
        // The LSP layer is intentionally left noisy; doctor owns the dedupe.
        assert!(
            lsp_codes.iter().any(|c| c == "app-env-contract")
                && lsp_codes.iter().any(|c| c == "env-schema-contract"),
            "LSP should still emit both codes (dedupe lives in doctor); got: {lsp_codes:?}"
        );

        let root = std::env::temp_dir().join(format!(
            "lazuli-doctor-env-dedupe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp project");
        let target = root.join("registry.lzi");
        fs::write(&target, source).expect("write registry.lzi");

        let package = DoctorPackage::load(&target, SecurityProfile::Strict)
            .expect("load registry.lzi package");
        let diagnostics = package.diagnostics();
        let _ = fs::remove_dir_all(&root);

        let env_line_codes: Vec<&str> = diagnostics
            .iter()
            .filter(|d| {
                d.line == 4 && (d.code == "app-env-contract" || d.code == "env-schema-contract")
            })
            .map(|d| d.code.as_str())
            .collect();

        assert_eq!(
            env_line_codes.len(),
            1,
            "exactly one of app-env-contract / env-schema-contract should survive dedupe; got: {env_line_codes:?}"
        );
        assert_eq!(
            env_line_codes[0], "env-schema-contract",
            "env-schema-contract should win the dedupe (registry-scoped owner)"
        );
    }

    #[test]
    fn doctor_emits_plugin_not_declared_when_lzi_refs_undeclared_plugin() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.lzi",
                r#"
feature billing
  command charge
    policy @lazuli/plugin-payments
"#,
            )],
            &minimal_manifest(""),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("PLUGIN-NOT-DECLARED-001"));
    }

    #[test]
    fn doctor_emits_semantic_plugin_001_for_unknown_alias_without_plugin() {
        // B3 — `@semantic.BrazilianCPF` reference with no plugin in
        // `Lazurite.toml [plugins]` that declares the alias should
        // surface SEMANTIC-PLUGIN-001. See
        // `docs/proposals/semantic-types-plugin-locales.md` §New diagnostics.
        let package = package_from_sources_with_manifest(
            vec![(
                "host.lzi",
                r#"
feature host
  domain
    resource Host
      cpf: @semantic.BrazilianCPF optional
"#,
            )],
            &minimal_manifest(""),
        );
        let diagnostics = package.diagnostics();
        // The legacy `semantic_type_unknown` may also fire when no
        // alias map resolves the name; both share the same root cause
        // and either signals the gap to the author.
        let signals: Vec<&str> = diagnostics
            .iter()
            .filter(|d| {
                (d.code == "SEMANTIC-PLUGIN-001" || d.code == "semantic_type_unknown")
                    && d.message.contains("BrazilianCPF")
            })
            .map(|d| d.code.as_str())
            .collect();
        assert!(
            !signals.is_empty(),
            "expected SEMANTIC-PLUGIN-001 or semantic_type_unknown, got: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_plugin_unused_when_manifest_declares_unreferenced_plugin() {
        let package = package_from_sources_with_manifest(
            vec![("app.lzi", "app Demo\n")],
            &minimal_manifest(
                r#"
[plugins]
"@lazuli/plugin-payments" = { module = "example.com/payments", version = "v0.1.0" }
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("PLUGIN-UNUSED-001"));
    }

    #[test]
    fn doctor_emits_plugin_namespace_mismatch_for_known_plugin_adapter_ref() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.lzi",
                r#"
app Demo
  integrations
    payments adapter @adapter.payments
"#,
            )],
            &minimal_manifest(
                r#"
[plugins]
"@lazuli/plugin-payments" = { module = "example.com/payments", version = "v0.1.0" }
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("PLUGIN-NAMESPACE-MISMATCH-001"));
    }

    #[test]
    fn doctor_emits_submodule_drift_when_generated_go_runtime_differs() {
        let package = package_from_sources_with_manifest(
            vec![("app.lzi", "app Demo\n")],
            &minimal_manifest(
                r#"
[generate.go]
submodule = true
"#,
            ),
        );
        fs::write(
            package.project_root.join("go.mod"),
            "module example.com/demo\n\nrequire lazuli.dev/runtime v0.1.0\n",
        )
        .unwrap();
        fs::create_dir_all(package.project_root.join("dist/go")).unwrap();
        fs::write(
            package.project_root.join("dist/go/go.mod"),
            "module example.com/demo/dist\n\nrequire lazuli.dev/runtime v0.2.0\n",
        )
        .unwrap();

        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("SUBMODULE-DRIFT-001"));
    }

    #[test]
    fn doctor_emits_migration_strategy_conflict_for_manual_before_deploy() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.lzi",
                r#"
app Demo
  deploy
    migrations before_deploy
"#,
            )],
            &minimal_manifest(
                r#"
[migrations]
generated = "migrations/generated"
manual = "migrations/manual"
strategy = "manual"
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("MIGRATION-STRATEGY-CONFLICT-001"));
    }

    #[test]
    fn doctor_emits_frontend_audience_unknown_for_manifest_only_audience() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.web.lzx",
                r#"
surface demo web
  audience admin
    view dashboard Page
"#,
            )],
            &minimal_manifest(
                r#"
[frontends.web]
target = "tanstack-vite"
out = "dist/ts-web"
audiences = ["unknown"]
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("FRONTEND-AUDIENCE-UNKNOWN-001"));
    }

    #[test]
    fn doctor_emits_audience_no_frontend_for_unshipped_lzx_audience() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.web.lzx",
                r#"
surface demo web
  audience admin
    view dashboard Page
"#,
            )],
            &minimal_manifest(
                r#"
[frontends.web]
target = "tanstack-vite"
out = "dist/ts-web"
audiences = []
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("AUDIENCE-NO-FRONTEND-001"));
    }

    #[test]
    fn doctor_emits_frontend_out_collision_defensively() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.web.lzx",
                r#"
surface demo web
  audience admin
    view dashboard Page
"#,
            )],
            &minimal_manifest(
                r#"
[frontends.admin]
target = "tanstack-vite"
out = "dist/ts"
audiences = ["admin"]

[frontends.web]
target = "tanstack-vite"
out = "dist/ts"
audiences = ["admin"]
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("FRONTEND-OUT-COLLISION-001"));
    }

