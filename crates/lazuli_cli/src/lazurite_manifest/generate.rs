//! `[generate]` block schema — for now only `[generate.go]` exists;
//! future SDK targets land alongside as sibling sub-blocks.

use serde::{Deserialize, Serialize};

/// Top-level `[generate]` block. Today only `[generate.go]` exists; a
/// future SDK target gains a sibling sub-block here.
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Generate {
    /// Optional `[generate.go]` sub-block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go: Option<GenerateGo>,
}

/// `[generate.go]` knobs — every field carries a default so pilots
/// can omit the whole block.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenerateGo {
    /// Output directory (relative to the project root).
    #[serde(default = "default_go_out")]
    pub out: String,
    /// Whether to run `gofmt` on generated files.
    #[serde(default = "default_true")]
    pub gofmt: bool,
    /// Whether the strict §6.2.1 error catalog is enforced as a write
    /// gate.
    #[serde(default = "default_true")]
    pub strict: bool,
    /// Whether to emit the `main.go` entry point.
    #[serde(default = "default_true")]
    pub emit_main: bool,
    /// Whether the per-feature Go package is a sub-module.
    #[serde(default = "default_true")]
    pub submodule: bool,
    /// Optional `replace` directive added to the generated `go.mod`
    /// during local development.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_go_uses_dist_go() {
        let go = GenerateGo::default();
        assert_eq!(go.out, "dist/go");
        assert!(go.gofmt);
        assert!(go.strict);
        assert!(go.emit_main);
        assert!(go.submodule);
    }
}
