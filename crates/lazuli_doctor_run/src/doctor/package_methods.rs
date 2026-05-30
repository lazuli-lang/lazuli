//! Additional `impl DoctorPackage` blocks for the layered-coverage and
//! `VOCAB-CONTEXT-*` dispatches that don't fit inside the core
//! `package.rs` (which already owns the load + standard `diagnostics()`
//! path). Kept in a sibling file so each concern stays under the
//! per-file LOC budget; both blocks see the same private fields via
//! `super`.

use std::collections::BTreeSet;

use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor_config::{
    DoctorProfile as SecurityProfile, effective_severity, effective_severity_over_base,
};
use lazuli_syntax::parse_feature_skeletons;

use super::parsers::is_lzi_path;
use super::{DoctorDiagnostic, DoctorPackage, DoctorSeverity, RuleCategory};

include!("package_methods_impl1.rs");
include!("package_methods_impl2.rs");

// Today's date as an ISO `YYYY-MM-DD` string, for the `VOCAB-KNOWLEDGE-STALE-001`
// `revalidate_by` comparison.
//
// Derived from the system clock with a stdlib-only civil-date conversion
// (Howard Hinnant's `days -> y/m/d` algorithm) — no `chrono`/`time`
// dependency, keeping the helper wire-thin per the founding principle. The
// rule does the lexical compare; this just supplies the reference date the
// same way the doctor walker would pass `ctx.now`'s date. Falls back to the
// canonical dev pivot if the clock is before the Unix epoch (unreachable in
// practice).
fn current_iso_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if secs == 0 {
        // Pre-epoch / clock unreadable: anchor at the Lazuli dev pivot so the
        // rule stays deterministic rather than firing on a bogus date.
        return "2026-05-29".to_string();
    }
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

// Convert a count of days since the Unix epoch (1970-01-01) into a
// `(year, month, day)` Gregorian date. Hinnant's branch-free algorithm;
// valid for the full proleptic Gregorian range.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    // Smoke pairing — the methods in this file dispatch into rich
    // analyzer state built by the parent `DoctorPackage`, which the
    // unit tests under `crates/lazuli_cli/tests` already cover end-to-
    // end. We just guard against the public surface disappearing.
    use super::*;

    #[test]
    fn impl_block_compiles() {
        let _ = DoctorPackage::coverage_report;
        let _ = DoctorPackage::knowledge_vocab_diagnostics;
    }

    #[test]
    fn civil_from_days_known_anchors() {
        // Epoch and a few well-known dates.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(18_993), (2022, 1, 1));
        assert_eq!(civil_from_days(20_602), (2026, 5, 29));
        assert_eq!(civil_from_days(20_605), (2026, 6, 1));
    }

    #[test]
    fn current_iso_date_is_well_formed() {
        let s = current_iso_date();
        assert_eq!(s.len(), 10, "YYYY-MM-DD is 10 chars: {s}");
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(s.as_bytes()[7], b'-');
        // Sanity: year is in the plausible modern range.
        let year: i64 = s[0..4].parse().expect("year parses");
        assert!((2020..2100).contains(&year), "year out of range: {year}");
    }
}
