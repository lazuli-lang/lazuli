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
pub mod spec_actor_matrix;
pub mod spec_predicate;
pub mod spec_transition_state;
pub mod view_e2e_pair;
pub mod view_extensibility;

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

/// `[doctor.coverage] preset = "<name>"` — opinionated layer-threshold
/// preset that pilots can opt into without authoring all six per-layer
/// sub-blocks by hand. Orthogonal to `[doctor] profile` (security
/// profile); presets target test-coverage stance specifically.
///
/// Frente 1 / 2026-05-24 — see
/// `docs/canonical-semantics.md#coverage-presets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoveragePreset {
    /// Block `handler_go` strictly (90/95); warn-only on the spec
    /// and view layers. Mirrors the historical hostpoint tuning and
    /// the scaffold-default expectation: handler tests are the
    /// non-negotiable TDD pair, the other layers are aspirational
    /// while specs/view-e2e ramp up.
    TddStrict,
    /// Block every layer at 70/85 — for pilots with full spec /
    /// actor-matrix / transition / view-extensibility / e2e
    /// authorship. The "mature TDD shop" stance.
    TddMature,
    /// Block every layer at 90/95 — the no-mercy bar. For shipping
    /// production code where a missing spec or unpaired view IS a
    /// release-blocker. Same threshold `tdd-strict` applies to
    /// handler_go, applied universally. Pilots that select this
    /// preset are declaring "every IR construct ships with paired
    /// tests; if doctor says we're below 90%, we don't ship".
    TddIronHand,
    /// All zeros across the board; report only, never gate. Useful
    /// for prototypes that still want the coverage report rendered
    /// but don't want any layer to fail CI.
    Off,
}

impl CoveragePreset {
    /// Parse a string preset name from `Lazurite.toml`. Returns
    /// `None` for any unknown name; callers should surface that as a
    /// config error so unknown presets don't silently degrade into
    /// vacuous-pass behavior.
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim() {
            "tdd-strict" => Some(Self::TddStrict),
            "tdd-mature" => Some(Self::TddMature),
            "tdd-iron-hand" => Some(Self::TddIronHand),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TddStrict => "tdd-strict",
            Self::TddMature => "tdd-mature",
            Self::TddIronHand => "tdd-iron-hand",
            Self::Off => "off",
        }
    }
}

/// Iron-hand meta-bundle — opinionated rule-severity escalation map
/// that ships alongside [`preset_thresholds`]. Returns the set of rule
/// codes that the preset wants to escalate from their category-default
/// severity to a stricter one.
///
/// The `tdd-iron-hand` preset is the canonical "production ship-bar"
/// stance: not only does it block all six coverage layers at 90/95
/// (numerical TDD), it also forces every feature to carry a context
/// header (`purpose`, `non_goals`, `attach_ctx`). The three
/// `VOCAB-CONTEXT-*` rules normally surface as `warning`; under iron
/// hand they become `error` so CI gates on missing structural
/// documentation.
///
/// Other presets return an empty map — their coverage thresholds
/// already express the strictness, no rule-severity escalation is
/// added.
///
/// Manifest-authored overrides
/// (`[doctor.test_discipline.severity_override]`) win over this map,
/// matching the existing precedence in `doctor_severity_for`: a pilot
/// that wants to opt the lint back down to `warning` (e.g. mid-flight
/// migration) writes:
///
/// ```toml
/// [doctor.test_discipline.severity_override."VOCAB-CONTEXT-CTXMD-001"]
/// severity = "warning"
/// reason = "ctx.md backfill scheduled for sprint 24"
/// ```
pub fn preset_severity_overrides(preset: CoveragePreset) -> BTreeMap<String, &'static str> {
    let mut out = BTreeMap::new();
    if matches!(preset, CoveragePreset::TddIronHand) {
        out.insert("VOCAB-CONTEXT-PURPOSE-001".to_string(), "error");
        out.insert("VOCAB-CONTEXT-NONGOALS-001".to_string(), "error");
        out.insert("VOCAB-CONTEXT-CTXMD-001".to_string(), "error");
    }
    out
}

