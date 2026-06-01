    // 0022 — PLUGIN-CONTRACT-001 doctor gate (the contract-rule half).
    //
    // Builds a temp project with one local adapter plugin and asserts the
    // doctor fires PLUGIN-CONTRACT-001 (error) when its declared
    // `implements` interface is unknown, and stays clean when it names a
    // real bucket interface. Both surfaces (this gate + `lazuli plugin
    // verify`'s L3) call the SHARED
    // `lazuli_manifest::plugin_contract::classify_adapter_contract`; the
    // verify direction of the drift guard lives in `lazuli_cli`'s
    // `plugin_verify.rs` integration test.

    use std::fs;

    use super::test_support_core::*;
    use crate::doctor::*;

    /// Write a minimal project: `Lazurite.toml` + one local plugin whose
    /// `manifest.toml` declares `implements = [<iface>]`.
    fn project_with_adapter(tag: &str, iface: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "lazuli-0022-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("plugins/paygw")).unwrap();
        fs::write(
            root.join("plugins/paygw/manifest.toml"),
            format!(
                r#"implements = ["{iface}"]

[plugin]
name = "paygw"
namespace = "@lazuli/plugin-paygw"
go_module = "example.test/plugin/paygw"

[env]
required = []
"#
            ),
        )
        .unwrap();
        fs::write(
            root.join("Lazurite.toml"),
            r#"[project]
name = "contract-fixture"
module = "example.test/contract"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@lazuli/plugin-paygw" = { path = "plugins/paygw" }
"#,
        )
        .unwrap();
        fs::write(root.join("app.lzi"), "app Contract\n").unwrap();
        root
    }

    fn contract_001(diagnostics: &[DoctorDiagnostic]) -> Vec<&DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code == "PLUGIN-CONTRACT-001")
            .collect()
    }

    #[test]
    fn plugin_contract_001_fires_on_unknown_interface() {
        // Typo: `PaymentGatway` is not a known bucket interface.
        let root = project_with_adapter("unknown", "payments.PaymentGatway");
        let diagnostics = DoctorPackage::load(&root, SecurityProfile::Strict)
            .expect("load package")
            .diagnostics();
        let fired = contract_001(&diagnostics);
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            fired.len(),
            1,
            "exactly one PLUGIN-CONTRACT-001 expected; got: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        let d = fired[0];
        assert_eq!(d.severity, DoctorSeverity::Error);
        assert!(d.message.contains("@lazuli/plugin-paygw"));
        assert!(d.message.contains("payments.PaymentGatway"));
        // did-you-mean hint to the nearest real interface.
        assert!(d.message.contains("payments.PaymentGateway"));
        // honest static-limit tail.
        assert!(d.message.contains("var _ <Interface> = (*Adapter)(nil)"));
    }

    #[test]
    fn plugin_contract_001_clean_on_ok_adapter() {
        let root = project_with_adapter("ok", "payments.PaymentGateway");
        let diagnostics = DoctorPackage::load(&root, SecurityProfile::Strict)
            .expect("load package")
            .diagnostics();
        let fired = contract_001(&diagnostics);
        let _ = fs::remove_dir_all(&root);

        assert!(
            fired.is_empty(),
            "a real bucket interface must NOT fire PLUGIN-CONTRACT-001; got: {:?}",
            fired.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn plugin_contract_001_na_on_semantic_only_plugin() {
        // A semantic-only plugin (no implements/[binds]) contributes no
        // contract link — never a FAIL.
        let root = std::env::temp_dir().join(format!(
            "lazuli-0022-semantic-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("plugins/scal")).unwrap();
        fs::write(
            root.join("plugins/scal/manifest.toml"),
            r#"[plugin]
name = "scal"
namespace = "@lazuli/plugin-scal"

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
name = "semantic-fixture"
module = "example.test/semantic"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@lazuli/plugin-scal" = { path = "plugins/scal" }
"#,
        )
        .unwrap();
        fs::write(root.join("app.lzi"), "app Semantic\n").unwrap();

        let diagnostics = DoctorPackage::load(&root, SecurityProfile::Strict)
            .expect("load package")
            .diagnostics();
        let fired = contract_001(&diagnostics);
        let _ = fs::remove_dir_all(&root);

        assert!(
            fired.is_empty(),
            "semantic-only plugin must not fire PLUGIN-CONTRACT-001"
        );
    }
