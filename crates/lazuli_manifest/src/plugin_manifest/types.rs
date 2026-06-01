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

/// The four plugin kinds the compiler discriminates on (spec 0021).
///
/// FROZEN CONTRACT — specs 0022 (verify) and 0023 (scaffold) match on
/// this enum exhaustively. Adding a variant is a breaking change to
/// those specs.
///
/// `kind` is **inferred** from the sections present in a manifest
/// (see [`PluginManifest::resolved_kind`]); it is never a required
/// TOML key. The enum's `#[default]` is `Semantic` so a bare
/// identity-only manifest keeps the historical no-op behaviour (it
/// contributes no aliases on the 0019 semantic path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// Contributes `[[semantic_types]]` — the 0019 scalar/validator path
    /// (e.g. `scalars-br`).
    #[default]
    Semantic,
    /// Binds a Go interface + declares an env-var contract — the largest
    /// real category (mercadopago, smtp, object-store, ...). Modeled fully
    /// via `implements` / `[env]` / `[binds]`.
    Adapter,
    /// Reserved; thin stub in v1 (full schema deferred).
    Capability,
    /// Reserved; thin stub in v1 (full schema deferred).
    Design,
}

/// Top-level shape of a plugin's `manifest.toml`. Every field is
/// optional so manifests authored against a different sibling
/// contract (e.g. `@lazuli/plugin-object-store`'s storage-contract manifest
/// with top-level `name`/`version`/`implements` keys) deserialise
/// cleanly without forcing the semantic-types path. The B3 alias
/// builder skips manifests that lack `[plugin]` or `[[semantic_types]]`.
///
/// Spec 0021 makes this struct **kind-discriminated**: it carries an
/// optional explicit `kind` override plus the typed per-kind contract
/// sections (adapter `implements`/`[env]`/`[binds]`; thin
/// `[capability]`/`[design]` stubs). All new fields are
/// `#[serde(default)]` so every one of the 24 real adapter manifests
/// (none of which declare `kind`) keeps deserialising unchanged. The
/// `[[semantic_types]]` path is byte-for-byte untouched (0019 depends
/// on it). Use [`PluginManifest::resolved_kind`] to read the inferred
/// kind.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginManifest {
    #[serde(default)]
    pub plugin: Option<PluginIdentity>,
    #[serde(default)]
    pub semantic_types: Vec<PluginSemanticTypeDecl>,
    /// Explicit top-level `kind` override. When `Some`, it wins over
    /// inference (D1 step 1). Absent in all 24 real manifests today —
    /// they classify via [`PluginManifest::resolved_kind`]. NOTE: smtp's
    /// `[plugin].kind = "notifications/email-sender"` is a *free-form
    /// catalog string* on `PluginIdentity`, NOT this enum, and does not
    /// feed inference.
    #[serde(default)]
    pub kind: Option<PluginKind>,
    /// `implements = ["bucket.Interface", ...]` — the framework contract
    /// bucket(s) this adapter satisfies (e.g. `"payments.PaymentGateway"`,
    /// `"storage.ObjectStore"`). 0022 verifies each against a real Go
    /// interface. Empty for non-adapter manifests.
    #[serde(default)]
    pub implements: Vec<String>,
    /// `[env]` — the environment-variable contract. 0022/doctor surface
    /// this; the scaffolder (0023) seeds it.
    #[serde(default)]
    pub env: Option<PluginEnvContract>,
    /// `[binds]` — the Go interface this adapter binds against + its
    /// exported methods.
    #[serde(default)]
    pub binds: Option<PluginBindsContract>,
    /// `[capability]` — reserved. v1 models a thin marker only; the full
    /// capability contract is DEFERRED (see [`PluginCapabilityStub`]).
    #[serde(default)]
    pub capability: Option<PluginCapabilityStub>,
    /// `[design]` — reserved. v1 models a thin marker only; the full
    /// design-emitter contract is DEFERRED (see [`PluginDesignStub`]).
    #[serde(default)]
    pub design: Option<PluginDesignStub>,
}

