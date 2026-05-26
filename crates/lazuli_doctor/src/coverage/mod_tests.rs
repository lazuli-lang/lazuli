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
