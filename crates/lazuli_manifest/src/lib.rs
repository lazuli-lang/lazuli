//! Shared manifest parsing for the Lazuli toolchain.
//!
//! Extracted from `lazuli_cli` (Wave D3a) so the LSP can run the doctor
//! engine without depending on the CLI. The three modules move verbatim:
//!
//! - `lazurite_manifest` — `Lazurite.toml` schema + resolver helpers.
//! - `app_manifest` — `app.lzi` manifest / contracts / registry parsers.
//! - `plugin_manifest` — plugin `manifest.toml` loader + alias map.
//!
//! `lazuli_cli` re-exports these three modules under their original
//! `crate::` paths (shim-first), so every existing call site keeps
//! compiling unchanged.

pub mod app_manifest;
pub mod lazurite_manifest;
pub mod plugin_manifest;
