//! Design-token core emitters — the six artifacts that Lazuli always emits
//! from `design.lzi` regardless of plugin opt-in. See `docs/proposals/
//! design-tokens.md` §4.
//!
//! This module re-exports the six `emit_*` functions and the canonical
//! `lazuli_ir` design-token IR consumed by those emitters.

pub use lazuli_ir::{
    ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion, ScaleToken,
    ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
};

pub mod ir {
    pub use super::{
        ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion,
        ScaleToken, ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
    };
}

#[path = "design_tokens_ts.rs"]
mod tokens_ts;

#[path = "design_tokens_css.rs"]
mod tokens_css;

#[path = "design_tailwind_v3.rs"]
mod tailwind_v3;

#[path = "design_tailwind_v4.rs"]
mod tailwind_v4;

#[path = "design_tokens_mobile.rs"]
mod tokens_mobile;

#[path = "design_allowlist.rs"]
mod allowlist;

pub use allowlist::emit_allowlist_json;
pub use tailwind_v3::emit_tailwind_v3_preset;
pub use tailwind_v4::emit_tailwind_v4_theme;
pub use tokens_css::emit_tokens_css;
pub use tokens_mobile::emit_tokens_mobile_ts;
pub use tokens_ts::emit_tokens_ts;

#[cfg(test)]
mod integration_tests {
    use super::ir::{
        ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion,
        ScaleToken, ShadowToken, TextScaleToken, Typography, WeightToken, ZToken,
    };
    use super::*;

