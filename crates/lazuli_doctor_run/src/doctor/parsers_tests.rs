// Inline tests for `parsers.rs` — included via
// `#[cfg(test)] mod tests { include!("parsers_tests.rs"); }` so the host
// file stays under the 500-LOC ceiling. This is a SPEC-19 split fragment;
// the internal-hygiene audits attribute it to `parsers.rs`.

use super::*;

#[test]
fn major_minor_drops_patch() {
    assert_eq!(major_minor("0.5.7"), "0.5");
    assert_eq!(major_minor("1.2.0-rc1"), "1.2");
}

#[test]
fn major_minor_passes_through_when_short() {
    assert_eq!(major_minor("1.2"), "1.2");
    assert_eq!(major_minor("3"), "3");
    assert_eq!(major_minor(""), "");
}

#[test]
fn is_one_dot_zero_plus_threshold() {
    assert!(is_one_dot_zero_plus("1.0.0"));
    assert!(is_one_dot_zero_plus("2.3"));
    assert!(!is_one_dot_zero_plus("0.9.9"));
    assert!(!is_one_dot_zero_plus("garbage"));
}

#[test]
fn lzi_and_lzx_extension_dispatch() {
    assert!(is_lzi_path(Path::new("features/billing/billing.lzi")));
    assert!(!is_lzi_path(Path::new("features/billing/billing.lzx")));
    assert!(is_lzx_path(Path::new("ui.lzx")));
    assert!(!is_lzx_path(Path::new("ui.lzi")));
}

#[test]
fn normalise_path_collapses_param_slots() {
    assert_eq!(normalise_path("/users/:id"), "/users/:_");
    assert_eq!(normalise_path("/users/:userId"), "/users/:_");
    assert_eq!(normalise_path("/health"), "/health");
}

#[test]
fn parse_iso_date_accepts_well_formed_and_rejects_garbage() {
    assert_eq!(parse_iso_date("2026-05-29"), Some((2026, 5, 29)));
    assert_eq!(parse_iso_date("  2026-05-29 "), Some((2026, 5, 29)));
    assert_eq!(parse_iso_date("2026-13-01"), None);
    assert_eq!(parse_iso_date("not-a-date"), None);
}

#[test]
fn duration_and_size_parsers_catch_typos() {
    assert!(is_parseable_duration("30s"));
    assert!(is_parseable_duration("500ms"));
    assert!(!is_parseable_duration("30"));
    assert!(!is_parseable_duration(""));

    assert!(is_parseable_size("16kb"));
    assert!(is_parseable_size("512b"));
    assert!(!is_parseable_size("16"));
    assert!(!is_parseable_size("kb"));
}
