//! Thin subprocess wrappers used by `lazuli new`'s happy path:
//! `git init`, `go mod tidy`, and a sanity `lazuli doctor` run.
//!
//! Isolated from the scaffold-logic files so a future test pass can
//! stub them with a trait-based runner without touching template
//! rendering. Each native call is best-effort: the caller in
//! `commands::new::mod` already swallows errors and continues so the
//! user sees a `warning:` line rather than a hard fail when, say,
//! `go` is not on PATH.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use crate::doctor;

/// `git init && git add -A && git commit -m "initial: lazuli new"`.
pub(crate) fn run_git_init(project: &Path) -> Result<()> {
    run_command(project, "git", &["init"])?;
    run_command(project, "git", &["add", "-A"])?;
    run_command(project, "git", &["commit", "-m", "initial: lazuli new"])?;
    Ok(())
}

/// `go mod tidy`. Run after the runtime is wired into `go.work`.
pub(crate) fn run_go_mod_tidy(project: &Path) -> Result<()> {
    run_command(project, "go", &["mod", "tidy"])
}

/// Smoke-test the freshly scaffolded project with `lazuli doctor`
/// (strict). Catches malformed templates before the user hits them.
pub(crate) fn run_doctor_sanity_check(project: &Path) -> Result<()> {
    doctor::doctor_command(project, Some(SecurityProfile::Strict), false, false)
}

/// Spawn a synchronous subprocess and bail on non-zero exit. Used by
/// the three wrappers above; intentionally not `pub` outside this
/// module so any new subprocess call in `commands::new` is forced
/// through the wrapper layer.
fn run_command(project: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(project)
        .status()
        .with_context(|| format!("failed to start `{}`", command_display(program, args)))?;
    if !status.success() {
        bail!(
            "`{}` exited with status {}",
            command_display(program, args),
            status
        );
    }
    Ok(())
}

/// `"git init"`-style render of program + args for error messages.
fn command_display(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}
