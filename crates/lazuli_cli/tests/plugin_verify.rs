//! `lazuli plugin verify` CLI tests (spec 0022).
//!
//! Drives `build_reports` over the `ok-adapter` / `bad-adapter` fixtures
//! and the `--json`/`--plugin` surfaces, plus the verify↔doctor drift
//! guard. The drift-guard half (that the same fixture is BOTH an L3 `Fail`
//! in verify AND a `PLUGIN-CONTRACT-001` in doctor) is anchored here in the
//! verify direction; the doctor direction lives in
//! `lazuli_doctor_run`'s `plugin_contract` tests, and both call the shared
//! `lazuli_manifest::plugin_contract::classify_adapter_contract`.

use std::path::{Path, PathBuf};
use std::process::Command;

use lazuli_cli::lazurite_manifest;
use lazuli_cli::plugin_verify::{LinkStatus, Overall, build_reports, run_plugin_verify};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("plugin-verify")
        .join(name)
}

fn load(name: &str) -> (lazurite_manifest::Manifest, PathBuf) {
    let root = fixture(name);
    let manifest = lazurite_manifest::load(&root)
        .expect("load Lazurite.toml")
        .expect("fixture has a Lazurite.toml");
    (manifest, root)
}

fn link_status(report: &lazuli_cli::plugin_verify::PluginVerifyReport, id: &str) -> LinkStatus {
    report
        .links
        .iter()
        .find(|l| l.id == id)
        .unwrap_or_else(|| panic!("link {id} not found in {:?}", report.plugin))
        .status
        .clone()
}

#[test]
fn plugin_verify_passes_on_ok_adapter() {
    let (manifest, root) = load("ok-adapter");
    let reports = build_reports(&manifest, &root, None);
    assert_eq!(reports.len(), 1);
    let r = &reports[0];
    assert_eq!(r.plugin, "@lazuli/plugin-paygw");
    assert_eq!(r.overall, Overall::Pass, "links: {:#?}", r.links);
    // Every link is Pass or n/a — never Fail/Skipped.
    for l in &r.links {
        assert!(
            matches!(l.status, LinkStatus::Pass | LinkStatus::Na),
            "link {} should be Pass/n/a, got {:?}: {}",
            l.id,
            l.status,
            l.detail
        );
    }
    assert_eq!(link_status(r, "L1 manifest"), LinkStatus::Pass);
    assert_eq!(link_status(r, "L3 contract"), LinkStatus::Pass);
    assert_eq!(link_status(r, "L4 import"), LinkStatus::Pass);
    assert_eq!(link_status(r, "L5 env"), LinkStatus::Pass);
}

#[test]
fn plugin_verify_fails_on_bad_adapter_with_broken_link() {
    let (manifest, root) = load("bad-adapter");
    let reports = build_reports(&manifest, &root, None);
    assert_eq!(reports.len(), 1);
    let r = &reports[0];
    assert_eq!(r.overall, Overall::Fail);
    // The broken link is L3 contract (unknown interface).
    assert_eq!(link_status(r, "L3 contract"), LinkStatus::Fail);
    let l3 = r.links.iter().find(|l| l.id == "L3 contract").unwrap();
    assert!(
        l3.detail
            .contains("implements 'payments.PaymentGatway' is not a known bucket interface"),
        "L3 detail should name the unknown interface, got: {}",
        l3.detail
    );
    assert!(
        l3.detail.contains("did you mean 'payments.PaymentGateway'"),
        "L3 detail should carry the did-you-mean hint, got: {}",
        l3.detail
    );
    // Honest-limit note present on the L3 line.
    assert!(l3.detail.contains("var _ <Interface> = (*Adapter)(nil)"));
}

#[test]
fn plugin_verify_run_returns_nonzero_on_bad_adapter() {
    // run_plugin_verify renders + returns the exit code (no process::exit
    // in the inner fn). bad-adapter → exit 1.
    let root = fixture("bad-adapter");
    let code = run_plugin_verify(&root, None, true).expect("verify runs");
    assert_eq!(code, 1);

    let ok_code = run_plugin_verify(&fixture("ok-adapter"), None, true).expect("verify runs");
    assert_eq!(ok_code, 0);
}

