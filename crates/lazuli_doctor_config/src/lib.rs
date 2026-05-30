//! `lazuli_doctor_config` — the single source of truth for doctor
//! profile / preset resolution and per-finding severity escalation.
//!
//! ## Why this crate exists
//!
//! Before this crate, the doctor's severity decision was scattered
//! across four sites in `lazuli_cli` (the category-aware
//! `doctor_severity_for`, the four `resolve_<cat>_severity` preset
//! helpers, and the `context_vocab_diagnostics` manifest-override
//! closure) plus the coverage `preset_severity_overrides` map in
//! `lazuli_doctor`. Each could drift from the others. Worse, the 3-value
//! profile enum (`SecurityProfile`) lived UP in `lazuli_lsp`, and the
//! CLI doctor reached up into the LSP to borrow it — an inverted
//! dependency.
//!
//! This crate lifts the resolution into ONE leaf crate that both the
//! CLI and (in a later wave) the LSP depend on. It owns:
//!
//! - [`DoctorProfile`] — the 3-value profile enum, relocated here from
//!   `lazuli_lsp::types::SecurityProfile`. `lazuli_lsp` re-exports it as
//!   `SecurityProfile` for ABI stability.
//! - [`ResolvedDoctorConfig`] — a fully-resolved, IO-free snapshot of
//!   the `[doctor]` / `[doctor.coverage]` / `[doctor.<cat>]` blocks plus
//!   the active profile.
//! - [`effective_severity`] — THE single pure severity resolver. Its
//!   answer is the exact union of the four CLI resolvers + the coverage
//!   escalation map, with precedence
//!   `manifest override > coverage preset > category preset > profile
//!   default`.
//!
//! ## Boundary
//!
//! The crate is `tower-lsp`-free and CLI-free: it depends only on
//! `lazuli_doctor` (for [`DoctorSeverity`], [`RuleCategory`], and the
//! per-category preset enums), `serde`, and `toml`. The
//! `DoctorSeverity` → editor `DiagnosticSeverity` mapping stays in
//! `lazuli_lsp`; the `--fail-on` / `--format` output gates stay in the
//! CLI. This keeps the crate a true leaf над `lazuli_doctor`.

use std::collections::BTreeMap;

use lazuli_doctor::coverage::{CoveragePreset, preset_severity_overrides};
use lazuli_doctor::error_handling::preset::{
    ErrorHandlingPreset, preset_rule_severity as error_handling_preset_rule_severity,
};
use lazuli_doctor::internal_hygiene::preset::{
    InternalHygienePreset, preset_rule_severity as internal_hygiene_preset_rule_severity,
};
use lazuli_doctor::lzi_hygiene::preset::{
    LziHygienePreset, preset_rule_severity as lzi_hygiene_preset_rule_severity,
};
use lazuli_doctor::test_discipline::preset::{
    TestDisciplinePreset, preset_rule_severity as test_discipline_preset_rule_severity,
};
pub use lazuli_doctor::{DoctorSeverity, RuleCategory};

mod manifest;

pub use manifest::{
    CoverageSection, Doctor, ErrorHandlingDoctor, InternalHygieneDoctor, LayerThresholdConfig,
    LziHygieneDoctor, SeverityOverride, TestDisciplineDoctor,
};

include!("lib_p1.rs");
include!("lib_p2.rs");
include!("lib_p3.rs");
