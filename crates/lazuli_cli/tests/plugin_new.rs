//! Spec 0023 — `lazuli plugin new` scaffolder.
//!
//! The acceptance oracle per the spec is "scaffold → the manifest is valid
//! + (0022's verify, not yet built) would pass". Since 0022's `plugin
//! verify` is not yet landed, these tests bind the strongest in-process
//! checks the ADR names (Decision 4):
//!
//! 1. file-shape — the exact emitted file manifest exists, each non-empty;
//! 2. manifest validity — the emitted `manifest.toml` deserializes against
//!    the 0021 typed `lazuli_manifest::PluginManifest` schema and reports
//!    the right `resolved_kind()`;
//! 3. token substitution — names/modules/namespaces landed correctly;
//! 4. conditional Go — if `go` is on PATH, `go test ./...` the scaffold.
//!
//! Scaffolding is driven through the built `lazuli` binary (the public
//! surface) since the handler is crate-private.

use std::path::{Path, PathBuf};
use std::process::Command;

use lazuli_manifest::plugin_manifest::{PluginKind, PluginManifest};

/// Run `lazuli plugin new <name> --kind <kind> --out <out>` (+ optional
/// `--namespace`). Returns the command's success status.
fn run_plugin_new(name: &str, kind: &str, out: &Path, namespace: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lazuli"));
    cmd.args(["plugin", "new", name, "--kind", kind, "--out"])
        .arg(out);
    if let Some(ns) = namespace {
        cmd.args(["--namespace", ns]);
    }
    cmd.output().expect("failed to run lazuli plugin new")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn parse_manifest(dir: &Path) -> PluginManifest {
    let toml_src = read(&dir.join("manifest.toml"));
    toml::from_str::<PluginManifest>(&toml_src)
        .unwrap_or_else(|e| panic!("manifest.toml did not deserialize against 0021 schema: {e}"))
}

fn assert_non_empty(path: &Path) {
    let md = std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("expected file {}: {e}", path.display()));
    assert!(md.len() > 0, "file {} is empty", path.display());
}

