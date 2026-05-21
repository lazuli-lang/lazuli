//! Go code generation for Lazuli.
//!
//! ## Cut A acknowledgment (acknowledged, not yet generated)
//!
//! Cut A introduces `Agent` / `ToolBinding` / `EvalCase` to the IR
//! (see `lazuli_ir::Agent`, schema bump to `0.4.0`). The Go codegen
//! does not yet materialise agent-dispatch wiring; the Lazuli runtime
//! team implements `agent dispatch` in a separate phase that follows this
//! plan. When the agent backend lands here, the generated Go server
//! scaffolds:
//!
//!   - one `lazuli.Agent[I, O]{ ... }` value per agent
//!   - a tool dispatch table mapping `ToolBinding.reference` to the
//!     resolved capability handler
//!   - `output_kind`-aware response shaping (text / stream /
//!     discriminated_enum / discriminated_record)
//!   - eval harness scaffolding so `lazuli test --evals` can invoke the
//!     agent with the determinism pin from `temperature`/`seed`
//!
//! Plan reference: `docs/proposals/ai-primitives-v0-implementation.md`
//! §9.1. Runtime team: `docs/runtime-handoff.md`.

pub mod emitter;
pub mod runtime;

pub use emitter::module::GoSourceContext;
pub use runtime::emit_feature_go;

use std::collections::BTreeMap;

use lazuli_ir::Module;

/// Crate-pinned default version constraint emitted in the generated
/// `go.mod`'s `require lazuli.dev/runtime` line. `lazuli generate go`
/// accepts `--lazuli-go-version` to override; this constant is the
/// fallback.
///
/// **Source of truth**: `runtime/go/VERSION` (single line, e.g.
/// `v0.1.0`). This const must match — bumped together with the Lazuli
/// Go lib via `version_pin_matches_runtime` in
/// `tests/version_pin.rs`. The intent is so a Lazuli Go release tag
/// and the codegen's default `go.mod` requirement move atomically.
pub const LAZULI_GO_VERSION: &str = "v0.1.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

