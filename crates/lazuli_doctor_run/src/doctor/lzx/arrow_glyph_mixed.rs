//! `LZX-ARROW-GLYPH-MIXED-001` — a single `.lzx`/`.lzi` file mixes the
//! Unicode arrow (`→`, U+2192) and its ASCII fallback (`->`) on
//! arrow-bearing lines.
//!
//! Fires when one file uses BOTH glyphs across its arrow-bearing
//! constructs (resume arms, `tab -> view`, `tab_group` `case`s,
//! `when_denied_route` arms — every line `split_lzx_arrow` in
//! `lazuli_syntax::parser::common` accepts). The two glyphs are exactly
//! equivalent to the parser, so mixing them is purely an editorial wart:
//! a cold-reading author (or a diff reviewer) sees two spellings of one
//! operator in the same file and has to wonder whether the difference is
//! load-bearing. It isn't.
//!
//! This is the `.lzx` twin of [`super::cells_mixed_form`] — the "do not
//! mix two equivalent forms" precedent. Where `cells-mixed-form` rejects
//! two equivalent *render* spellings in one view, this rejects two
//! equivalent *arrow* spellings in one file.
//!
//! Severity: profile-scoped — `strict` warns, `iron-hand` errors (see
//! [`Finding::severity_for_profile`]). Scope is ONE file: an
//! internally-consistent file (all `→`, or all `->`) never fires; the
//! rule only speaks up when a file is self-inconsistent. The canonical
//! glyph is the ASCII `->`.
//!
//! ## Example fires
//!
//! ```text
//! resume customer_status
//!   none -> view list      # ASCII
//!   active → view detail    # Unicode — mixes with the ASCII arm above
//! ```
//!
//! ## Example silent
//!
//! ```text
//! resume customer_status
//!   none -> view list       # all ASCII — internally consistent
//!   active -> view detail
//! ```
//!
//! ## Inline allow
//!
//! `# doctor:allow LZX-ARROW-GLYPH-MIXED-001 — reason "..."` anywhere in
//! the file suppresses the finding (whole-file opt-out, mirroring
//! `super::super::super` `LZI-FILE-SIZE-001`). The rule is advisory
//! hygiene; the escape hatch is the canonical one.

use std::path::{Path, PathBuf};

use lazuli_doctor::DoctorSeverity;
use lazuli_doctor::allow_comment::source_contains_doctor_allow;

/// The doctor profile a rule's severity is resolved against. `strict`
/// warns advisory-hygiene rules; `iron-hand` escalates them to a
/// merge-blocking error. Every other profile defers to `strict`'s
/// answer for this rule (it carries no inherent error band of its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// `[doctor] profile = "strict"` — advisory warning.
    Strict,
    /// `[doctor] profile = "iron-hand"` — merge-blocking error.
    IronHand,
}

/// One `LZX-ARROW-GLYPH-MIXED-001` finding — exactly one per
/// self-inconsistent file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path of the offending `.lzx`/`.lzi` file.
    pub path: PathBuf,
    /// 1-based line of the first NON-canonical (`→`) arrow — the line a
    /// fix should rewrite to `->`.
    pub line: usize,
    /// Rendered diagnostic body.
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "LZX-ARROW-GLYPH-MIXED-001";

    /// Render the canonical diagnostic message. `unicode_line` /
    /// `ascii_line` are the first 1-based lines carrying each glyph, so
    /// the body can point the author at both spellings.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = Finding::message(94, 92);
    /// ```
    pub fn message(unicode_line: usize, ascii_line: usize) -> String {
        format!(
            "file mixes the Unicode arrow `→` (first at line {unicode_line}) and \
             the ASCII arrow `->` (first at line {ascii_line}) on arrow-bearing \
             lines (resume arms, `tab -> view`, `tab_group` cases, \
             `when_denied_route` arms). The two glyphs are equivalent to the \
             parser, so mixing them is an editorial wart, not a semantic \
             difference. Fix by rewriting every `→` to the canonical ASCII \
             `->` (or, if you must, opt out with `# doctor:allow \
             LZX-ARROW-GLYPH-MIXED-001 — reason \"...\"`)."
        )
    }

    /// Resolve this finding's severity for the active doctor profile:
    /// `strict` → `Warning`, `iron-hand` → `Error`. Hygiene rules carry
    /// no inherent error band; the profile decides whether the wart
    /// blocks a merge.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let f = Finding { path: "x.lzx".into(), line: 94, message: String::new() };
    /// // assert_eq!(f.severity_for_profile(Profile::Strict), DoctorSeverity::Warning);
    /// // assert_eq!(f.severity_for_profile(Profile::IronHand), DoctorSeverity::Error);
    /// ```
    pub fn severity_for_profile(&self, profile: Profile) -> DoctorSeverity {
        match profile {
            Profile::Strict => DoctorSeverity::Warning,
            Profile::IronHand => DoctorSeverity::Error,
        }
    }
}

/// True when `line` carries the ASCII arrow `->`. The Unicode arrow `→`
/// is a distinct 3-byte sequence and never contains the ASCII pair, so
/// the two probes are independent.
fn has_ascii_arrow(line: &str) -> bool {
    line.contains("->")
}

/// True when `line` carries the Unicode arrow `→` (U+2192).
fn has_unicode_arrow(line: &str) -> bool {
    line.contains('\u{2192}')
}