/// If a Go toolchain is on PATH, `go test ./...` the scaffolded dir and
/// assert success. Otherwise skip with a logged note (ADR Decision 4 — Go
/// is not guaranteed in the Rust test env).
fn go_test_if_available(dir: &Path) {
    let probe = Command::new("go").arg("version").output();
    let Ok(probe) = probe else {
        eprintln!("[plugin_new] go not on PATH — skipping go test for {}", dir.display());
        return;
    };
    if !probe.status.success() {
        eprintln!("[plugin_new] `go version` failed — skipping go test for {}", dir.display());
        return;
    }
    let out = Command::new("go")
        .args(["test", "./..."])
        .current_dir(dir)
        .output()
        .expect("failed to run go test");
    assert!(
        out.status.success(),
        "go test ./... failed in {}:\nstdout:\n{}\nstderr:\n{}",
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn tmp_out(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.push(format!("lazuli-plugin-new-{label}-{pid}-{nanos}"));
    dir
}

#[test]
fn plugin_new_semantic_scaffolds_valid_manifest() {
    let out = tmp_out("semantic");
    let result = run_plugin_new("demo-scalar", "semantic", &out, None);
    assert!(
        result.status.success(),
        "plugin new failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let manifest = parse_manifest(&out);
    assert_eq!(manifest.resolved_kind(), PluginKind::Semantic);
    assert_eq!(manifest.semantic_types.len(), 1, "exactly one example semantic type");
    let st = &manifest.semantic_types[0];
    assert_eq!(st.name, "ScalarValue");
    assert_eq!(st.alias, "@semantic.ScalarValue");
    assert_eq!(st.carrier_type, "String");
    assert_eq!(st.validator, "ValidateScalarValue");
    let plugin = manifest.plugin.expect("[plugin] block present");
    assert_eq!(plugin.name, "demo-scalar");
    assert_eq!(plugin.namespace, "@lazuli/plugin-demo-scalar");
    assert_eq!(plugin.effective_go_module(), Some("lazuli.dev/plugin/demo-scalar"));

    go_test_if_available(&out);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn plugin_new_adapter_scaffolds_contract() {
    let out = tmp_out("adapter");
    let result = run_plugin_new("demo-adapter", "adapter", &out, None);
    assert!(
        result.status.success(),
        "plugin new failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let manifest = parse_manifest(&out);
    assert_eq!(manifest.resolved_kind(), PluginKind::Adapter);
    assert!(!manifest.implements.is_empty(), "adapter declares implements");
    assert!(manifest.env.is_some(), "adapter declares [env]");
    assert!(manifest.binds.is_some(), "adapter declares [binds]");

    // The compile-time assertion is the load-bearing adapter contract.
    let adapter_go = read(&out.join("adapter.go"));
    assert!(
        adapter_go.contains("var _ Interface = (*Adapter)(nil)"),
        "adapter.go missing compile-time assertion"
    );

    go_test_if_available(&out);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn plugin_new_emits_expected_file_manifest() {
    // semantic
    let sem = tmp_out("manifest-sem");
    assert!(run_plugin_new("foo-bar", "semantic", &sem, None).status.success());
    for f in [
        "manifest.toml",
        "foo-bar.go",
        "foo-bar_test.go",
        "go.mod",
        "README.md",
        "CHANGELOG.md",
        "LICENSE",
        ".gitignore",
        ".github/workflows/go.yml",
    ] {
        assert_non_empty(&sem.join(f));
    }
    let _ = std::fs::remove_dir_all(&sem);

    // adapter
    let adp = tmp_out("manifest-adp");
    assert!(run_plugin_new("foo-bar", "adapter", &adp, None).status.success());
    for f in [
        "manifest.toml",
        "adapter.go",
        "adapter_test.go",
        "go.mod",
        "README.md",
        "CHANGELOG.md",
        "LICENSE",
        ".gitignore",
        ".github/workflows/go.yml",
    ] {
        assert_non_empty(&adp.join(f));
    }
    let _ = std::fs::remove_dir_all(&adp);
}

#[test]
fn plugin_new_default_kind_is_semantic() {
    let out = tmp_out("default-kind");
    // No --kind flag.
    let result = Command::new(env!("CARGO_BIN_EXE_lazuli"))
        .args(["plugin", "new", "defaulted", "--out"])
        .arg(&out)
        .output()
        .expect("run");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let manifest = parse_manifest(&out);
    assert_eq!(manifest.resolved_kind(), PluginKind::Semantic);
    assert!(out.join("defaulted.go").exists());
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn plugin_new_namespace_default_and_override() {
    let def = tmp_out("ns-default");
    assert!(run_plugin_new("baz", "semantic", &def, None).status.success());
    let m = parse_manifest(&def);
    assert_eq!(m.plugin.unwrap().namespace, "@lazuli/plugin-baz");
    let _ = std::fs::remove_dir_all(&def);

    let ovr = tmp_out("ns-override");
    assert!(run_plugin_new("baz", "semantic", &ovr, Some("@acme/widgets")).status.success());
    let m2 = parse_manifest(&ovr);
    assert_eq!(m2.plugin.unwrap().namespace, "@acme/widgets");
    // The override must propagate everywhere it's substituted.
    assert!(read(&ovr.join("README.md")).contains("@acme/widgets"));
    let _ = std::fs::remove_dir_all(&ovr);
}

#[test]
fn plugin_new_rejects_non_kebab_name() {
    let out = tmp_out("bad-name");
    let result = run_plugin_new("Foo_Bar", "semantic", &out, None);
    assert!(!result.status.success(), "non-kebab name must be rejected");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("kebab-case"),
        "expected anchored kebab-case error, got: {stderr}"
    );
    // Nothing written on the error path.
    assert!(!out.join("manifest.toml").exists(), "no partial scaffold on bad name");
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn plugin_new_refuses_non_empty_out() {
    let out = tmp_out("non-empty");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("existing.txt"), "occupied").unwrap();

    let result = run_plugin_new("collide", "semantic", &out, None);
    assert!(!result.status.success(), "non-empty out must be refused");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("non-empty"), "expected non-empty error, got: {stderr}");
    // Pre-existing file untouched; no scaffold written.
    assert!(out.join("existing.txt").exists());
    assert!(!out.join("manifest.toml").exists());
    let _ = std::fs::remove_dir_all(&out);
}