/// Preset-derived thresholds. Independent of `CoverageProfile` —
/// the two compose via [`resolve_coverage_thresholds`].
pub fn preset_thresholds(preset: CoveragePreset) -> CoverageThresholds {
    let layers: &[(&str, u32, u32)] = match preset {
        CoveragePreset::TddStrict => &[
            ("handler_go", 90, 95),
            ("spec_predicate", 0, 90),
            ("spec_actor_matrix", 0, 50),
            ("spec_transition_state", 0, 50),
            ("view_e2e_pair", 0, 50),
            ("view_extensibility", 0, 90),
        ],
        CoveragePreset::TddMature => &[
            ("handler_go", 70, 85),
            ("spec_predicate", 70, 85),
            ("spec_actor_matrix", 70, 85),
            ("spec_transition_state", 70, 85),
            ("view_e2e_pair", 70, 85),
            ("view_extensibility", 70, 85),
        ],
        CoveragePreset::TddIronHand => &[
            ("handler_go", 90, 95),
            ("spec_predicate", 90, 95),
            ("spec_actor_matrix", 90, 95),
            ("spec_transition_state", 90, 95),
            ("view_e2e_pair", 90, 95),
            ("view_extensibility", 90, 95),
        ],
        CoveragePreset::Off => &[
            ("handler_go", 0, 0),
            ("spec_predicate", 0, 0),
            ("spec_actor_matrix", 0, 0),
            ("spec_transition_state", 0, 0),
            ("view_e2e_pair", 0, 0),
            ("view_extensibility", 0, 0),
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
    use super::*;

    #[test]
    fn empty_features_produces_vacuous_pass() {
        let features: Vec<Feature> = Vec::new();
        let lzx: Vec<LzxViewRef> = Vec::new();
        let thresholds = profile_default_thresholds(CoverageProfile::Strict);
        let report =
            build_coverage_report(&features, &lzx, CoverageProfile::Strict, &thresholds, None);
        assert_eq!(report.schema_version, 1);
        assert_eq!(report.gate_result.verdict, "pass");
        assert!(report.gate_result.below_block.is_empty());
        // Every layer present even with no features.
        for layer_name in [
            "spec_predicate",
            "spec_actor_matrix",
            "spec_transition_state",
            "view_extensibility",
            "view_e2e_pair",
            "handler_go",
        ] {
            assert!(
                report.layers.contains_key(layer_name),
                "missing layer {layer_name}"
            );
        }
    }

    #[test]
    fn profile_default_thresholds_match_proposal_matrix() {
        let strict = profile_default_thresholds(CoverageProfile::Strict);
        // Strict: warn-only — block_under = 0.
        assert_eq!(strict.get("spec_predicate").unwrap().block_under, 0);
        assert_eq!(strict.get("spec_predicate").unwrap().warn_under, 80);
        let prod = profile_default_thresholds(CoverageProfile::Production);
        // Production blocks per Wave 6.3 matrix.
        assert_eq!(prod.get("spec_predicate").unwrap().block_under, 50);
        assert_eq!(prod.get("spec_actor_matrix").unwrap().block_under, 70);
    }

    #[test]
    fn prototype_never_gates() {
        let proto = profile_default_thresholds(CoverageProfile::Prototype);
        for (_, t) in proto.per_layer.iter() {
            assert_eq!(t.block_under, 0);
            assert_eq!(t.warn_under, 0);
        }
    }

    // ---------- Frente 1 — coverage preset resolution ----------

    #[test]
    fn preset_parse_recognizes_canonical_names() {
        assert_eq!(
            CoveragePreset::parse("tdd-strict"),
            Some(CoveragePreset::TddStrict)
        );
        assert_eq!(
            CoveragePreset::parse("tdd-mature"),
            Some(CoveragePreset::TddMature)
        );
        assert_eq!(
            CoveragePreset::parse("tdd-iron-hand"),
            Some(CoveragePreset::TddIronHand)
        );
        assert_eq!(CoveragePreset::parse("off"), Some(CoveragePreset::Off));
        // Surrounding whitespace tolerated.
        assert_eq!(
            CoveragePreset::parse("  tdd-strict  "),
            Some(CoveragePreset::TddStrict)
        );
    }

    #[test]
    fn preset_tdd_iron_hand_blocks_every_layer_at_ninety() {
        let t = preset_thresholds(CoveragePreset::TddIronHand);
        assert_eq!(t.per_layer.len(), 6, "must cover the same 6 layers");
        for (name, lt) in t.per_layer.iter() {
            assert_eq!(lt.block_under, 90, "{name} should block at 90");
            assert_eq!(lt.warn_under, 95, "{name} should warn at 95");
        }
        // Spot-check every expected layer is present.
        for layer in [
            "handler_go",
            "spec_predicate",
            "spec_actor_matrix",
            "spec_transition_state",
            "view_e2e_pair",
            "view_extensibility",
        ] {
            assert!(t.get(layer).is_some(), "{layer} missing from iron-hand");
        }
    }

    #[test]
    fn preset_parse_rejects_unknown_names() {
        assert_eq!(CoveragePreset::parse("tdd-loose"), None);
        assert_eq!(CoveragePreset::parse(""), None);
        assert_eq!(CoveragePreset::parse("strict"), None); // profile name leaked in
    }

    #[test]
    fn preset_tdd_strict_blocks_only_handler_go() {
        let t = preset_thresholds(CoveragePreset::TddStrict);
        let handler = t.get("handler_go").expect("handler_go entry");
        assert_eq!(handler.block_under, 90);
        assert_eq!(handler.warn_under, 95);
        // Every other layer warn-only.
        for layer in [
            "spec_predicate",
            "spec_actor_matrix",
            "spec_transition_state",
            "view_e2e_pair",
            "view_extensibility",
        ] {
            let lt = t.get(layer).expect(layer);
            assert_eq!(lt.block_under, 0, "{layer} should warn-only");
            assert!(lt.warn_under > 0, "{layer} should warn at >0");
        }
    }

    #[test]
    fn preset_tdd_mature_blocks_every_layer() {
        let t = preset_thresholds(CoveragePreset::TddMature);
        for (_, lt) in t.per_layer.iter() {
            assert_eq!(lt.block_under, 70);
            assert_eq!(lt.warn_under, 85);
        }
    }

    #[test]
    fn preset_off_never_gates() {
        let t = preset_thresholds(CoveragePreset::Off);
        for (_, lt) in t.per_layer.iter() {
            assert_eq!(lt.block_under, 0);
            assert_eq!(lt.warn_under, 0);
        }
    }

    /// Resolution precedence: preset overrides profile defaults.
    #[test]
    fn resolve_preset_overrides_profile_defaults() {
        let thresholds = resolve_coverage_thresholds(
            CoverageProfile::Strict,
            Some(CoveragePreset::TddStrict),
            BTreeMap::new(),
            None,
        );
        let handler = thresholds.get("handler_go").unwrap();
        // Strict profile would have left handler_go at (0, 70); preset lifts it to (90, 95).
        assert_eq!(handler.block_under, 90);
        assert_eq!(handler.warn_under, 95);
    }

    /// Resolution precedence: per-layer override wins over preset.
    #[test]
    fn resolve_per_layer_override_wins_over_preset() {
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "handler_go".to_string(),
            LayerThreshold {
                block_under: 30,
                warn_under: 40,
            },
        );
        let thresholds = resolve_coverage_thresholds(
            CoverageProfile::Strict,
            Some(CoveragePreset::TddStrict),
            overrides,
            None,
        );
        let handler = thresholds.get("handler_go").unwrap();
        assert_eq!(handler.block_under, 30);
        assert_eq!(handler.warn_under, 40);
        // Untouched layers still carry preset values.
        let spec_pred = thresholds.get("spec_predicate").unwrap();
        assert_eq!(spec_pred.warn_under, 90); // tdd-strict spec_predicate
    }

    /// When no preset is supplied, profile defaults still apply (the
    /// backwards-compat path).
    #[test]
    fn resolve_no_preset_falls_back_to_profile() {
        let thresholds =
            resolve_coverage_thresholds(CoverageProfile::Strict, None, BTreeMap::new(), None);
        // Strict profile: handler_go warn_under = 70, block_under = 0.
        let handler = thresholds.get("handler_go").unwrap();
        assert_eq!(handler.block_under, 0);
        assert_eq!(handler.warn_under, 70);
    }

    /// Preset + profile interaction: `off` clears every gate even if
    /// the profile is Production (which would otherwise block).
    #[test]
    fn resolve_off_preset_neutralizes_production_profile() {
        let thresholds = resolve_coverage_thresholds(
            CoverageProfile::Production,
            Some(CoveragePreset::Off),
            BTreeMap::new(),
            None,
        );
        for (_, lt) in thresholds.per_layer.iter() {
            assert_eq!(lt.block_under, 0);
            assert_eq!(lt.warn_under, 0);
        }
    }

    /// Aggregate method passes through verbatim from the override
    /// side; preset itself never sets it.
    #[test]
    fn resolve_passes_aggregate_method_through_override() {
        let thresholds = resolve_coverage_thresholds(
            CoverageProfile::Strict,
            Some(CoveragePreset::TddStrict),
            BTreeMap::new(),
            Some("all_pass".to_string()),
        );
        assert_eq!(thresholds.aggregate_method.as_deref(), Some("all_pass"));
    }

    // ── iron-hand meta-bundle severity overrides ─────────────────────────────

    #[test]
    fn iron_hand_escalates_three_vocab_context_rules_to_error() {
        let overrides = preset_severity_overrides(CoveragePreset::TddIronHand);
        assert_eq!(overrides.len(), 3);
        assert_eq!(overrides.get("VOCAB-CONTEXT-PURPOSE-001"), Some(&"error"));
        assert_eq!(overrides.get("VOCAB-CONTEXT-NONGOALS-001"), Some(&"error"));
        assert_eq!(overrides.get("VOCAB-CONTEXT-CTXMD-001"), Some(&"error"));
    }

    #[test]
    fn other_presets_emit_no_severity_escalation() {
        for preset in [
            CoveragePreset::TddStrict,
            CoveragePreset::TddMature,
            CoveragePreset::Off,
        ] {
            assert!(
                preset_severity_overrides(preset).is_empty(),
                "preset {:?} must not escalate any rule severities — only iron-hand bundles \
                 the structural documentation gate",
                preset
            );
        }
    }
}
