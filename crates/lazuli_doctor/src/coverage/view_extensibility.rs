//! `view_extensibility` coverage layer.
//!
//! Walks every `ExperienceView` lifted from `.lzx` files (carried on
//! `Feature.surfaces[i].audiences[j].views` and similar; today the
//! tests metadata lives on the syntax-level `LzxExperienceView.tests:
//! Vec<String>` since Wave 4 has not landed an `accepted by /
//! rejected by` typed enum yet — coverage works against the raw
//! string slot until Wave 4 widens the surface).
//!
//! Per Wave 6.2 row 4 (canonical metric table):
//!
//! | Denominator | Numerator |
//! | --- | --- |
//! | views with `extensible_by` declared | views with ≥1 `accepted by` OR `rejected by` assertion |
//!
//! This layer counts the **lifted** `Feature.experience` view shape
//! (see `lazuli_ir::ExperienceView`). The `.lzx`-side
//! `LzxExperienceView` is what the doctor package parses today; the
//! IR's `ExperienceView` is not actually populated by current
//! pipelines, so a fallback to the syntax-level slot is wired by the
//! doctor's caller (see `view_e2e_pair`'s `LzxViewRef` parallel for
//! the same boundary handling).

use lazuli_ir::Feature;

use super::LayerCoverage;

/// View descriptor surfaced from the doctor pipeline. Has the
/// extensibility hint and `tests` strings already projected so this
/// calculator stays IR-version-agnostic.
#[derive(Debug, Clone)]
pub struct ViewSnapshot {
    /// Experience surface that hosts the view.
    pub experience: String,
    /// View name as authored in `.lzx`.
    pub view: String,
    /// `true` when the view declares an `extensible_by` hint.
    pub extensible: bool,
    /// `accepted by <feature>` / `rejected by <feature>` assertion
    /// text, verbatim from `.lzx`.
    pub tests: Vec<String>,
}

/// Direct IR variant: walk `Feature` for any embedded view shapes.
/// Kept for parity with the other calculators; in practice the doctor
/// caller passes a pre-built `Vec<ViewSnapshot>` since `.lzx`
/// experiences are parsed outside the per-feature lower step.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::coverage::view_extensibility::compute;
///
/// // The IR slot isn't populated by current pipelines, so the
/// // calculator returns a vacuous-pass layer until `.lzx` walking
/// // lifts views into `Feature.surfaces`.
/// let layer = compute(&[]);
/// assert_eq!(layer.total, 0);
/// ```
pub fn compute(features: &[Feature]) -> LayerCoverage {
    let _ = features; // legacy lowering leaves the IR slot empty
    // Doctor wires the calculator via `compute_from_snapshots` once
    // it has flattened the `.lzx` documents into `ViewSnapshot`s.
    LayerCoverage::new(0, 0).with_source("ir-walk")
}

/// Compute the layer from a pre-built snapshot vec. Only `extensible`
/// snapshots enter the denominator; coverage counts when any test
/// string starts with `accepted by ` or `rejected by `. Emits
/// `source = "lzx-walk"`.
pub fn compute_from_snapshots(snapshots: &[ViewSnapshot]) -> LayerCoverage {
    let mut total = 0usize;
    let mut covered = 0usize;
    for v in snapshots {
        if !v.extensible {
            continue;
        }
        total += 1;
        let has_assertion = v
            .tests
            .iter()
            .any(|t| is_accepted_by(t) || is_rejected_by(t));
        if has_assertion {
            covered += 1;
        }
    }
    LayerCoverage::new(covered, total).with_source("lzx-walk")
}

fn is_accepted_by(s: &str) -> bool {
    s.trim_start().starts_with("accepted by ")
}

fn is_rejected_by(s: &str) -> bool {
    s.trim_start().starts_with("rejected by ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(view: &str, extensible: bool, tests: Vec<&str>) -> ViewSnapshot {
        ViewSnapshot {
            experience: "e".to_string(),
            view: view.to_string(),
            extensible,
            tests: tests.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn non_extensible_view_excluded() {
        let v = snap("post_card", false, Vec::new());
        let l = compute_from_snapshots(&[v]);
        assert_eq!(l.total, 0);
    }

    #[test]
    fn extensible_with_no_assertions_uncovered() {
        let v = snap("post_card", true, Vec::new());
        let l = compute_from_snapshots(&[v]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 0);
    }

    #[test]
    fn extensible_with_accepted_by_covered() {
        let v = snap("post_card", true, vec!["accepted by customer_tags"]);
        let l = compute_from_snapshots(&[v]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 1);
    }

    #[test]
    fn extensible_with_rejected_by_covered() {
        let v = snap("post_card", true, vec!["rejected by billing"]);
        let l = compute_from_snapshots(&[v]);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 1);
    }
}
