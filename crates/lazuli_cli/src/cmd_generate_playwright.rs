//! `lazuli generate playwright --playwright-target=<...>` dispatch.
//!
//! Reads the workspace IR + active plugin list, then calls the matching
//! emitter from `lazuli_codegen_ts::playwright::*` and writes the result
//! to `{project_root}/e2e/_generated/<filename>.ts`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::PlaywrightTarget;

pub fn run(input: &Path, target: PlaywrightTarget) -> Result<()> {
    let project_root = input;
    let out_dir = project_root.join("e2e").join("_generated");
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    match target {
        PlaywrightTarget::ApiPolicy => emit_api_policy(project_root, &out_dir),
        PlaywrightTarget::LifecycleGate => emit_lifecycle_gate(project_root, &out_dir),
        PlaywrightTarget::ScalarFixturesBarrel => emit_barrel(project_root, &out_dir),
        PlaywrightTarget::All => {
            emit_api_policy(project_root, &out_dir)?;
            emit_lifecycle_gate(project_root, &out_dir)?;
            emit_barrel(project_root, &out_dir)?;
            Ok(())
        }
    }
}

fn emit_api_policy(_project_root: &Path, out_dir: &Path) -> Result<()> {
    // Stub: real IR loading + filtering wired by a follow-up cell.
    // For v1, emit with an empty command set — file is created and
    // valid TS, just with zero protected commands.
    let body = lazuli_codegen_ts::playwright::emit_playwright_api_policy(&[]);
    let path = out_dir.join("api-policy.spec.ts");
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))
}

fn emit_lifecycle_gate(_project_root: &Path, out_dir: &Path) -> Result<()> {
    let body = lazuli_codegen_ts::playwright::emit_playwright_lifecycle_gate(&[]);
    let path = out_dir.join("lifecycle-gate.spec.ts");
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))
}

fn emit_barrel(_project_root: &Path, out_dir: &Path) -> Result<()> {
    let body = lazuli_codegen_ts::playwright::emit_playwright_scalar_fixtures_barrel(&[]);
    let path = out_dir.join("scalar-fixtures.gen.ts");
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))
}
