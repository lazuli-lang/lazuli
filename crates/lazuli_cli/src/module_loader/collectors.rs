//! Filesystem walkers + per-package source aggregation.
//!
//! Three walks share the same skip-list and recursion shape; their
//! outputs feed `build_module_from_path`, the inspect orchestrator,
//! and the doctor source-snapshot routines.
//!
//! Lifted out of the `module_loader` god-file in the rails-style R9
//! split.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const SKIP: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".lazuli",
    // `.claude/worktrees/` holds agent git worktrees — full repo copies
    // whose nested `.lzi` files would otherwise be collected as phantom
    // packages (D5 false positive). No authored `.lzi` lives under
    // `.claude`, so skipping the whole subtree is safe.
    ".claude",
    "dist",
    "node_modules",
    "target",
];

/// Recursively collect every `.lzi` file under a package root, skipping
/// well-known noise directories (build output, vcs metadata, vendored
/// deps). Honors the Lazurite convention (`features/<name>/<name>.lzi`)
/// without requiring callers to enumerate features.
pub(crate) fn collect_package_lzi_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIP.iter().any(|s| *s == name) {
                continue;
            }
            collect_package_lzi_files(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("lzi") {
            out.push(path);
        }
    }
    Ok(())
}

pub(crate) fn collect_package_lzx_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIP.iter().any(|s| *s == name) {
                continue;
            }
            collect_package_lzx_files(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("lzx") {
            out.push(path);
        }
    }
    Ok(())
}

pub(crate) fn collect_lzx_experience_module(input: &Path) -> lazuli_ir::ExperienceModule {
    let mut module = lazuli_ir::ExperienceModule {
        app: None,
        routes: Vec::new(),
        experiences: Vec::new(),
        surfaces: Vec::new(),
    };
    let mut files = Vec::new();
    let result = if input.is_dir() {
        collect_package_lzx_files(input, &mut files)
    } else if input.extension().and_then(|s| s.to_str()) == Some("lzx") {
        files.push(input.to_path_buf());
        Ok(())
    } else {
        Ok(())
    };
    if let Err(err) = result {
        eprintln!("lazuli: skipping .lzx route lift: {err:#}");
        return module;
    }
    files.sort();
    for path in files {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("lazuli: skipping {}: {err}", path.display());
                continue;
            }
        };
        let parsed = match lazuli_syntax::parse_lzx_document(&source) {
            Ok(parsed) => parsed,
            Err(err) => {
                eprintln!(
                    "lazuli: skipping {}: lzx parse failed: {:?}",
                    path.display(),
                    err
                );
                continue;
            }
        };
        let lowered = lazuli_analyzer::lower_lzx_document(&parsed);
        if module.app.is_none() {
            module.app = lowered.app;
        }
        module.routes.extend(lowered.routes);
        module.experiences.extend(lowered.experiences);
        module.surfaces.extend(lowered.surfaces);
    }
    module
}

pub(crate) fn read_package_lzi_source(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_package_lzi_files(dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        bail!("{} contains no `.lzi` files to inspect", dir.display());
    }

    let mut source = String::new();
    for path in files {
        if !source.is_empty() {
            source.push_str("\n\n");
        }
        source.push_str(
            &fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?,
        );
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_package_lzi_files_skips_claude_worktrees() {
        // D5 regression: `.claude/worktrees/<wt>/` agent worktrees are full
        // repo copies; their nested `.lzi` must not be collected.
        let tmp = std::env::temp_dir().join(format!("lzd5-cli-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let real = tmp.join("app/features/billing");
        let phantom = tmp.join(".claude/worktrees/wt-a/app/features/billing");
        fs::create_dir_all(&real).unwrap();
        fs::create_dir_all(&phantom).unwrap();
        fs::write(real.join("billing.lzi"), "feature billing\n").unwrap();
        fs::write(phantom.join("billing.lzi"), "feature billing\n").unwrap();

        let mut out = Vec::new();
        collect_package_lzi_files(&tmp, &mut out).unwrap();

        assert!(out.iter().any(|p| p == &real.join("billing.lzi")));
        assert!(
            !out.iter().any(|p| p.starts_with(tmp.join(".claude"))),
            "phantom .claude package must be skipped: {out:?}"
        );
        let _ = fs::remove_dir_all(&tmp);
    }
}
