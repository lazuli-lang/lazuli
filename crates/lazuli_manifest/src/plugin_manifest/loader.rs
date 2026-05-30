//! Plugin-manifest file IO + path resolution.
//!
//! Two entry points:
//!
//! * `load_plugin_manifest` — reads `<root>/manifest.toml` if present
//!   and deserialises it. Returns `Ok(None)` when the file is absent;
//!   parse or read failures become `PluginManifestError::Read`.
//! * `resolve_plugin_root` — projects a `Lazurite.toml [plugins]`
//!   entry into a concrete filesystem root. `dev.plugin_paths`
//!   overrides take precedence regardless of the base plugin shape;
//!   remote plugins without an override return `None`.
//!
//! The `default_error_code` helper is the convention fallback used by
//! `alias_map.rs` when a scalar entry omits `error_code`.

use std::path::{Path, PathBuf};

use crate::lazurite_manifest::{Manifest, Plugin};

use super::errors::PluginManifestError;
use super::types::{PLUGIN_MANIFEST_FILENAME, PluginManifest};

/// `BrazilianCPF` → `cpf_invalid`. Strips a leading nationality prefix
/// (the common pattern across the scalars-br catalog) and lowercases,
/// then appends `_invalid`. Used as the fallback when a scalar entry
/// doesn't declare an explicit `error_code`.
pub(super) fn default_error_code(name: &str) -> String {
    for prefix in ["Brazilian"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return format!("{}_invalid", rest.to_ascii_lowercase());
        }
    }
    format!("{}_invalid", name.to_ascii_lowercase())
}

/// Load `manifest.toml` from a single plugin root. Returns `Ok(None)`
/// when the file is absent (a plugin without semantic-types is a valid
/// shape — only Go adapter is required by the plugin-authoring
/// contract). Read/parse failures surface as `Err`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::plugin_manifest::loader::load_plugin_manifest;
///
/// // let manifest = load_plugin_manifest(Path::new("plugins/foo"))?;
/// ```
pub fn load_plugin_manifest(
    plugin_root: &Path,
) -> Result<Option<PluginManifest>, PluginManifestError> {
    let path = plugin_root.join(PLUGIN_MANIFEST_FILENAME);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path).map_err(|err| PluginManifestError::Read {
        plugin: plugin_root.display().to_string(),
        path: path.clone(),
        message: err.to_string(),
    })?;
    let manifest: PluginManifest =
        toml::from_str(&raw).map_err(|err| PluginManifestError::Read {
            plugin: plugin_root.display().to_string(),
            path: path.clone(),
            message: err.to_string(),
        })?;
    Ok(Some(manifest))
}

/// Resolve `<project_root>` + a `Lazurite.toml [plugins]` entry to a
/// concrete plugin root directory. For `Plugin::Local { path }` the
/// path is resolved relative to the project root (absolute paths pass
/// through). For `Plugin::Remote` we consult `dev.plugin_paths` first
/// and bail with `Ok(None)` if no local override exists — the proposal
/// is explicit that module-mode plugins without local overrides skip
/// the static manifest read entirely.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::plugin_manifest::loader::resolve_plugin_root;
///
/// // let root = resolve_plugin_root(&manifest, Path::new("."),
/// //                                "@lazuli/plugin-foo");
/// ```
pub fn resolve_plugin_root(
    manifest: &Manifest,
    project_root: &Path,
    plugin_ref: &str,
) -> Option<PathBuf> {
    let plugin = manifest.plugins.get(plugin_ref)?;
    // `dev.plugin_paths` override takes precedence regardless of the
    // base plugin shape. This mirrors the existing `go.mod replace`
    // emission at `crates/lazuli_codegen_go/src/emitter/module.rs:842`.
    if let Some(dev) = manifest.dev.as_ref()
        && let Some(path) = dev.plugin_paths.get(plugin_ref)
    {
        return Some(absolutise(project_root, Path::new(path)));
    }
    match plugin {
        Plugin::Local { path } => Some(absolutise(project_root, Path::new(path))),
        Plugin::Remote { .. } => None,
    }
}

pub(super) fn absolutise(project_root: &Path, candidate: &Path) -> PathBuf {
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_error_code_strips_brazilian_prefix() {
        assert_eq!(default_error_code("BrazilianCPF"), "cpf_invalid");
        assert_eq!(default_error_code("Phone"), "phone_invalid");
    }

    #[test]
    fn load_plugin_manifest_missing_file_returns_ok_none() {
        let dir = std::env::temp_dir().join("lazuli-plugin-loader-test");
        let _ = std::fs::create_dir_all(&dir);
        let result = load_plugin_manifest(&dir).expect("ok none");
        assert!(result.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
