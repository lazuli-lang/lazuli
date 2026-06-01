    // 0020 — doctor↔codegen plugin-resolution AGREEMENT (doctor half).
    //
    // These tests pin the doctor side of the agreement invariant: the
    // plugin-semantic checks resolve `@semantic.<X>` through the SAME
    // upward-walked root + alias map codegen uses, so `SEMANTIC-PLUGIN-001`
    // fires iff the same reference would leave a residual on the generate
    // path. The full cross-surface drift guard (doctor set == generate
    // residual set) lives in `lazuli_cli`'s `plugin_doctor_agreement.rs`
    // integration test, where `lazuli_cli::module_loader` is reachable
    // (this engine crate must NOT depend on `lazuli_cli`).

    use std::fs;
    use std::path::{Path, PathBuf};

    use super::test_support_core::*;
    use crate::doctor::*;

    fn plugin_agree_fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("plugin_agree")
    }

    /// SEMANTIC-PLUGIN-001 findings naming a specific `@semantic.<X>`.
    fn semantic_plugin_001_for(diagnostics: &[DoctorDiagnostic], alias: &str) -> usize {
        diagnostics
            .iter()
            .filter(|d| d.code == "SEMANTIC-PLUGIN-001" && d.message.contains(alias))
            .count()
    }

    /// `lazuli doctor app` (features subdir, manifest one level UP) resolves
    /// the repo-root `[plugins]` via the SHARED upward walk — NO false
    /// `SEMANTIC-PLUGIN-001` for `@semantic.Foo`. Proves the shared
    /// `find_project_root` replaced `doctor_project_root`'s pass-through:
    /// before 0020 this input either short-circuited to silence (no manifest
    /// at `app/`) or false-flagged Foo.
    #[test]
    fn doctor_resolves_plugin_from_features_subdir() {
        let subdir = plugin_agree_fixture().join("app");
        let diagnostics = DoctorPackage::load(&subdir, SecurityProfile::Strict)
            .expect("load package from features subdir")
            .diagnostics();

        assert_eq!(
            semantic_plugin_001_for(&diagnostics, "@semantic.Foo"),
            0,
            "doctor run from app/ must resolve @semantic.Foo via the upward walk \
             (no false SEMANTIC-PLUGIN-001); got: {:?}",
            diagnostics
                .iter()
                .filter(|d| d.code == "SEMANTIC-PLUGIN-001")
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    /// `find_project_root` (the SHARED walk, re-homed to `lazuli_manifest`)
    /// ascends from the `app/` subdir to the repo-root `Lazurite.toml` —
    /// the exact root codegen's resolver uses. This is the identity the
    /// agreement rests on: ONE walk, two surfaces.
    #[test]
    fn find_project_root_shared_with_codegen() {
        use lazuli_manifest::lazurite_manifest::find_project_root;

        let root = plugin_agree_fixture();
        let subdir = root.join("app");
        let found = find_project_root(&subdir).expect("upward walk finds the manifest root");
        assert_eq!(
            found.canonicalize().unwrap(),
            root.canonicalize().unwrap(),
            "find_project_root must ascend from app/ to the Lazurite.toml root"
        );
    }

    /// A fixture with `[plugins]` present but `@semantic.Bar` that no plugin
    /// provides: doctor fires SEMANTIC-PLUGIN-001 on `@semantic.Bar`. This is
    /// the doctor half of `doctor_flags_unresolved_alias_generate_also_fails`
    /// — the generate half (residual on the same alias) is asserted in
    /// `lazuli_cli`'s agreement test against the SAME shape.
    #[test]
    fn doctor_flags_unresolved_alias() {
        let root =
            std::env::temp_dir().join(format!("lazuli-0020-unresolved-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        // Plugin provides Foo only.
        fs::create_dir_all(root.join("plugins/mini")).unwrap();
        fs::write(
            root.join("plugins/mini/manifest.toml"),
            r#"[plugin]
name = "mini"
namespace = "@lazuli/plugin-mini"

[[semantic_types]]
name = "Foo"
alias = "@semantic.Foo"
carrier_type = "String"
validator = "ValidateFoo"
"#,
        )
        .unwrap();
        fs::write(
            root.join("Lazurite.toml"),
            r#"[project]
name = "unresolved"
module = "example.test/unresolved"
schema = 1

[lazurite]
app_dir = "app"

[lazuli]
runtime = "0.1.0"

[plugins]
"@lazuli/plugin-mini" = { path = "plugins/mini" }
"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("app/features/widget")).unwrap();
        fs::write(root.join("app/app.lzi"), "app Unresolved\n").unwrap();
        // References @semantic.Bar — NO plugin provides it.
        fs::write(
            root.join("app/features/widget/widget.lzi"),
            "feature widget\n  domain\n    resource Widget\n      bar: @semantic.Bar required\n",
        )
        .unwrap();

        let diagnostics = DoctorPackage::load(&root.join("app"), SecurityProfile::Strict)
            .expect("load unresolved package")
            .diagnostics();
        let _ = fs::remove_dir_all(&root);

        assert!(
            semantic_plugin_001_for(&diagnostics, "@semantic.Bar") >= 1,
            "doctor must flag @semantic.Bar (no plugin provides it) — generate leaves \
             the same residual; got: {:?}",
            diagnostics
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
        // And it must NOT flag Foo (the plugin DOES provide it).
        assert_eq!(
            semantic_plugin_001_for(&diagnostics, "@semantic.Foo"),
            0,
            "doctor must resolve @semantic.Foo (plugin provides it)"
        );
    }

    /// Single `.lzi` with no ancestor `Lazurite.toml` → the upward walk
    /// returns the input root, the alias map is empty, `@semantic.Foo` is
    /// field-anchored SEMANTIC-PLUGIN-001 (or the legacy
    /// `semantic_type_unknown`), and the load does NOT panic / hard-error.
    /// Mirrors 0019's silent single-file case.
    #[test]
    fn single_file_check_stays_silent_no_hard_error() {
        let root =
            std::env::temp_dir().join(format!("lazuli-0020-singlefile-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let lzi = root.join("solo.lzi");
        fs::write(
            &lzi,
            "feature solo\n  domain\n    resource Solo\n      foo: @semantic.Foo required\n",
        )
        .unwrap();

        // No Lazurite.toml here. Loading must succeed (no panic) and leave
        // the alias unresolved for a field-anchored signal.
        let result = DoctorPackage::load(&lzi, SecurityProfile::Strict);
        let ok = result.is_ok();
        let diagnostics = result.map(|p| p.diagnostics()).unwrap_or_default();
        let _ = fs::remove_dir_all(&root);

        assert!(ok, "single-file doctor load must not hard-error");
        // The alias is unresolved — either SEMANTIC-PLUGIN-001 or the
        // legacy semantic_type_unknown signals it; both are acceptable and
        // NEITHER is a panic.
        let signalled = diagnostics.iter().any(|d| {
            (d.code == "SEMANTIC-PLUGIN-001" || d.code == "semantic_type_unknown")
                && d.message.contains("Foo")
        });
        assert!(
            signalled,
            "single-file unresolved @semantic.Foo must surface a field signal; got: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    /// An app with NO `[plugins]` block produces zero plugin-semantic
    /// findings (common-case regression — the bulk of apps).
    #[test]
    fn no_plugins_app_unchanged() {
        let root =
            std::env::temp_dir().join(format!("lazuli-0020-noplugins-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Lazurite.toml"),
            r#"[project]
name = "no-plugins"
module = "example.test/no-plugins"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();
        fs::write(root.join("app.lzi"), "app Plain\n").unwrap();
        fs::create_dir_all(root.join("features/plain")).unwrap();
        fs::write(
            root.join("features/plain/plain.lzi"),
            "feature plain\n  domain\n    resource Plain\n      title: Text required\n",
        )
        .unwrap();

        let diagnostics = DoctorPackage::load(&root, SecurityProfile::Strict)
            .expect("load no-plugins package")
            .diagnostics();
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "SEMANTIC-PLUGIN-001"
                    || d.code == "SEMANTIC-PLUGIN-002"
                    || d.code == "PLUGIN-UNUSED-001")
                .count(),
            0,
            "no-plugins app must produce zero plugin-semantic findings; got: {:?}",
            codes(&diagnostics)
        );
    }
