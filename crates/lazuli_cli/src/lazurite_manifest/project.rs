//! `[project]`, `[lazuli]`, `[lazurite]`, and `[plugins]` schema —
//! the four sections that identify the project and pin its runtime,
//! template, and plugin dependencies.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Project {
    pub name: String,
    pub module: String,
    pub schema: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LazuliPin {
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Lazurite {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_version: Option<String>,
    /// Optional subdir (relative to project root) holding `app.lzi`,
    /// `design.lzi`, and `registry.lzi`. Defaults to the project root.
    /// Set to `"app"` to group Lazuli sources under an `app/` subdir
    /// alongside the shell/UI code already conventional there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_dir: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Plugin {
    Remote { module: String, version: String },
    Local { path: String },
}