    /// Build the Beta fixture from `docs/proposals/design-tokens.md` §8.1
    /// exactly. This is the canonical test artifact: every emitter is checked
    /// against it.
    fn demo_design() -> Design {
        Design {
            name: "example".to_owned(),
            extends: None,
            colors: vec![
                ColorToken {
                    name: "primary".to_owned(),
                    states: vec![
                        ColorState {
                            kind: ColorStateKind::Base,
                            value: "#7c3aed".to_owned(),
                            dark: None,
                        },
                        ColorState {
                            kind: ColorStateKind::Hover,
                            value: "#6d28d9".to_owned(),
                            dark: None,
                        },
                        ColorState {
                            kind: ColorStateKind::Foreground,
                            value: "#ffffff".to_owned(),
                            dark: None,
                        },
                    ],
                    span_ref: None,
                },
                ColorToken {
                    name: "background".to_owned(),
                    states: vec![
                        ColorState {
                            kind: ColorStateKind::Base,
                            value: "#ffffff".to_owned(),
                            dark: Some("#09090b".to_owned()),
                        },
                        ColorState {
                            kind: ColorStateKind::Foreground,
                            value: "#f4f4f5".to_owned(),
                            dark: Some("#18181b".to_owned()),
                        },
                    ],
                    span_ref: None,
                },
                ColorToken {
                    name: "foreground".to_owned(),
                    states: vec![
                        ColorState {
                            kind: ColorStateKind::Base,
                            value: "#09090b".to_owned(),
                            dark: Some("#fafafa".to_owned()),
                        },
                        ColorState {
                            kind: ColorStateKind::Foreground,
                            value: "#71717a".to_owned(),
                            dark: Some("#a1a1aa".to_owned()),
                        },
                    ],
                    span_ref: None,
                },
                ColorToken {
                    name: "success".to_owned(),
                    states: vec![ColorState {
                        kind: ColorStateKind::Base,
                        value: "#16a34a".to_owned(),
                        dark: None,
                    }],
                    span_ref: None,
                },
                ColorToken {
                    name: "warning".to_owned(),
                    states: vec![ColorState {
                        kind: ColorStateKind::Base,
                        value: "#ea580c".to_owned(),
                        dark: None,
                    }],
                    span_ref: None,
                },
                ColorToken {
                    name: "danger".to_owned(),
                    states: vec![ColorState {
                        kind: ColorStateKind::Base,
                        value: "#dc2626".to_owned(),
                        dark: None,
                    }],
                    span_ref: None,
                },
            ],
            typography: Typography {
                families: vec![
                    FamilyToken {
                        name: "sans".to_owned(),
                        value: "Inter, system-ui, sans-serif".to_owned(),
                    },
                    FamilyToken {
                        name: "mono".to_owned(),
                        value: "JetBrains Mono, monospace".to_owned(),
                    },
                ],
                scale: vec![
                    TextScaleToken {
                        name: "sm".to_owned(),
                        size: "0.875rem".to_owned(),
                        line_height: "1.25rem".to_owned(),
                    },
                    TextScaleToken {
                        name: "base".to_owned(),
                        size: "1rem".to_owned(),
                        line_height: "1.5rem".to_owned(),
                    },
                    TextScaleToken {
                        name: "lg".to_owned(),
                        size: "1.125rem".to_owned(),
                        line_height: "1.75rem".to_owned(),
                    },
                    TextScaleToken {
                        name: "xl".to_owned(),
                        size: "1.25rem".to_owned(),
                        line_height: "1.75rem".to_owned(),
                    },
                    TextScaleToken {
                        name: "2xl".to_owned(),
                        size: "1.5rem".to_owned(),
                        line_height: "2rem".to_owned(),
                    },
                ],
                weights: vec![
                    WeightToken {
                        name: "regular".to_owned(),
                        value: 400,
                    },
                    WeightToken {
                        name: "medium".to_owned(),
                        value: 500,
                    },
                    WeightToken {
                        name: "semibold".to_owned(),
                        value: 600,
                    },
                    WeightToken {
                        name: "bold".to_owned(),
                        value: 700,
                    },
                ],
                tracking: vec![],
            },
            spaces: vec![
                ScaleToken {
                    name: "1".to_owned(),
                    value: "0.25rem".to_owned(),
                },
                ScaleToken {
                    name: "2".to_owned(),
                    value: "0.5rem".to_owned(),
                },
                ScaleToken {
                    name: "3".to_owned(),
                    value: "0.75rem".to_owned(),
                },
                ScaleToken {
                    name: "4".to_owned(),
                    value: "1rem".to_owned(),
                },
                ScaleToken {
                    name: "6".to_owned(),
                    value: "1.5rem".to_owned(),
                },
                ScaleToken {
                    name: "8".to_owned(),
                    value: "2rem".to_owned(),
                },
            ],
            radii: vec![
                ScaleToken {
                    name: "sm".to_owned(),
                    value: "0.125rem".to_owned(),
                },
                ScaleToken {
                    name: "base".to_owned(),
                    value: "0.25rem".to_owned(),
                },
                ScaleToken {
                    name: "md".to_owned(),
                    value: "0.375rem".to_owned(),
                },
                ScaleToken {
                    name: "lg".to_owned(),
                    value: "0.5rem".to_owned(),
                },
            ],
            shadows: vec![
                ShadowToken {
                    name: "sm".to_owned(),
                    value: "0 1px 2px 0 rgb(0 0 0 / 0.05)".to_owned(),
                },
                ShadowToken {
                    name: "base".to_owned(),
                    value: "0 1px 3px 0 rgb(0 0 0 / 0.1)".to_owned(),
                },
                ShadowToken {
                    name: "md".to_owned(),
                    value: "0 4px 6px -1px rgb(0 0 0 / 0.1)".to_owned(),
                },
            ],
            motion: Motion {
                durations: vec![
                    ScaleToken {
                        name: "fast".to_owned(),
                        value: "150ms".to_owned(),
                    },
                    ScaleToken {
                        name: "base".to_owned(),
                        value: "200ms".to_owned(),
                    },
                ],
                easings: vec![EasingToken {
                    name: "out".to_owned(),
                    value: "cubic-bezier(0, 0, 0.2, 1)".to_owned(),
                }],
            },
            breakpoints: vec![
                ScaleToken {
                    name: "sm".to_owned(),
                    value: "640px".to_owned(),
                },
                ScaleToken {
                    name: "md".to_owned(),
                    value: "768px".to_owned(),
                },
                ScaleToken {
                    name: "lg".to_owned(),
                    value: "1024px".to_owned(),
                },
            ],
            z_indices: vec![
                ZToken {
                    name: "dropdown".to_owned(),
                    value: 1000,
                },
                ZToken {
                    name: "modal".to_owned(),
                    value: 1300,
                },
            ],
            custom: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn design_emitters_match_proposal_section_8() {
        let d = demo_design();

        // -- tokens.ts ---------------------------------------------------------
        let ts = emit_tokens_ts(&d);
        // multi-state primary
        assert!(ts.contains("primary: {"));
        assert!(ts.contains("base: \"#7c3aed\","));
        assert!(ts.contains("hover: \"#6d28d9\","));
        assert!(ts.contains("foreground: \"#ffffff\","));
        // background with dark
        assert!(ts.contains("background: {"));
        assert!(ts.contains("base: { light: \"#ffffff\", dark: \"#09090b\" },"));
        // single-flat
        assert!(ts.contains("success: \"#16a34a\","));
        assert!(ts.contains("danger: \"#dc2626\","));
        // typography scale with quoted "2xl"
        assert!(ts.contains("\"2xl\": { size: \"1.5rem\", lineHeight: \"2rem\" },"));
        // weights as numbers
        assert!(ts.contains("bold: 700,"));
        // type aliases
        assert!(ts.contains("export type ColorToken = keyof typeof tokens.color;"));
        assert!(ts.contains("export type BreakpointToken = keyof typeof tokens.breakpoint;"));

        // -- tokens.css --------------------------------------------------------
        let css = emit_tokens_css(&d);
        assert!(css.contains(":root {"));
        assert!(css.contains("  --color-primary-base: #7c3aed;"));
        assert!(css.contains("  --color-primary-hover: #6d28d9;"));
        assert!(css.contains("  --color-primary-foreground: #ffffff;"));
        assert!(css.contains("  --color-background-base: #ffffff;"));
        assert!(css.contains("  --color-success: #16a34a;"));
        assert!(css.contains("  --font-sans: Inter, system-ui, sans-serif;"));
        assert!(css.contains("  --text-base: 1rem;"));
        assert!(css.contains("  --text-base--line-height: 1.5rem;"));
        assert!(css.contains("  --space-4: 1rem;"));
        assert!(css.contains("  --shadow-base: 0 1px 3px 0 rgb(0 0 0 / 0.1);"));
        assert!(css.contains("  --duration-fast: 150ms;"));
        assert!(css.contains("  --easing-out: cubic-bezier(0, 0, 0.2, 1);"));
        assert!(css.contains("  --z-modal: 1300;"));
        // dark overrides
        assert!(css.contains("[data-theme=\"dark\"] {"));
        assert!(css.contains("  --color-background-base: #09090b;"));
        assert!(css.contains("  --color-foreground-base: #fafafa;"));

        // -- tailwind v3 preset -----------------------------------------------
        let v3 = emit_tailwind_v3_preset(&d);
        assert!(v3.contains("import type { Config } from \"tailwindcss\";"));
        assert!(v3.contains("darkMode: [\"class\", '[data-theme=\"dark\"]'],"));
        assert!(v3.contains("primary: {"));
        assert!(v3.contains("DEFAULT: \"var(--color-primary-base)\","));
        assert!(v3.contains("hover: \"var(--color-primary-hover)\","));
        assert!(v3.contains("success: \"var(--color-success)\","));
        // radius.base → DEFAULT
        assert!(v3.contains("DEFAULT: \"0.25rem\","));
        // shadow.base → DEFAULT
        assert!(v3.contains("DEFAULT: \"0 1px 3px 0 rgb(0 0 0 / 0.1)\","));
        // motion.base → DEFAULT
        assert!(v3.contains("DEFAULT: \"200ms\","));
        // text scale
        assert!(v3.contains("\"2xl\": [\"1.5rem\", { lineHeight: \"2rem\" }],"));
        // breakpoints + z
        assert!(v3.contains("md: \"768px\","));
        assert!(v3.contains("modal: \"1300\","));

        // -- tailwind v4 theme -------------------------------------------------
        let v4 = emit_tailwind_v4_theme(&d);
        assert!(v4.contains("@import \"./tokens.css\";"));
        assert!(v4.contains("@theme {"));
        assert!(v4.contains("  --color-primary: var(--color-primary-base);"));
        assert!(v4.contains("  --color-primary-hover: var(--color-primary-hover);"));
        assert!(v4.contains("  --color-success: var(--color-success);"));
        assert!(v4.contains("  --font-sans: var(--font-sans);"));
        assert!(v4.contains("  --text-base: var(--text-base);"));
        assert!(v4.contains("  --text-base--line-height: var(--text-base--line-height);"));
        assert!(v4.contains("  --spacing-4: var(--space-4);"));
        assert!(v4.contains("  --radius-md: var(--radius-md);"));
        assert!(v4.contains("  --shadow-base: var(--shadow-base);"));
        assert!(v4.contains("  --duration-fast: var(--duration-fast);"));
        assert!(v4.contains("  --ease-out: var(--easing-out);"));
        assert!(v4.contains("  --breakpoint-md: 768px;"));
        assert!(v4.contains("  --z-modal: var(--z-modal);"));

        // -- mobile tokens.ts --------------------------------------------------
        let mobile = emit_tokens_mobile_ts(&d, 16);
        assert!(mobile.contains("React Native shape"));
        assert!(mobile.contains("primary: {"));
        assert!(mobile.contains("base: \"#7c3aed\","));
        // background dark
        assert!(mobile.contains("base: { light: \"#ffffff\", dark: \"#09090b\" },"));
        // rem→px (1rem=16; 0.25rem=4)
        assert!(mobile.contains("\"1\": 4,"));
        assert!(mobile.contains("\"4\": 16,"));
        assert!(mobile.contains("\"8\": 32,"));
        // font shorthand
        assert!(mobile.contains("sans: \"Inter\","));
        assert!(mobile.contains("mono: \"JetBrains Mono\","));
        // scale with px numbers
        assert!(mobile.contains("base: { fontSize: 16, lineHeight: 24 },"));
        // shadow object
        assert!(mobile.contains("base: {"));
        assert!(mobile.contains("shadowColor: \"#000000\","));
        assert!(mobile.contains("shadowOffset: { width: 0, height: 1 },"));
        assert!(mobile.contains("shadowRadius: 3,"));
        // motion strips ms
        assert!(mobile.contains("fast: 150,"));
        // easing kept as string
        assert!(mobile.contains("out: \"cubic-bezier(0, 0, 0.2, 1)\","));
        // No breakpoint / z in mobile
        assert!(!mobile.contains("breakpoint:"));
        // The mobile output must NOT contain a top-level `z` key; check by
        // looking for the leading two-space indent + "z:" literal.
        assert!(!mobile.contains("\n  z: "));

        // -- allowlist.json ----------------------------------------------------
        // Buckets are keyed by Tailwind prefix; values are BARE SUFFIXES
        // (matches `lazuli_doctor::design::helpers::Allowlist::contains`).
        let allow = emit_allowlist_json(&d);
        // Parse to ensure valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&allow).expect("valid JSON");
        let obj = parsed.as_object().expect("top-level object");
        let bg = obj.get("bg").unwrap().as_array().unwrap();
        let bg_strs: Vec<&str> = bg.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(bg_strs.contains(&"primary"));
        assert!(bg_strs.contains(&"primary-hover"));
        assert!(bg_strs.contains(&"primary-foreground"));
        assert!(bg_strs.contains(&"success"));
        // Sanity-check: no `success-hover` since success has only Base.
        assert!(!bg_strs.contains(&"success-hover"));
        let rounded = obj.get("rounded").unwrap().as_array().unwrap();
        let rounded_strs: Vec<&str> = rounded.iter().map(|v| v.as_str().unwrap()).collect();
        // `base` token → `DEFAULT` slot (Tailwind preset shape).
        assert!(rounded_strs.contains(&"DEFAULT"));
        assert!(rounded_strs.contains(&"sm"));
        assert!(rounded_strs.contains(&"md"));
        assert!(rounded_strs.contains(&"lg"));
        let shadow = obj.get("shadow").unwrap().as_array().unwrap();
        let shadow_strs: Vec<&str> = shadow.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(shadow_strs.contains(&"DEFAULT"));
        assert!(shadow_strs.contains(&"sm"));
        assert!(shadow_strs.contains(&"md"));
        let z = obj.get("z").unwrap().as_array().unwrap();
        let z_strs: Vec<&str> = z.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(z_strs.contains(&"dropdown"));
        assert!(z_strs.contains(&"modal"));
        let font = obj.get("font").unwrap().as_array().unwrap();
        let font_strs: Vec<&str> = font.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(font_strs.contains(&"sans"));
        assert!(font_strs.contains(&"bold"));
        let text_size = obj.get("text-size").unwrap().as_array().unwrap();
        let ts_strs: Vec<&str> = text_size.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(ts_strs.contains(&"base"));
        assert!(ts_strs.contains(&"2xl"));
    }
}
