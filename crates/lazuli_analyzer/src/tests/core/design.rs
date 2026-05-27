    use lazuli_ir as ir;

    use lazuli_syntax::{parse_feature_skeletons, parse_lzx_document};

    use crate::auth::lower_auth_identity;
    use crate::query::parse_query_filter_line;
    use crate::resource::lower_validate_line;
    use crate::{
        AnalyzeError, lower_audit_block, lower_feature_skeleton, lower_lzx_document,
        lower_policy_atom_with_args, parse_cap_file_type, resolve_invalidates_targets,
        type_ref_from_syntax,
    };


    #[test]
    fn lower_registry_tool_entry_with_effect_and_pii_classes() {
        // Pin the IR shape for `RegistryToolEntry`. The actual
        // registry.lzi parser lands in a later phase; this test
        // documents the contract that doctor's
        // `tool_registry_effect_required_diagnostics` will read.
        let entry = ir::RegistryToolEntry {
            name: "web_search".to_owned(),
            effect: ir::ToolEffect::Read,
            pii_classes: vec![ir::QualifiedName {
                feature: None,
                name: "@pii.contact".to_owned(),
            }],
            adapter: Some(ir::QualifiedName {
                feature: None,
                name: "@adapter.serp".to_owned(),
            }),
            span_ref: None,
        };

        let serialized = serde_json::to_value(&entry).unwrap();
        assert_eq!(serialized["name"], "web_search");
        assert_eq!(serialized["effect"], "read");
        assert_eq!(serialized["pii_classes"][0]["name"], "@pii.contact");
        assert_eq!(serialized["adapter"]["name"], "@adapter.serp");
    }

    // -------------------------------------------------------------------------
    // L0 #2 — design tokens lowering tests.
    // -------------------------------------------------------------------------

    use lazuli_syntax::parse_design_document;

    use crate::lower_design;

    fn lower_design_source(source: &str) -> ir::Design {
        let ast = parse_design_document(source).expect("parses");
        lower_design(&ast).expect("lowers")
    }

    #[test]
    fn lower_design_lifts_flat_color_as_base_state() {
        let source = "
design example
  color
    success \"#16a34a\"
";
        let design = lower_design_source(source);
        assert_eq!(design.name, "example");
        assert!(design.extends.is_none());
        assert_eq!(design.colors.len(), 1);
        let success = &design.colors[0];
        assert_eq!(success.name, "success");
        assert_eq!(success.states.len(), 1);
        assert_eq!(success.states[0].kind, ir::ColorStateKind::Base);
        assert_eq!(success.states[0].value, "#16a34a");
    }

    #[test]
    fn lower_design_lifts_sub_block_color_states() {
        let source = "
design example
  color
    primary
      base \"#7c3aed\"
      hover \"#6d28d9\"
      active \"#5b21b6\"
      foreground \"#ffffff\"
";
        let design = lower_design_source(source);
        let primary = &design.colors[0];
        assert_eq!(primary.states.len(), 4);
        let kinds: Vec<ir::ColorStateKind> = primary.states.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ir::ColorStateKind::Base,
                ir::ColorStateKind::Hover,
                ir::ColorStateKind::Active,
                ir::ColorStateKind::Foreground,
            ]
        );
    }

    #[test]
    fn lower_design_preserves_dark_suffix() {
        let source = "
design example
  color
    background
      base \"#ffffff\" dark \"#09090b\"
";
        let design = lower_design_source(source);
        let bg = &design.colors[0];
        assert_eq!(bg.states[0].value, "#ffffff");
        assert_eq!(bg.states[0].dark.as_deref(), Some("#09090b"));
    }

    #[test]
    fn lower_design_extends_rejected_with_cut_b_code() {
        let source = "
design alpha
  extends base
  color
    primary
      base \"#10b981\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("DESIGN-EXTENDS-CUT-B"),
            "expected DESIGN-EXTENDS-CUT-B, got: {msg}"
        );
        assert!(matches!(err, AnalyzeError::DesignExtendsCutB { .. }));
    }

    #[test]
    fn lower_design_multi_layer_shadow_rejected() {
        let source = "
design example
  shadow
    elevated \"0 1px 2px 0 rgb(0 0 0 / 0.05), 0 4px 6px -1px rgb(0 0 0 / 0.1)\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("DESIGN-SHADOW-MULTI-LAYER"),
            "expected DESIGN-SHADOW-MULTI-LAYER, got: {msg}"
        );
        assert!(matches!(
            err,
            AnalyzeError::DesignShadowMultiLayer { ref name } if name == "elevated"
        ));
    }

    #[test]
    fn lower_design_single_layer_shadow_with_inner_commas_passes() {
        // Commas inside `rgb(...)` are inner; they do NOT trigger the
        // multi-layer rejection. The closed grammar accepts single-layer
        // shadows whose inner color uses `rgb(r, g, b)` notation.
        let source = "
design example
  shadow
    base \"0 1px 3px 0 rgb(0, 0, 0, 0.1)\"
";
        let design = lower_design_source(source);
        assert_eq!(design.shadows.len(), 1);
        assert_eq!(design.shadows[0].value, "0 1px 3px 0 rgb(0, 0, 0, 0.1)");
    }

    #[test]
    fn lower_design_typography_full_round_trip() {
        let source = "
design example
  typography
    family
      sans \"Inter, system-ui, sans-serif\"
    scale
      base size 1rem, line_height 1.5rem
    weight
      medium 500
      bold 700
    tracking
      tight -0.025em
";
        let design = lower_design_source(source);
        assert_eq!(design.typography.families[0].name, "sans");
        assert_eq!(
            design.typography.families[0].value,
            "Inter, system-ui, sans-serif"
        );
        assert_eq!(design.typography.scale[0].size, "1rem");
        assert_eq!(design.typography.scale[0].line_height, "1.5rem");
        // u16 parse.
        assert_eq!(design.typography.weights[0].value, 500);
        assert_eq!(design.typography.weights[1].value, 700);
        // Tracking preserves text including negative.
        assert_eq!(design.typography.tracking[0].value, "-0.025em");
    }

    #[test]
    fn lower_design_z_values_parsed_as_i32() {
        let source = "
design example
  z
    docked 10
    modal 1300
    toast 1500
";
        let design = lower_design_source(source);
        assert_eq!(design.z_indices.len(), 3);
        assert_eq!(design.z_indices[0].value, 10);
        assert_eq!(design.z_indices[1].value, 1300);
        assert_eq!(design.z_indices[2].value, 1500);
    }

    #[test]
    fn lower_design_rejects_invalid_hex() {
        let source = "
design example
  color
    bogus \"not-a-hex\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        assert!(
            matches!(err, AnalyzeError::DesignColorHexInvalid { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn lower_design_rejects_unknown_color_state() {
        // Construct AST directly (parser surface uses kind=String, so an
        // unknown identifier passes parse but should fail lowering).
        use lazuli_syntax::{
            ColorStateAst, ColorTokenAst, DesignDeclAst, MotionAst, Span, TypographyAst,
        };

        let ast = DesignDeclAst {
            name: "example".to_owned(),
            extends: None,
            colors: vec![ColorTokenAst {
                name: "primary".to_owned(),
                states: vec![ColorStateAst {
                    kind: "disabled".to_owned(),
                    value: "#7c3aed".to_owned(),
                    dark: None,
                }],
                span: Span::new(0, 1),
            }],
            typography: TypographyAst::default(),
            spaces: Vec::new(),
            radii: Vec::new(),
            shadows: Vec::new(),
            motion: MotionAst::default(),
            breakpoints: Vec::new(),
            z_indices: Vec::new(),
            custom: Vec::new(),
            span: Span::new(0, 1),
        };
        let err = lower_design(&ast).unwrap_err();
        assert!(matches!(
            err,
            AnalyzeError::DesignColorStateUnknown { ref token, ref state }
                if token == "primary" && state == "disabled"
        ));
    }

    #[test]
    fn lower_design_full_example_round_trip() {
        let source = "
design example
  color
    primary
      base \"#7c3aed\"
      hover \"#6d28d9\"
      foreground \"#ffffff\"
    success \"#16a34a\"

  typography
    family
      sans \"Inter, system-ui, sans-serif\"
    scale
      base size 1rem, line_height 1.5rem

  space
    \"1\" 0.25rem
    \"4\" 1rem

  radius
    sm 0.125rem

  shadow
    base \"0 1px 3px 0 rgb(0 0 0 / 0.1)\"

  motion
    duration
      fast 150ms
    easing
      out \"cubic-bezier(0, 0, 0.2, 1)\"

  breakpoint
    sm 640px

  z
    modal 1300
";
        let design = lower_design_source(source);
        // Every group has at least one entry.
        assert!(!design.colors.is_empty());
        assert!(!design.typography.families.is_empty());
        assert!(!design.typography.scale.is_empty());
        assert!(!design.spaces.is_empty());
        assert!(!design.radii.is_empty());
        assert!(!design.shadows.is_empty());
        assert!(!design.motion.durations.is_empty());
        assert!(!design.motion.easings.is_empty());
        assert!(!design.breakpoints.is_empty());
        assert!(!design.z_indices.is_empty());
        // SpanRef preserved.
        assert!(design.span_ref.is_some());
        // Serializes round-trip cleanly.
        let json = serde_json::to_value(&design).unwrap();
        assert_eq!(json["name"], "example");
        assert_eq!(json["colors"][0]["name"], "primary");
        // States serialize with snake_case kind.
        assert_eq!(json["colors"][0]["states"][0]["kind"], "base");
        // ColorStateKind serializes as snake_case.
        assert_eq!(json["colors"][0]["states"][2]["kind"], "foreground");
    }

    // ── Z2 — `custom` 9th meta-group lowering ──────────────────────────────

    #[test]
    fn lower_design_lifts_custom_group_with_base_and_dark() {
        let source = r##"
design the canonical pilot
  custom
    chat-bubble-mine "#dcf8c6" dark "#005c4b"
    chat-bubble-other "#ffffff"
    map-marker-active "#ff5722"
"##;
        let design = lower_design_source(source);
        assert_eq!(design.custom.len(), 3);
        assert_eq!(design.custom[0].name, "chat-bubble-mine");
        assert_eq!(design.custom[0].base, "#dcf8c6");
        assert_eq!(design.custom[0].dark.as_deref(), Some("#005c4b"));
        assert_eq!(design.custom[1].dark, None);
        assert_eq!(design.custom[2].name, "map-marker-active");
    }

    #[test]
    fn lower_design_preserves_invalid_custom_hex_for_doctor() {
        // Analyzer is intentionally permissive on `custom` hex values —
        // doctor's `design-custom-invalid-value` rule does the proposal-
        // pending validation. See `docs/proposals/design-tokens-custom.md` §4.
        let source = r##"
design the canonical pilot
  custom
    oops "not-a-color"
    chat-bubble "#dcf8c6" dark "rgb(5,5,5)"
"##;
        let design = lower_design_source(source);
        assert_eq!(design.custom.len(), 2);
        assert_eq!(design.custom[0].base, "not-a-color");
        assert_eq!(design.custom[1].dark.as_deref(), Some("rgb(5,5,5)"));
    }
