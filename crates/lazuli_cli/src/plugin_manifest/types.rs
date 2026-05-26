//! Surface types for plugin `manifest.toml` parsing.
//!
//! Lives next to `mod.rs` (the loader + alias builder orchestrator)
//! and `errors.rs` (the `PluginManifestError` enum). All structs here
//! are re-exported through the parent module so existing
//! `crate::plugin_manifest::PluginManifest` style imports keep working.

use serde::Deserialize;

/// Filename for the per-plugin manifest. Lives next to the plugin's
/// Go `adapter.go` per `docs/plugin-authoring.md:45`.
pub const PLUGIN_MANIFEST_FILENAME: &str = "manifest.toml";

/// Top-level shape of a plugin's `manifest.toml`. Every field is
/// optional so manifests authored against a different sibling
/// contract (e.g. `@lazuli/plugin-object-store`'s storage-contract manifest
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
///
/// `go_module` + `ts_package` (W2): the plugin's package paths in each
/// host runtime. When omitted, codegen falls back to the v1 convention
/// (`lazuli.dev/plugin/<short>` for Go, `@lazuli/plugin-<short>` for TS).
#[derive(Debug, Clone, Deserialize)]
pub struct PluginIdentity {
    pub name: String,
    pub namespace: String,
    #[serde(default)]
    pub go_module: Option<String>,
    #[serde(default)]
    pub ts_package: Option<String>,
    /// One-line description surfaced in the plugin catalog (LSP
    /// hover, docs site, `lazuli plugins` CLI). When absent the
    /// catalog falls back to the README's first paragraph.
    #[serde(default)]
    pub description: Option<String>,
}

/// One `[[semantic_types]]` entry. `alias` is the source-form authoring
/// shape (`@semantic.BrazilianCPF`). `name` must equal the alias terminal
/// segment. `carrier_type` is the closed-catalog Lazuli builtin name
/// (v1: `"String"` only — wider carriers gated by a separate proposal).
/// `validator` is the exported Go function on the plugin adapter; it
/// pairs with `name` to form the runtime validate-tag key. `formatter`
/// is optional and unused by storage validity.
///
/// W2 (ir-semantic-auto-validate-2026-05-22): adds `error_code`,
/// `message_key`, `ts_validator`. All optional — codegen derives
/// conventions when absent.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginSemanticTypeDecl {
    pub name: String,
    pub alias: String,
    pub carrier_type: String,
    pub validator: String,
    #[serde(default)]
    pub formatter: Option<String>,
    /// Stable error code surfaced to clients when validation fails.
    /// Defaults to the alias terminal lower-cased with `_invalid` suffix
    /// (e.g. `BrazilianCPF` → `cpf_invalid`).
    #[serde(default)]
    pub error_code: Option<String>,
    /// Optional i18n key the runtime resolver consults for a localized
    /// error message. Empty → no key resolution (client formats from
    /// `error_code`).
    #[serde(default)]
    pub message_key: Option<String>,
    /// Exported TS/JS function on the plugin's npm package
    /// (e.g. `validateCPF`). When present, TS codegen emits a
    /// client-side preflight so bad input fails locally before the
    /// round-trip. Empty → preflight skipped; server is sole enforcer.
    #[serde(default)]
    pub ts_validator: Option<String>,
}

/// Resolved alias entry — everything the analyzer, doctor, codegen,
/// and inspect need to project a `@semantic.<Name>` reference into a
/// `BuiltinType::SemanticPluginType` variant + carry the validator
/// metadata through to tag emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPluginSemantic {
    /// Plugin namespace as written in `Lazurite.toml [plugins]`
    /// (e.g. `@lazuli/plugin-scalars-br`). The IR variant carries this string
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
    /// Effective Go module path of the plugin
    /// (e.g. `lazuli.dev/plugin/scalars-br`). Plugin-level value or
    /// `lazuli.dev/plugin/<short>` convention fallback.
    pub go_module: String,
    /// Effective TS/npm package of the plugin (e.g. `@lazuli/plugin-scalars-br`).
    /// Plugin-level value or `@lazuli/plugin-<short>` convention fallback.
    pub ts_package: String,
    /// Effective error code surfaced to clients (e.g. `cpf_invalid`).
    /// Scalar-level value or convention fallback (`<terminal stem>_invalid`).
    pub error_code: String,
    /// Effective i18n message key. Scalar-level value or empty.
    pub message_key: String,
    /// Effective TS validator function name (e.g. `validateCPF`).
    /// Scalar-level value or empty (skip preflight emission).
    pub ts_validator: String,
}

