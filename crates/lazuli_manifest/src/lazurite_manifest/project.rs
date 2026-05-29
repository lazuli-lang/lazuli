//! `[project]`, `[lazuli]`, `[lazurite]`, and `[plugins]` schema —
//! the four sections that identify the project and pin its runtime,
//! template, and plugin dependencies.

use serde::{Deserialize, Serialize};

/// `[project]` block — identity of the workspace.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Project {
    /// Project name (Cargo-style identifier, used in generated
    /// artifacts).
    pub name: String,
    /// Go-style module path for generated backend code.
    pub module: String,
    /// Manifest schema version — must equal `1`.
    pub schema: u32,
}

/// `[lazuli]` block — pin the Lazuli runtime + optional dev path.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LazuliPin {
    /// Runtime version this manifest targets.
    pub runtime: String,
    /// Optional path to the Lazuli source checkout, used by codegen
    /// to emit portable `dist/lazurite.vite.mjs` aliases (and other
    /// dev-time runtime path resolution). Absolute or relative to
    /// the project root. When `None`, codegen assumes
    /// `@lazuli/runtime` is installed as an npm package (no alias
    /// needed); when `Some`, codegen emits aliases pointing at the
    /// source tree via `import.meta.url`-relative paths so the
    /// generated file works regardless of where the project is
    /// checked out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// `[lazurite]` block — Lazurite-scaffold metadata + optional
/// `app_dir` override.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Lazurite {
    /// Template the project was scaffolded from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Template version this project tracks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_version: Option<String>,
    /// Optional subdir (relative to project root) holding `app.lzi`,
    /// `design.lzi`, and `registry.lzi`. Defaults to the project root.
    /// Set to `"app"` to group Lazuli sources under an `app/` subdir
    /// alongside the shell/UI code already conventional there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_dir: Option<String>,
}

/// One `[plugins.<key>]` entry. Untagged so the TOML can use either
/// the remote (`module` + `version`) or the local (`path`) shape
/// directly without a discriminator field.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Plugin {
    /// Plugin published as an npm/Cargo package — pinned by version.
    Remote {
        /// Module identifier (e.g., `@lazuli/plugin-mobile`).
        module: String,
        /// Pinned version.
        version: String,
    },
    /// Plugin sitting at a local path (typical for in-monorepo
    /// development of a plugin alongside its consumer).
    Local {
        /// Path relative to the project root.
        path: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_remote_deserializes_from_inline_table() {
        let toml = r#"module = "@lazuli/plugin-mobile"
version = "1.0.0""#;
        let plugin: Plugin = toml::from_str(toml).expect("parse");
        match plugin {
            Plugin::Remote { module, version } => {
                assert_eq!(module, "@lazuli/plugin-mobile");
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("expected Remote"),
        }
    }
}
