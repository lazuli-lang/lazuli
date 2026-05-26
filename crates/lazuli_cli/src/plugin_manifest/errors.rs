//! `PluginManifestError` enum and `Display`/`Error` impls.
//!
//! Carved out of `mod.rs` for the rails-style split. Re-exported by the
//! parent module so `crate::plugin_manifest::PluginManifestError` stays
//! the canonical path.

use std::path::PathBuf;

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
