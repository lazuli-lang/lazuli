//! Package-wide `@semantic.<Name> → ResolvedPluginSemantic` alias map.
//!
//! `build_alias_map` is the only entry point. It iterates every declared
//! plugin (`Lazurite.toml [plugins]` keys in alphabetical order via
//! `BTreeMap`), resolves each to a filesystem root, loads its
//! `manifest.toml`, validates the namespace + carrier + name/alias
//! consistency, and produces a deterministic alias index.
//!
//! Conflicts (two plugins owning the same alias) surface as
//! `PluginManifestError::Conflict` with sorted namespaces so doctor
//! output stays byte-stable.

use std::collections::BTreeMap;
use std::path::Path;

use crate::lazurite_manifest::Manifest;

use super::errors::PluginManifestError;
use super::loader::{default_error_code, load_plugin_manifest, resolve_plugin_root};
use super::types::ResolvedPluginSemantic;

/// Build the package-wide `@semantic.<Name> → ResolvedPluginSemantic`
/// alias map. Iterates every declared plugin in deterministic order,
/// reads each manifest, and surfaces conflicts as errors.
///
/// Sites without a `Lazurite.toml` (e.g. single-file `lazuli check`)
/// pass `None`; the map is empty and every `@semantic.<plugin name>`
/// reference will fall through to `SEMANTIC-PLUGIN-001` in the doctor.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::plugin_manifest::alias_map::build_alias_map;
///
/// let aliases = build_alias_map(None, Path::new(".")).expect("build");
/// assert!(aliases.is_empty());
/// ```
pub fn build_alias_map(
    manifest: Option<&Manifest>,
    project_root: &Path,
) -> Result<BTreeMap<String, ResolvedPluginSemantic>, PluginManifestError> {
    let Some(manifest) = manifest else {
        return Ok(BTreeMap::new());
    };
    // First pass: collect every (alias, plugin) pair so duplicates can
    // be reported in one shot (the proposal requires conflict detection
    // even if both plugins are loaded successfully).
    let mut by_alias: BTreeMap<String, Vec<ResolvedPluginSemantic>> = BTreeMap::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = resolve_plugin_root(manifest, project_root, plugin_ref) else {
            // Remote plugins without a dev override skip the static
            // manifest read entirely. Doctor surfaces the field-level
            // SEMANTIC-PLUGIN-001 against the call site, which is the
            // right diagnostic anchor.
            continue;
        };
        // Best-effort parse — manifests using a different sibling
        // contract (e.g. storage contracts) are skipped rather than
        // fatal. Only parse failures that affect a real
        // `[[semantic_types]]` declaration matter to the alias map.
        let plugin_manifest = match load_plugin_manifest(&plugin_root) {
            Ok(Some(m)) => m,
            Ok(None) => continue,
            Err(PluginManifestError::Read { path, message, .. }) => {
                // Annotate with the plugin ref so any downstream
                // doctor error points at the right [plugins] key. Skip
                // rather than fail because the plugin may not declare
                // semantic types and the doctor's other plugin lints
                // cover the underlying parse breakage.
                eprintln!(
                    "lazuli: skipping plugin manifest for `{}` at `{}`: {}",
                    plugin_ref,
                    path.display(),
                    message
                );
                continue;
            }
            Err(other) => return Err(other),
        };
        // Manifests without a `[plugin]` block carry no semantic-type
        // contribution by definition. Skip cleanly.
        if plugin_manifest.semantic_types.is_empty() {
            continue;
        }
        let identity = match plugin_manifest.plugin.as_ref() {
            Some(id) => id,
            None => {
                // A manifest with [[semantic_types]] but no [plugin]
                // block can't be resolved (no namespace, no short
                // name). Surface as a parse error so the author fixes
                // the manifest.
                return Err(PluginManifestError::NamespaceMismatch {
                    plugin: plugin_ref.clone(),
                    manifest_namespace: String::new(),
                });
            }
        };
        // Manifest's declared namespace must match the
        // `Lazurite.toml [plugins]` key — locks plugin identity to one
        // authoritative source.
        if identity.namespace != *plugin_ref {
            return Err(PluginManifestError::NamespaceMismatch {
                plugin: plugin_ref.clone(),
                manifest_namespace: identity.namespace.clone(),
            });
        }
        for entry in &plugin_manifest.semantic_types {
            // Closed-catalog carrier check — v1 accepts `String` only.
            let carrier = match entry.carrier_type.as_str() {
                "String" => lazuli_ir::BuiltinType::Text,
                other => {
                    return Err(PluginManifestError::UnsupportedCarrier {
                        plugin: plugin_ref.clone(),
                        alias: entry.alias.clone(),
                        carrier_type: other.to_owned(),
                    });
                }
            };
            // `name` must equal alias terminal segment so authors can
            // round-trip from inspect JSON back to source text without
            // an indirection table.
            let expected_terminal = entry
                .alias
                .strip_prefix("@semantic.")
                .unwrap_or(entry.alias.as_str());
            if expected_terminal != entry.name {
                return Err(PluginManifestError::NameAliasMismatch {
                    plugin: plugin_ref.clone(),
                    alias: entry.alias.clone(),
                    name: entry.name.clone(),
                });
            }
            let resolved = ResolvedPluginSemantic {
                plugin_namespace: plugin_ref.clone(),
                plugin_short_name: identity.name.clone(),
                name: entry.name.clone(),
                alias: entry.alias.clone(),
                carrier,
                validator: entry.validator.clone(),
                formatter: entry.formatter.clone(),
                go_module: identity
                    .go_module
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("lazuli.dev/plugin/{}", identity.name)),
                ts_package: identity
                    .ts_package
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("@lazuli/plugin-{}", identity.name)),
                error_code: entry
                    .error_code
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| default_error_code(&entry.name)),
                message_key: entry.message_key.clone().unwrap_or_default(),
                ts_validator: entry.ts_validator.clone().unwrap_or_default(),
            };
            by_alias
                .entry(entry.alias.clone())
                .or_default()
                .push(resolved);
        }
    }
    // Second pass: collapse the per-alias vector into a single entry,
    // surfacing conflict errors with sorted namespaces for byte-stable
    // diagnostic output.
    let mut out = BTreeMap::new();
    for (alias, mut candidates) in by_alias {
        if candidates.len() > 1 {
            let mut plugins: Vec<String> = candidates
                .iter()
                .map(|c| c.plugin_namespace.clone())
                .collect();
            plugins.sort();
            return Err(PluginManifestError::Conflict { alias, plugins });
        }
        out.insert(alias, candidates.remove(0));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_manifest_returns_empty_map() {
        let map = build_alias_map(None, Path::new(".")).expect("build");
        assert!(map.is_empty());
    }
}
