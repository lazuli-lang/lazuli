//! `lazuli generate transition <feature>.<workflow>.<name>` subcommand.
//!
//! Wave 3 — appends a `transition <name>` inside an existing
//! `workflow <workflow>` block (or creates the workflow shell + transition).
//! Pre-populated `tests` block carries `@TODO authored:` markers that trip
//! `TEST-STUB-001`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

/// Run `lazuli generate transition <feature>.<workflow>.<name>`.
pub fn run(ident: &str, project_root: &Path) -> Result<()> {
    let (feature, workflow, name) = parse_ident(ident)?;
    validate_part(&feature, "feature")?;
    validate_part(&workflow, "workflow")?;
    validate_part(&name, "transition name")?;

    let feat_root = app_root(project_root)?.join("features").join(&feature);
    let lzi_path = feat_root.join(format!("{}.lzi", feature));
    if !lzi_path.exists() {
        return Err(anyhow!(
            "feature .lzi not found: {} — run `lazuli generate feature {}` first",
            lzi_path.display(),
            feature
        ));
    }

    let existing =
        fs::read_to_string(&lzi_path).with_context(|| format!("reading {}", lzi_path.display()))?;
    let workflow_header = format!("  workflow {}\n", workflow);
    let transition_needle = format!("    transition {}\n", name);
    if existing.contains(&workflow_header) && existing.contains(&transition_needle) {
        return Err(anyhow!(
            "transition `{}.{}.{}` already exists in {} — refusing to clobber",
            feature,
            workflow,
            name,
            lzi_path.display()
        ));
    }

    let block = if existing.contains(&workflow_header) {
        render_transition_only(&name)
    } else {
        render_workflow_with_transition(&workflow, &name)
    };
    let mut updated = existing;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&block);
    fs::write(&lzi_path, updated).with_context(|| format!("writing {}", lzi_path.display()))?;
    println!(
        "appended transition `{}.{}.{}` with `tests` block ({} @TODO authored: markers) to {}",
        feature,
        workflow,
        name,
        block.matches("@TODO authored:").count(),
        lzi_path.display()
    );
    Ok(())
}

fn render_workflow_with_transition(workflow: &str, name: &str) -> String {
    format!(
        "
  workflow {workflow}
    # @TODO authored: declare states + initial
    states draft, active
    initial draft

    transition {name}
      from draft
      to active
      policy @policy.update
      tests
        # @TODO authored: cover state-edge boundaries
        allows from draft
        denies from active
        # @TODO authored: cover policy-actor boundary
        allows as @role.editor
        denies as @role.viewer
"
    )
}

fn render_transition_only(name: &str) -> String {
    format!(
        "
  # Note: appended below the parent workflow header — move it inside the
  # workflow body if necessary.
    transition {name}
      from draft
      to active
      policy @policy.update
      tests
        # @TODO authored: cover state-edge boundaries
        allows from draft
        denies from active
        # @TODO authored: cover policy-actor boundary
        allows as @role.editor
        denies as @role.viewer
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

fn parse_ident(ident: &str) -> Result<(String, String, String)> {
    let mut parts = ident.split('.');
    let feature = parts.next().unwrap_or_default();
    let workflow = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if feature.is_empty() || workflow.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(anyhow!(
            "transition ident must be <feature>.<workflow>.<name>; got {ident}"
        ));
    }
    Ok((feature.to_string(), workflow.to_string(), name.to_string()))
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
    fn appends_workflow_with_transition_when_workflow_missing() {
        let project = tempdir();
        write_lzi(project.path(), "post", "feature post\n  purpose \"...\"\n");

        run("post.publication.publish", project.path()).unwrap();

        let lzi = fs::read_to_string(project.path().join("features/post/post.lzi")).unwrap();
        assert!(lzi.contains("workflow publication"));
        assert!(lzi.contains("transition publish"));
        assert!(lzi.contains("tests"));
        assert!(lzi.contains("@TODO authored:"));
    }

    #[test]
    fn appends_transition_only_when_workflow_exists() {
        let project = tempdir();
        write_lzi(
            project.path(),
            "post",
            "feature post\n\n  workflow publication\n    states draft, active\n    initial draft\n",
        );

        run("post.publication.publish", project.path()).unwrap();

        let lzi = fs::read_to_string(project.path().join("features/post/post.lzi")).unwrap();
        // Only one workflow block:
        assert_eq!(lzi.matches("workflow publication").count(), 1);
        assert!(lzi.contains("transition publish"));
        assert!(lzi.contains("@TODO authored:"));
    }

    #[test]
    fn refuses_to_clobber_existing_transition() {
        let project = tempdir();
        write_lzi(
            project.path(),
            "post",
            "feature post\n\n  workflow publication\n    states draft, active\n    initial draft\n\n    transition publish\n      from draft\n      to active\n",
        );

        let err = run("post.publication.publish", project.path()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }
}
