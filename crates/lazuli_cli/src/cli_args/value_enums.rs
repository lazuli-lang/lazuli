//! `ValueEnum` flags + `From` adapters that lower clap-side enums into
//! the analyzer/runtime-side types (`SecurityProfile`,
//! `cmd_test_types::Layer`, `cmd_design::*`).
//!
//! Lifted out of the `cli_args` god-file in the rails-style R9 split.

use clap::ValueEnum;
use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use crate::cmd_design;
use crate::cmd_test_types;

/// CLI ValueEnum mirror of `cmd_test_types::Layer` — kept distinct so
/// the test-runner types stay free of `clap` dependency.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum TestLayerFlag {
    Spec,
    View,
    Handler,
    Ts,
    E2e,
}

impl From<TestLayerFlag> for cmd_test_types::Layer {
    fn from(flag: TestLayerFlag) -> Self {
        match flag {
            TestLayerFlag::Spec => cmd_test_types::Layer::Spec,
            TestLayerFlag::View => cmd_test_types::Layer::View,
            TestLayerFlag::Handler => cmd_test_types::Layer::Handler,
            TestLayerFlag::Ts => cmd_test_types::Layer::Ts,
            TestLayerFlag::E2e => cmd_test_types::Layer::E2e,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum PlaywrightTarget {
    ApiPolicy,
    LifecycleGate,
    ScalarFixturesBarrel,
    All,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum GenerateKind {
    Openapi,
    Go,
    Feature,
    Handler,
    Ts,
    Playwright,
    // Wave 3 — scaffold authoring kinds. Each appends a new construct
    // to an existing feature `.lzi` (or `.lzx` for View) with a pre-
    // populated `tests` block + `@TODO authored:` markers so the
    // scaffold ships RED (per docs/proposals/tdd-bdd-first-2026-05-23.md
    // Wave 3 + TEST-STUB-001 sentinel).
    Command,
    View,
    Rule,
    Transition,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum DesignImportFormat {
    Figma,
    StyleDictionary,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum DesignExportTarget {
    Figma,
    StyleDictionary,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum InspectFormat {
    Json,
    Lazuli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum InspectInclude {
    Manifest,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CheckSecurityProfile {
    Prototype,
    Strict,
    Production,
}

impl From<CheckSecurityProfile> for SecurityProfile {
    fn from(profile: CheckSecurityProfile) -> Self {
        match profile {
            CheckSecurityProfile::Prototype => SecurityProfile::Prototype,
            CheckSecurityProfile::Strict => SecurityProfile::Strict,
            CheckSecurityProfile::Production => SecurityProfile::Production,
        }
    }
}

impl From<DesignImportFormat> for cmd_design::ImportFormat {
    fn from(format: DesignImportFormat) -> Self {
        match format {
            DesignImportFormat::Figma => cmd_design::ImportFormat::Figma,
            DesignImportFormat::StyleDictionary => cmd_design::ImportFormat::StyleDictionary,
        }
    }
}

impl From<DesignExportTarget> for cmd_design::ExportTarget {
    fn from(target: DesignExportTarget) -> Self {
        match target {
            DesignExportTarget::Figma => cmd_design::ExportTarget::Figma,
            DesignExportTarget::StyleDictionary => cmd_design::ExportTarget::StyleDictionary,
        }
    }
}