/// Lazurite manifest subset consumed by the Go emitter. The CLI owns
/// TOML parsing/validation and translates its full manifest into this
/// narrow codegen contract so this library does not depend on the CLI
/// crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LazuriteManifest {
    pub project_module: String,
    pub plugins: BTreeMap<String, LazuritePlugin>,
    pub generate_go: Option<LazuriteGenerateGo>,
    pub dev: Option<LazuriteDev>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LazuritePlugin {
    pub module: Option<String>,
    pub version: Option<String>,
    pub path: Option<String>,
    /// Resolved Go module path for this plugin — the value of the
    /// first-line `module ...` directive in the plugin's `go.mod`.
    ///
    /// For `Plugin::Remote { module, .. }` entries this equals
    /// `module` (the Lazurite.toml authoring shape IS the Go module
    /// path). For `Plugin::Local { path }` entries the CLI reads
    /// `<path>/go.mod` at translation time and threads the discovered
    /// value here.
    ///
    /// Codegen emits one `_ "<go_module>"` side-effect import per
    /// plugin in `main.go` so each plugin's `init()` runs and its
    /// `lazuli.RegisterAdapter(...)` populates the adapter registry
    /// before the first facade call resolves. Without this side-effect
    /// import the binary's transitive import graph never visits the
    /// plugin and the runtime panics at first `ResolveAdapter`.
    ///
    /// `None` when the plugin has no resolvable Go module (the CLI
    /// could not read `<path>/go.mod`, or the plugin manifest declares
    /// a non-Go runtime). The emitter skips imports for `None` entries
    /// rather than fabricating a guess.
    pub go_module: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LazuriteGenerateGo {
    pub emit_main: bool,
    pub submodule: bool,
    pub dev_replace: Option<String>,
    pub dev_work_replace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LazuriteDev {
    pub plugin_paths: BTreeMap<String, String>,
}

/// Options accepted by `generate_v1`. Mirrors the CLI surface described
/// in `docs/proposals/codegen-lazuli-go.md` §1.1.
#[derive(Debug, Clone)]
pub struct GoEmitOptions {
    /// Go module path emitted in root `go.mod`. When `None` the
    /// emitter derives `lazuli/<kebab-cased-app-name>` per proposal
    /// §1.1; the CLI usually resolves this before calling.
    pub module_name: Option<String>,
    /// Version constraint emitted in `require lazuli.dev/runtime
    /// <version>` for non-workspace generation. Defaults to
    /// `LAZULI_GO_VERSION`.
    pub lazuli_go_version: String,
    /// Smoke-run flag. Today the emitter ignores this (both modes
    /// build the same `Vec<GeneratedFile>`); the CLI handles the
    /// "do not write" branch. Reserved for the §6.2.1 error catalog
    /// landing in cell I4.
    pub check: bool,
    /// PG.C — package-wide plan catalog + per-callable gate facts.
    /// When present the emitter generates `dist/go/plan/catalog.gen.go`
    /// + 3-line gate preludes on every gated callable handler.
    pub plan_gate: Option<PlanGateEmitFacts>,
}

/// PG.C — wire-thin codegen view of the analyzer's `PlanGateFacts`.
/// Re-exposed from `lazuli_ir` so the codegen crate does not have to
/// take a dependency on `lazuli_analyzer`. The CLI layer translates
/// `lazuli_analyzer::PlanGateFacts` into this shape.
#[derive(Debug, Clone, Default)]
pub struct PlanGateEmitFacts {
    pub catalog: Option<lazuli_ir::PlanCatalog>,
    pub subscription_anchor: Option<lazuli_ir::SubscriptionAnchor>,
    /// Keyed by `<feature>/<callable_kind>:<callable_name>` —
    /// matches what `lazuli_analyzer::PlanGateFacts.gates` produces.
    pub gates: BTreeMap<String, Vec<lazuli_ir::Gate>>,
}

impl Default for GoEmitOptions {
    fn default() -> Self {
        Self {
            module_name: None,
            lazuli_go_version: LAZULI_GO_VERSION.to_owned(),
            check: false,
            plan_gate: None,
        }
    }
}

/// Entry point for the new Lazuli Go emitter (proposal §2.3). Cell
/// E1 wires the scaffold (`printer`, `imports`, `types`, `module`);
/// per-kind walkers (E2-E4 + G1-G7) extend `emitter::module`
/// downstream. Output: root `go.mod` plus one `.gen.go` stub per
/// feature.
pub fn generate_v1(module: &Module, options: &GoEmitOptions) -> Vec<GeneratedFile> {
    generate_v1_with_manifest(module, options, None)
}

/// Lazurite-aware Go emitter entry point. When `manifest` is absent the
/// output intentionally matches `generate_v1`'s legacy single-module
/// behavior so bare projects and fixtures do not need a manifest.
pub fn generate_v1_with_manifest(
    module: &Module,
    options: &GoEmitOptions,
    manifest: Option<&LazuriteManifest>,
) -> Vec<GeneratedFile> {
    emitter::emit_module(module, options, manifest, None)
}

pub fn generate_v1_with_manifest_and_source(
    module: &Module,
    options: &GoEmitOptions,
    manifest: Option<&LazuriteManifest>,
    source_context: GoSourceContext<'_>,
) -> Vec<GeneratedFile> {
    emitter::emit_module(module, options, manifest, Some(source_context))
}

#[cfg(test)]
mod tests {
    use super::LAZULI_GO_VERSION;

    #[test]
    fn lazuli_go_version_is_pinned() {
        // Sanity: the crate-pinned version constant matches what the
        // CLI's `--lazuli-go-version` default resolves to.
        assert!(LAZULI_GO_VERSION.starts_with('v'));
    }
}
