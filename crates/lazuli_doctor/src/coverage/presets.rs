//! Coverage-preset enum + helpers. Promoted out of `coverage/mod.rs`
//! during the rails-style split (Frente R9). The preset surface is a
//! self-contained "test-coverage stance" concern, orthogonal to the
//! security profile.

use std::collections::BTreeMap;

use super::{CoverageThresholds, LayerThreshold};

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
