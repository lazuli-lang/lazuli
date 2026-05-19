//! B3 — plugin `manifest.toml` loader and `@semantic.<Name>` alias map.
//!
//! See `docs/proposals/semantic-types-plugin-locales.md` for the full
//! contract. Wire-thin: this module owns TOML parsing for the plugin
//! manifest file + the alias lookup index. Resolution of a specific
//! `@semantic.<Name>` reference happens in the analyzer's post-pass
//! (see `crates/lazuli_analyzer/src/lib.rs::resolve_plugin_semantics`).
//!
//! ## Plugin location resolution
//!
//! Per `Lazurite.toml [plugins]` semantics:
//! - `Plugin::Local { path }` — dev mode; `path` is a filesystem path
//!   (absolute or relative to the project root) pointing at the plugin
//!   repo root. `manifest.toml` lives at `<path>/manifest.toml`.
//! - `Plugin::Remote { module, version }` — module mode; `module` is a
//!   Go module path resolved via `dev.plugin_paths` overrides. When no
//!   override is declared we skip the lookup (the plugin's manifest is
//!   not on the local filesystem). Module-mode resolution is intentionally
//!   conservative in v1 — pilots universally use `path` while the
//!   ecosystem matures (Hostpoint reality: every plugin is local).
//!
//! ## Determinism
//!
//! Output is a `BTreeMap`; iteration is alphabetical by alias. Two
//! plugins declaring the same alias produce a `Conflict` error with
//! both namespaces sorted in the error message.
//!
//! Wire-thin: no runtime network fetch. No filesystem discovery
//! outside the paths declared in `Lazurite.toml`. The proposal
//! `docs/proposals/semantic-types-plugin-locales.md` §Manifest mechanism
//! is explicit about this — "no global plugin search, no registry
//! fetch during analysis".

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::lazurite_manifest::{Manifest, Plugin};

/// Filename for the per-plugin manifest. Lives next to the plugin's
/// Go `adapter.go` per `docs/plugin-authoring.md:45`.
pub const PLUGIN_MANIFEST_FILENAME: &str = "manifest.toml";

/// Top-level shape of a plugin's `manifest.toml`. Every field is
/// optional so manifests authored against a different sibling
/// contract (e.g. `@plugin/object-store`'s storage-contract manifest
/// with top-level `name`/`version`/`implements` keys) deserialise
/// cleanly without forcing the semantic-types path. The B3 alias
/// builder skips manifests that lack `[plugin]` or `[[semantic_types]]`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginManifest {
    #[serde(default)]
    pub plugin: Option<PluginIdentity>,
    #[serde(default)]
    pub semantic_types: Vec<PluginSemanticTypeDecl>,
}

/// `[plugin]` block — namespace + short name. `namespace` must equal
/// the `Lazurite.toml [plugins]` key; `name` is used as the prefix in
/// generated Go validate tags (`<name>.<validator>`).
#[derive(Debug, Clone, Deserialize)]
pub struct PluginIdentity {
    pub name: String,
    pub namespace: String,
}

/// One `[[semantic_types]]` entry. `alias` is the source-form authoring
/// shape (`@semantic.BrazilianCPF`). `name` must equal the alias terminal
/// segment. `carrier_type` is the closed-catalog Lazuli builtin name
/// (v1: `"String"` only — wider carriers gated by a separate proposal).
/// `validator` is the exported Go function on the plugin adapter; it
/// pairs with `name` to form the runtime validate-tag key. `formatter`
/// is optional and unused by storage validity.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginSemanticTypeDecl {
    pub name: String,
    pub alias: String,
    pub carrier_type: String,
    pub validator: String,
    #[serde(default)]
    pub formatter: Option<String>,
}

