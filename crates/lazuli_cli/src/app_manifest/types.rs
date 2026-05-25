//! Side-channel parser output types shared across `app_manifest` sub-modules.
//!
//! These types carry doctor-visible defect signal that cannot ride in the
//! shipped IR. The IR (`lazuli_ir::nodes::app_manifest::*`) models the
//! happy path — `RegistryToolEntry::effect` is a required field, so an
//! `effect`-less tool block has no place in the IR. Doctor still needs to
//! diagnose the source, so the parser captures the offending entries in a
//! side-channel and threads them out via [`RegistryParseOutput`].
//!
//! Today only `parse_app_registry_with_defects` produces a defect channel;
//! the other entry points (`parse_app_manifest`, `parse_app_workspace`,
//! `parse_app_contracts`, `parse_app_profiles`) report directly through the
//! IR (or absence of it). Add a new struct here when a new parser needs to
//! return both an IR and a defect list.
//!
//! See: `lazuli_ir::nodes::app_manifest::AppRegistry`,
//!      `lazuli_doctor::diagnostics::tool_registry_effect_required_diagnostics`.

use lazuli_ir::AppRegistry;

/// Side-channel captured during registry parsing for entries that exist
/// syntactically but lack `effect`. The IR's `RegistryToolEntry` carries
/// `effect` as a required field, so we cannot encode a missing-effect
/// entry there. Doctor consumes this list to emit
/// `tool_registry_effect_required_diagnostics`.
#[derive(Debug, Clone)]
pub struct RegistryToolEntryDefect {
    /// 1-based line where the offending `tool <name>` header appears.
    pub line: usize,
    pub name: String,
    pub reason: RegistryToolDefectReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryToolDefectReason {
    EffectMissing,
    EffectInvalid,
}

/// Output of `parse_app_registry`. Splits the well-formed registry IR
/// from the defect list so doctor can surface both.
#[derive(Debug, Clone, Default)]
pub struct RegistryParseOutput {
    pub registry: Option<AppRegistry>,
    pub tool_defects: Vec<RegistryToolEntryDefect>,
}
