//! `lazuli spike-generate` — regenerate the runtime-form
//! `customer.gen.go` and `customer.gen.ts` artifacts from a runtime
//! spec.
//!
//! Without `--spec`, the in-process `lazuli_codegen_spec::customer_spike`
//! fixture is used; with `--spec <path>`, a JSON `RuntimeFeature`
//! manifest is loaded (see `examples/runtime-spec/customer.json`) and
//! fed through the same emitters. The command is a deliberate
//! pre-cursor / smoke test for the more general `lazuli generate go` /
//! `lazuli generate ts` paths — it lets us iterate on the
//! `lazuli_codegen_go::emit_feature_go` /
//! `lazuli_codegen_ts::emit_feature_ts` shapes without touching the
//! full `.lzi` lift pipeline.
//!
//! Cross-refs:
//! - `lazuli_codegen_go::emit_feature_go` / `lazuli_codegen_ts::emit_feature_ts`
//!   — the emitters under test.
//! - `lazuli_codegen_spec::customer_spike` — the hardcoded fixture.
//! - `commands/generate.rs` (TBD) — the production `.lzi`-driven path.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Handler for the `Commands::SpikeGenerate` clap arm.
///
/// Loads a `RuntimeFeature` (from `--spec` JSON or the in-process
/// `customer_spike` fixture), pipes it through `emit_feature_go` and
/// `emit_feature_ts`, and writes the artifacts under
/// `dist/{go,web}/customer/`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::spike_generate::spike_generate_command;
///
/// // spike_generate_command(Path::new("."), None)?;
/// ```
pub fn spike_generate_command(root: &Path, spec: Option<&Path>) -> Result<()> {
    let feature = match spec {
        Some(path) => {
            let text =
                fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str(&text)
                .with_context(|| format!("parse runtime spec JSON {}", path.display()))?
        }
        None => lazuli_codegen_spec::customer_spike(),
    };
    let go_path = root.join("dist/go/customer/customer.gen.go");
    let ts_path = root.join("dist/web/customer/src/customer.gen.ts");

    let go_source = lazuli_codegen_go::emit_feature_go(&feature);
    let ts_source = lazuli_codegen_ts::emit_feature_ts(&feature);

    if let Some(parent) = go_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    if let Some(parent) = ts_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    fs::write(&go_path, go_source).with_context(|| format!("write {}", go_path.display()))?;
    fs::write(&ts_path, ts_source).with_context(|| format!("write {}", ts_path.display()))?;

    println!("wrote {}", go_path.display());
    println!("wrote {}", ts_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_run_writes_both_artifacts() {
        let root = std::env::temp_dir().join("lazuli_cli_spike_test");
        let _ = fs::remove_dir_all(&root);
        spike_generate_command(&root, None).expect("fixture run succeeds");
        assert!(root.join("dist/go/customer/customer.gen.go").exists());
        assert!(root.join("dist/web/customer/src/customer.gen.ts").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
