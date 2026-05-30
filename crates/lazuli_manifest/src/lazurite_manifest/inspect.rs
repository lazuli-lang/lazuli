//! `lazuli inspect` projection of `Lazurite.toml` — borrow-friendly
//! shapes that serialise the manifest into the canonical JSON output
//! without dragging the parse-time types into the public surface.

use serde::Serialize;

use super::frontends::FrontendTarget;
use super::generate::Generate;
use super::migrations::{DevOverrides, Migrations, Seeds};
use super::project::{LazuliPin, Lazurite, Project};

/// Borrow-friendly projection of `Lazurite.toml` emitted by
/// `lazuli inspect --format json`. Avoids cloning the parsed manifest
/// while keeping the JSON output stable.
#[derive(Debug, Serialize)]
pub struct InspectManifest<'a> {
    /// Where the manifest was loaded from (`"file"` or `"defaults"`).
    pub origin: &'static str,
    /// `[project]` block.
    pub project: &'a Project,
    /// `[lazuli]` pin.
    pub lazuli: &'a LazuliPin,
    /// Optional `[lazurite]` block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lazurite: Option<&'a Lazurite>,
    /// Flattened `[plugins]` view.
    pub plugins: Vec<InspectPlugin<'a>>,
    /// `[generate]` block.
    pub generate: &'a Generate,
    /// Flattened `[frontends]` view.
    pub frontends: Vec<InspectFrontend<'a>>,
    /// Optional `[migrations]` block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migrations: Option<&'a Migrations>,
    /// Optional `[seeds]` block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seeds: Option<&'a Seeds>,
    /// Optional `[dev]` overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<&'a DevOverrides>,
}

/// One entry in `InspectManifest::plugins`.
#[derive(Debug, Serialize)]
pub struct InspectPlugin<'a> {
    /// Canonical plugin reference (`@lazuli/plugin-…`).
    #[serde(rename = "ref")]
    pub plugin_ref: &'a str,
    /// Module path (when an authored override exists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<&'a str>,
    /// Pinned plugin version, when authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<&'a str>,
    /// Optional local-path override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<&'a str>,
    /// `"manifest"` when authored, `"catalog"` when synthesized.
    pub source: &'static str,
}

/// One entry in `InspectManifest::frontends`.
#[derive(Debug, Serialize)]
pub struct InspectFrontend<'a> {
    /// `[frontends.<name>]` key.
    pub name: &'a str,
    /// Frontend runtime.
    pub target: &'a FrontendTarget,
    /// Optional source override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'a str>,
    /// Generated-output directory.
    pub out: &'a str,
    /// Audiences this frontend renders.
    pub audiences: &'a [String],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_plugin_serializes_ref() {
        let plugin = InspectPlugin {
            plugin_ref: "@lazuli/plugin-foo",
            module: None,
            version: None,
            path: None,
            source: "catalog",
        };
        let json = serde_json::to_string(&plugin).unwrap();
        assert!(json.contains("\"ref\":\"@lazuli/plugin-foo\""));
    }
}
