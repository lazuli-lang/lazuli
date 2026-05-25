//! `lazuli generate view <feature>.<name>` subcommand.
//!
//! Wave 3 — appends a `view list <name>` block in the matching `.lzx`
//! surface, pre-populated with a `tests` block carrying
//! `accepted by`/`rejected by` extensibility markers anchored by
//! `@TODO authored:` comments. The markers trip `TEST-STUB-001`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

/// Run `lazuli generate view <feature>.<name>`.
///
/// Looks for `<app_dir>/features/<feature>/<feature>.web.lzx`. Falls
/// back to `<feature>.lzx` for non-frontend-specific surfaces.
pub fn run(ident: &str, project_root: &Path) -> Result<()> {
    let (feature, name) = parse_ident(ident)?;
    validate_part(&feature, "feature")?;
    validate_part(&name, "view name")?;

    let feat_root = app_root(project_root)?.join("features").join(&feature);
    let lzx_path = pick_lzx_target(&feat_root, &feature)?;

    let existing =
        fs::read_to_string(&lzx_path).with_context(|| format!("reading {}", lzx_path.display()))?;
    let needle = format!("\n  view list {}\n", name);
    let alt_needle = format!("\n  view {}\n", name);
    if existing.contains(&needle) || existing.contains(&alt_needle) {
        return Err(anyhow!(
            "view `{}.{}` already exists in {} — refusing to clobber",
            feature,
            name,
            lzx_path.display()
        ));
    }

    let block = render_view_block(&name);
    let mut updated = existing;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&block);
    fs::write(&lzx_path, updated).with_context(|| format!("writing {}", lzx_path.display()))?;
    println!(
        "appended view `{}.{}` with `tests` block ({} @TODO authored: markers) to {}",
        feature,
        name,
        block.matches("@TODO authored:").count(),
        lzx_path.display()
    );
    Ok(())
}

fn pick_lzx_target(feat_root: &Path, feature: &str) -> Result<PathBuf> {
    let web = feat_root.join(format!("{}.web.lzx", feature));
    if web.exists() {
        return Ok(web);
    }
    let bare = feat_root.join(format!("{}.lzx", feature));
    if bare.exists() {
        return Ok(bare);
    }
    Err(anyhow!(
        "no surface `.lzx` found for feature `{}` at {} (looked for `{}.web.lzx` and `{}.lzx`) — \
         scaffold the feature surface first",
        feature,
        feat_root.display(),
        feature,
        feature
    ))
}

fn render_view_block(name: &str) -> String {
    format!(
        "
  # @TODO authored: replace anchor with the real extension anchor (e.g. @anchor.{name}_detail)
  view list {name}
    anchor @anchor.{name}_list
    extensible_by
    tests
      # @TODO authored: list features whose `extends` should be accepted at this anchor
      accepted by {name}_extras
      # @TODO authored: list features whose `extends` should be rejected at this anchor
      rejected by billing
"
    )
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
        return Err(anyhow!("view ident must be <feature>.<name>; got {ident}"));
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

    fn write_lzx(root: &Path, feature: &str, suffix: &str, content: &str) {
        let feature_root = root.join("features").join(feature);
        fs::create_dir_all(&feature_root).unwrap();
        fs::write(
            feature_root.join(format!("{feature}.{suffix}.lzx")),
            content,
        )
        .unwrap();
    }

    #[test]
    fn appends_view_block_with_todo_markers() {
        let project = tempdir();
        write_lzx(
            project.path(),
            "post",
            "web",
            "surface post web\n  uses feature post\n",
        );

        run("post.recent", project.path()).unwrap();

        let lzx = fs::read_to_string(project.path().join("features/post/post.web.lzx")).unwrap();
        assert!(lzx.contains("view list recent"));
        assert!(lzx.contains("tests"));
        assert!(lzx.contains("accepted by"));
        assert!(lzx.contains("rejected by"));
        assert!(lzx.contains("@TODO authored:"));
    }

    #[test]
    fn errors_when_no_lzx_surface() {
        let project = tempdir();
        fs::create_dir_all(project.path().join("features/post")).unwrap();
        let err = run("post.recent", project.path()).unwrap_err();
        assert!(err.to_string().contains("no surface `.lzx` found"));
    }

    #[test]
    fn refuses_to_clobber_existing_view() {
        let project = tempdir();
        write_lzx(
            project.path(),
            "post",
            "web",
            "surface post web\n  uses feature post\n\n  view list recent\n    anchor @anchor.post_list\n",
        );

        let err = run("post.recent", project.path()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }
}
