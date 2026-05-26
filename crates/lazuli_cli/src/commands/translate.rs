//! `lazuli translate extract` — emit per-feature, per-locale
//! translation catalog stubs derived from the IR's
//! `feature.translation` block.
//!
//! For every feature carrying a `translation` block, the handler
//! enumerates declared `key` entries, walks the source `.lzi` files for
//! `@translation.<key>` references the IR doesn't yet model
//! structurally (it's a text-pattern walk doctor also performs), and
//! writes one JSON catalog stub per `(feature, locale)` pair into the
//! `--out` directory. The output shape is intentionally minimal —
//! `{ "<key>": "<text or empty>" }` — so existing localization
//! pipelines (Lokalise, Crowdin, hand-edited JSON, etc.) can consume
//! it without bespoke parsing.
//!
//! `--check` flips the handler into CI gate mode: missing variants for
//! the project's default locale are hard failures; missing variants
//! for non-default supported locales become warnings;
//! `@translation.<key>` references the feature's catalog doesn't
//! declare are hard failures regardless of locale.
//!
//! `--locale <tag>` restricts the emit to one locale (useful for
//! incremental refreshes during translation cycles).
//!
//! Cross-refs:
//! - `lazuli_ir::TranslationBlock` — the typed shape `feature
//!   translation` lifts to.
//! - `crate::build_module_from_path` — the compile entry the handler
//!   shares with the rest of the CLI.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::build_module_from_path;

/// Handler for the `TranslateCommand::Extract` clap arm.
///
/// Compiles `input` to IR, walks the locale catalog from the app
/// manifest, and writes per-`(feature, locale)` JSON catalog stubs to
/// `out`. `check` flips into CI gate mode (missing default-locale
/// variants and undeclared `@translation.<key>` references are errors);
/// `locale_filter` restricts the emit to a single locale.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::translate::translate_extract_command;
///
/// // translate_extract_command(Path::new("."), Path::new("translations"), None, false)?;
/// ```
pub fn translate_extract_command(
    input: &Path,
    out: &Path,
    locale_filter: Option<&str>,
    check: bool,
) -> Result<()> {
    let module = build_module_from_path(input)?;

    // Locale catalog from the app manifest. Defaults to `[default]` when
    // a project authors only the bare scalar.
    let supported: Vec<String> = match module.app.as_ref() {
        Some(app) => match app.locale.as_ref() {
            Some(locale) => locale.supported.clone(),
            None => app
                .default_locale
                .as_ref()
                .map(|d| vec![d.clone()])
                .unwrap_or_default(),
        },
        None => Vec::new(),
    };
    let default_locale = module
        .app
        .as_ref()
        .and_then(|app| {
            app.locale
                .as_ref()
                .map(|l| l.default.clone())
                .or_else(|| app.default_locale.clone())
        })
        .unwrap_or_default();
    if supported.is_empty() {
        anyhow::bail!(
            "no `app.locale.supported` (or `default_locale`) declared; cannot extract translations"
        );
    }

    let mut missing: Vec<String> = Vec::new();
    let mut unresolved_refs: Vec<String> = Vec::new();

    // Per-feature catalog stubs.
    for feature in &module.features {
        let Some(translation) = &feature.translation else {
            continue;
        };
        let declared: std::collections::BTreeSet<&str> =
            translation.keys.iter().map(|k| k.name.as_str()).collect();

        // Resolve `@translation.<key>` references walked in the source
        // file. The legacy `Rule` IR slot does not yet carry
        // `message_ref`; doctor uses a text-pattern walk for this and
        // we mirror that here.
        let feature_paths: Vec<PathBuf> = match feature.span_ref.as_ref() {
            Some(_) => collect_feature_lzi_paths(input, &feature.name)?,
            None => Vec::new(),
        };
        for path in &feature_paths {
            let text =
                fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            for line in text.lines() {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix("message @translation.") {
                    let key = rest.split_whitespace().next().unwrap_or("");
                    if !key.is_empty() && !declared.contains(key) {
                        unresolved_refs.push(format!("{}.{}", feature.name, key));
                    }
                }
            }
        }
        for locale in &supported {
            if let Some(filter) = locale_filter {
                if filter != locale.as_str() {
                    continue;
                }
            }
            let catalog_path = translation.catalog.replace("<locale>", locale);
            let stub_path = out
                .join(format!("{}.{}.json", feature.name, locale))
                .to_owned();
            // Write a minimal `{ "<key>": "<text or empty>" }` stub.
            let mut entries: Vec<(String, String)> = Vec::new();
            for key in &translation.keys {
                let variant = key
                    .variants
                    .iter()
                    .find(|v| v.locale.as_str() == locale.as_str());
                let text = match variant {
                    Some(v) => v.text.clone(),
                    None => {
                        let key_id = format!("{}.{}.{}", feature.name, key.name, locale);
                        missing.push(key_id);
                        String::new()
                    }
                };
                entries.push((key.name.clone(), text));
            }
            let mut json = String::new();
            json.push_str("{\n");
            for (idx, (k, v)) in entries.iter().enumerate() {
                json.push_str(&format!(
                    "  \"{}\": \"{}\"{}\n",
                    json_escape(k),
                    json_escape(v),
                    if idx + 1 < entries.len() { "," } else { "" }
                ));
            }
            json.push_str("}\n");
            if let Some(parent) = stub_path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("creating output directory {}", parent.display())
                    })?;
                }
            }
            fs::write(&stub_path, &json)
                .with_context(|| format!("writing {}", stub_path.display()))?;
            println!(
                "extracted {} keys to {} (catalog template: {})",
                entries.len(),
                stub_path.display(),
                catalog_path
            );
        }
    }

    if check {
        let mut failures: Vec<String> = Vec::new();
        for entry in &missing {
            // The default locale must always be authored; warn for
            // non-default supported tags but only fail CI for default.
            if entry.ends_with(&format!(".{}", default_locale)) {
                failures.push(format!("missing variant for default locale: {entry}"));
            } else {
                eprintln!("warning: missing variant for supported locale: {entry}");
            }
        }
        for entry in &unresolved_refs {
            failures.push(format!("unresolved `@translation.{entry}` reference"));
        }
        if !failures.is_empty() {
            for failure in &failures {
                eprintln!("error: {failure}");
            }
            anyhow::bail!(
                "translate extract --check failed ({} issue(s))",
                failures.len()
            );
        }
    } else if !missing.is_empty() {
        for entry in &missing {
            eprintln!("warning: missing variant: {entry}");
        }
    }

    Ok(())
}

