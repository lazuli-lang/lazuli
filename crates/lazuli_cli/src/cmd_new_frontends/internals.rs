//! Filesystem + manifest helpers shared by the web and mobile
//! scaffold writers.
//!
//! Two invariants enforced here:
//!
//! - `write_if_absent` is the idempotency guard — a second scaffold
//!   pass never overwrites user edits.
//! - `append_manifest_block` adds a section to `Lazurite.toml` only
//!   when the header is not yet present, so re-running scaffolds does
//!   not double-append the `[frontends.<x>]` block.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub(super) fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))
}

/// Recursively copy `src` into `dst`, skipping any file that already
/// exists at the destination (idempotency: a second scaffold pass — or
/// a user-edited vendored file — is never clobbered). Directories are
/// created as needed. `node_modules` is skipped: the vendored package
/// is consumed via the workspace, so its own installed deps are
/// resolved by the consumer's `pnpm install`, not copied.
pub(super) fn copy_dir_if_absent(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("creating {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("reading dir {}", src.display()))? {
        let entry = entry.with_context(|| format!("reading entry under {}", src.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("statting {}", entry.path().display()))?;
        let name = entry.file_name();
        // Never vendor an installed node_modules tree — it's huge and
        // the consumer's pnpm install resolves the package's deps.
        if name == "node_modules" {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if file_type.is_dir() {
            copy_dir_if_absent(&from, &to)?;
        } else if !to.exists() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            fs::copy(&from, &to)
                .with_context(|| format!("copying {} -> {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Write `content` to `path` only if the path does not already exist.
/// This is the idempotency guard: a second scaffold pass never overwrites
/// user edits.
pub(super) fn write_if_absent(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

/// Append `snippet` to `manifest_path` unless `header` already appears
/// in the existing manifest (idempotency: re-running won't double-append).
/// If the manifest does not exist, it is created from scratch with just
/// the snippet (orchestrator usually writes `Lazurite.toml` itself; this
/// branch exists for tests / `--in-place` usage).
pub(super) fn append_manifest_block(
    manifest_path: &Path,
    header: &str,
    snippet: &str,
) -> Result<()> {
    let manifest_buf: PathBuf = manifest_path.to_path_buf();
    let existing = if manifest_buf.exists() {
        fs::read_to_string(&manifest_buf)
            .with_context(|| format!("reading {}", manifest_buf.display()))?
    } else {
        String::new()
    };

    if existing.contains(header) {
        return Ok(());
    }

    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(snippet);

    fs::write(&manifest_buf, out).with_context(|| format!("writing {}", manifest_buf.display()))
}
