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
//!   ecosystem matures (the canonical pilot reality: every plugin is local).
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
//!
//! ## Sub-files (rails-style split)
//!
//! * [`types`] — `PluginManifest`, `PluginIdentity`,
//!   `PluginSemanticTypeDecl`, `ResolvedPluginSemantic`,
//!   `PLUGIN_MANIFEST_FILENAME`.
//! * [`errors`] — `PluginManifestError` + `Display`/`Error` impls.
//! * [`loader`] — `load_plugin_manifest`, `resolve_plugin_root`, plus
//!   the `default_error_code` + `absolutise` helpers.
//! * [`alias_map`] — `build_alias_map`, the orchestrator.

mod alias_map;
mod errors;
mod loader;
#[cfg(test)]
mod tests;
mod types;

pub use alias_map::build_alias_map;
pub use errors::PluginManifestError;
pub use loader::{load_plugin_manifest, resolve_plugin_root};
pub use types::{
    PluginIdentity, PluginManifest, PluginSemanticTypeDecl, ResolvedPluginSemantic,
    PLUGIN_MANIFEST_FILENAME,
};
