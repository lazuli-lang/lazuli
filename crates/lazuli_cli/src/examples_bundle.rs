use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

pub fn run_examples_validate(
    project_root: &Path,
    check_decay: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let curated_dir = curated_dir(project_root);
    if !curated_dir.exists() {
        return Err("examples/curated/ does not exist; run from project root".into());
    }

    let mut warnings = Vec::new();
    let today_epoch_day = if check_decay {
        Some(current_epoch_day()?)
    } else {
        None
    };
    for entry in fs::read_dir(&curated_dir)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        validate_curated_example(project_root, &path)?;
        if let Some(today_epoch_day) = today_epoch_day {
            warnings.extend(decay_warnings_for_example(&path, today_epoch_day)?);
        }
    }

    if !warnings.is_empty() {
        eprintln!("Decay warnings:");
        for warning in warnings {
            eprintln!("  {warning}");
        }
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

    super::check_command(&lzi_path, super::CheckSecurityProfile::Prototype, false)?;

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

fn decay_warnings_for_example(
    path: &Path,
    today_epoch_day: i64,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let manifest_path = path.join("manifest.toml");
    let manifest: toml::Value = fs::read_to_string(&manifest_path)?.parse()?;
    let Some(last_validated_str) = manifest
        .get("provenance")
        .and_then(|provenance| provenance.get("last_validated"))
        .and_then(|value| value.as_str())
    else {
        return Ok(Vec::new());
    };
    let Some(last_validated_day) = parse_epoch_day(last_validated_str) else {
        return Ok(Vec::new());
    };

    let age_days = today_epoch_day.saturating_sub(last_validated_day);
    if age_days <= 90 {
        return Ok(Vec::new());
    }

    Ok(vec![format!(
        "{}: provenance.last_validated = {} ({} days ago, > 90). Re-validate or rotate to docs/recipes/ per INCLUSION_POLICY.md.",
        path.display(),
        last_validated_str,
        age_days
    )])
}

fn current_epoch_day() -> Result<i64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64 / 86_400)
}

fn parse_epoch_day(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - (month <= 2) as i32;
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let day = day as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as i64
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

        run_examples_validate(&root, false).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn examples_validate_check_decay_warns_for_old_provenance() {
        let root = temp_root("bundle_decay_old");
        let example_dir = write_example(&root, "minimal_command", "minimal command");
        rewrite_last_validated(&example_dir, "2026-01-01");

        let warnings =
            decay_warnings_for_example(&example_dir, parse_epoch_day("2026-05-13").unwrap())
                .unwrap();

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("provenance.last_validated = 2026-01-01"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn examples_validate_check_decay_silent_for_recent_provenance() {
        let root = temp_root("bundle_decay_recent");
        let example_dir = write_example(&root, "minimal_command", "minimal command");

        let warnings =
            decay_warnings_for_example(&example_dir, parse_epoch_day("2026-05-13").unwrap())
                .unwrap();

        assert!(warnings.is_empty(), "got {warnings:?}");
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

    fn rewrite_last_validated(dir: &Path, date: &str) {
        let manifest = fs::read_to_string(dir.join("manifest.toml")).unwrap();
        fs::write(
            dir.join("manifest.toml"),
            manifest.replace(
                "last_validated = \"2026-05-13\"",
                &format!("last_validated = \"{date}\""),
            ),
        )
        .unwrap();
    }
}