/// `lazuli translate extract` helper — collect the `.lzi` paths that
/// host a given feature. We mirror what `build_module_from_path` does
/// — walk the package's `.lzi` files and return any that contain a
/// `feature <name>` header.
fn collect_feature_lzi_paths(root: &Path, feature_name: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let candidates: Vec<PathBuf> = if root.is_dir() {
        let mut acc: Vec<PathBuf> = Vec::new();
        for entry in
            fs::read_dir(root).with_context(|| format!("reading directory {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("lzi") {
                acc.push(path);
            }
        }
        acc
    } else {
        vec![root.to_path_buf()]
    };
    let header = format!("feature {feature_name}");
    for path in candidates {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        for line in text.lines() {
            if line.trim_start() == header || line.trim_start().starts_with(&format!("{header} ")) {
                out.push(path.clone());
                break;
            }
        }
    }
    Ok(out)
}

/// Minimal JSON string escape used by the catalog-stub emitter — kept
/// in-module because the rest of the CLI relies on `serde_json` for
/// the same job. Strictly handles the JSON-mandated escapes (`"`,
/// `\\`, `\n`, `\r`, `\t`); higher control codes flow through as-is
/// since translation catalogs target human-readable text.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_missing_input_errors() {
        let result = translate_extract_command(
            Path::new("__lazuli_no_such_file.lzi"),
            Path::new("__lazuli_translate_out"),
            None,
            false,
        );
        assert!(result.is_err());
    }
}
