//! `[testing]` block schema — sibling T0-T5. Consumed by `lazuli test`.
//!
//! Five optional sub-blocks (`go`, `playwright`, `ts`, `spec`) plus a
//! shared `default_layers` field. Resolver helpers on `Manifest`
//! (`testing_ts_resolved`, `testing_playwright_resolved`,
//! `testing_default_layers`) apply layout-derived canonical defaults so
//! pilots on the canonical scaffold don't need to author every block.

use serde::{Deserialize, Serialize};

/// Sibling T0-T5 — `[testing]` block consumed by `lazuli test`.
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Testing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_layers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go: Option<TestingGo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playwright: Option<TestingPlaywright>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<TestingTs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<TestingSpec>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingGo {
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub coverage: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_out: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_pattern: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingPlaywright {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workers: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_root: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TestingTs {
    /// Frente 1 — defaults to `"vitest"` when omitted. Pilots that
    /// follow the canonical scaffold need only `[testing.ts]` to opt
    /// in without restating the runner choice. Use `runner = "jest"`
    /// to switch.
    #[serde(default = "default_ts_runner")]
    pub runner: String,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_root: Option<String>,
    #[serde(default)]
    pub coverage: bool,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

/// Frente 1 — `[testing.ts] runner` defaults to `"vitest"` since
/// that's the canonical scaffold choice. Pilots opting into Jest
/// must set `runner = "jest"` explicitly.
pub(super) fn default_ts_runner() -> String {
    "vitest".to_string()
}
