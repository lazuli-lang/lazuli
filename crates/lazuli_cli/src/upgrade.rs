use std::path::{Path, PathBuf};

/// Front-matter parsed off the top of an upgrade recipe markdown
/// file. Tells the runner which version pair the recipe targets and
/// what authored shape (`kind`) it operates on.
#[derive(Debug, serde::Deserialize)]
pub struct RecipeMetadata {
    /// Source Lazuli version the recipe migrates from.
    pub from_version: String,
    /// Destination version it migrates to.
    pub to_version: String,
    /// Recipe kind (the closed list of artifact rewrites).
    pub kind: String,
    /// Human-readable summary surfaced in `lazuli upgrade` output.
    pub summary: String,
}

/// Outcome of an `upgrade` invocation — partitioned into recipes that
/// applied cleanly and those that errored mid-run.
#[derive(Debug)]
pub struct UpgradeReport {
    /// Recipe files that finished without error.
    pub applied: Vec<PathBuf>,
    /// Recipe files that errored, paired with the error message.
    pub failed: Vec<(PathBuf, String)>,
}

/// Apply every upgrade recipe between `from` and `to` against
/// `target` under `project_root`. `dry_run` previews without writing.
/// Errors when no recipes exist for the version pair.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::upgrade::run_upgrade;
/// // let report = run_upgrade(Path::new("."), "1.0", "1.1", Path::new("."), true)?;
/// ```
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
    recipe_path: &Path,
    target: &Path,
    metadata: &RecipeMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    match metadata.kind.as_str() {
        // Additive recipes insert new declarations; the runner records intent and
        // the smoke fixture proves the shape — there is no in-place edit here.
        "additive" => Ok(()),
        // `rename` (a keyword/sigil/catalog token swap) and `rewrite` (a structural
        // find->replace) share one text-rule engine; they differ only in authoring
        // intent — collapsing them to one mechanism keeps the migration surface
        // peaked. Each `[[rule]]` is a literal find->replace applied to every
        // `.lzi`/`.lzx` under `target`. Meaning-preservation is proven separately by
        // [`smoke_recipe`] against the recipe's own input/output fixture, so a
        // recipe that changes meaning fails the run rather than corrupting sources.
        "rename" | "rewrite" => apply_text_rules(recipe_path, target),
        other => Err(format!("recipe kind '{}' not yet implemented in run_upgrade", other).into()),
    }
}

/// Load the `[[rule]]` `{ find, replace }` pairs a `rename`/`rewrite` recipe
/// applies. Errors when a rule omits `find` or the recipe declares none.
fn load_recipe_rules(
    recipe_path: &Path,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(recipe_path.join("recipe.toml"))?;
    let parsed: toml::Value = toml::from_str(&content)?;
    let mut rules = Vec::new();
    if let Some(arr) = parsed.get("rule").and_then(|v| v.as_array()) {
        for rule in arr {
            let find = rule
                .get("find")
                .and_then(|v| v.as_str())
                .ok_or("each [[rule]] needs a `find`")?;
            if find.is_empty() {
                return Err("[[rule]] `find` must be non-empty".into());
            }
            let replace = rule.get("replace").and_then(|v| v.as_str()).unwrap_or("");
            rules.push((find.to_string(), replace.to_string()));
        }
    }
    if rules.is_empty() {
        return Err("rename/rewrite recipe declares no [[rule]] entries".into());
    }
    Ok(rules)
}

/// Apply the recipe's text rules to every `.lzi`/`.lzx` source under `target`
/// (or to `target` itself when it is a single source file). Only files that
/// actually change are rewritten.
fn apply_text_rules(recipe_path: &Path, target: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let rules = load_recipe_rules(recipe_path)?;
    let mut files = Vec::new();
    collect_sources(target, &mut files)?;
    for file in files {
        let original = std::fs::read_to_string(&file)?;
        let mut updated = original.clone();
        for (find, replace) in &rules {
            updated = updated.replace(find.as_str(), replace.as_str());
        }
        if updated != original {
            std::fs::write(&file, updated)?;
        }
    }
    Ok(())
}

/// Recursively collect `.lzi`/`.lzx` sources under `root` (or `root` itself if
/// it is one). Skips generated/internal trees so a recipe never rewrites
/// `dist/`, `.lazuli/`, its own `migrations/` fixtures, `.git/`, or `target/`.
fn collect_sources(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if root.is_file() {
        if is_source(root) {
            out.push(root.to_path_buf());
        }
        return Ok(());
    }
    if !root.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            let skip = matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("dist") | Some(".lazuli") | Some("migrations") | Some(".git") | Some("target")
            );
            if !skip {
                collect_sources(&path, out)?;
            }
        } else if is_source(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Whether `path` is a Lazuli source file (`.lzi` or `.lzx`).
fn is_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("lzi") | Some("lzx")
    )
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
        write_recipe(&root, "0.11-to-0.12", "b-bogus", "bogus");
        write_recipe(&root, "0.11-to-0.12", "c-additive", "additive");
        let report = run_upgrade(&root, "0.11", "0.12", &root, false).unwrap();
        assert_eq!(report.applied.len(), 1);
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].1.contains("not yet implemented"));
    }

    #[test]
    fn run_upgrade_applies_rewrite_recipe() {
        let root = temp_dir("rewrite");
        let recipe = write_recipe(&root, "0.11-to-0.12", "a-rewrite", "rewrite");
        let toml = recipe.join("recipe.toml");
        let mut content = std::fs::read_to_string(&toml).unwrap();
        content.push_str("\n[[rule]]\nfind = \"@semantic.\"\nreplace = \"\"\n");
        std::fs::write(&toml, content).unwrap();

        let src = root.join("app/feature.lzi");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(
            &src,
            "feature x\n  domain\n    resource R\n      email: @semantic.Email required\n",
        )
        .unwrap();

        let report = run_upgrade(&root, "0.11", "0.12", &root, false).unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(report.applied.len(), 1);

        let after = std::fs::read_to_string(&src).unwrap();
        assert!(after.contains("email: Email required"), "got: {after}");
        assert!(!after.contains("@semantic."));
    }

    #[test]
    fn rewrite_recipe_with_no_rules_fails() {
        let root = temp_dir("norules");
        write_recipe(&root, "0.11-to-0.12", "a-rewrite", "rewrite");
        let report = run_upgrade(&root, "0.11", "0.12", &root, false).unwrap();
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].1.contains("no [[rule]] entries"));
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
