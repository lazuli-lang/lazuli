//! Wave 6 — Per-layer coverage report.
//!
//! See `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 6 for the
//! design. Coverage is reported and gated **per layer**, never as a
//! single aggregated percentage that hides which paradigm is weak.
//! Aggregate is opt-in only with explicit method disclosure.
//!
//! Layer catalog (canonical):
//!
//! - [`spec_predicate`] — walks IR predicates; counts branches; counts
//!   coverage from `tests` blocks (pure-IR)
//! - [`spec_actor_matrix`] — walks `policy @policy.X` references vs
//!   `auth.roles` (pure-IR)
//! - [`spec_transition_state`] — walks transitions; computes
//!   `from <state>` coverage (pure-IR)
//! - [`view_extensibility`] — counts views with assertions present
//!   (pure-IR)
//! - [`view_e2e_pair`] — filesystem-checks
//!   `e2e/<feature>/<view>.spec.ts` (filesystem)
//! - [`handler_go`] — parses `go test -coverprofile` output if file
//!   present (external)
//!
//! Pure-IR layers compute coverage at parse-time with zero runtime
//! dependency, zero instrumentation, zero flakiness.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use lazuli_ir::Feature;

pub mod handler_go;
pub mod presets;
pub mod spec_actor_matrix;
pub mod spec_predicate;
pub mod spec_transition_state;
pub mod view_e2e_pair;
pub mod view_extensibility;

pub use presets::{preset_severity_overrides, preset_thresholds, CoveragePreset};

#[cfg(test)]
pub(crate) mod test_support;

/// Top-level coverage report shape. Serialized into `DoctorReport.coverage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub schema_version: u32,
    pub layers: BTreeMap<String, LayerCoverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<AggregateCoverage>,
    pub thresholds: Thresholds,
    pub gate_result: GateResult,
}

/// One layer's coverage state. The denominator/numerator naming is
/// deliberately neutral: spec-IR layers count branches/pairs; handler
/// layer counts lines; view-e2e counts views. Use the optional
/// `source` field to disclose the measurement method when
/// non-obvious.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerCoverage {
    pub covered: usize,
    pub total: usize,
    pub pct: f64,
    /// `"pass"` | `"warn"` | `"block"` — populated by `apply_thresholds`.
    pub verdict: String,
    /// Optional measurement-method disclosure
    /// (e.g. `"go-coverprofile"`, `"ir-walk"`, `"filesystem"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Optional raw-file pointer (e.g. `coverage.out` path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_file: Option<String>,
}