/// Resolved alias entry — everything the analyzer, doctor, codegen,
/// and inspect need to project a `@semantic.<Name>` reference into a
/// `BuiltinType::SemanticPluginType` variant + carry the validator
/// metadata through to tag emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPluginSemantic {
    /// Plugin namespace as written in `Lazurite.toml [plugins]`
    /// (e.g. `@plugin/scalars-br`). The IR variant carries this string
    /// verbatim so cold readers see the registration source.
    pub plugin_namespace: String,
    /// Plugin short name (e.g. `scalars-br`). Used to build the
    /// `<name>.<validator>` validate-tag key in Go codegen.
    pub plugin_short_name: String,
    /// Alias terminal name (e.g. `BrazilianCPF`). IR variant carries
    /// this as `name`.
    pub name: String,
    /// Full alias (`@semantic.BrazilianCPF`). Map key.
    pub alias: String,
    /// Built-in carrier type (v1: always `Text`).
    pub carrier: lazuli_ir::BuiltinType,
    /// Exported Go validator function name (e.g. `ValidateCPF`).
    pub validator: String,
    /// Optional exported formatter (rarely used in v1; reserved for
    /// future display-side codegen).
    pub formatter: Option<String>,
}

/// Error produced when alias resolution fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginManifestError {
    /// Could not read or parse a plugin's `manifest.toml`. Carries the
    /// plugin namespace + the underlying error message so doctor can
    /// surface the cause.
    Read {
        plugin: String,
        path: PathBuf,
        message: String,
    },
    /// Two or more active plugins declare the same `@semantic.<Name>`
    /// alias. Carries the alias + sorted list of conflicting plugin
    /// namespaces so the error message is deterministic.
    Conflict { alias: String, plugins: Vec<String> },
    /// Manifest entry's `name` doesn't equal the alias terminal segment
    /// (e.g. `alias = "@semantic.CPF"` but `name = "BrazilianCPF"`).
    NameAliasMismatch {
        plugin: String,
        alias: String,
        name: String,
    },
    /// `carrier_type` is outside the v1 closed catalog. Today only
    /// `String` is accepted; wider carriers (`Integer`, `Decimal`,
    /// structured) need a separate proposal per §Manifest mechanism.
    UnsupportedCarrier {
        plugin: String,
        alias: String,
        carrier_type: String,
    },
    /// `manifest.plugin.namespace` doesn't match the `Lazurite.toml`
    /// `[plugins]` key that activated it. The proposal requires they
    /// be identical so plugin namespace ownership stays unambiguous.
    NamespaceMismatch {
        plugin: String,
        manifest_namespace: String,
    },
}

impl std::fmt::Display for PluginManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read {
                plugin,
                path,
                message,
            } => write!(
                f,
                "failed to read plugin manifest for `{}` at `{}`: {}",
                plugin,
                path.display(),
                message
            ),
            Self::Conflict { alias, plugins } => write!(
                f,
                "plugin semantic alias `{}` is declared by multiple plugins: {}",
                alias,
                plugins.join(", ")
            ),
            Self::NameAliasMismatch {
                plugin,
                alias,
                name,
            } => write!(
                f,
                "plugin `{}` semantic entry name `{}` does not match alias terminal segment of `{}`",
                plugin, name, alias
            ),
            Self::UnsupportedCarrier {
                plugin,
                alias,
                carrier_type,
            } => write!(
                f,
                "plugin `{}` semantic `{}` uses carrier_type `{}`; v1 closed catalog accepts only `String`",
                plugin, alias, carrier_type
            ),
            Self::NamespaceMismatch {
                plugin,
                manifest_namespace,
            } => write!(
                f,
                "plugin `{}` manifest declares namespace `{}`; must match the Lazurite.toml [plugins] key",
                plugin, manifest_namespace
            ),
        }
    }
}

impl std::error::Error for PluginManifestError {}

/// Load `manifest.toml` from a single plugin root. Returns `Ok(None)`
/// when the file is absent (a plugin without semantic-types is a valid
/// shape — only Go adapter is required by the plugin-authoring
/// contract). Read/parse failures surface as `Err`.
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
pub fn resolve_plugin_root(
    manifest: &Manifest,
    project_root: &Path,
    plugin_ref: &str,
) -> Option<PathBuf> {
    let plugin = manifest.plugins.get(plugin_ref)?;
    // `dev.plugin_paths` override takes precedence regardless of the
    // base plugin shape. This mirrors the existing `go.mod replace`
    // emission at `crates/lazuli_codegen_go/src/emitter/module.rs:842`.
    if let Some(dev) = manifest.dev.as_ref() {
        if let Some(path) = dev.plugin_paths.get(plugin_ref) {
            return Some(absolutise(project_root, Path::new(path)));
        }
    }
    match plugin {
        Plugin::Local { path } => Some(absolutise(project_root, Path::new(path))),
        Plugin::Remote { .. } => None,
    }
}

