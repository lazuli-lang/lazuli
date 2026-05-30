use crate::AnalyzeError;
use lazuli_syntax::parse_feature_skeletons;

/// `length 120 min 100` — § 10.2 rejects `length + min`.
#[test]
fn length_plus_min_emits_constraint_conflict() {
    let source = r#"
feature post
  domain
    resource Post
      title: Text length 120 min 100
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::ConstraintConflict { field, combo }) => {
            assert_eq!(field, "title");
            assert_eq!(combo, "length+min");
        }
        other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
    }
}

/// `between 0 and 100 max 50` — §10.2 rejects `between + max`.
#[test]
fn between_plus_max_emits_constraint_conflict() {
    let source = r#"
feature score
  domain
    resource Score
      points: Integer between 0 and 100 max 50
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::ConstraintConflict { field, combo }) => {
            assert_eq!(field, "points");
            assert_eq!(combo, "between+max");
        }
        other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
    }
}

/// `in ["a", "b"] pattern "^a"` — §10.2 says use enum instead.
#[test]
fn in_plus_pattern_emits_constraint_conflict() {
    let source = r#"
feature acl
  domain
    resource Member
      role: Text in ["a", "b"] pattern "^a"
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::ConstraintConflict { field, combo }) => {
            assert_eq!(field, "role");
            assert_eq!(combo, "in+pattern");
        }
        other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
    }
}

/// `Text required min 2 default ""` — §10.3 rejects empty default
/// because the empty string has length 0 < 2.
#[test]
fn empty_default_violates_min_constraint() {
    let source = r#"
feature account
  domain
    resource Account
      handle: Text required min 2 = ""
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::DefaultViolatesConstraint { field, rule, .. }) => {
            assert_eq!(field, "handle");
            assert!(rule.starts_with("min="), "expected min rule, got {}", rule);
        }
        other => panic!("expected DefaultViolatesConstraint, got: {:?}", other.err()),
    }
}

/// Valid combination: `min N max M` (without between/length) passes.
#[test]
fn min_max_combination_passes_lowering() {
    let source = r#"
feature post
  domain
    resource Post
      title: Text required min 2 max 80
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = crate::lower_feature_skeleton(&features[0]).expect("lowers");
    let field = &feature.resources[0].fields[0];
    assert_eq!(field.constraints.min, Some(2));
    assert_eq!(field.constraints.max, Some(80));
}

// -------------------------------------------------------------------------
// Wave-B-CL4 — `inline_validator_range_invariant_001`
// -------------------------------------------------------------------------

/// `min 10 max 5` — N>M yields an empty domain.
#[test]
fn min_greater_than_max_emits_range_invariant() {
    let source = r#"
feature post
  domain
    resource Post
      score: Integer required min 10 max 5
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::InlineValidatorRangeInvariant {
            field,
            rule,
            low,
            high,
        }) => {
            assert_eq!(field, "score");
            assert_eq!(rule, "min>max");
            assert_eq!(low, "10");
            assert_eq!(high, "5");
        }
        other => panic!(
            "expected InlineValidatorRangeInvariant, got: {:?}",
            other.err()
        ),
    }
}

/// `between 100 and 0` — A>B yields an empty domain.
#[test]
fn between_with_inverted_bounds_emits_range_invariant() {
    let source = r#"
feature score
  domain
    resource Score
      points: Integer required between 100 and 0
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::InlineValidatorRangeInvariant {
            field,
            rule,
            low,
            high,
        }) => {
            assert_eq!(field, "points");
            assert_eq!(rule, "between");
            assert_eq!(low, "100");
            assert_eq!(high, "0");
        }
        other => panic!(
            "expected InlineValidatorRangeInvariant, got: {:?}",
            other.err()
        ),
    }
}

/// `min 5 max 5` — equal bounds are valid (single-value domain).
#[test]
fn min_equals_max_passes_range_invariant() {
    let source = r#"
feature post
  domain
    resource Post
      flag: Integer required min 5 max 5
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = crate::lower_feature_skeleton(&features[0]).expect("lowers");
    let field = &feature.resources[0].fields[0];
    assert_eq!(field.constraints.min, Some(5));
    assert_eq!(field.constraints.max, Some(5));
}

// -------------------------------------------------------------------------
// Wave-B-CL4 — `inline_validator_type_mismatch_001`
// -------------------------------------------------------------------------

/// `pattern "..."` on `Boolean` — §10.1 restricts `pattern` to Text.
#[test]
fn pattern_on_boolean_emits_type_mismatch() {
    let source = r#"
feature account
  domain
    resource Account
      enabled: Boolean pattern "^t"
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::InlineValidatorTypeMismatch {
            field,
            field_type,
            constraint,
            ..
        }) => {
            assert_eq!(field, "enabled");
            assert_eq!(field_type, "Boolean");
            assert_eq!(constraint, "pattern");
        }
        other => panic!(
            "expected InlineValidatorTypeMismatch, got: {:?}",
            other.err()
        ),
    }
}

/// `length N` on `Integer` — §10.1 restricts `length` to Text.
#[test]
fn length_on_integer_emits_type_mismatch() {
    let source = r#"
feature score
  domain
    resource Score
      points: Integer length 3
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::InlineValidatorTypeMismatch {
            field,
            field_type,
            constraint,
            ..
        }) => {
            assert_eq!(field, "points");
            assert_eq!(field_type, "Integer");
            assert_eq!(constraint, "length");
        }
        other => panic!(
            "expected InlineValidatorTypeMismatch, got: {:?}",
            other.err()
        ),
    }
}