impl PluginManifest {
    /// Resolve the plugin's kind via the FROZEN inference ladder
    /// (ADR D1). Precedence:
    ///
    /// 1. Explicit top-level `kind` override wins.
    /// 2. Else if `semantic_types` is non-empty → `Semantic`. *(Preserves
    ///    0019: any manifest the semantic resolver cares about keeps
    ///    classifying as semantic, even if it also carries adapter
    ///    sections.)*
    /// 3. Else if any adapter section is present (`implements` non-empty,
    ///    or `[env]`, or `[binds]`) → `Adapter`. *(Captures the real
    ///    adapters.)*
    /// 4. Else → `Semantic` (historical default for identity-only
    ///    manifests; harmless — contributes no aliases).
    ///
    /// `Capability`/`Design` are never *inferred* in v1 — they only arise
    /// from an explicit `kind`.
    pub fn resolved_kind(&self) -> PluginKind {
        if let Some(k) = self.kind {
            return k;
        }
        if !self.semantic_types.is_empty() {
            return PluginKind::Semantic;
        }
        if !self.implements.is_empty() || self.env.is_some() || self.binds.is_some() {
            return PluginKind::Adapter;
        }
        PluginKind::Semantic
    }
}

/// `[env]` block — the adapter's environment-variable contract.
/// `required`/`optional` are the two blessed keys seen across
/// mercadopago, object-store, google-maps, smtp, etc. `required_for_auth`
/// (smtp) is tolerated as an extra conditional-required bucket. Per-variable
/// detail sub-tables (`[env.SMTP_TLS_MODE]`) are intentionally NOT modeled
/// in v1 — they are catalog-doc metadata, not contract; serde drops them
/// harmlessly (they live under a different TOML path than the arrays).
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct PluginEnvContract {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    /// smtp-style "required only when auth is configured" bucket. Tolerated;
    /// 0022 may treat as a third (conditional) tier. Empty for most adapters.
    #[serde(default)]
    pub required_for_auth: Vec<String>,
}

/// `[binds]` block — the public Go surface this adapter exposes and the
/// interface it (will) bind to. Grounded in smtp's:
///   `interface = "github.com/.../lazuli-plugin-smtp.EmailSender"`
///   `methods   = ["SendEmail", "SendEmailBatch"]`
/// 0022 resolves `interface` to a real Go interface and checks `methods`
/// against it. `[contract].methods` / `[provides].go_interface` (the legacy
/// spellings on social-apple / social-google) are NOT auto-mapped here — the
/// blessed shape is `[binds]`; legacy plugins keep parsing (their keys land
/// in no modeled field and are dropped) and migrate to `[binds]` when 0023
/// lands.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct PluginBindsContract {
    /// Fully-qualified Go interface path (`<module>.<Interface>`). Optional
    /// so a partially-authored manifest still parses; 0022 treats absent as
    /// "unbound — verify-skip with a note".
    #[serde(default)]
    pub interface: Option<String>,
    /// Method names the adapter exports that 0022 checks against `interface`.
    #[serde(default)]
    pub methods: Vec<String>,
}

/// `[capability]` — reserved. v1 models identity only; the full capability
/// contract (provided behaviours, activation guards) is DEFERRED to a later
/// spec. Present here so `kind = "capability"` round-trips and matches
/// exhaustively.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct PluginCapabilityStub {
    /// Free-form capability name(s); typed shape DEFERRED. Captured so an
    /// early author isn't blocked and the section round-trips.
    #[serde(default)]
    pub provides: Vec<String>,
}

/// `[design]` — reserved. v1 models identity only; the full design-emitter
/// contract (token namespaces, emitted surfaces) is DEFERRED to a later spec.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct PluginDesignStub {
    /// Free-form emitted-surface name(s); typed shape DEFERRED.
    #[serde(default)]
    pub emits: Vec<String>,
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
    /// Free-form catalog `kind` string (e.g. smtp's
    /// `"notifications/email-sender"`). Tolerated + surfaced; it is
    /// DISTINCT from [`PluginKind`] and does NOT feed kind inference
    /// (D1 step 1 reads the *top-level* `kind`, not this one). Without
    /// this field serde already drops `[plugin].kind` silently; modeling
    /// it makes the catalog string explicit.
    #[serde(default)]
    pub kind: Option<String>,
    /// Legacy alias for `go_module` (the `module` spelling on
    /// smtp/captcha/waf/etc.). Prefer `go_module`; read both via
    /// [`PluginIdentity::effective_go_module`].
    #[serde(default)]
    pub module: Option<String>,
}

impl PluginIdentity {
    /// Effective Go module path, preferring the blessed `go_module`
    /// spelling then falling back to the legacy `module` alias
    /// (D4). Returns `None` when neither is set.
    pub fn effective_go_module(&self) -> Option<&str> {
        self.go_module.as_deref().or(self.module.as_deref())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_manifest_default_has_empty_collections() {
        let manifest = PluginManifest::default();
        assert!(manifest.plugin.is_none());
        assert!(manifest.semantic_types.is_empty());
    }
}
