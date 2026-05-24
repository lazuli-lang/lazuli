//! `lazuli generate command <feature>.<name>` subcommand.
//!
//! Wave 3 — appends a `command` block with a pre-populated `tests` block
//! seeded with `@TODO authored:` markers. The markers trip
//! `TEST-STUB-001` (warning) so the scaffold ships red and fades to green
//! as the author replaces the stub assertions with real ones.
//!
//! Path convention: appends to `<app_dir>/features/<feature>/<feature>.lzi`.
//! Existing files are NEVER clobbered — re-running the generator with the
//! same name errors out.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

/// Run `lazuli generate command <feature>.<name>`.
pub fn run(ident: &str, project_root: &Path) -> Result<()> {
    let (feature, name) = parse_ident(ident)?;
    validate_part(&feature, "feature")?;
    validate_part(&name, "command name")?;

    let feat_root = app_root(project_root)?.join("features").join(&feature);
    let lzi_path = feat_root.join(format!("{}.lzi", feature));
    if !lzi_path.exists() {
        return Err(anyhow!(
            "feature .lzi not found: {} — run `lazuli generate feature {}` first",
            lzi_path.display(),
            feature
        ));
    }

    let existing = fs::read_to_string(&lzi_path)
        .with_context(|| format!("reading {}", lzi_path.display()))?;
    let needle = format!("\n  command {}\n", name);
    if existing.contains(&needle) {
        return Err(anyhow!(
            "command `{}.{}` already exists in {} — refusing to clobber",
            feature,
            name,
            lzi_path.display()
        ));
    }

    let block = render_command_block(&name);
    let mut updated = existing;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&block);
    fs::write(&lzi_path, updated).with_context(|| format!("writing {}", lzi_path.display()))?;
    println!(
        "appended command `{}.{}` with `tests` block ({} @TODO authored: markers) to {}",
        feature,
        name,
        count_todo_markers(&block),
        lzi_path.display()
    );
    Ok(())
}

fn render_command_block(name: &str) -> String {
    format!(
        "
  command {name}
    policy @policy.update
    # @TODO authored: replace placeholder predicate with real requires expression
    requires self.id != null
    tests
      # @TODO authored: cover @policy.update predicate (replace placeholder actors)
      allows as @role.editor
      denies as @role.viewer
      # @TODO authored: cover requires-predicate boundary
      allows when self.id != null
      denies when self.id = null
"
    )
}

fn count_todo_markers(block: &str) -> usize {
    block.matches("@TODO authored:").count()
}

fn app_root(project_root: &Path) -> Result<PathBuf> {
    let manifest = crate::lazurite_manifest::load(project_root)
        .map_err(|err| anyhow!("failed to load Lazurite.toml: {err}"))?;
    Ok(manifest
        .as_ref()
        .map(|manifest| manifest.app_root(project_root))
        .unwrap_or_else(|| project_root.to_path_buf()))
}

fn parse_ident(ident: &str) -> Result<(String, String)> {
    let mut parts = ident.split('.');
    let feature = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if feature.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(anyhow!(
            "command ident must be <feature>.<name>; got {ident}"
        ));
    }
    Ok((feature.to_string(), name.to_string()))
}

fn validate_part(name: &str, kind: &str) -> Result<()> {
    if name.len() < 2 || name.len() > 64 {
        return Err(anyhow!("{kind} must be 2-64 chars; got {}", name.len()));
    }
    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err(anyhow!("{kind} must start with a lowercase letter"));
    }
    for &byte in bytes {
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_') {
            return Err(anyhow!(
                "{kind} must be snake_case (lowercase, digits, underscores)"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tempdir() -> TempDir {
        TempDir::new().unwrap()
    }

    fn write_lzi(root: &Path, feature: &str, content: &str) {
        let feature_root = root.join("features").join(feature);
        fs::create_dir_all(&feature_root).unwrap();
        fs::write(feature_root.join(format!("{feature}.lzi")), content).unwrap();
    }

    #[test]
    fn appends_tests_block_with_todo_markers() {
        let project = tempdir();
        write_lzi(project.path(), "post", "feature post\n  purpose \"...\"\n");

        run("post.publish", project.path()).unwrap();

        let lzi = fs::read_to_string(project.path().join("features/post/post.lzi")).unwrap();
        assert!(lzi.contains("command publish"));
        assert!(lzi.contains("tests"));
        assert!(lzi.contains("@TODO authored:"));
        assert!(lzi.contains("allows as @role.editor"));
        assert!(lzi.contains("denies as @role.viewer"));
        assert!(lzi.contains("allows when"));
        assert!(lzi.contains("denies when"));
    }

    #[test]
    fn refuses_to_clobber_existing_command() {
        let project = tempdir();
        write_lzi(
            project.path(),
            "post",
            "feature post\n\n  command publish\n    policy @policy.update\n",
        );

        let err = run("post.publish", project.path()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn errors_when_feature_lzi_missing() {
        let project = tempdir();
        let err = run("post.publish", project.path()).unwrap_err();
        assert!(err.to_string().contains("feature .lzi not found"));
    }

    #[test]
    fn rejects_invalid_ident() {
        let project = tempdir();
        let err = run("postpublish", project.path()).unwrap_err();
        assert!(err.to_string().contains("must be <feature>.<name>"));
    }
}
