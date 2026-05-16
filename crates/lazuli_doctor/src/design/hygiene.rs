//! Catalog hygiene rules (§6.2): `design-token-unused`,
//! `design-token-duplicate-value`, `design-token-missing-dark`.
//!
//! All three are info-severity (§6.2 explicitly justifies low severity
//! pre-pilot since the corresponding remediation paths — aliasing,
//! intentional same-in-both-themes — arrive in Cut B).
//!
//! These rules operate over a `HygieneCatalog` view that pairs the
//! `allowlist.json` token names with their declared values (and dark
//! overrides) — Cell B can emit a sibling `catalog.json` carrying that
//! shape. Until Cell B does so, the hygiene rules return empty Vec
//! (no false positives, no noise).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::helpers::{iter_class_strings, scan_lines, walk_tsx_files};

// ── catalog shape ────────────────────────────────────────────────────────────

/// Mirror of `dist/ts-web/design/catalog.json` — Cell B emits this
/// alongside `allowlist.json`. Optional companion file; hygiene rules
/// suppress when it's absent.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HygieneCatalog {
    /// Color tokens keyed by `<group>.<state>` (e.g. `primary.base`).
    /// Value is the hex with dark optionally set.
    #[serde(default)]
    pub colors: HashMap<String, ColorEntry>,
    /// Other tokens keyed by `<group>.<name>` → string value.
    #[serde(default)]
    pub values: HashMap<String, String>,
    /// Reverse map: `<prefix>-<suffix>` → token group path. Allows the
    /// `unused` check to translate `bg-primary` → `colors.primary.base`.
    #[serde(default)]
    pub class_to_token: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColorEntry {
    pub light: String,
    #[serde(default)]
    pub dark: Option<String>,
    /// The semantic group this color belongs to (e.g. `background`,
    /// `foreground`). Used by `missing-dark` to detect intra-group
    /// inconsistency.
    #[serde(default)]
    pub group: Option<String>,
}

pub fn read_catalog(root: &Path) -> Option<HygieneCatalog> {
    let path = root
        .join("dist")
        .join("ts-web")
        .join("design")
        .join("catalog.json");
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str::<HygieneCatalog>(&raw).ok()
}

// ── unused (info) ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusedFinding {
    pub token_path: String,
}

impl UnusedFinding {
    pub const CODE: &'static str = "design-token-unused";

    pub fn message(&self) -> String {
        format!(
            "token `{}` is declared but never referenced by any `.tsx` file. \
             Remove the token or document why it's reserved.",
            self.token_path,
        )
    }
}

pub fn check_unused(root: &Path, catalog: &HygieneCatalog) -> Vec<UnusedFinding> {
    if catalog.class_to_token.is_empty() {
        return Vec::new();
    }
    // Collect every class token referenced anywhere under root.
    let mut referenced_classes: std::collections::HashSet<String> = Default::default();
    for path in walk_tsx_files(root) {
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (_, line) in scan_lines(&content) {
            for class_str in iter_class_strings(line) {
                for raw in class_str.split_ascii_whitespace() {
                    let token = raw.rsplit(':').next().unwrap_or(raw);
                    let token = token.strip_prefix('!').unwrap_or(token);
                    if token.is_empty() || token.contains('[') {
                        continue;
                    }
                    referenced_classes.insert(token.to_string());
                }
            }
        }
    }
    let referenced_tokens: std::collections::HashSet<&String> = catalog
        .class_to_token
        .iter()
        .filter(|(class, _)| referenced_classes.contains(*class))
        .map(|(_, token)| token)
        .collect();

    let mut out: Vec<UnusedFinding> = catalog
        .class_to_token
        .values()
        .filter(|t| !referenced_tokens.contains(t))
        .map(|t| UnusedFinding {
            token_path: t.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.token_path.cmp(&b.token_path));
    out.dedup_by(|a, b| a.token_path == b.token_path);
    out
}

// ── duplicate-value (info) ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateValueFinding {
    pub value: String,
    pub token_paths: Vec<String>,
}

impl DuplicateValueFinding {
    pub const CODE: &'static str = "design-token-duplicate-value";

    pub fn message(&self) -> String {
        format!(
            "value `{}` is declared by {} tokens ({}) — consolidate or alias when Cut B lands.",
            self.value,
            self.token_paths.len(),
            self.token_paths.join(", "),
        )
    }
}

pub fn check_duplicate_value(catalog: &HygieneCatalog) -> Vec<DuplicateValueFinding> {
    // Build value → [token_path]
    let mut by_value: HashMap<String, Vec<String>> = HashMap::new();
    for (path, entry) in &catalog.colors {
        by_value
            .entry(entry.light.to_ascii_lowercase())
            .or_default()
            .push(path.clone());
    }
    for (path, value) in &catalog.values {
        by_value
            .entry(value.to_ascii_lowercase())
            .or_default()
            .push(path.clone());
    }
    let mut out: Vec<DuplicateValueFinding> = by_value
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(value, mut paths)| {
            paths.sort();
            DuplicateValueFinding { value, token_paths: paths }
        })
        .collect();
    out.sort_by(|a, b| a.value.cmp(&b.value));
    out
}