impl LayerCoverage {
    pub fn new(covered: usize, total: usize) -> Self {
        let pct = if total == 0 {
            100.0
        } else {
            (covered as f64 / total as f64) * 100.0
        };
        Self {
            covered,
            total,
            pct,
            verdict: "pending".to_string(),
            source: None,
            raw_file: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// Aggregate. Optional. Always carries `method` disclosure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateCoverage {
    pub pct: f64,
    pub method: String,
    pub disclosure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    pub applied_profile: String,
    pub by_layer: BTreeMap<String, LayerThreshold>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LayerThreshold {
    pub block_under: u32,
    pub warn_under: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub verdict: String,
    pub below_block: Vec<String>,
    pub below_warn: Vec<String>,
}

/// Threshold map keyed by layer name. Profile-default values supplied
/// by [`profile_default_thresholds`]; project-level overrides come
/// from `[doctor.coverage]` in `Lazurite.toml`.
#[derive(Debug, Clone, Default)]
pub struct CoverageThresholds {
    pub per_layer: BTreeMap<String, LayerThreshold>,
    pub aggregate_method: Option<String>,
}

impl CoverageThresholds {
    pub fn get(&self, layer: &str) -> Option<LayerThreshold> {
        self.per_layer.get(layer).copied()
    }

    /// Per-layer override merge: every entry in `overrides` replaces
    /// the same layer (if present) or extends the map (if new).
    /// Aggregate method is overridden only when `Some` on the
    /// override side, so passing an empty override preserves the
    /// base aggregate method.
    pub fn merge_overrides(mut self, overrides: CoverageThresholds) -> Self {
        for (layer, threshold) in overrides.per_layer {
            self.per_layer.insert(layer, threshold);
        }
        if overrides.aggregate_method.is_some() {
            self.aggregate_method = overrides.aggregate_method;
        }
        self
    }
}

/// Resolve the effective `CoverageThresholds` from a base profile, an
/// optional preset, and an optional per-layer override map.
/// Resolution precedence (highest wins):
///
///   1. per-layer `[doctor.coverage.<layer>]` override
///   2. `[doctor.coverage] preset = "<name>"`
///   3. profile-default thresholds (from `profile_default_thresholds`)
///
/// `per_layer_overrides` is the raw `BTreeMap` from
/// `CoverageSection::per_layer` (manifest sub-block form);
/// `aggregate_method` passes through verbatim.
pub fn resolve_coverage_thresholds(
    profile: CoverageProfile,
    preset: Option<CoveragePreset>,
    per_layer_overrides: BTreeMap<String, LayerThreshold>,
    aggregate_method: Option<String>,
) -> CoverageThresholds {
    let base = profile_default_thresholds(profile);
    let after_preset = match preset {
        Some(p) => base.merge_overrides(preset_thresholds(p)),
        None => base,
    };
    let overrides = CoverageThresholds {
        per_layer: per_layer_overrides,
        aggregate_method,
    };
    after_preset.merge_overrides(overrides)
}

/// Security profile mapping for default thresholds. Mirrors
/// `lazuli_lsp::SecurityProfile` without importing the LSP crate (the
/// doctor crate stays minimal-dependency).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageProfile {
    /// No gating. Coverage is reported; no layer ever fails CI.
    Prototype,
    /// Warn-only matrix per Wave 6.3.
    Strict,
    /// Block matrix per Wave 6.3.
    Production,
}

/// Profile-derived default thresholds. Used when no
/// `[doctor.coverage]` section is present in the manifest.
pub fn profile_default_thresholds(profile: CoverageProfile) -> CoverageThresholds {
    let layers: &[(&str, u32, u32)] = match profile {
        // Prototype: zeros mean "report only; never gate".
        CoverageProfile::Prototype => &[
            ("spec_predicate", 0, 0),
            ("spec_actor_matrix", 0, 0),
            ("spec_transition_state", 0, 0),
            ("view_extensibility", 0, 0),
            ("handler_go", 0, 0),
            ("view_e2e_pair", 0, 0),
        ],
        // Strict: warn-only — block_under = 0, warn_under > 0.
        CoverageProfile::Strict => &[
            ("spec_predicate", 0, 80),
            ("spec_actor_matrix", 0, 90),
            ("spec_transition_state", 0, 80),
            ("view_extensibility", 0, 80),
            ("handler_go", 0, 70),
            ("view_e2e_pair", 0, 60),
        ],
        // Production: matrix per Wave 6.3.
        CoverageProfile::Production => &[
            ("spec_predicate", 50, 80),
            ("spec_actor_matrix", 70, 90),
            ("spec_transition_state", 50, 80),
            ("view_extensibility", 50, 80),
            ("handler_go", 50, 70),
            ("view_e2e_pair", 30, 60),
        ],
    };
    let per_layer = layers
        .iter()
        .map(|(name, block, warn)| {
            (
                (*name).to_string(),
                LayerThreshold {
                    block_under: *block,
                    warn_under: *warn,
                },
            )
        })
        .collect();
    CoverageThresholds {
        per_layer,
        aggregate_method: None,
    }
}

/// Build the full coverage report from a list of features + optional
/// project root for filesystem-dependent layers.
///
/// All four pure-IR layers run unconditionally; `handler_go` and
/// `view_e2e_pair` degrade gracefully when external inputs are absent
/// (coverprofile file missing, no project root provided, etc.) — they
/// report `total = 0` and `pct = 100.0` (vacuous pass).
pub fn build_coverage_report(
    features: &[Feature],
    lzx_views: &[LzxViewRef],
    profile: CoverageProfile,
    thresholds: &CoverageThresholds,
    project_root: Option<&Path>,
) -> CoverageReport {
    build_coverage_report_with_e2e_root(
        features,
        lzx_views,
        profile,
        thresholds,
        project_root,
        None,
    )
}

/// Frente 2 — view_e2e_pair now honors a configurable Playwright
/// discovery root (`[testing.playwright].discovery_root` in
/// `Lazurite.toml`). Callers with manifest access pass it through
/// here; the bare `build_coverage_report` shim keeps backwards-compat.
pub fn build_coverage_report_with_e2e_root(
    features: &[Feature],
    lzx_views: &[LzxViewRef],
    profile: CoverageProfile,
    thresholds: &CoverageThresholds,
    project_root: Option<&Path>,
    e2e_discovery_root: Option<&Path>,
) -> CoverageReport {
    let mut layers: BTreeMap<String, LayerCoverage> = BTreeMap::new();
    layers.insert(
        "spec_predicate".to_string(),
        spec_predicate::compute(features),
    );
    layers.insert(
        "spec_actor_matrix".to_string(),
        spec_actor_matrix::compute(features),
    );
    layers.insert(
        "spec_transition_state".to_string(),
        spec_transition_state::compute(features),
    );
    layers.insert(
        "view_extensibility".to_string(),
        view_extensibility::compute(features),
    );
    layers.insert(
        "view_e2e_pair".to_string(),
        view_e2e_pair::compute(lzx_views, project_root, e2e_discovery_root),
    );
    layers.insert("handler_go".to_string(), handler_go::compute(project_root));
    apply_thresholds(&mut layers, thresholds);
    let gate_result = compute_gate(&layers);
    let applied_profile = match profile {
        CoverageProfile::Prototype => "prototype",
        CoverageProfile::Strict => "strict",
        CoverageProfile::Production => "production",
    }
    .to_string();
    CoverageReport {
        schema_version: 1,
        layers,
        aggregate: None,
        thresholds: Thresholds {
            applied_profile,
            by_layer: thresholds.per_layer.clone(),
        },
        gate_result,
    }
}

/// Filesystem-resolvable view reference. Produced by the doctor
/// package from `.lzx` parses; consumed by `view_e2e_pair` calculator.
#[derive(Debug, Clone)]
pub struct LzxViewRef {
    /// `experience` name (e.g. `customer.account` or `account`).
    /// Used as the `<feature>` segment in
    /// `e2e/<feature>/<view>.spec.ts` per Wave 3.5.2 path convention.
    pub experience: String,
    pub view: String,
}

/// Mutate each `LayerCoverage` to set its `verdict` based on the
/// per-layer threshold (or `"pass"` if no threshold registered).
pub fn apply_thresholds(
    layers: &mut BTreeMap<String, LayerCoverage>,
    thresholds: &CoverageThresholds,
) {
    for (layer_name, layer) in layers.iter_mut() {
        let verdict = match thresholds.get(layer_name) {
            Some(t) if layer.pct < t.block_under as f64 => "block",
            Some(t) if layer.pct < t.warn_under as f64 => "warn",
            _ => "pass",
        };
        layer.verdict = verdict.to_string();
    }
}

fn compute_gate(layers: &BTreeMap<String, LayerCoverage>) -> GateResult {
    let mut below_block = Vec::new();
    let mut below_warn = Vec::new();
    for (name, layer) in layers.iter() {
        match layer.verdict.as_str() {
            "block" => below_block.push(name.clone()),
            "warn" => below_warn.push(name.clone()),
            _ => {}
        }
    }
    let verdict = if !below_block.is_empty() {
        "block"
    } else if !below_warn.is_empty() {
        "warn"
    } else {
        "pass"
    }
    .to_string();
    GateResult {
        verdict,
        below_block,
        below_warn,
    }
}

#[cfg(test)]
mod tests {
    include!("mod_tests.rs");
}
