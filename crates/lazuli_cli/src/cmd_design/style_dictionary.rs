//! Amazon Style Dictionary codec.
//!
//! Style Dictionary's source format mirrors W3C structurally but drops
//! the `$` prefix from `$value` / `$type` / `$extensions`. So both the
//! encoder and the decoder are one key-rewrite away from the canonical
//! Figma codec: encode by running `design_to_figma` then stripping
//! `$`s; decode by adding `$`s back then running `figma_to_design`.
//!
//! This keeps a single source of truth for every parse rule (closed
//! catalogs, dark-mode extension, color states) in `figma.rs`.

use anyhow::Result;
use serde_json::{Map, Value};

use super::Design;
use super::figma::{design_to_figma, figma_to_design};

pub(super) fn design_to_style_dictionary(design: &Design) -> Value {
    let figma = design_to_figma(design);
    convert_keys_dollar_to_plain(figma)
}

pub(super) fn style_dictionary_to_design(value: &Value) -> Result<Design> {
    let figma = convert_keys_plain_to_dollar(value.clone());
    figma_to_design(&figma)
}

/// Translates `$value` / `$type` / `$extensions` -> `value` / `type` /
/// `extensions`. Style Dictionary's source format mirrors W3C structurally
/// but drops the `$` prefix.
fn convert_keys_dollar_to_plain(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                let new_key = match key.as_str() {
                    "$value" => "value".to_string(),
                    "$type" => "type".to_string(),
                    "$extensions" => "extensions".to_string(),
                    _ => key,
                };
                out.insert(new_key, convert_keys_dollar_to_plain(child));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(convert_keys_dollar_to_plain)
                .collect(),
        ),
        other => other,
    }
}

fn convert_keys_plain_to_dollar(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                let new_key = match key.as_str() {
                    "value" => "$value".to_string(),
                    "type" => "$type".to_string(),
                    "extensions" => "$extensions".to_string(),
                    _ => key,
                };
                out.insert(new_key, convert_keys_plain_to_dollar(child));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(convert_keys_plain_to_dollar)
                .collect(),
        ),
        other => other,
    }
}