// ── missing-dark (info) ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingDarkFinding {
    pub token_path: String,
    pub group: String,
}

impl MissingDarkFinding {
    pub const CODE: &'static str = "design-token-missing-dark";

    pub fn message(&self) -> String {
        format!(
            "color `{}` lacks a `dark` variant but other tokens in group `{}` declare one. \
             Decide if this is intentional (same in both themes) and suppress via \
             `# lazuli-allow: design-token-missing-dark — same in both themes`.",
            self.token_path, self.group,
        )
    }
}

pub fn check_missing_dark(catalog: &HygieneCatalog) -> Vec<MissingDarkFinding> {
    // Build group → (has_any_dark, [(path, has_dark)])
    let mut groups: HashMap<String, Vec<(String, bool)>> = HashMap::new();
    for (path, entry) in &catalog.colors {
        let Some(group) = entry.group.clone() else {
            continue;
        };
        groups
            .entry(group)
            .or_default()
            .push((path.clone(), entry.dark.is_some()));
    }
    let mut out = Vec::new();
    for (group, members) in groups {
        let any_dark = members.iter().any(|(_, has)| *has);
        if !any_dark {
            continue;
        }
        for (path, has) in members {
            if !has {
                out.push(MissingDarkFinding {
                    token_path: path,
                    group: group.clone(),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then(a.token_path.cmp(&b.token_path))
    });
    out
}

// ── re-exports for the wire-up file ──────────────────────────────────────────

/// Group findings type used when the orchestrator wants to ship a single
/// list per `.lzi`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HygieneFinding {
    Unused(UnusedFinding),
    DuplicateValue(DuplicateValueFinding),
    MissingDark(MissingDarkFinding),
}

impl HygieneFinding {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unused(_) => UnusedFinding::CODE,
            Self::DuplicateValue(_) => DuplicateValueFinding::CODE,
            Self::MissingDark(_) => MissingDarkFinding::CODE,
        }
    }
}

/// Convenience aggregator: runs all three hygiene checks, returns a
/// flattened Vec. Useful when the orchestrator wants one entry-point.
pub fn check_all(root: &Path, _ignored_path_anchor: &Path) -> Vec<HygieneFinding> {
    let Some(catalog) = read_catalog(root) else {
        return Vec::new();
    };
    let mut out: Vec<HygieneFinding> = Vec::new();
    out.extend(check_unused(root, &catalog).into_iter().map(HygieneFinding::Unused));
    out.extend(
        check_duplicate_value(&catalog)
            .into_iter()
            .map(HygieneFinding::DuplicateValue),
    );
    out.extend(
        check_missing_dark(&catalog)
            .into_iter()
            .map(HygieneFinding::MissingDark),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(light: &str, dark: Option<&str>, group: Option<&str>) -> ColorEntry {
        ColorEntry {
            light: light.into(),
            dark: dark.map(String::from),
            group: group.map(String::from),
        }
    }

    #[test]
    fn unused_skips_when_catalog_empty() {
        let cat = HygieneCatalog::default();
        let td = std::env::temp_dir().join("lazuli-hygiene-empty");
        let _ = std::fs::create_dir_all(&td);
        let f = check_unused(&td, &cat);
        let _ = std::fs::remove_dir_all(&td);
        assert!(f.is_empty());
    }

    #[test]
    fn duplicate_value_detects_two_tokens_same_hex() {
        let mut colors = HashMap::new();
        colors.insert(
            "success".to_string(),
            color("#16a34a", None, Some("status")),
        );
        colors.insert(
            "green".to_string(),
            color("#16a34a", None, Some("brand")),
        );
        let cat = HygieneCatalog {
            colors,
            values: HashMap::new(),
            class_to_token: HashMap::new(),
        };
        let f = check_duplicate_value(&cat);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].value, "#16a34a");
        assert_eq!(f[0].token_paths, vec!["green".to_string(), "success".to_string()]);
    }

    #[test]
    fn missing_dark_flags_inconsistent_group() {
        let mut colors = HashMap::new();
        colors.insert(
            "background.base".to_string(),
            color("#fff", Some("#000"), Some("background")),
        );
        colors.insert(
            "background.muted".to_string(),
            color("#eee", None, Some("background")),
        );
        let cat = HygieneCatalog {
            colors,
            values: HashMap::new(),
            class_to_token: HashMap::new(),
        };
        let f = check_missing_dark(&cat);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].token_path, "background.muted");
        assert_eq!(f[0].group, "background");
    }

    #[test]
    fn missing_dark_silent_when_no_one_has_dark() {
        let mut colors = HashMap::new();
        colors.insert(
            "danger".to_string(),
            color("#dc2626", None, Some("severity")),
        );
        colors.insert(
            "warning".to_string(),
            color("#ea580c", None, Some("severity")),
        );
        let cat = HygieneCatalog {
            colors,
            values: HashMap::new(),
            class_to_token: HashMap::new(),
        };
        assert!(check_missing_dark(&cat).is_empty());
    }
}
