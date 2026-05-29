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

/// `[testing.go]` sub-block — `go test` flag bag + coverage shape.
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingGo {
    /// Extra `go test` flags appended to the invocation.
    #[serde(default)]
    pub flags: Vec<String>,
    /// Whether `lazuli test --coverage` collects a Go coverprofile.
    #[serde(default)]
    pub coverage: bool,
    /// Path the Go coverprofile is written to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_out: Option<String>,
    /// Restrict the runner to packages matching this Go pattern (e.g.
    /// `./internal/...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_pattern: Option<String>,
}

/// `[testing.playwright]` sub-block — Playwright config and worker
/// knobs.
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingPlaywright {
    /// Path to `playwright.config.ts`. When `None` the resolver
    /// derives `<layout>/playwright.config.ts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// Override the worker count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workers: Option<u32>,
    /// Restrict to one Playwright project (e.g. `"chromium"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Directory the runner discovers `.spec.ts` from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_root: Option<String>,
    /// Extra Playwright CLI flags.
    #[serde(default)]
    pub flags: Vec<String>,
}

/// `[testing.ts]` sub-block — front-end test runner.
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

/// `[testing.spec]` sub-block — placeholders for spec-runner config
/// (today only the security `profile` override).
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingSpec {
    /// Override the security profile used by `lazuli check`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

/// Frente 1 — `[testing.ts] runner` defaults to `"vitest"` since
/// that's the canonical scaffold choice. Pilots opting into Jest
/// must set `runner = "jest"` explicitly.
pub(super) fn default_ts_runner() -> String {
    "vitest".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testing_default_is_empty() {
        let t = Testing::default();
        assert!(t.default_layers.is_none());
        assert!(t.go.is_none());
        assert!(t.playwright.is_none());
        assert!(t.ts.is_none());
        assert!(t.spec.is_none());
    }
}
