
    use super::*;
    use crate::design::ir::{
        ColorState, ColorStateKind, ColorToken, Design, EasingToken, FamilyToken, Motion,
        ScaleToken, ShadowToken, TextScaleToken, TrackingToken, Typography, WeightToken, ZToken,
    };
    use lazuli_ir::CustomToken;

    fn minimal() -> Design {
        Design {
            name: "tiny".to_owned(),
            extends: None,
            colors: vec![ColorToken {
                name: "success".to_owned(),
                states: vec![ColorState {
                    kind: ColorStateKind::Base,
                    value: "#16a34a".to_owned(),
                    dark: None,
                }],
                span_ref: None,
            }],
            typography: Typography {
                families: vec![FamilyToken {
                    name: "sans".to_owned(),
                    value: "Inter, system-ui, sans-serif".to_owned(),
                }],
                scale: vec![TextScaleToken {
                    name: "base".to_owned(),
                    size: "1rem".to_owned(),
                    line_height: "1.5rem".to_owned(),
                }],
                weights: vec![],
                tracking: vec![],
            },
            spaces: vec![ScaleToken {
                name: "4".to_owned(),
                value: "1rem".to_owned(),
            }],
            radii: vec![ScaleToken {
                name: "base".to_owned(),
                value: "0.25rem".to_owned(),
            }],
            shadows: vec![],
            motion: Motion::default(),
            breakpoints: vec![ScaleToken {
                name: "sm".to_owned(),
                value: "640px".to_owned(),
            }],
            z_indices: vec![ZToken {
                name: "modal".to_owned(),
                value: 1300,
            }],
            custom: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn minimal_fixture_emits_preset_structure() {
        let out = emit_tailwind_v3_preset(&minimal());
        assert!(out.contains("import type { Config } from \"tailwindcss\";"));
        assert!(out.contains("export const lazuliPreset: Partial<Config> = {"));
        assert!(out.contains("darkMode: [\"class\", '[data-theme=\"dark\"]'],"));
        assert!(out.contains("colors: {"));
        assert!(out.contains("success: \"var(--color-success)\","));
        // radius.base → DEFAULT
        assert!(out.contains("DEFAULT: \"0.25rem\","));
        // zIndex strings
        assert!(out.contains("modal: \"1300\","));
        assert!(out.contains("sm: \"640px\","));
    }

    #[test]
    fn multi_state_color_emits_default_plus_states() {
        let mut d = minimal();
        d.colors.push(ColorToken {
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
                    kind: ColorStateKind::Active,
                    value: "#5b21b6".to_owned(),
                    dark: None,
                },
                ColorState {
                    kind: ColorStateKind::Foreground,
                    value: "#ffffff".to_owned(),
                    dark: None,
                },
            ],
            span_ref: None,
        });
        let out = emit_tailwind_v3_preset(&d);
        assert!(out.contains("primary: {"));
        assert!(out.contains("DEFAULT: \"var(--color-primary-base)\","));
        assert!(out.contains("hover: \"var(--color-primary-hover)\","));
        assert!(out.contains("active: \"var(--color-primary-active)\","));
        assert!(out.contains("foreground: \"var(--color-primary-foreground)\","));
    }

    #[test]
    fn dark_variant_preset_does_not_inline_dark_hex() {
        // Dark variants live in tokens.css; the preset stays var-backed.
        let mut d = minimal();
        d.colors.push(ColorToken {
            name: "background".to_owned(),
            states: vec![ColorState {
                kind: ColorStateKind::Base,
                value: "#ffffff".to_owned(),
                dark: Some("#09090b".to_owned()),
            }],
            span_ref: None,
        });
        let out = emit_tailwind_v3_preset(&d);
        // The preset must reference the variable, NOT inline either hex.
        assert!(out.contains("background: \"var(--color-background)\","));
        assert!(!out.contains("#09090b"));
        assert!(!out.contains("#ffffff"));
    }

    #[test]
    fn empty_motion_easings_emits_empty_block() {
        let d = minimal();
        let out = emit_tailwind_v3_preset(&d);
        assert!(out.contains("transitionTimingFunction: {"));
        // Must close the block even when empty.
        assert!(out.contains("transitionTimingFunction: {\n      },"));
    }

    #[test]
    fn shadow_base_maps_to_default() {
        let mut d = minimal();
        d.shadows.push(ShadowToken {
            name: "base".to_owned(),
            value: "0 1px 3px 0 rgb(0 0 0 / 0.1)".to_owned(),
        });
        d.shadows.push(ShadowToken {
            name: "md".to_owned(),
            value: "0 4px 6px -1px rgb(0 0 0 / 0.1)".to_owned(),
        });
        let out = emit_tailwind_v3_preset(&d);
        assert!(out.contains("DEFAULT: \"0 1px 3px 0 rgb(0 0 0 / 0.1)\","));
        assert!(out.contains("md: \"0 4px 6px -1px rgb(0 0 0 / 0.1)\","));
    }

    #[test]
    fn motion_base_duration_maps_to_default() {
        let mut d = minimal();
        d.motion.durations.push(ScaleToken {
            name: "fast".to_owned(),
            value: "150ms".to_owned(),
        });
        d.motion.durations.push(ScaleToken {
            name: "base".to_owned(),
            value: "200ms".to_owned(),
        });
        d.motion.easings.push(EasingToken {
            name: "out".to_owned(),
            value: "cubic-bezier(0, 0, 0.2, 1)".to_owned(),
        });
        let out = emit_tailwind_v3_preset(&d);
        assert!(out.contains("transitionDuration: {"));
        assert!(out.contains("fast: \"150ms\","));
        assert!(out.contains("DEFAULT: \"200ms\","));
        assert!(out.contains("out: \"cubic-bezier(0, 0, 0.2, 1)\","));
    }

    #[test]
    fn font_weight_emitted_as_string_not_number() {
        let mut d = minimal();
        d.typography.weights.push(WeightToken {
            name: "semibold".to_owned(),
            value: 600,
        });
        let out = emit_tailwind_v3_preset(&d);
        // Tailwind v3 expects string values for fontWeight.
        assert!(out.contains("semibold: \"600\","));
    }

    #[test]
    fn letter_spacing_uses_tracking_values() {
        let mut d = minimal();
        d.typography.tracking.push(TrackingToken {
            name: "tight".to_owned(),
            value: "-0.025em".to_owned(),
        });
        let out = emit_tailwind_v3_preset(&d);
        assert!(out.contains("letterSpacing: {"));
        assert!(out.contains("tight: \"-0.025em\","));
    }

    #[test]
    fn custom_token_emits_inside_colors_map_as_var_string() {
        // Z2 — `custom` 9th meta-group lowers as a flat var-string entry
        // alongside Shadcn-semantic palette names in the same `colors` map.
        let mut d = minimal();
        d.custom.push(CustomToken {
            name: "chat-bubble-mine".to_owned(),
            base: "#dcf8c6".to_owned(),
            dark: None,
            span_ref: None,
        });
        let out = emit_tailwind_v3_preset(&d);
        assert!(out.contains(
            "\"chat-bubble-mine\": \"var(--color-chat-bubble-mine)\","
        ));
    }

    #[test]
    fn custom_token_with_dark_preset_stays_var_backed() {
        // Dark variant lives in tokens.css. Preset is identical shape.
        let mut d = minimal();
        d.custom.push(CustomToken {
            name: "chat-bubble-mine".to_owned(),
            base: "#dcf8c6".to_owned(),
            dark: Some("#005c4b".to_owned()),
            span_ref: None,
        });
        let out = emit_tailwind_v3_preset(&d);
        assert!(out.contains(
            "\"chat-bubble-mine\": \"var(--color-chat-bubble-mine)\","
        ));
        // Preset must NOT inline the dark hex.
        assert!(!out.contains("#005c4b"));
    }

    #[test]
    fn quoted_scale_name_is_preserved_in_font_size() {
        let mut d = minimal();
        d.typography.scale.push(TextScaleToken {
            name: "2xl".to_owned(),
            size: "1.5rem".to_owned(),
            line_height: "2rem".to_owned(),
        });
        let out = emit_tailwind_v3_preset(&d);
        assert!(out.contains("\"2xl\": [\"1.5rem\", { lineHeight: \"2rem\" }],"));
    }