#[test]
fn plugin_verify_json_shape() {
    use lazuli_cli::plugin_verify::VerifyDocument;
    let (manifest, root) = load("bad-adapter");
    let reports = build_reports(&manifest, &root, None);
    let ok = reports.iter().all(|r| r.overall == Overall::Pass);
    let doc = VerifyDocument {
        plugins: reports,
        ok,
    };
    let json = serde_json::to_string(&doc).expect("serialize");
    // Parseable + carries the failing link with id/status/detail.
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(value["ok"], serde_json::json!(false));
    let plugins = value["plugins"].as_array().expect("plugins array");
    assert_eq!(plugins.len(), 1);
    let links = plugins[0]["links"].as_array().expect("links array");
    let l3 = links
        .iter()
        .find(|l| l["id"] == "L3 contract")
        .expect("L3 link present");
    assert_eq!(l3["status"], serde_json::json!("fail"));
    assert!(l3["detail"].as_str().unwrap().contains("PaymentGatway"));
}

#[test]
fn plugin_verify_scopes_to_single_plugin() {
    let (manifest, root) = load("ok-adapter");
    let scoped = build_reports(&manifest, &root, Some("@lazuli/plugin-paygw"));
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].plugin, "@lazuli/plugin-paygw");

    // Unknown ns → run_plugin_verify bails (non-zero / Err).
    let err = run_plugin_verify(&root, Some("@lazuli/plugin-nope"), true);
    assert!(err.is_err(), "unknown --plugin ref should error");
}

#[test]
fn scaffolded_adapter_passes_verify_green() {
    // The 0023↔0022 loop: a fresh `lazuli plugin new <name> --kind adapter`
    // MUST pass `lazuli plugin verify` with zero edits. This is the
    // scaffolder's acceptance oracle.
    let project = std::env::temp_dir().join(format!(
        "lazuli-scaffold-verify-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&project);
    let plugin_dir = project.join("plugins").join("demo");
    std::fs::create_dir_all(project.join("plugins")).unwrap();

    // Scaffold through the real binary (the public surface).
    let out = Command::new(env!("CARGO_BIN_EXE_lazuli"))
        .args(["plugin", "new", "demo", "--kind", "adapter", "--out"])
        .arg(&plugin_dir)
        .output()
        .expect("run lazuli plugin new");
    assert!(
        out.status.success(),
        "plugin new failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Declare it in a project manifest.
    std::fs::write(
        project.join("Lazurite.toml"),
        r#"[project]
name = "scaffold-loop"
module = "example.test/scaffold-loop"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@lazuli/plugin-demo" = { path = "plugins/demo" }
"#,
    )
    .unwrap();

    let manifest = lazurite_manifest::load(&project).unwrap().unwrap();
    let reports = build_reports(&manifest, &project, None);
    let _ = std::fs::remove_dir_all(&project);

    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].overall,
        Overall::Pass,
        "a fresh adapter scaffold must pass verify green; links: {:#?}",
        reports[0].links
    );
    assert_eq!(link_status(&reports[0], "L3 contract"), LinkStatus::Pass);
    assert_eq!(link_status(&reports[0], "L4 import"), LinkStatus::Pass);
}

#[test]
fn verify_and_doctor_agree_drift_guard() {
    // Both surfaces call the shared classifier. Here we assert the verify
    // direction: bad-adapter's L3 is Fail, ok-adapter's L3 is Pass. The
    // doctor direction (PLUGIN-CONTRACT-001 fires on bad, clean on ok) is
    // pinned in `lazuli_doctor_run`'s `plugin_contract` tests — and because
    // both compute from `classify_adapter_contract`, agreement is
    // structural, not coincidental.
    let (bad_m, bad_root) = load("bad-adapter");
    let bad = build_reports(&bad_m, &bad_root, None);
    assert_eq!(link_status(&bad[0], "L3 contract"), LinkStatus::Fail);

    let (ok_m, ok_root) = load("ok-adapter");
    let ok = build_reports(&ok_m, &ok_root, None);
    assert_eq!(link_status(&ok[0], "L3 contract"), LinkStatus::Pass);

    // Drift guard: the SAME classifier both surfaces share agrees with
    // verify's L3 verdict on each fixture.
    use lazuli_cli::plugin_manifest::load_plugin_manifest;
    use lazuli_manifest::plugin_contract::{
        ContractStatus, RegistryView, classify_adapter_contract,
    };
    let reg = RegistryView::empty();

    let bad_typed = load_plugin_manifest(&bad_root.join("plugins/paygw"))
        .unwrap()
        .unwrap();
    assert!(matches!(
        classify_adapter_contract(&bad_typed, "@lazuli/plugin-paygw", &reg),
        ContractStatus::UnknownInterface { .. }
    ));

    let ok_typed = load_plugin_manifest(&ok_root.join("plugins/paygw"))
        .unwrap()
        .unwrap();
    assert_eq!(
        classify_adapter_contract(&ok_typed, "@lazuli/plugin-paygw", &reg),
        ContractStatus::Ok
    );
}
