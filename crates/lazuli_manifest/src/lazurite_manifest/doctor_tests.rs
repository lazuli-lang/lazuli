//! Tests for `[doctor.*]` parsing — Wave 0.5 (severity overrides),
//! Wave 1.5 (test_discipline preset), Wave 6 (coverage thresholds +
//! preset). Exercises the full `Manifest` → `Doctor` path because
//! these blocks are only meaningful in the context of a parsed
//! `Lazurite.toml` file.

#![cfg(test)]

use super::{Manifest, ManifestError};

fn parse_manifest(contents: &str) -> Result<Manifest, ManifestError> {
    let manifest: Manifest = toml::from_str(contents)?;
    manifest.validate()?;
    Ok(manifest)
}

/// Wave 0.5 — `[doctor.test_discipline]` parses with no
/// per-rule overrides authored. Most projects will start here.
#[test]
fn parse_empty_doctor_block() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor]
profile = "strict"

[doctor.test_discipline]
"#,
    )
    .unwrap();

    let doctor = manifest.doctor.expect("doctor block parsed");
    assert_eq!(doctor.profile.as_deref(), Some("strict"));
    let td = doctor
        .test_discipline
        .expect("test_discipline block parsed");
    assert!(td.severity_override.is_empty());
}

/// Wave 0.5 — per-rule severity overrides with `reason` lift
/// cleanly. Whether the `reason` is blank or missing is a
/// `DOCTOR-OVERRIDE-NEEDS-REASON-001` concern, not a parse error.
#[test]
fn parse_doctor_severity_override_with_reason() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor.test_discipline.severity_override.TEST-MISSING-AUTHORED-001]
severity = "warning"
reason = "legacy billing feature; refactor scheduled Q3"

[doctor.test_discipline.severity_override.TEST-PREDICATE-UNCOVERED-001]
severity = "info"
"#,
    )
    .unwrap();

    let td = manifest
        .doctor
        .and_then(|d| d.test_discipline)
        .expect("test_discipline parsed");
    let with_reason = &td.severity_override["TEST-MISSING-AUTHORED-001"];
    assert_eq!(with_reason.severity, "warning");
    assert_eq!(
        with_reason.reason.as_deref(),
        Some("legacy billing feature; refactor scheduled Q3")
    );
    let without_reason = &td.severity_override["TEST-PREDICATE-UNCOVERED-001"];
    assert_eq!(without_reason.severity, "info");
    assert!(without_reason.reason.is_none());
}

/// Wave 6 — `[doctor.coverage]` parses per-layer thresholds + the
/// optional aggregate-method disclosure.
#[test]
fn parse_doctor_coverage_section() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor.coverage]
spec_predicate     = { block_under = 50, warn_under = 80 }
spec_actor_matrix  = { block_under = 70, warn_under = 90 }
aggregate_method   = "weighted-by-construct-count"
"#,
    )
    .unwrap();

    let doctor = manifest.doctor.expect("doctor section present");
    let coverage = doctor.coverage.expect("coverage section present");
    assert_eq!(
        coverage.aggregate_method.as_deref(),
        Some("weighted-by-construct-count")
    );
    let sp = coverage
        .per_layer
        .get("spec_predicate")
        .expect("spec_predicate entry");
    assert_eq!(sp.block_under, 50);
    assert_eq!(sp.warn_under, 80);
    let sa = coverage
        .per_layer
        .get("spec_actor_matrix")
        .expect("spec_actor_matrix entry");
    assert_eq!(sa.block_under, 70);
    assert_eq!(sa.warn_under, 90);
}

/// Frente 1 — `[doctor.coverage] preset = "<name>"` parses.
#[test]
fn parse_doctor_coverage_with_preset() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor.coverage]
preset = "tdd-strict"
"#,
    )
    .unwrap();

    let coverage = manifest
        .doctor
        .and_then(|d| d.coverage)
        .expect("coverage section");
    assert_eq!(coverage.preset.as_deref(), Some("tdd-strict"));
    assert!(coverage.per_layer.is_empty());
}

/// Frente 1 — preset + per-layer overrides coexist; preset is
/// captured as a string and individual layers as their own
/// `LayerThresholdConfig` entries.
#[test]
fn parse_doctor_coverage_preset_with_overrides() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor.coverage]
preset = "tdd-strict"

[doctor.coverage.handler_go]
block_under = 70
warn_under = 80
"#,
    )
    .unwrap();

    let coverage = manifest
        .doctor
        .and_then(|d| d.coverage)
        .expect("coverage section");
    assert_eq!(coverage.preset.as_deref(), Some("tdd-strict"));
    let handler = coverage
        .per_layer
        .get("handler_go")
        .expect("handler_go entry");
    assert_eq!(handler.block_under, 70);
    assert_eq!(handler.warn_under, 80);
}
