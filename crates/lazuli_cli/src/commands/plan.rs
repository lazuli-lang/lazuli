//! `lazuli plan --check <name>` — migrations-bucket schema-planning
//! surface.
//!
//! Today the command validates one thing: that the `deploy.checkpoint
//! <name> "<path>"` block declared in `app.lzi` points at an existing
//! snapshot JSON on disk, and that the snapshot's recorded
//! `lazuli_version` matches the analyzer running right now. Typed
//! field-level diff between two checkpoints lands in the Tier 4
//! follow-up cycle (Migrations bucket Route C).
//!
//! Cross-refs:
//! - `crate::app_manifest::parse_app_manifest` — IR lift for the
//!   surrounding `app` block + its `deploy.checkpoint` slot.
//! - `crate::lazurite_manifest::resolve_in_app_dir` — accepts either
//!   a direct path to `app.lzi` or a directory containing it.
//! - `docs/proposals/migrations-bucket-cycle.md` Route C — the typed
//!   diff that this stub will grow into.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{app_manifest, lazurite_manifest};

/// Handler for the `Commands::Plan` clap arm.
pub fn plan_command(input: &Path, check: Option<&str>) -> Result<()> {
    let Some(check_name) = check else {
        bail!("`lazuli plan` currently requires `--check <snapshot_name>`");
    };

    // Locate `app.lzi` — accept either a direct path or a directory.
    let app_path = if input.is_dir() {
        lazurite_manifest::resolve_in_app_dir(input, "app.lzi")
    } else {
        input.to_path_buf()
    };
    if !app_path.exists() {
        bail!("app manifest not found at {}", app_path.display());
    }

    let source = fs::read_to_string(&app_path)
        .with_context(|| format!("failed to read {}", app_path.display()))?;

    let manifest = app_manifest::parse_app_manifest(&source)
        .ok_or_else(|| anyhow::anyhow!("{} does not declare an `app` block", app_path.display()))?;

    let Some(deploy) = manifest.deploy.as_ref() else {
        bail!(
            "app `{}` declares no `deploy` block — nothing to plan",
            manifest.name
        );
    };
    let Some(checkpoint) = deploy.checkpoint.as_ref() else {
        bail!(
            "app `{}` declares no `deploy.checkpoint` — add `checkpoint <name> \"<path>\"` first",
            manifest.name
        );
    };
    if checkpoint.name != check_name {
        bail!(
            "checkpoint `{}` not declared in app `{}` (found `{}`)",
            check_name,
            manifest.name,
            checkpoint.name
        );
    }

    // Resolve checkpoint path relative to app.lzi's directory.
    let app_dir = app_path.parent().unwrap_or_else(|| Path::new("."));
    let snapshot_path = app_dir.join(&checkpoint.path);
    if !snapshot_path.exists() {
        bail!(
            "checkpoint `{}` references path `{}` that does not exist relative to {}",
            check_name,
            checkpoint.path,
            app_path.display()
        );
    }

    let text = fs::read_to_string(&snapshot_path)
        .with_context(|| format!("failed to read snapshot {}", snapshot_path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("snapshot {} is not valid JSON", snapshot_path.display()))?;

    let expected_version = env!("CARGO_PKG_VERSION");
    let snapshot_version = value
        .get("lazuli_version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if snapshot_version.is_empty() {
        println!(
            "checkpoint {}: ok (snapshot missing `lazuli_version`; regenerate to enable version drift detection)",
            check_name
        );
        return Ok(());
    }
    if snapshot_version != expected_version {
        println!(
            "checkpoint {}: ok (snapshot lazuli_version {} lags analyzer {}; consider regenerating)",
            check_name, snapshot_version, expected_version
        );
        return Ok(());
    }
    println!("checkpoint {}: ok", check_name);
    Ok(())
}