fn absolutise(project_root: &Path, candidate: &Path) -> PathBuf {
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    }
}

/// Build the package-wide `@semantic.<Name> → ResolvedPluginSemantic`
/// alias map. Iterates every declared plugin in deterministic order,
/// reads each manifest, and surfaces conflicts as errors.
///
/// Sites without a `Lazurite.toml` (e.g. single-file `lazuli check`)
/// pass `None`; the map is empty and every `@semantic.<plugin name>`
/// reference will fall through to `SEMANTIC-PLUGIN-001` in the doctor.
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
    use crate::lazurite_manifest::{LazuliPin, Project};
    use std::fs;

    fn make_manifest(plugins: BTreeMap<String, Plugin>) -> Manifest {
        Manifest {
            project: Project {
                name: "test".to_owned(),
                module: "github.com/example/test".to_owned(),
                schema: 1,
            },
            lazuli: LazuliPin {
                runtime: "0.1.0".to_owned(),
            },
            lazurite: None,
            plugins,
            generate: Default::default(),
            frontends: BTreeMap::new(),
            migrations: None,
            seeds: None,
            dev: None,
        }
    }

    fn write_manifest(dir: &Path, body: &str) {
        fs::write(dir.join(PLUGIN_MANIFEST_FILENAME), body).unwrap();
    }

    #[test]
    fn missing_manifest_yields_empty_map() {
        let tmp = tempfile::tempdir().unwrap();
        let mut plugins = BTreeMap::new();
        plugins.insert(
            "@plugin/empty".to_owned(),
            Plugin::Local {
                path: tmp.path().display().to_string(),
            },
        );
        let manifest = make_manifest(plugins);
        let map = build_alias_map(Some(&manifest), tmp.path()).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn happy_path_loads_one_alias() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            r#"
[plugin]
name = "scalars-br"
namespace = "@plugin/scalars-br"

[[semantic_types]]
name = "BrazilianCPF"
alias = "@semantic.BrazilianCPF"
carrier_type = "String"
validator = "ValidateCPF"
formatter = "FormatCPF"
"#,
        );
        let mut plugins = BTreeMap::new();
        plugins.insert(
            "@plugin/scalars-br".to_owned(),
            Plugin::Local {
                path: tmp.path().display().to_string(),
            },
        );
        let manifest = make_manifest(plugins);
        let map = build_alias_map(Some(&manifest), tmp.path()).unwrap();
        let entry = map.get("@semantic.BrazilianCPF").expect("alias resolved");
        assert_eq!(entry.plugin_namespace, "@plugin/scalars-br");
        assert_eq!(entry.plugin_short_name, "scalars-br");
        assert_eq!(entry.name, "BrazilianCPF");
        assert_eq!(entry.validator, "ValidateCPF");
        assert_eq!(entry.formatter.as_deref(), Some("FormatCPF"));
        assert_eq!(entry.carrier, lazuli_ir::BuiltinType::Text);
    }

    #[test]
    fn duplicate_alias_across_plugins_yields_conflict() {
        let tmp_a = tempfile::tempdir().unwrap();
        let tmp_b = tempfile::tempdir().unwrap();
        write_manifest(
            tmp_a.path(),
            r#"
[plugin]
name = "alpha"
namespace = "@plugin/alpha"

[[semantic_types]]
name = "MyType"
alias = "@semantic.MyType"
carrier_type = "String"
validator = "ValidateAlpha"
"#,
        );
        write_manifest(
            tmp_b.path(),
            r#"
[plugin]
name = "beta"
namespace = "@plugin/beta"

[[semantic_types]]
name = "MyType"
alias = "@semantic.MyType"
carrier_type = "String"
validator = "ValidateBeta"
"#,
        );
        let mut plugins = BTreeMap::new();
        plugins.insert(
            "@plugin/alpha".to_owned(),
            Plugin::Local {
                path: tmp_a.path().display().to_string(),
            },
        );
        plugins.insert(
            "@plugin/beta".to_owned(),
            Plugin::Local {
                path: tmp_b.path().display().to_string(),
            },
        );
        let manifest = make_manifest(plugins);
        let err = build_alias_map(Some(&manifest), tmp_a.path()).unwrap_err();
        match err {
            PluginManifestError::Conflict { alias, plugins } => {
                assert_eq!(alias, "@semantic.MyType");
                assert_eq!(plugins, vec!["@plugin/alpha", "@plugin/beta"]);
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_carrier_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            r#"
[plugin]
name = "scalars-br"
namespace = "@plugin/scalars-br"

[[semantic_types]]
name = "WideThing"
alias = "@semantic.WideThing"
carrier_type = "Integer"
validator = "ValidateWide"
"#,
        );
        let mut plugins = BTreeMap::new();
        plugins.insert(
            "@plugin/scalars-br".to_owned(),
            Plugin::Local {
                path: tmp.path().display().to_string(),
            },
        );
        let manifest = make_manifest(plugins);
        let err = build_alias_map(Some(&manifest), tmp.path()).unwrap_err();
        assert!(matches!(
            err,
            PluginManifestError::UnsupportedCarrier { ref carrier_type, .. } if carrier_type == "Integer"
        ));
    }

    #[test]
    fn name_alias_mismatch_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            r#"
[plugin]
name = "scalars-br"
namespace = "@plugin/scalars-br"

[[semantic_types]]
name = "Foo"
alias = "@semantic.Bar"
carrier_type = "String"
validator = "ValidateThing"
"#,
        );
        let mut plugins = BTreeMap::new();
        plugins.insert(
            "@plugin/scalars-br".to_owned(),
            Plugin::Local {
                path: tmp.path().display().to_string(),
            },
        );
        let manifest = make_manifest(plugins);
        let err = build_alias_map(Some(&manifest), tmp.path()).unwrap_err();
        assert!(matches!(err, PluginManifestError::NameAliasMismatch { .. }));
    }

    #[test]
    fn namespace_mismatch_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        // Namespace mismatch is only enforced when the manifest
        // actually declares semantic types — otherwise the manifest
        // is a non-semantic contract (e.g. storage adapter) and is
        // tolerated by the loader.
        write_manifest(
            tmp.path(),
            r#"
[plugin]
name = "scalars-br"
namespace = "@plugin/something-else"

[[semantic_types]]
name = "MyType"
alias = "@semantic.MyType"
carrier_type = "String"
validator = "ValidateMine"
"#,
        );
        let mut plugins = BTreeMap::new();
        plugins.insert(
            "@plugin/scalars-br".to_owned(),
            Plugin::Local {
                path: tmp.path().display().to_string(),
            },
        );
        let manifest = make_manifest(plugins);
        let err = build_alias_map(Some(&manifest), tmp.path()).unwrap_err();
        assert!(matches!(err, PluginManifestError::NamespaceMismatch { .. }));
    }

    #[test]
    fn non_semantic_manifest_tolerated() {
        // A plugin whose `manifest.toml` follows a different sibling
        // contract (e.g. storage-only adapter using top-level keys)
        // must not break the alias-map build. The loader skips
        // cleanly; doctor's other plugin lints cover unrelated
        // breakage.
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            r#"
name = "@plugin/object-store"
version = "0.1.0"
implements = ["storage.ObjectStore"]
"#,
        );
        let mut plugins = BTreeMap::new();
        plugins.insert(
            "@plugin/object-store".to_owned(),
            Plugin::Local {
                path: tmp.path().display().to_string(),
            },
        );
        let manifest = make_manifest(plugins);
        let map = build_alias_map(Some(&manifest), tmp.path()).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn none_manifest_yields_empty_map() {
        let tmp = tempfile::tempdir().unwrap();
        let map = build_alias_map(None, tmp.path()).unwrap();
        assert!(map.is_empty());
    }
}
