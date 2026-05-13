use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
pub struct ExampleEntry {
    pub name: String,
    pub intent: String,
    pub lzi_source: String,
    pub ir_snippet: serde_json::Value,
    pub common_errors: Vec<String>,
}

pub fn run_examples_bundle(
    project_root: &Path,
    out_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries = load_curated_examples(project_root)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let mut jsonl = entries
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    if !jsonl.is_empty() {
        jsonl.push('\n');
    }

    match out_path {
        Some(path) => fs::write(path, jsonl)?,
        None => print!("{jsonl}"),
    }

    Ok(())
}

pub fn run_examples_validate(project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let curated_dir = curated_dir(project_root);
    if !curated_dir.exists() {
        return Err("examples/curated/ does not exist; run from project root".into());
    }

    for entry in fs::read_dir(&curated_dir)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        validate_curated_example(project_root, &path)?;
    }

    Ok(())
}

fn load_curated_examples(
    project_root: &Path,
) -> Result<Vec<ExampleEntry>, Box<dyn std::error::Error>> {
    let curated_dir = curated_dir(project_root);
    if !curated_dir.exists() {
        return Err("examples/curated/ does not exist; run from project root".into());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&curated_dir)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        entries.push(load_curated_example(&path)?);
    }
    Ok(entries)
}

fn load_curated_example(path: &Path) -> Result<ExampleEntry, Box<dyn std::error::Error>> {
    let manifest_path = path.join("manifest.toml");
    let manifest: toml::Value = fs::read_to_string(&manifest_path)?.parse()?;
    let name = manifest
        .get("example")
        .and_then(|example| example.get("name"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("missing [example].name in {}", manifest_path.display()))?
        .to_owned();
    let intent = manifest
        .get("example")
        .and_then(|example| example.get("intent"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("missing [example].intent in {}", manifest_path.display()))?
        .to_owned();
    let common_errors = manifest
        .get("common_errors")
        .and_then(|errors| errors.get("codes"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let lzi_path = path.join(format!("{name}.lzi"));
    let lzi_source = fs::read_to_string(&lzi_path)?;
    let ir_snippet = serde_json::from_str(&fs::read_to_string(path.join("expected_ir.json"))?)?;

    Ok(ExampleEntry {
        name,
        intent,
        lzi_source,
        ir_snippet,
        common_errors,
    })
}

fn validate_curated_example(
    project_root: &Path,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let example = load_curated_example(path)?;
    let lzi_path = path.join(format!("{}.lzi", example.name));

    super::check_command(&lzi_path, super::CheckSecurityProfile::Prototype)?;

    let source = fs::read_to_string(&lzi_path)?;
    let inspect_path = lzi_path.strip_prefix(project_root).unwrap_or(&lzi_path);
    let actual =
        super::inspect_json_value(&source, inspect_path, super::ExpandSet::default(), &[])?;
    if normalize_snapshot(actual) != normalize_snapshot(example.ir_snippet) {
        return Err(format!(
            "IR snapshot mismatch for {}; regenerate {}",
            example.name,
            path.join("expected_ir.json").display()
        )
        .into());
    }

    Ok(())
}

fn curated_dir(project_root: &Path) -> PathBuf {
    project_root.join("examples").join("curated")
}

fn normalize_snapshot(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(source) = value.get_mut("source").and_then(|source| source.as_str()) {
        let normalized = source.replace('\\', "/");
        value["source"] = serde_json::Value::String(normalized);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn bundle_emits_deterministic_jsonl() {
        let root = temp_root("bundle_deterministic");
        write_example(&root, "zeta", "zeta intent");
        write_example(&root, "alpha", "alpha intent");
        let out = root.join("bundle.jsonl");

        run_examples_bundle(&root, Some(&out)).unwrap();
        let lines = fs::read_to_string(out).unwrap();

        let mut parsed = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed.remove(0)["name"], "alpha");
        assert_eq!(parsed.remove(0)["name"], "zeta");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bundle_skips_non_directory_entries() {
        let root = temp_root("bundle_skip_file");
        write_example(&root, "alpha", "alpha intent");
        fs::write(root.join("examples/curated/README.md"), "skip me").unwrap();
        let out = root.join("bundle.jsonl");

        run_examples_bundle(&root, Some(&out)).unwrap();
        let lines = fs::read_to_string(out).unwrap();

        assert_eq!(lines.lines().count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validate_runs_check_inspect_against_each() {
        let root = temp_root("bundle_validate");
        let example_dir = write_example(&root, "minimal_command", "minimal command");
        let lzi_path = example_dir.join("minimal_command.lzi");
        let source = fs::read_to_string(&lzi_path).unwrap();
        let inspect_path = lzi_path.strip_prefix(&root).unwrap();
        let expected = super::super::inspect_json_value(
            &source,
            inspect_path,
            super::super::ExpandSet::default(),
            &[],
        )
        .unwrap();
        fs::write(
            example_dir.join("expected_ir.json"),
            serde_json::to_string_pretty(&expected).unwrap(),
        )
        .unwrap();

        run_examples_validate(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lazuli_{name}_{nonce}"));
        fs::create_dir_all(root.join("examples/curated")).unwrap();
        root
    }

    fn write_example(root: &Path, name: &str, intent: &str) -> PathBuf {
        let dir = root.join("examples/curated").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("{name}.lzi")),
            "feature minimal_demo\n  domain\n    resource Item\n      name: Text required\n\n  command create_item\n    input\n      name: Text required\n    creates Item\n      name: input.name\n",
        )
        .unwrap();
        fs::write(
            dir.join("manifest.toml"),
            format!(
                "[example]\nname = \"{name}\"\nintent = \"{intent}\"\n\n[provenance]\npilots = [\"docs.example.minimal\"]\nlast_validated = \"2026-05-13\"\n\n[common_errors]\ncodes = [\"field_required\", \"resource_undefined\"]\n"
            ),
        )
        .unwrap();
        fs::write(dir.join("expected_ir.json"), "{}").unwrap();
        dir
    }
}
