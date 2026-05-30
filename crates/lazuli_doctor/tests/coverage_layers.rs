//! Wave 6 — integration tests for the `coverage::*` calculators.
//!
//! Lives in `tests/` (separate compilation unit) so it doesn't pull
//! in the broken `src/vocab/*` unit tests when running `cargo test
//! -p lazuli_doctor`. Tests here exercise only the public API.

use lazuli_doctor::coverage::{
    self, CoverageProfile, LzxViewRef, build_coverage_report, profile_default_thresholds,
    view_extensibility::ViewSnapshot,
};

#[test]
fn empty_features_vacuous_pass() {
    let thresholds = profile_default_thresholds(CoverageProfile::Strict);
    let report = build_coverage_report(&[], &[], CoverageProfile::Strict, &thresholds, None);
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.gate_result.verdict, "pass");
    assert!(report.gate_result.below_block.is_empty());
    assert!(report.gate_result.below_warn.is_empty());
}

#[test]
fn all_six_layers_present_in_report() {
    let thresholds = profile_default_thresholds(CoverageProfile::Production);
    let report = build_coverage_report(&[], &[], CoverageProfile::Production, &thresholds, None);
    for layer in [
        "spec_predicate",
        "spec_actor_matrix",
        "spec_transition_state",
        "view_extensibility",
        "view_e2e_pair",
        "handler_go",
    ] {
        assert!(report.layers.contains_key(layer), "missing layer {layer}");
    }
}

#[test]
fn view_e2e_pair_with_missing_specs_is_uncovered() {
    let tmp = tempfile::tempdir().unwrap();
    let views = vec![
        LzxViewRef {
            experience: "account".to_string(),
            view: "profile".to_string(),
        },
        LzxViewRef {
            experience: "account".to_string(),
            view: "settings".to_string(),
        },
    ];
    let layer = coverage::view_e2e_pair::compute(&views, Some(tmp.path()), None);
    assert_eq!(layer.total, 2);
    assert_eq!(layer.covered, 0);
}

#[test]
fn view_e2e_pair_with_one_present_spec_is_partially_covered() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("e2e").join("account");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("profile.spec.ts"), b"// stub").unwrap();
    let views = vec![
        LzxViewRef {
            experience: "account".to_string(),
            view: "profile".to_string(),
        },
        LzxViewRef {
            experience: "account".to_string(),
            view: "settings".to_string(),
        },
    ];
    let layer = coverage::view_e2e_pair::compute(&views, Some(tmp.path()), None);
    assert_eq!(layer.total, 2);
    assert_eq!(layer.covered, 1);
}

#[test]
fn view_extensibility_from_snapshots_distinguishes_extensible() {
    let snapshots = vec![
        ViewSnapshot {
            experience: "e".to_string(),
            view: "v_static".to_string(),
            extensible: false,
            tests: vec![],
        },
        ViewSnapshot {
            experience: "e".to_string(),
            view: "v_extensible_no_tests".to_string(),
            extensible: true,
            tests: vec![],
        },
        ViewSnapshot {
            experience: "e".to_string(),
            view: "v_extensible_with_accepted".to_string(),
            extensible: true,
            tests: vec!["allows extension customer_tags".to_string()],
        },
        ViewSnapshot {
            experience: "e".to_string(),
            view: "v_extensible_with_rejected".to_string(),
            extensible: true,
            tests: vec!["denies extension billing".to_string()],
        },
    ];
    let layer = coverage::view_extensibility::compute_from_snapshots(&snapshots);
    // Static view excluded; 3 extensible views; 2 covered.
    assert_eq!(layer.total, 3);
    assert_eq!(layer.covered, 2);
}

#[test]
fn handler_go_parses_coverprofile() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("coverage.out"),
        "mode: set\n\
         github.com/x/foo.go:1.0,3.0 7 1\n\
         github.com/x/foo.go:4.0,6.0 3 0\n\
         github.com/x/bar.go:1.0,2.0 5 4\n",
    )
    .unwrap();
    let layer = coverage::handler_go::compute(Some(tmp.path()));
    assert_eq!(layer.total, 15);
    assert_eq!(layer.covered, 12);
    assert_eq!(layer.source.as_deref(), Some("go-coverprofile"));
    assert!(layer.raw_file.as_deref().unwrap().contains("coverage.out"));
}

#[test]
fn handler_go_missing_coverprofile_is_vacuous_pass() {
    let tmp = tempfile::tempdir().unwrap();
    let layer = coverage::handler_go::compute(Some(tmp.path()));
    assert_eq!(layer.total, 0);
    assert_eq!(layer.covered, 0);
    assert!(layer.raw_file.as_deref().unwrap().starts_with("absent:"));
}

#[test]
fn production_profile_thresholds_block_under_50() {
    let prod = profile_default_thresholds(CoverageProfile::Production);
    assert_eq!(prod.get("spec_predicate").unwrap().block_under, 50);
    assert_eq!(prod.get("spec_actor_matrix").unwrap().block_under, 70);
    assert_eq!(prod.get("view_e2e_pair").unwrap().block_under, 30);
}

#[test]
fn prototype_profile_thresholds_are_zero() {
    let proto = profile_default_thresholds(CoverageProfile::Prototype);
    for (_, t) in proto.per_layer.iter() {
        assert_eq!(t.block_under, 0);
        assert_eq!(t.warn_under, 0);
    }
}

#[test]
fn strict_profile_is_warn_only() {
    let strict = profile_default_thresholds(CoverageProfile::Strict);
    for (_, t) in strict.per_layer.iter() {
        assert_eq!(t.block_under, 0, "strict must never block");
        assert!(t.warn_under > 0, "strict must warn somewhere");
    }
}

#[test]
fn coverprofile_parser_skips_mode_line() {
    let contents = "mode: count\n";
    let (covered, total) = coverage::handler_go::parse_coverprofile(contents);
    assert_eq!(total, 0);
    assert_eq!(covered, 0);
}
