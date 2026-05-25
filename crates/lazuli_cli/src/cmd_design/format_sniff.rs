//! Design-token catalog format detection.
//!
//! `lazuli design import` accepts two external JSON catalogs — W3C
//! Design Tokens (Figma Tokens Studio) and Amazon Style Dictionary.
//! When the user does not pass `--format`, `sniff_format` picks the
//! right codec via filename hints first (`*.figma.json` / `*.sd.json`),
//! then JSON structural inspection (W3C uses `$value` keys; Style
//! Dictionary uses bare `value`).
//!
//! `json_contains_key` is the small recursive helper used by the
//! structural fallback. Kept private to this module.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use super::ImportFormat;

/// Heuristic: filename `*.figma.json` => Figma; `*.sd.json` => Style
/// Dictionary; otherwise inspect the JSON for the W3C `$value` /
/// `$type` markers.
pub(super) fn sniff_format(path: &Path) -> Result<ImportFormat> {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    if lower.ends_with(".figma.json") {
        return Ok(ImportFormat::Figma);
    }
    if lower.ends_with(".sd.json") || lower.contains("style-dictionary") {
        return Ok(ImportFormat::StyleDictionary);
    }
    // Fallback to JSON inspection. Look for any `$value` field anywhere
    // in the tree — present in W3C, absent in Style Dictionary.
    let raw = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing JSON at {}", path.display()))?;
    if json_contains_key(&value, "$value") {
        Ok(ImportFormat::Figma)
    } else if json_contains_key(&value, "value") {
        Ok(ImportFormat::StyleDictionary)
    } else {
        Err(anyhow!(
            "could not sniff design-token format for {}: pass --format figma|style-dictionary",
            path.display()
        ))
    }
}

fn json_contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            if map.contains_key(key) {
                return true;
            }
            map.values().any(|child| json_contains_key(child, key))
        }
        Value::Array(items) => items.iter().any(|child| json_contains_key(child, key)),
        _ => false,
    }
}