/// Run `LZX-ARROW-GLYPH-MIXED-001` over one file's source.
///
/// Scans every line for the two arrow glyphs. Returns a single finding
/// iff BOTH glyphs appear somewhere in the file (self-inconsistency);
/// an internally-consistent file (all `→`, all `->`, or no arrows)
/// returns `None`. A whole-file `# doctor:allow LZX-ARROW-GLYPH-MIXED-001`
/// comment suppresses the finding.
///
/// The finding's `line` is the first NON-canonical (`→`) arrow line, so
/// the fix target points at the glyph to rewrite.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor_run::doctor::lzx::arrow_glyph_mixed::check;
///
/// // let finding = check("none -> a\nactive → b\n", Path::new("x.lzx"));
/// ```
pub fn check(source: &str, path: &Path) -> Option<Finding> {
    let mut first_ascii: Option<usize> = None;
    let mut first_unicode: Option<usize> = None;

    for (idx, line) in source.lines().enumerate() {
        let lineno = idx + 1;
        if first_ascii.is_none() && has_ascii_arrow(line) {
            first_ascii = Some(lineno);
        }
        if first_unicode.is_none() && has_unicode_arrow(line) {
            first_unicode = Some(lineno);
        }
    }

    let (ascii_line, unicode_line) = match (first_ascii, first_unicode) {
        // Both glyphs present → the file is self-inconsistent.
        (Some(a), Some(u)) => (a, u),
        // Internally consistent (one glyph or none) — never fires.
        _ => return None,
    };

    // Whole-file opt-out — honor the canonical advisory escape hatch.
    if source_contains_doctor_allow(source, Finding::CODE) {
        return None;
    }

    Some(Finding {
        path: path.to_path_buf(),
        // Point at the non-canonical glyph: that's the line to rewrite.
        line: unicode_line,
        message: Finding::message(unicode_line, ascii_line),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "features/customer/customer.lzx";

    #[test]
    fn violation_mixed_glyphs_in_one_file_fires() {
        // Mirrors the shape of the canonical full-capsule resume block
        // before its 2026-05 cleanup: ASCII arms plus one stray Unicode
        // arm.
        let src = "\
resume customer_status
  source query.lookup by_id
  none -> view list
  lead -> view list
  active → view detail
  paused -> view list
  * -> view list
";
        let finding = check(src, Path::new(PATH)).expect("mixed glyphs must fire");
        assert_eq!(finding.path, PathBuf::from(PATH));
        // First Unicode arrow is on the `active → view detail` line (5).
        assert_eq!(finding.line, 5);
        assert!(finding.message.contains("line 5")); // unicode first-line
        assert!(finding.message.contains("line 3")); // ascii first-line
        assert!(finding.message.contains("LZX-ARROW-GLYPH-MIXED-001"));
        assert!(finding.message.contains('\u{2192}'));
    }

    #[test]
    fn non_violation_all_ascii_is_silent() {
        let src = "\
resume customer_status
  none -> view list
  active -> view detail
  * -> view list
";
        assert!(check(src, Path::new(PATH)).is_none());
    }

    #[test]
    fn non_violation_all_unicode_is_silent() {
        // Internally consistent the OTHER way — still silent. Scope is
        // one file; we do not impose the canonical glyph on a file that
        // is uniform, only flag mixing.
        let src = "\
resume customer_status
  none → view list
  active → view detail
  * → view list
";
        assert!(check(src, Path::new(PATH)).is_none());
    }

    #[test]
    fn non_violation_no_arrows_is_silent() {
        let src = "experience customer\n  view login\n  view dashboard\n";
        assert!(check(src, Path::new(PATH)).is_none());
    }

    #[test]
    fn inline_allow_suppresses_the_finding() {
        let src = "\
# doctor:allow LZX-ARROW-GLYPH-MIXED-001 — reason \"migrating glyphs incrementally\"
resume customer_status
  none -> view list
  active → view detail
";
        assert!(
            check(src, Path::new(PATH)).is_none(),
            "whole-file allow comment must suppress the finding"
        );
    }

    #[test]
    fn inline_allow_for_a_different_code_does_not_suppress() {
        let src = "\
# doctor:allow LZX-CELLS-MIXED-FORM-001 — reason \"unrelated\"
resume customer_status
  none -> view list
  active → view detail
";
        assert!(
            check(src, Path::new(PATH)).is_some(),
            "an allow comment for another rule must NOT suppress this one"
        );
    }

    #[test]
    fn severity_is_profile_scoped() {
        let f = Finding {
            path: PathBuf::from(PATH),
            line: 5,
            message: String::new(),
        };
        assert_eq!(
            f.severity_for_profile(Profile::Strict),
            DoctorSeverity::Warning
        );
        assert_eq!(
            f.severity_for_profile(Profile::IronHand),
            DoctorSeverity::Error
        );
    }

    #[test]
    fn finding_line_points_at_unicode_even_when_ascii_comes_later() {
        // Unicode arrow appears BEFORE the ASCII one; the fix target is
        // still the non-canonical (Unicode) glyph.
        let src = "\
resume s
  a → view x
  b -> view y
";
        let finding = check(src, Path::new(PATH)).expect("must fire");
        assert_eq!(finding.line, 2, "points at the `→` line, not the `->` line");
    }
}
