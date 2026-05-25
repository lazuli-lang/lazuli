//! `--frontends` flag plumbing for `lazuli new`.
//!
//! Two responsibilities:
//!
//! 1. Parse the comma-separated `--frontends` flag into a `Vec<FrontendScaffold>`
//!    with deduplication and clear errors on unknown tokens.
//! 2. Warn the user (without failing) when an in-place run is about to
//!    skip user-owned config files (`tailwind.config.ts`,
//!    `tsconfig.json`, `vite.config.ts`) because they already exist —
//!    we never overwrite hand-authored config in `--in-place` mode.
//!
//! The actual frontend scaffolders (`scaffold_frontend_web`,
//! `scaffold_frontend_mobile`) live in `crate::cmd_new_frontends` and
//! are invoked from `commands::new::mod`.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Frontend opt-ins accepted by `lazuli new --frontends ...`. Closed
/// catalog — adding a third frontend means editing this enum, the
/// `parse_frontends` arm, and adding a `scaffold_frontend_<x>` entry
/// in `cmd_new_frontends`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrontendScaffold {
    Web,
    Mobile,
}

/// Parse `--frontends web,mobile` into a deduplicated, order-preserving
/// `Vec<FrontendScaffold>`. Empty tokens and unknown labels are hard
/// errors with the same canonical message ("web, mobile, or web,mobile")
/// so authors see a consistent allow-list.
pub(crate) fn parse_frontends(raw: &str) -> Result<Vec<FrontendScaffold>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for item in raw.split(',') {
        let item = item.trim();
        if item.is_empty() {
            bail!("empty frontend in --frontends; expected web, mobile, or web,mobile");
        }
        let frontend = match item {
            "web" => FrontendScaffold::Web,
            "mobile" => FrontendScaffold::Mobile,
            other => bail!("unknown frontend `{other}`; expected web, mobile, or web,mobile"),
        };
        if seen.insert(item.to_string()) {
            out.push(frontend);
        }
    }
    if out.is_empty() {
        bail!("--frontends requires web, mobile, or web,mobile");
    }
    Ok(out)
}

/// Print a `skipping <path>: already exists; user-owned` line for each
/// well-known config file that's already present under `app/web/`.
/// Returns `Ok(())` regardless — these are warnings, not errors.
pub(crate) fn log_user_owned_frontend_skips(project_root: &Path) -> Result<()> {
    for relative in [
        "app/web/tailwind.config.ts",
        "app/web/tsconfig.json",
        "app/web/vite.config.ts",
    ] {
        let path = project_root.join(relative);
        if path
            .try_exists()
            .with_context(|| format!("failed to inspect {}", path.display()))?
        {
            eprintln!("skipping {relative}: already exists; user-owned");
        }
    }
    Ok(())
}
