//! Shared fixtures for the cmd_design tests.
//!
//! The `demo_fixture` covers every IR group (colors with multi-state +
//! dark variants, typography sub-tokens, motion durations + easings,
//! breakpoints, z_indices) so codec round-trip tests assert structural
//! equality against a single source of truth.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion, ScaleToken,
    ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
};

/// Test-only access to the dark-mode extension key used by both codec
/// dialects. Mirrors the const in `figma.rs` so the test surface keeps
/// working through the split.
pub(super) const EXT_LAZULI_DARK: &str = "com.lazuli.dark";

pub(super) fn unique_temp_dir(label: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "lazuli-design-{label}-{}-{suffix}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// example-style fixture covering every group + dark variants + a
/// multi-state color sub-block.
pub(super) fn demo_fixture() -> Design {
    Design {
        name: "example".to_string(),
        extends: None,
        colors: vec![
            ColorToken {
                name: "primary".to_string(),
                states: vec![
                    ColorState {
                        kind: ColorStateKind::Base,
                        value: "#7c3aed".to_string(),
                        dark: None,
                    },
                    ColorState {
                        kind: ColorStateKind::Hover,
                        value: "#6d28d9".to_string(),
                        dark: None,
                    },
                    ColorState {
                        kind: ColorStateKind::Active,
                        value: "#5b21b6".to_string(),
                        dark: None,
                    },
                    ColorState {
                        kind: ColorStateKind::Foreground,
                        value: "#ffffff".to_string(),
                        dark: None,
                    },
                ],
            },
            ColorToken {
                name: "background".to_string(),
                states: vec![
                    ColorState {
                        kind: ColorStateKind::Base,
                        value: "#ffffff".to_string(),
                        dark: Some("#09090b".to_string()),
                    },
                    ColorState {
                        kind: ColorStateKind::Foreground,
                        value: "#09090b".to_string(),
                        dark: Some("#fafafa".to_string()),
                    },
                ],
            },
            ColorToken {
                name: "success".to_string(),
                states: vec![ColorState {
                    kind: ColorStateKind::Base,
                    value: "#16a34a".to_string(),
                    dark: None,
                }],
            },
        ],
        typography: Typography {
            families: vec![
                FamilyToken {
                    name: "sans".to_string(),
                    value: "Inter, system-ui, sans-serif".to_string(),
                },
                FamilyToken {
                    name: "mono".to_string(),
                    value: "JetBrains Mono, monospace".to_string(),
                },
            ],
            scale: vec![
                TextScaleToken {
                    name: "base".to_string(),
                    size: "1rem".to_string(),
                    line_height: "1.5rem".to_string(),
                },
                TextScaleToken {
                    name: "lg".to_string(),
                    size: "1.125rem".to_string(),
                    line_height: "1.75rem".to_string(),
                },
            ],
            weights: vec![
                WeightToken {
                    name: "regular".to_string(),
                    value: 400,
                },
                WeightToken {
                    name: "bold".to_string(),
                    value: 700,
                },
            ],
            tracking: vec![TrackingToken {
                name: "tight".to_string(),
                value: "-0.025em".to_string(),
            }],
        },
        spaces: vec![
            ScaleToken {
                name: "1".to_string(),
                value: "0.25rem".to_string(),
            },
            ScaleToken {
                name: "4".to_string(),
                value: "1rem".to_string(),
            },
        ],
        radii: vec![ScaleToken {
            name: "md".to_string(),
            value: "0.375rem".to_string(),
        }],
        shadows: vec![ShadowToken {
            name: "base".to_string(),
            value: "0 1px 3px 0 rgb(0 0 0 / 0.1)".to_string(),
        }],
        motion: Motion {
            durations: vec![ScaleToken {
                name: "fast".to_string(),
                value: "150ms".to_string(),
            }],
            easings: vec![EasingToken {
                name: "out".to_string(),
                value: "cubic-bezier(0, 0, 0.2, 1)".to_string(),
            }],
        },
        breakpoints: vec![ScaleToken {
            name: "md".to_string(),
            value: "768px".to_string(),
        }],
        z_indices: vec![ZToken {
            name: "modal".to_string(),
            value: 1300,
        }],
    }
}

/// Round-trip equality is checked against the sorted normal form
/// because Figma/SD JSON encodes groups as objects (unordered) and
/// re-import sorts by key. The original Beta fixture is authored in
/// semantic order, not alphabetic — so we sort both sides before
/// comparing structural equality.
pub(super) fn sort_for_round_trip(design: &mut Design) {
    design.colors.sort_by(|a, b| a.name.cmp(&b.name));
    design
        .typography
        .families
        .sort_by(|a, b| a.name.cmp(&b.name));
    design.typography.scale.sort_by(|a, b| a.name.cmp(&b.name));
    design
        .typography
        .weights
        .sort_by(|a, b| a.name.cmp(&b.name));
    design
        .typography
        .tracking
        .sort_by(|a, b| a.name.cmp(&b.name));
    design.spaces.sort_by(|a, b| a.name.cmp(&b.name));
    design.radii.sort_by(|a, b| a.name.cmp(&b.name));
    design.shadows.sort_by(|a, b| a.name.cmp(&b.name));
    design.motion.durations.sort_by(|a, b| a.name.cmp(&b.name));
    design.motion.easings.sort_by(|a, b| a.name.cmp(&b.name));
    design.breakpoints.sort_by(|a, b| a.name.cmp(&b.name));
    design.z_indices.sort_by(|a, b| a.name.cmp(&b.name));
}
