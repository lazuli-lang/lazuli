//! `lazuli new` subcommand — project scaffolder.
//!
//! Carved out of `main.rs` as part of Wave R3-D (Rails-style refactor).
//! The dispatcher in `main.rs` calls `commands::new::new_command`; the
//! actual work — template rendering, frontend opt-ins, `go.work` /
//! `Lazurite.toml` wiring — lives under this sub-tree:
//!
//! - `scaffold` — `scaffold_bare`, `scaffold_from_template`, and the
//!   small file/template helpers (`app_template`, `readme_template`,
//!   `write_scaffold_file`, `pascal_case_project_name`).
//! - `frontend` — `FrontendScaffold` enum, `parse_frontends`, and the
//!   in-place-only `log_user_owned_frontend_skips`.
//! - `package_json` — `merge_or_write_package_json` and its private
//!   `merge_package_json_object` / `merge_package_json_section`
//!   helpers, used by `--in-place` to fold the frontend template into
//!   an existing `app/web/package.json` without clobbering scripts.
//! - `runtime_wiring` — runtime discovery (`LAZULI_RUNTIME_PATH` +
//!   ancestor-walk), `go.work` injection, and the
//!   `[lazuli] path = "..."` Lazurite.toml edit.
//! - `process` — `run_git_init`, `run_go_mod_tidy`,
//!   `run_doctor_sanity_check`, and the generic `run_command`
//!   wrapper. Native subprocess calls don't belong inline with
//!   template logic; isolating them simplifies test stubs.
//!
//! ABI contract: `lazuli new --help` and `lazuli new <args>` behave
//! byte-identically to the pre-split build. The split is purely
//! organizational; flag definitions stay in `main.rs::Commands::New`.
//!
//! Cross-refs: `docs/proposals/rails-style-refactor-2026-05-24.md`
//! §Wave R3-D.

pub(crate) mod frontend;
pub(crate) mod package_json;
pub(crate) mod process;
pub(crate) mod runtime_wiring;
pub(crate) mod scaffold;

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{cmd_new_frontends, templates};
use frontend::{FrontendScaffold, log_user_owned_frontend_skips, parse_frontends};
use package_json::merge_or_write_package_json;
use process::{run_doctor_sanity_check, run_git_init, run_go_mod_tidy};
use runtime_wiring::{inject_runtime_into_go_work, locate_lazuli_runtime_dir};
use scaffold::{
    default_module_name, pascal_case_project_name, scaffold_bare, scaffold_from_template,
};

/// Handler for `lazuli new`. Dispatches to either the in-place editor
/// (existing Lazurite project, only adds frontends) or the full
/// project scaffolder (fresh directory).
pub(crate) fn new_command(
    project: Option<&Path>,
    template: &str,
    bare: bool,
    no_git: bool,
    module: Option<String>,
    frontends: Option<String>,
    in_place: bool,
) -> Result<()> {
    if in_place {
        return new_in_place_command(project, template, bare, no_git, module, frontends);
    }

    let project = project.ok_or_else(|| {
        anyhow::anyhow!("missing project directory; pass a project name or use --in-place")
    })?;
    new_project_command(project, template, bare, no_git, module, frontends)
}

fn new_project_command(
    project: &Path,
    template: &str,
    bare: bool,
    no_git: bool,
    module: Option<String>,
    frontends: Option<String>,
) -> Result<()> {
    if project
        .try_exists()
        .with_context(|| format!("failed to inspect {}", project.display()))?
    {
        bail!("project path already exists: {}", project.display());
    }

    let app_name = pascal_case_project_name(project)?;
    let bare = bare || template == "bare";
    if !bare && template != "default" {
        bail!("unknown template `{template}`; supported templates: default, bare");
    }

    if bare {
        scaffold_bare(project, &app_name)?;
    } else {
        let module = module.unwrap_or_else(|| default_module_name(project));
        scaffold_from_template(&templates::DEFAULT_TEMPLATE, project, &app_name, &module)?;

        // The default `go.work` lists `.` and `./dist/go`. If we can
        // discover the local Lazuli runtime source (`runtime/go/`) on
        // this machine — either from `LAZULI_RUNTIME_PATH` or by
        // walking from this CLI binary's location — append a third
        // `use <abs path>` so `go build`/`go mod tidy` resolves
        // `lazuli.dev/runtime` without a published module. Hands-off
        // for installed (system) Lazuli binaries: if no runtime is
        // discovered the file stays as the user can wire the path
        // manually following the README hint.
        if let Some(runtime_dir) = locate_lazuli_runtime_dir()
            && let Err(err) = inject_runtime_into_go_work(project, &runtime_dir)
        {
            eprintln!(
                "warning: failed to write runtime path into go.work ({}): {err:#}",
                runtime_dir.display()
            );
        }

        if let Err(err) = run_go_mod_tidy(project) {
            eprintln!("warning: failed to run `go mod tidy`: {err:#}");
        }
        if let Err(err) = run_doctor_sanity_check(project) {
            eprintln!("warning: failed to run `lazuli doctor`: {err:#}");
        }
    }

    if let Some(frontends) = frontends.as_deref() {
        for frontend in parse_frontends(frontends)? {
            match frontend {
                FrontendScaffold::Web => {
                    cmd_new_frontends::scaffold_frontend_web(project, &app_name)?
                }
                FrontendScaffold::Mobile => {
                    cmd_new_frontends::scaffold_frontend_mobile(project, &app_name)?
                }
            }
        }
    }

    if !no_git {
        run_git_init(project)?;
    }

    println!("created {}", project.display());
    Ok(())
}

fn new_in_place_command(
    project: Option<&Path>,
    template: &str,
    bare: bool,
    _no_git: bool,
    module: Option<String>,
    frontends: Option<String>,
) -> Result<()> {
    if bare || template != "default" || module.is_some() {
        bail!("--in-place only supports --frontends on an existing Lazurite project");
    }

    let project_root = match project {
        Some(project) => project.to_path_buf(),
        None => std::env::current_dir().context("failed to determine current directory")?,
    };
    let manifest = project_root.join("Lazurite.toml");
    if !manifest
        .try_exists()
        .with_context(|| format!("failed to inspect {}", manifest.display()))?
    {
        bail!(
            "no Lazurite project in {}; run without --in-place to scaffold a new project",
            project_root.display()
        );
    }

    let frontends = frontends.as_deref().ok_or_else(|| {
        anyhow::anyhow!("--in-place requires --frontends web, mobile, or web,mobile")
    })?;
    let app_name = pascal_case_project_name(&project_root)?;

    for frontend in parse_frontends(frontends)? {
        match frontend {
            FrontendScaffold::Web => {
                let package_json = project_root.join("app").join("web").join("package.json");
                let package_json_exists = package_json
                    .try_exists()
                    .with_context(|| format!("failed to inspect {}", package_json.display()))?;
                log_user_owned_frontend_skips(&project_root)?;
                cmd_new_frontends::scaffold_frontend_web(&project_root, &app_name)?;
                if package_json_exists {
                    merge_or_write_package_json(&package_json, templates::FRONTEND_PACKAGE_JSON)?;
                }
            }
            FrontendScaffold::Mobile => {
                cmd_new_frontends::scaffold_frontend_mobile(&project_root, &app_name)?
            }
        }
    }

    println!("updated {}", project_root.display());
    Ok(())
}
