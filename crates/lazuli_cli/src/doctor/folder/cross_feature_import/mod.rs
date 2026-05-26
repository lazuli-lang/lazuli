//! Doctor rule `cross-feature-direct-import`.
//!
//! Flags TypeScript `import` statements in
//! `app/features/A/{web,mobile}/<...>.tsx` that reference
//! `app/features/B/{web,mobile}/<...>` for A != B. Cross-feature
//! visual dependencies must go via slot bindings (`@client.<slot>` declared
//! in `B.<target>.lzx`) so the audience scope and typed contract are
//! preserved.
//!
//! Allowed cross-feature paths:
//!   - `@generated/<feat>/<...>` — generated SDK is shared across features.
//!   - `@web/<...>` — frontend adapter primitives (shell, theme, ui, lib).
//!
//! Forbidden:
//!   - `@app/features/B/web/cells/<...>` from `app/features/A/`.
//!   - `@app/features/B/web/views/<...>` from `app/features/A/`.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// One direct cross-feature import flagged by the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path of the `.ts`/`.tsx` that issued the bad import.
    pub source_file: PathBuf,
    /// Feature whose frontend tree the import references.
    pub target_feature: String,
    /// Feature that contains `source_file`.
    pub source_feature: String,
    /// Verbatim import specifier as authored.
    pub import_specifier: String,
    /// Pre-rendered diagnostic message.
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "cross-feature-direct-import";

    /// Build the canonical diagnostic message for the rule. Lives on
    /// the impl so the walker can format the string before allocating
    /// a `Finding`.
    pub fn message(
        source_file: &Path,
        source_feature: &str,
        target_feature: &str,
        import_specifier: &str,
    ) -> String {
        format!(
            "`{}` (feature `{}`) imports directly from feature `{}` via \
             `{}`. Cross-feature visual dependencies must go via slot \
             bindings (`@client.<slot>` declared in \
             `{}.<target>.lzx`) or shared `frontends/<target>/ui/` primitives.",
            source_file.display(),
            source_feature,
            target_feature,
            import_specifier,
            target_feature,
        )
    }
}

/// Walk feature roots, scan each `.tsx`/`.ts` for import statements
/// that reference a sibling feature's frontend tree. Emit Finding per
/// violation.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::doctor::folder::cross_feature_import::check;
///
/// // let findings = check(Path::new("."));
/// ```
pub fn check(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for features_root in [root.join("app").join("features"), root.join("features")] {
        for feature_dir in read_sorted_dirs(&features_root) {
            let Some(source_feature) = file_name_str(&feature_dir).map(str::to_owned) else {
                continue;
            };

            for target in ["web", "mobile"] {
                let frontend_root = feature_dir.join(target);
                walk_frontend_files(&frontend_root, &mut |source_file| {
                    scan_file(root, source_file, &source_feature, &mut findings);
                });
            }
        }
    }

    findings.sort_by(|a, b| {
        (&a.source_file, &a.target_feature, &a.import_specifier).cmp(&(
            &b.source_file,
            &b.target_feature,
            &b.import_specifier,
        ))
    });
    findings
}

// --- helpers ---

/// Extract the import specifier from a line like `import X from "Y";`
/// or `import { X } from "Y";` or `import type { X } from "Y";`.
/// Returns the string between the matched quotes, or None.
fn extract_import_specifier(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("import") {
        return None;
    }
    let from_idx = trimmed.find(" from ")?;
    let after_from = &trimmed[from_idx + " from ".len()..];
    let after_from = after_from.trim_start();
    let quote = after_from.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after_from[1..];
    let end = rest.find(quote)?;
    Some(&rest[..end])
}

/// Parse a feature import specifier into (feat,
/// target). Returns None if the specifier doesn't match the shape.
/// `target` is "web" or "mobile"; anything else returns None.
fn parse_feature_import(spec: &str) -> Option<(&str, &str)> {
    for prefix in ["@app/features/", "@/features/"] {
        if let Some(rest) = spec.strip_prefix(prefix) {
            return parse_frontend_feature_path(rest);
        }
    }
    None
}

fn scan_file(root: &Path, source_file: &Path, source_feature: &str, findings: &mut Vec<Finding>) {
    let Ok(contents) = fs::read_to_string(source_file) else {
        return;
    };

    for line in contents.lines() {
        let Some(import_specifier) = extract_import_specifier(line) else {
            continue;
        };
        let Some((target_feature, _target)) =
            parse_import_target(root, source_file, import_specifier)
        else {
            continue;
        };
        if target_feature == source_feature {
            continue;
        }

        findings.push(Finding {
            source_file: source_file.to_path_buf(),
            target_feature: target_feature.clone(),
            source_feature: source_feature.to_owned(),
            import_specifier: import_specifier.to_owned(),
            message: Finding::message(
                source_file,
                source_feature,
                &target_feature,
                import_specifier,
            ),
        });
    }
}

fn parse_import_target(
    root: &Path,
    source_file: &Path,
    import_specifier: &str,
) -> Option<(String, String)> {
    if let Some(parsed) = parse_feature_import(import_specifier) {
        return Some((parsed.0.to_owned(), parsed.1.to_owned()));
    }

    if !import_specifier.starts_with('.') {
        return None;
    }

    if let Some((feature, target)) = parse_relative_feature_import(import_specifier) {
        return Some((feature.to_owned(), target.to_owned()));
    }

    let normalized = normalize_path(source_file.parent()?.join(import_specifier));
    if let Some(parsed) = parse_normalized_feature_path(root, &normalized) {
        return Some(parsed);
    }

    None
}

fn parse_normalized_feature_path(root: &Path, path: &Path) -> Option<(String, String)> {
    for features_root in [root.join("app").join("features"), root.join("features")] {
        let features_root = normalize_path(features_root);
        let Ok(rel) = path.strip_prefix(features_root) else {
            continue;
        };
        let mut parts = rel.components();
        let feat = component_str(parts.next()?)?;
        let target = component_str(parts.next()?)?;
        if target != "web" && target != "mobile" {
            return None;
        }
        return Some((feat.to_owned(), target.to_owned()));
    }
    None
}

fn parse_relative_feature_import(spec: &str) -> Option<(&str, &str)> {
    let mut rest = spec;
    loop {
        if let Some(next) = rest.strip_prefix("../") {
            rest = next;
        } else if let Some(next) = rest.strip_prefix("./") {
            rest = next;
        } else {
            break;
        }
    }
    parse_frontend_feature_path(rest)
}

fn parse_frontend_feature_path(path: &str) -> Option<(&str, &str)> {
    let mut parts = path.splitn(3, '/');
    let feat = parts.next()?;
    let target = parts.next()?;
    if feat.is_empty() || (target != "web" && target != "mobile") {
        return None;
    }
    Some((feat, target))
}

fn walk_frontend_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if file_name_str(&path).is_some_and(should_skip_dir) {
                continue;
            }
            walk_frontend_files(&path, visit);
        } else if file_type.is_file() && is_ts_or_tsx(&path) {
            visit(&path);
        }
    }
}

fn read_sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            file_type.is_dir().then(|| entry.path())
        })
        .collect();
    dirs.sort_by_key(|path| file_name_str(path).map(str::to_owned));
    dirs
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn component_str(component: Component<'_>) -> Option<&str> {
    match component {
        Component::Normal(value) => value.to_str(),
        _ => None,
    }
}

fn is_ts_or_tsx(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "tsx")
    )
}

fn file_name_str(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | "dist" | "target" | ".git" | ".lazuli" | ".next" | ".expo"
    )
}


#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests_helpers;
#[cfg(test)]
mod tests_integration;
