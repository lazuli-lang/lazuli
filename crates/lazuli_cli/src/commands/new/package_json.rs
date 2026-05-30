//! `package.json` merger used by `lazuli new --in-place --frontends web`.
//!
//! Existing projects often carry hand-authored `scripts`,
//! `dependencies`, and `devDependencies` in `app/web/package.json`;
//! a brute `fs::write` of the frontend template would clobber that
//! work. `merge_or_write_package_json` instead reads the existing
//! file, parses it as JSON, and folds the template's keys IN ONLY
//! WHERE MISSING — never overwriting a user-set value.
//!
//! Three key categories:
//!
//! - **Scalar headers** (`name`, `private`, `type`): inserted from the
//!   template only when absent in the existing file.
//! - **Sections** (`scripts`, `dependencies`, `devDependencies`):
//!   merged key-by-key via `entry().or_insert_with(...)` so each
//!   script or dep is only added if the existing object doesn't
//!   already define it.
//! - **Everything else** (e.g. user-added `eslintConfig`,
//!   `husky`): left untouched.
//!
//! When the target file doesn't exist at all, the function falls back
//! to a plain `fs::write(template)` (the `--in-place` path doesn't
//! reach this branch in practice because the call site gates on
//! `package_json_exists`, but the unconditional fallback keeps the
//! function safe to use elsewhere).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Read existing `package.json`, fold the template into it, write back
/// pretty-printed with a trailing newline. If `path` doesn't exist
/// yet, write the template verbatim.
pub(crate) fn merge_or_write_package_json(path: &Path, template: &str) -> Result<()> {
    if !path
        .try_exists()
        .with_context(|| format!("failed to inspect {}", path.display()))?
    {
        fs::write(path, template).with_context(|| format!("writing {}", path.display()))?;
        return Ok(());
    }

    let existing_text =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut existing: serde_json::Value = serde_json::from_str(&existing_text)
        .with_context(|| format!("parsing {}", path.display()))?;
    let template: serde_json::Value =
        serde_json::from_str(template).context("parsing frontend package.json template")?;

    merge_package_json_object(&mut existing, &template)?;

    let mut out = serde_json::to_string_pretty(&existing)?;
    out.push('\n');
    fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Fold scalar headers (when missing) and merge `scripts` /
/// `dependencies` / `devDependencies` (key-by-key, no overwrites).
fn merge_package_json_object(
    existing: &mut serde_json::Value,
    template: &serde_json::Value,
) -> Result<()> {
    let existing_obj = existing
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("package.json root must be a JSON object"))?;
    let template_obj = template
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("package.json template root must be a JSON object"))?;

    for key in ["name", "private", "type"] {
        if !existing_obj.contains_key(key)
            && let Some(value) = template_obj.get(key)
        {
            existing_obj.insert(key.to_string(), value.clone());
        }
    }

    for key in ["scripts", "dependencies", "devDependencies"] {
        merge_package_json_section(existing_obj, template_obj, key)?;
    }

    Ok(())
}

/// Merge one section (`scripts`, `dependencies`, `devDependencies`).
/// When the section is absent on the existing side, copy the template
/// section verbatim. When present, iterate the template's entries and
/// `or_insert_with(...)` each one — user values always win.
fn merge_package_json_section(
    existing_obj: &mut serde_json::Map<String, serde_json::Value>,
    template_obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<()> {
    let Some(template_section) = template_obj.get(key) else {
        return Ok(());
    };
    let Some(template_section) = template_section.as_object() else {
        bail!("package.json template section `{key}` must be an object");
    };

    if !existing_obj.contains_key(key) {
        existing_obj.insert(
            key.to_string(),
            serde_json::Value::Object(template_section.clone()),
        );
        return Ok(());
    }

    let existing_section = existing_obj
        .get_mut(key)
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| anyhow::anyhow!("package.json section `{key}` must be an object"))?;
    for (dep, version) in template_section {
        existing_section
            .entry(dep.clone())
            .or_insert_with(|| version.clone());
    }

    Ok(())
}
