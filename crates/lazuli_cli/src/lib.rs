pub mod inspect {
    pub mod features_summary;
}
// Shim-first (Wave D3a): the manifest trio now lives in the shared
// `lazuli_manifest` leaf crate so the LSP can run the doctor engine
// without depending on the CLI. Re-export under the original crate-root
// paths so every `crate::lazurite_manifest::X` / `crate::plugin_manifest::Z`
// call site keeps resolving unchanged.
pub use lazuli_manifest::{lazurite_manifest, plugin_manifest};
pub mod plugin_semantic_resolver;
pub mod version;
