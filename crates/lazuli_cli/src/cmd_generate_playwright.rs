//! `lazuli generate playwright --playwright-target=<...>` dispatch.
//!
//! Reads the workspace IR + active plugin list, then calls the matching
//! emitter from `lazuli_codegen_ts::playwright::*` and writes the result
//! to `{project_root}/e2e/_generated/<filename>.ts`.
//!
//! Wave 3.5 (TDD/BDD-first proposal): in addition to the three global
//! files, every `target` mode (including the targeted ones) ALSO emits
//! one per-view spec at `e2e/<experience>/<view>.spec.ts` for every
//! `view <name>` declared in any `.lzx` under the project. Per-view
//! emission is **idempotent**: existing spec files are never clobbered
//! so authored tests survive subsequent regenerations.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use lazuli_syntax::{LzxExperience, LzxExperienceView, parse_lzx_document};

use crate::PlaywrightTarget;

/// Run `lazuli generate playwright --playwright-target=<…>`.
///
/// Writes the matching global emitter output to
/// `{project_root}/e2e/_generated/<filename>.ts` and (per Wave 3.5)
/// also seeds one per-view spec at `e2e/<experience>/<view>.spec.ts`
/// for every `view` declared under any `.lzx` — idempotently, so
/// authored tests survive subsequent regenerations.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_generate_playwright::run;
/// use lazuli_cli::PlaywrightTarget;
///
/// // run(Path::new("."), PlaywrightTarget::ApiPolicy)?;
/// ```
pub fn run(input: &Path, target: PlaywrightTarget) -> Result<()> {
    let project_root = input;
    let out_dir = project_root.join("e2e").join("_generated");
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    match target {
        PlaywrightTarget::ApiPolicy => emit_api_policy(project_root, &out_dir)?,
        PlaywrightTarget::LifecycleGate => emit_lifecycle_gate(project_root, &out_dir)?,
        PlaywrightTarget::ScalarFixturesBarrel => emit_barrel(project_root, &out_dir)?,
        PlaywrightTarget::All => {
            emit_api_policy(project_root, &out_dir)?;
            emit_lifecycle_gate(project_root, &out_dir)?;
            emit_barrel(project_root, &out_dir)?;
        }
    }

    // Wave 3.5 — per-view spec scaffolding. Runs for every target mode so
    // a focused regeneration (e.g. `--playwright-target lifecycle-gate`)
    // still keeps the file-pair invariant green: every `view` in `.lzx`
    // gets a paired `e2e/<experience>/<view>.spec.ts`.
    emit_per_view_specs(project_root)?;

    Ok(())
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

// ── Wave 3.5: per-view spec scaffolding ───────────────────────────────────────

/// Walk every `.lzx` under `project_root`, parse each, and emit one
/// `e2e/<experience>/<view>.spec.ts` per `view <name>` block. Existing
/// files are skipped (idempotent re-runs preserve authored tests).
fn emit_per_view_specs(project_root: &Path) -> Result<()> {
    let mut lzx_files = Vec::new();
    collect_lzx_paths(project_root, &mut lzx_files)?;
    // Deterministic emission order so log lines are stable across runs.
    lzx_files.sort();

    for lzx_path in lzx_files {
        let source = match fs::read_to_string(&lzx_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // Parse failures are surfaced by `lazuli doctor`, not here.
        // Silently skip so generate stays scaffolding-only.
        let document = match parse_lzx_document(&source) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for experience in &document.experiences {
            for view in &experience.views {
                emit_one_view_spec(project_root, experience, view)?;
            }
        }
    }
    Ok(())
}

fn emit_one_view_spec(
    project_root: &Path,
    experience: &LzxExperience,
    view: &LzxExperienceView,
) -> Result<()> {
    // Canonical path MUST match
    // `lazuli_doctor::test_discipline::test_view_e2e_missing_001::expected_spec_path`.
    // Keep the two in sync — drift means the doctor rule keeps firing
    // even after a generate.
    let spec_path = project_root
        .join("e2e")
        .join(&experience.name)
        .join(format!("{}.spec.ts", view.name));

    if spec_path.exists() {
        // Idempotent: never clobber an authored spec. Authors get to
        // delete the `@TODO authored:` markers without fear of losing
        // their work on the next `lazuli generate playwright` run.
        return Ok(());
    }

    if let Some(parent) = spec_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let body = render_view_spec_template(experience, view);
    fs::write(&spec_path, body).with_context(|| format!("write {}", spec_path.display()))?;
    println!("Created {}", spec_path.display());
    Ok(())
}

/// Render the per-view Playwright spec body.
///
/// The template lives at
/// `lazurite/templates/default/e2e/<feature>/<view>.spec.ts.tmpl`
/// for documentation purposes. The actual emission is done inline here
/// so generate has zero filesystem dependency on the lazurite templates
/// dir — the rule cares about the path layout, not the spec contents.
fn render_view_spec_template(experience: &LzxExperience, view: &LzxExperienceView) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// Auto-generated by `lazuli generate playwright` (Wave 3.5).\n\
         // File-pair check `TEST-VIEW-E2E-MISSING-001` requires this spec to exist;\n\
         // doctor never runs Playwright — it only verifies the file is here.\n\
         // Replace every `@TODO authored:` marker with a real assertion.\n\
         import {{ test, expect }} from '@playwright/test';\n\
         \n\
         test.describe('{exp}.{view}', () => {{\n  \
            test('renders for authorized audience', async ({{ page }}) => {{\n    \
                // @TODO authored: navigate to the view's route\n    \
                await page.goto('/{view}');\n    \
                // @TODO authored: assert visible content from view's anchor/columns\n    \
                await expect(page.locator('[data-testid=\"{view}-root\"]')).toBeVisible();\n  \
            }});\n",
        exp = experience.name,
        view = view.name,
    ));

    // One test case per `submit feature.command.<X>` reference (a view
    // may declare at most one `submit` today; future grammar may allow
    // multiple). Action `name -> target` rows on the view are NOT
    // unwrapped here — they belong to the host view's surface and are
    // covered by their own commands' tests downstream.
    for command_ref in submits_for_view(view) {
        out.push_str(&format!(
            "\n  \
            test('submits {cmd_ref}', async ({{ page }}) => {{\n    \
                // @TODO authored: complete this scenario for command {cmd_ref}\n    \
                await page.goto('/{view}');\n    \
                // @TODO authored: fill inputs, click submit, assert success path\n  \
            }});\n",
            cmd_ref = command_ref,
            view = view.name,
        ));
    }

    out.push_str("});\n");
    out
}

/// Collect every `submit feature.command.<X>` reference attached to a
/// view. Today `LzxExperienceView` carries at most one `submit` slot,
/// but the generator already plans for the Wave 4 expansion in which
/// view-action rows (`action <name> -> <feature>.command.<X>`) may also
/// participate in E2E scaffolding.
fn submits_for_view(view: &LzxExperienceView) -> Vec<String> {
    let mut refs = Vec::new();
    if let Some(submit) = view.submit.as_ref() {
        refs.push(submit.trim().to_string());
    }
    refs
}

/// Walk `project_root` recursively, gathering every `.lzx` file. The
/// `e2e/` and `dist/` directories are skipped to avoid scanning emitted
/// fixtures or stray test artefacts. Mirrors the convention used by
/// `lazuli doctor`'s walker.
fn collect_lzx_paths(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("failed to list {}", root.display()))? {
        let entry = entry.with_context(|| format!("failed to read {}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Skip output dirs to keep generate idempotent and fast.
                if matches!(name, "e2e" | "dist" | "node_modules" | ".git" | "target") {
                    continue;
                }
            }
            collect_lzx_paths(&path, out)?;
        } else if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("lzx") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const SAMPLE_LZX: &str = "\
experience customer
  imports customer

  view list
    source customer.query.list

  view lead_capture
    submit customer.command.capture_lead
";

    #[test]
    fn per_view_emission_creates_one_spec_per_view() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path();
        let lzx_dir = project_root.join("app").join("features").join("customer");
        fs::create_dir_all(&lzx_dir).unwrap();
        fs::write(lzx_dir.join("customer.lzx"), SAMPLE_LZX).unwrap();

        emit_per_view_specs(project_root).expect("emit per-view specs");

        let list_path = project_root.join("e2e/customer/list.spec.ts");
        let lead_path = project_root.join("e2e/customer/lead_capture.spec.ts");
        assert!(list_path.exists(), "list spec must be created");
        assert!(lead_path.exists(), "lead_capture spec must be created");

        // Template content sanity (no test contents validation in doctor —
        // we only check generator output stays recognisable as a stub).
        let list = fs::read_to_string(&list_path).unwrap();
        assert!(list.contains("@TODO authored:"));
        assert!(list.contains("customer.list"));

        let lead = fs::read_to_string(&lead_path).unwrap();
        assert!(lead.contains("submits customer.command.capture_lead"));
    }

    #[test]
    fn per_view_emission_does_not_clobber_existing_spec() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path();
        let lzx_dir = project_root.join("app");
        fs::create_dir_all(&lzx_dir).unwrap();
        fs::write(lzx_dir.join("customer.lzx"), SAMPLE_LZX).unwrap();

        let spec_dir = project_root.join("e2e/customer");
        fs::create_dir_all(&spec_dir).unwrap();
        let authored = spec_dir.join("list.spec.ts");
        fs::write(&authored, "// HAND-AUTHORED — do not regenerate\n").unwrap();

        emit_per_view_specs(project_root).expect("emit per-view specs");

        let after = fs::read_to_string(&authored).unwrap();
        assert!(
            after.contains("HAND-AUTHORED"),
            "existing spec must not be clobbered; got: {after}"
        );
    }

    #[test]
    fn lzx_walker_skips_e2e_dir() {
        // Stray .lzx files inside `e2e/` (e.g. authored test fixtures) must
        // not be parsed — they'd otherwise loop the generator.
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path();
        fs::create_dir_all(project_root.join("e2e/fixtures")).unwrap();
        fs::write(
            project_root.join("e2e/fixtures/decoy.lzx"),
            "experience decoy\n  imports decoy\n  view bad\n",
        )
        .unwrap();

        let mut files = Vec::new();
        collect_lzx_paths(project_root, &mut files).unwrap();
        assert!(files.is_empty(), "e2e/ tree must be skipped, got {files:?}");
    }
}
