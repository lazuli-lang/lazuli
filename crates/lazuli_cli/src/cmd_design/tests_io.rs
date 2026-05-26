//! Integration tests for the design command — `import` / `export` /
//! `diff` paths plus format sniffing and error surfaces.
//!
//! Sibling of the codec round-trip tests in `mod.rs`. Shared fixtures
//! live in `tests_fixtures.rs`.

use std::fs;

use serde_json::{Value, json};

use super::tests_fixtures::{demo_fixture, unique_temp_dir};
use super::{
    Design, ExportTarget, ImportFormat, Motion, ScaleToken, Typography, compute_diff,
    design_to_figma, diff_with_format, export, figma_to_design, import, read_design, sniff_format,
    write_design,
};

#[test]
fn import_overwrite_false_fails_when_design_exists() {
    let tmp = unique_temp_dir("import-overwrite-false");
    let out = tmp.join("design.lzi");
    write_design(&out, &demo_fixture()).unwrap();

    let external = tmp.join("tokens.figma.json");
    fs::write(
        &external,
        serde_json::to_string_pretty(&design_to_figma(&demo_fixture())).unwrap(),
    )
    .unwrap();

    let err = import(&external, ImportFormat::Figma, &out, false).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("--overwrite"),
        "expected --overwrite hint in error, got: {msg}"
    );

    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn import_overwrite_true_rewrites_design() {
    let tmp = unique_temp_dir("import-overwrite-true");
    let out = tmp.join("design.lzi");
    // Seed `design.lzi` with a minimal Design.
    let initial = Design {
        name: "old".to_string(),
        extends: None,
        colors: Vec::new(),
        typography: Typography::default(),
        spaces: Vec::new(),
        radii: Vec::new(),
        shadows: Vec::new(),
        motion: Motion::default(),
        breakpoints: Vec::new(),
        z_indices: Vec::new(),
    };
    write_design(&out, &initial).unwrap();

    let external = tmp.join("tokens.figma.json");
    fs::write(
        &external,
        serde_json::to_string_pretty(&design_to_figma(&demo_fixture())).unwrap(),
    )
    .unwrap();

    import(&external, ImportFormat::Figma, &out, true).unwrap();

    let after = read_design(&out).unwrap();
    assert!(
        !after.colors.is_empty(),
        "import should have replaced design"
    );
    assert!(after.spaces.iter().any(|t| t.name == "1"));

    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn diff_detects_added_token() {
    let current = demo_fixture();
    let mut incoming = current.clone();
    incoming.spaces.push(ScaleToken {
        name: "8".to_string(),
        value: "2rem".to_string(),
    });

    let report = compute_diff(&current, &incoming);
    assert_eq!(report.added, vec!["space.8".to_string()]);
    assert!(report.removed.is_empty());
    assert!(report.changed.is_empty());
}

#[test]
fn diff_detects_removed_token() {
    let current = demo_fixture();
    let mut incoming = current.clone();
    incoming.spaces.retain(|t| t.name != "1");

    let report = compute_diff(&current, &incoming);
    assert_eq!(report.removed, vec!["space.1".to_string()]);
    assert!(report.added.is_empty());
    assert!(report.changed.is_empty());
}

#[test]
fn diff_detects_value_change() {
    let current = demo_fixture();
    let mut incoming = current.clone();
    for tok in &mut incoming.spaces {
        if tok.name == "4" {
            tok.value = "0.875rem".to_string();
        }
    }

    let report = compute_diff(&current, &incoming);
    assert!(report.added.is_empty());
    assert!(report.removed.is_empty());
    assert_eq!(report.changed.len(), 1);
    let change = &report.changed[0];
    assert_eq!(change.path, "space.4");
    assert_eq!(change.from_value, "1rem");
    assert_eq!(change.to_value, "0.875rem");
}

#[test]
fn diff_against_identical_json_is_empty() {
    let tmp = unique_temp_dir("diff-identical");
    let design = demo_fixture();
    let external = tmp.join("tokens.figma.json");
    fs::write(
        &external,
        serde_json::to_string_pretty(&design_to_figma(&design)).unwrap(),
    )
    .unwrap();

    let report = diff_with_format(&external, ImportFormat::Figma, &design).unwrap();
    assert!(
        report.is_empty(),
        "expected empty diff, got: {}",
        report.render()
    );
    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn empty_groups_do_not_crash_export() {
    let tmp = unique_temp_dir("empty-groups");
    let out = tmp.join("tokens.figma.json");
    let empty = Design {
        name: "empty".to_string(),
        extends: None,
        colors: Vec::new(),
        typography: Typography::default(),
        spaces: Vec::new(),
        radii: Vec::new(),
        shadows: Vec::new(),
        motion: Motion::default(),
        breakpoints: Vec::new(),
        z_indices: Vec::new(),
    };
    export(&out, ExportTarget::Figma, &empty).unwrap();
    let raw = fs::read_to_string(&out).unwrap();
    let parsed: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed.as_object().unwrap().len(), 0);

    // Style Dictionary path too.
    let sd_path = tmp.join("tokens.sd.json");
    export(&sd_path, ExportTarget::StyleDictionary, &empty).unwrap();
    let sd_raw = fs::read_to_string(&sd_path).unwrap();
    let sd_parsed: Value = serde_json::from_str(&sd_raw).unwrap();
    assert_eq!(sd_parsed.as_object().unwrap().len(), 0);

    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn invalid_json_input_returns_err() {
    let tmp = unique_temp_dir("invalid-json");
    let bad = tmp.join("garbage.figma.json");
    fs::write(&bad, "{ not valid json").unwrap();

    let out = tmp.join("design.lzi");
    let err = import(&bad, ImportFormat::Figma, &out, true).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("parsing JSON") || msg.contains("expected"),
        "expected parse error message, got: {msg}"
    );

    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn export_writes_deterministic_sorted_output() {
    // Two consecutive exports of the same Design must produce
    // byte-identical files — matches `lazuli generate go` discipline.
    let tmp = unique_temp_dir("deterministic");
    let design = demo_fixture();
    let a = tmp.join("a.figma.json");
    let b = tmp.join("b.figma.json");
    export(&a, ExportTarget::Figma, &design).unwrap();
    export(&b, ExportTarget::Figma, &design).unwrap();
    let a_raw = fs::read_to_string(&a).unwrap();
    let b_raw = fs::read_to_string(&b).unwrap();
    assert_eq!(a_raw, b_raw, "export must be deterministic");
    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn sniff_format_detects_figma_and_style_dictionary() {
    let tmp = unique_temp_dir("sniff");
    let figma_path = tmp.join("tokens.figma.json");
    let figma_doc = json!({
        "color": {
            "primary": { "$value": "#7c3aed", "$type": "color" }
        }
    });
    fs::write(&figma_path, serde_json::to_string(&figma_doc).unwrap()).unwrap();
    assert_eq!(sniff_format(&figma_path).unwrap(), ImportFormat::Figma);

    let sd_path = tmp.join("tokens.sd.json");
    let sd_doc = json!({
        "color": {
            "primary": { "value": "#7c3aed", "type": "color" }
        }
    });
    fs::write(&sd_path, serde_json::to_string(&sd_doc).unwrap()).unwrap();
    assert_eq!(
        sniff_format(&sd_path).unwrap(),
        ImportFormat::StyleDictionary
    );

    let _ = fs::remove_dir_all(tmp);
}

#[test]
fn unknown_color_state_is_rejected() {
    let bad = json!({
        "color": {
            "primary": {
                "base":  { "$value": "#7c3aed", "$type": "color" },
                "weird": { "$value": "#000000", "$type": "color" }
            }
        }
    });
    let err = figma_to_design(&bad).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown state") || msg.contains("closed catalog"),
        "expected closed-catalog rejection, got: {msg}"
    );
}
