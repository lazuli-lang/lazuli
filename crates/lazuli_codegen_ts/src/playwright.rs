//! Playwright e2e artifact emitters.
//!
//! The concrete emitters land in the CODEGEN-* sibling cells. CLI-1 keeps
//! this public module in place so `lazuli generate playwright` can compile
//! independently while those cells fill in the bodies.

use std::path::PathBuf;

use lazuli_ir::Module;

use crate::GeneratedFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaywrightEmitOpts {
    pub command_namespace: String,
    pub api_url_env_var: String,
    pub api_url_fallback: String,
    pub out_dir: PathBuf,
    pub helpers_package: String,
}

// stub: filled by CODEGEN-* sibling cell
pub fn emit_peer_dep_check(_opts: &PlaywrightEmitOpts) -> GeneratedFile {
    todo!("CODEGEN-* fills the Playwright peer-dependency check emitter")
}

// stub: filled by CODEGEN-* sibling cell
pub fn emit_api_policy_spec(_ir: &Module, _opts: &PlaywrightEmitOpts) -> Vec<GeneratedFile> {
    todo!("CODEGEN-1 fills the Playwright api-policy emitter")
}

// stub: filled by CODEGEN-* sibling cell
pub fn emit_lifecycle_gate_spec(_ir: &Module, _opts: &PlaywrightEmitOpts) -> Vec<GeneratedFile> {
    todo!("CODEGEN-2a fills the Playwright lifecycle-gate emitter")
}

// stub: filled by CODEGEN-* sibling cell
pub fn emit_scalar_fixtures_barrel_file(_opts: &PlaywrightEmitOpts) -> GeneratedFile {
    todo!("CODEGEN-2b fills the Playwright scalar-fixtures-barrel emitter")
}
