//! `[generate]` block schema — for now only `[generate.go]` exists;
//! future SDK targets land alongside as sibling sub-blocks.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Generate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go: Option<GenerateGo>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenerateGo {
    #[serde(default = "default_go_out")]
    pub out: String,
    #[serde(default = "default_true")]
    pub gofmt: bool,
    #[serde(default = "default_true")]
    pub strict: bool,
    #[serde(default = "default_true")]
    pub emit_main: bool,
    #[serde(default = "default_true")]
    pub submodule: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_replace: Option<String>,
}

/// Frente 1 — canonical defaults for `[generate.go]`. Applied
/// transparently when the block is absent from `Lazurite.toml`, so
/// pilots can omit boilerplate that matches the canonical layout.
impl Default for GenerateGo {
    fn default() -> Self {
        Self {
            out: default_go_out(),
            gofmt: true,
            strict: true,
            emit_main: true,
            submodule: true,
            dev_replace: None,
        }
    }
}

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn default_go_out() -> String {
    "dist/go".to_string()
}