/// `between A and B` on `Text` — §10.1 restricts `between` to numerics.
#[test]
fn between_on_text_emits_type_mismatch() {
    let source = r#"
feature account
  domain
    resource Account
      handle: Text between 2 and 30
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::InlineValidatorTypeMismatch {
            field,
            field_type,
            constraint,
            ..
        }) => {
            assert_eq!(field, "handle");
            assert_eq!(field_type, "Text");
            assert_eq!(constraint, "between");
        }
        other => panic!(
            "expected InlineValidatorTypeMismatch, got: {:?}",
            other.err()
        ),
    }
}

// -------------------------------------------------------------------------
// Wave-B-CL4 — `inline_validator_pattern_compile_001`
// -------------------------------------------------------------------------

/// `pattern "[a"` — unbalanced character class.
#[test]
fn pattern_unbalanced_class_emits_compile_error() {
    let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "[a"
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::InlineValidatorPatternCompile {
            field,
            pattern,
            reason,
        }) => {
            assert_eq!(field, "handle");
            assert_eq!(pattern, "[a");
            assert!(reason.contains("unbalanced `[`"), "reason: {}", reason);
        }
        other => panic!(
            "expected InlineValidatorPatternCompile, got: {:?}",
            other.err()
        ),
    }
}

/// `pattern "^a("` — unbalanced group paren.
#[test]
fn pattern_unbalanced_paren_emits_compile_error() {
    let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^a("
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::InlineValidatorPatternCompile {
            field,
            pattern,
            reason,
        }) => {
            assert_eq!(field, "handle");
            assert_eq!(pattern, "^a(");
            assert!(reason.contains("unbalanced `(`"), "reason: {}", reason);
        }
        other => panic!(
            "expected InlineValidatorPatternCompile, got: {:?}",
            other.err()
        ),
    }
}

/// `pattern "^a)"` — extra closing paren, no matching `(`.
#[test]
fn pattern_extra_closing_paren_emits_compile_error() {
    let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^a)"
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let result = crate::lower_feature_skeleton(&features[0]);
    match result {
        Err(AnalyzeError::InlineValidatorPatternCompile {
            field,
            pattern,
            reason,
        }) => {
            assert_eq!(field, "handle");
            assert_eq!(pattern, "^a)");
            assert!(reason.contains("unbalanced `)`"), "reason: {}", reason);
        }
        other => panic!(
            "expected InlineValidatorPatternCompile, got: {:?}",
            other.err()
        ),
    }
}

/// Sanity: well-formed pattern passes.
#[test]
fn pattern_well_formed_passes() {
    let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^[a-z][a-z0-9-]{2,29}$"
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = crate::lower_feature_skeleton(&features[0]).expect("lowers");
    let field = &feature.resources[0].fields[0];
    assert_eq!(
        field.constraints.pattern.as_deref(),
        Some("^[a-z][a-z0-9-]{2,29}$")
    );
}

// -------------------------------------------------------------------------
// Cross-feature contracts §5.4 — lowering of `uses [<feature>...] [version v<N>]`
// populates parallel `uses` / `uses_spans` / `uses_versions` lists.
// -------------------------------------------------------------------------

#[test]
fn lowers_uses_with_mixed_pins() {
    let source = r#"
feature billing
  uses account version v2
  uses notifications
  uses org, user version v1
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = crate::lower_feature_skeleton(&features[0]).expect("lowers");

    assert_eq!(
        feature.uses,
        vec![
            "account".to_owned(),
            "notifications".to_owned(),
            "org".to_owned(),
            "user".to_owned(),
        ]
    );
    assert_eq!(feature.uses_versions, vec![Some(2), None, Some(1), Some(1)]);
    assert_eq!(feature.uses_spans.len(), 4);
    // First two lines and last line have distinct spans.
    assert_ne!(feature.uses_spans[0], feature.uses_spans[1]);
    assert_ne!(feature.uses_spans[1], feature.uses_spans[2]);
    // Comma-list entries share the source line, hence the span.
    assert_eq!(feature.uses_spans[2], feature.uses_spans[3]);
}

#[test]
fn auto_photo_synthesizes_4_commands_and_2_records() {
    // Inline a minimal feature skeleton with a per-user resource
    // carrying an optional @cap.File field. Expect synthesis to
    // populate feature.commands with 4 names ending in
    // _upload/_upload/_/url and feature.records with the 2
    // intent + display records.
    let source = r#"
feature photoshare
  defaults
    tenancy org

  uses org
  uses account

  policies
    photoshare_only: @scope.authenticated, @role.host
      when_denied @translation.x

  domain
    resource PhotoShare
      org: Org required
      user: User required unique
      avatar: @cap.File(max_size:5mb,accept:image/jpeg,visibility:signed,signed_ttl:1h) optional
      created_at: DateTime required
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = crate::lower_feature_skeleton(&features[0]).expect("lowering succeeds");

    let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
    assert!(
        cmd_names.contains(&"request_avatar_upload"),
        "request_avatar_upload missing; got {:?}",
        cmd_names
    );
    assert!(cmd_names.contains(&"confirm_avatar_upload"));
    assert!(cmd_names.contains(&"clear_avatar"));
    assert!(cmd_names.contains(&"get_avatar_url"));

    let record_names: Vec<&str> = feature.records.iter().map(|r| r.name.as_str()).collect();
    assert!(record_names.contains(&"AvatarUploadIntent"));
    assert!(record_names.contains(&"AvatarDisplayUrl"));

    // Marker must be set on synthesized commands.
    let req = feature
        .commands
        .iter()
        .find(|c| c.name == "request_avatar_upload")
        .unwrap();
    assert!(req.synthesized_from_cap_file.is_some());
}
