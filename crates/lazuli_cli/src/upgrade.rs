use std::path::{Path, PathBuf};

#[derive(Debug, serde::Deserialize)]
pub struct RecipeMetadata {
    pub from_version: String,
    pub to_version: String,
    pub kind: String,
    pub summary: String,
}

#[derive(Debug)]
pub struct UpgradeReport {
    pub applied: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

pub fn run_upgrade(
    project_root: &Path,
    from: &str,
    to: &str,
    target: &Path,
    dry_run: bool,
) -> Result<UpgradeReport, Box<dyn std::error::Error>> {
    let recipe_root = project_root
        .join("migrations/recipes")
        .join(format!("{}-to-{}", from, to));
    if !recipe_root.exists() {
        return Err(format!(
            "no recipes from {} to {} at {}",
            from,
            to,
            recipe_root.display()
        )
        .into());
    }

    let mut recipes = Vec::new();
    for entry in std::fs::read_dir(&recipe_root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let metadata = load_recipe_metadata(&path)?;
        recipes.push((path, metadata));
    }
    recipes.sort_by(|a, b| a.0.file_name().cmp(&b.0.file_name()));

    let mut report = UpgradeReport {
        applied: Vec::new(),
        failed: Vec::new(),
    };

    for (recipe_path, metadata) in &recipes {
        if major_minor(&metadata.from_version) != from || major_minor(&metadata.to_version) != to {
            report.failed.push((
                recipe_path.clone(),
                format!(
                    "recipe metadata declares {} to {}, expected {} to {}",
                    metadata.from_version, metadata.to_version, from, to
                ),
            ));
            break;
        }
        if metadata.summary.trim().is_empty() {
            report
                .failed
                .push((recipe_path.clone(), "recipe summary is required".to_owned()));
            break;
        }

        let applied = if dry_run {
            Ok(())
        } else {
            apply_recipe(recipe_path, target, metadata)
        };

        match applied {
            Ok(()) => match smoke_recipe(recipe_path) {
                Ok(()) => report.applied.push(recipe_path.clone()),
                Err(error) => {
                    report.failed.push((
                        recipe_path.clone(),
                        format!("smoke failed after apply: {error}"),
                    ));
                    break;
                }
            },
            Err(error) => {
                report.failed.push((recipe_path.clone(), error.to_string()));
                break;
            }
        }
    }

    Ok(report)
}

fn major_minor(version: &str) -> String {
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return version.to_owned();
    };
    let Some(minor) = parts.next() else {
        return version.to_owned();
    };
    format!("{major}.{minor}")
}

fn apply_recipe(
    _recipe_path: &Path,
    _target: &Path,
    metadata: &RecipeMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    match metadata.kind.as_str() {
        "additive" => Ok(()),
        other => Err(format!("recipe kind '{}' not yet implemented in run_upgrade", other).into()),
    }
}

pub(crate) fn smoke_recipe(recipe_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let input = recipe_path.join("input.lzi");
    let output = recipe_path.join("output.lzi");

    if !input.exists() && !output.exists() {
        return Ok(());
    }
    if !input.exists() || !output.exists() {
        return Err("smoke: recipe must provide both input.lzi and output.lzi".into());
    }
    if std::fs::read(&input)? == std::fs::read(&output)? {
        return Ok(());
    }

    let current_exe = std::env::current_exe()?;
    let input_ir = std::process::Command::new(&current_exe)
        .args([
            "inspect",
            input.to_str().ok_or("non-utf8 input path")?,
            "--format=json",
        ])
        .output()?;
    let output_ir = std::process::Command::new(&current_exe)
        .args([
            "inspect",
            output.to_str().ok_or("non-utf8 output path")?,
            "--format=json",
        ])
        .output()?;

    if !input_ir.status.success() {
        return Err(format!(
            "smoke: inspect input failed: {}",
            String::from_utf8_lossy(&input_ir.stderr)
        )
        .into());
    }
    if !output_ir.status.success() {
        return Err(format!(
            "smoke: inspect output failed: {}",
            String::from_utf8_lossy(&output_ir.stderr)
        )
        .into());
    }

    if input_ir.stdout != output_ir.stdout {
        return Err("smoke: input post-upgrade IR != output IR".into());
    }
    Ok(())
}

pub(crate) fn load_recipe_metadata(
    recipe_path: &Path,
) -> Result<RecipeMetadata, Box<dyn std::error::Error>> {
    let toml_path = recipe_path.join("recipe.toml");
    let content = std::fs::read_to_string(&toml_path)?;
    let parsed: toml::Value = toml::from_str(&content)?;
    let recipe_section = parsed.get("recipe").ok_or("missing [recipe] section")?;
    Ok(RecipeMetadata {
        from_version: recipe_section
            .get("from_version")
            .and_then(|v| v.as_str())
            .ok_or("from_version required")?
            .to_string(),
        to_version: recipe_section
            .get("to_version")
            .and_then(|v| v.as_str())
            .ok_or("to_version required")?
            .to_string(),
        kind: recipe_section
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("additive")
            .to_string(),
        summary: recipe_section
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lazuli-upgrade-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_recipe(root: &Path, transition: &str, name: &str, kind: &str) -> PathBuf {
        let recipe = root.join("migrations/recipes").join(transition).join(name);
        std::fs::create_dir_all(&recipe).unwrap();
        std::fs::write(
            recipe.join("recipe.toml"),
            format!(
                "[recipe]\nfrom_version = \"0.11.0\"\nto_version = \"0.12.0\"\nkind = \"{kind}\"\nsummary = \"test\"\n"
            ),
        )
        .unwrap();
        recipe
    }

    #[test]
    fn load_recipe_metadata_parses_toml() {
        let root = temp_dir("metadata");
        let recipe = write_recipe(&root, "0.11-to-0.12", "sample", "additive");
        let metadata = load_recipe_metadata(&recipe).unwrap();
        assert_eq!(metadata.from_version, "0.11.0");
        assert_eq!(metadata.to_version, "0.12.0");
        assert_eq!(metadata.kind, "additive");
        assert_eq!(metadata.summary, "test");
    }

    #[test]
    fn run_upgrade_applies_additive_recipes() {
        let root = temp_dir("apply");
        write_recipe(&root, "0.11-to-0.12", "a-additive", "additive");
        let report = run_upgrade(&root, "0.11", "0.12", &root, false).unwrap();
        assert_eq!(report.applied.len(), 1);
        assert!(report.failed.is_empty());
    }

    #[test]
    fn run_upgrade_halts_chain_on_failure() {
        let root = temp_dir("halt");
        write_recipe(&root, "0.11-to-0.12", "a-additive", "additive");
        write_recipe(&root, "0.11-to-0.12", "b-rename", "rename");
        write_recipe(&root, "0.11-to-0.12", "c-additive", "additive");
        let report = run_upgrade(&root, "0.11", "0.12", &root, false).unwrap();
        assert_eq!(report.applied.len(), 1);
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].1.contains("not yet implemented"));
    }

    #[test]
    fn smoke_recipe_passes_when_input_output_ir_matches() {
        let root = temp_dir("smoke");
        let recipe = write_recipe(&root, "0.11-to-0.12", "same", "additive");
        let fixture = "app Acme\n  lazuli_version \"0.12\"\n";
        std::fs::write(recipe.join("input.lzi"), fixture).unwrap();
        std::fs::write(recipe.join("output.lzi"), fixture).unwrap();
        assert!(smoke_recipe(&recipe).is_ok());
    }
}
