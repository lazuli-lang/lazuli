//! `ESC-RAWSQL-IN-HANDLER-001` — raw SQL hidden in a `@fn` Go handler.
//!
//! ## What it checks
//!
//! For each `features/<f>/handlers/*.go`, scan for a multi-line raw SQL
//! read call (`db.Query(`, `db.QueryRow(`, `lazuli.DB().Query(`,
//! `tx.Query(`, ...) carrying a multi-line string literal. Cross-reference
//! the feature's lowered `.lzi`: fire when the matching read (by handler
//! file stem) is NOT declared as a visible `query.sql`/`query.list`/
//! `query.lookup` — i.e. the read exists only as an opaque `@fn` Go body.
//! To `lazuli inspect`, the exposure checker, and the LSP, such a read
//! does not exist.
//!
//! ## Severity
//!
//! `Warning` (waivable-to-convert, NOT waivable-to-silence — ADR 0010,
//! Decision 1). A `# doctor:allow ESC-RAWSQL-IN-HANDLER-001` is honored
//! mechanically by the uniform allow-comment contract, but the diagnostic
//! body states the only correct resolution is to *convert* the read into a
//! declared `query.sql`/`query.compose`; a bare silence records debt, it
//! does not resolve the finding.
//!
//! ## Trigger cue / fixture
//!
//! Fires when a handler runs raw SQL for a read the `.lzi` declares only
//! as an opaque `@fn`. The canonical fixture is grounded on hostpoint
//! `features/trust/handlers/list_property_reviews.go` (a multi-JOIN review
//! read declared as `fn list_property_reviews: Function[...]`); see
//! `rawsql_hidden_in_handler_fires`.
//!
//! ## Example silent
//!
//! The same handler when the feature declares a `query.sql` for that read
//! — the escape is then visible to a cold `.lzi` audit; see
//! `declared_query_sql_silent`.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use crate::allow_comment::source_contains_doctor_allow;

/// One hidden-raw-SQL finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path to the Go handler that hides the raw SQL read.
    pub handler_path: PathBuf,
    /// Feature owning the handler.
    pub feature: String,
    /// Handler stem (the read name), e.g. `list_property_reviews`.
    pub read_name: String,
    /// True if a `# doctor:allow` was present (the finding still fires; the
    /// flag only enriches the body to mark the silence as debt).
    pub waived: bool,
}

impl Finding {
    /// Rule code.
    pub const CODE: &'static str = "ESC-RAWSQL-IN-HANDLER-001";

    /// The verbatim "convert, don't silence" sentence appended to every
    /// body. Pinned as a const so the grader convention can match it.
    pub const CONVERT_NOTE: &'static str = "Convert this read into a declared \
`query.sql`/`query.compose` (with `returns`, `policy`, `params`) so it is visible to a cold \
`.lzi` audit — see docs/lazuli_way/escape-hatch-decision-tree.md. A bare `# doctor:allow` \
records debt, it does NOT resolve the finding.";

    /// Human-readable diagnostic message.
    pub fn message(&self) -> String {
        let lede = format!(
            "handler `{}` runs raw SQL for read `{}` in feature `{}`, but the `.lzi` declares \
it only as an opaque `@fn` (no `query.sql`/`returns`) — the read is invisible to a cold \
`.lzi` audit. {}",
            self.handler_path.display(),
            self.read_name,
            self.feature,
            Self::CONVERT_NOTE,
        );
        if self.waived {
            format!(
                "{lede} (a `# doctor:allow` is present here — the silence is recorded as debt, not a fix.)"
            )
        } else {
            lede
        }
    }
}

/// Go method calls that run a raw SQL read returning rows. We require a
/// multi-line string literal argument (per the ADR: target reads, not a
/// one-line imperative statement) to fire.
const QUERY_CALL_MARKERS: &[&str] = &[
    "db.Query(",
    "db.QueryRow(",
    "db.QueryContext(",
    "db.QueryRowContext(",
    "tx.Query(",
    "tx.QueryRow(",
    ".DB().Query(",
    ".DB().QueryRow(",
];

/// True if `source` contains a raw SQL read call whose argument spans a
/// multi-line backtick string literal.
pub fn has_multiline_raw_sql_read(source: &str) -> bool {
    for marker in QUERY_CALL_MARKERS {
        let mut from = 0;
        while let Some(rel) = source[from..].find(marker) {
            let after = from + rel + marker.len();
            if opens_multiline_backtick(&source[after..]) {
                return true;
            }
            from = after;
        }
    }
    false
}

/// After a query call's `(`, is the (first non-space) argument a backtick
/// raw string that contains a newline before its closing backtick?
fn opens_multiline_backtick(rest: &str) -> bool {
    let trimmed = rest.trim_start();
    let Some(stripped) = trimmed.strip_prefix('`') else {
        return false;
    };
    match stripped.find('`') {
        Some(end) => stripped[..end].contains('\n'),
        None => stripped.contains('\n'),
    }
}

/// Derive the read name from a handler file path: its file stem.
pub fn handler_read_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    if stem.is_empty() {
        return None;
    }
    Some(stem.to_owned())
}

/// True if the feature makes the read named `read_name` VISIBLE to a cold
/// `.lzi` audit — i.e. it carries a declared `query.*` for that name. When
/// this is true the escape is sanctioned and the rule must NOT fire.
fn feature_declares_visible_read(feature: &Feature, read_name: &str) -> bool {
    feature.queries.iter().any(|q| q.name() == read_name)
}

/// Run `ESC-RAWSQL-IN-HANDLER-001` for one feature.
///
/// `feature_dir` is the on-disk feature directory (the dir holding the
/// `.lzi`); raw SQL is sought under `<feature_dir>/handlers/*.go`. Returns
/// one finding per handler that hides a raw SQL read the `.lzi` declares
/// only as an opaque `@fn`.
pub fn check(feature: &Feature, feature_dir: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let handlers_dir = feature_dir.join("handlers");
    if !handlers_dir.exists() {
        return findings;
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&handlers_dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.sort();

    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("go") {
            continue;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_test.go"))
        {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !has_multiline_raw_sql_read(&source) {
            continue;
        }
        let Some(read_name) = handler_read_name(&path) else {
            continue;
        };
        if feature_declares_visible_read(feature, &read_name) {
            continue;
        }
        // Waivable-to-convert, NOT waivable-to-silence: read the allow
        // comment (to mark debt) but DO NOT suppress the finding.
        let waived = source_contains_doctor_allow(&source, Finding::CODE);
        findings.push(Finding {
            handler_path: path.clone(),
            feature: feature.name.clone(),
            read_name,
            waived,
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let skeletons =
            lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
        lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
    }

    // Grounded on hostpoint features/trust/handlers/list_property_reviews.go:
    // a multi-JOIN review read declared in the .lzi only as an opaque @fn.
    const LIST_PROPERTY_REVIEWS_GO: &str = r#"
package handlers

func ListPropertyReviews(ctx lazuli.Ctx) ([]PropertyReview, error) {
    rows, err := lazuli.DB().Query(`
        SELECT r.id, r.rating, r.comment, u.name, p.title
        FROM property_reviews r
        JOIN users u ON u.id = r.author_id
        JOIN properties p ON p.id = r.property_id
        WHERE p.org_id = $1
        ORDER BY r.created_at DESC
    `, ctx.OrgID())
    return nil, err
}
"#;

    fn write_handler(dir: &Path, stem: &str, src: &str) {
        let handlers = dir.join("handlers");
        std::fs::create_dir_all(&handlers).unwrap();
        std::fs::write(handlers.join(format!("{stem}.go")), src).unwrap();
    }

    #[test]
    fn rawsql_hidden_in_handler_fires() {
        let dir = tempfile::tempdir().unwrap();
        write_handler(dir.path(), "list_property_reviews", LIST_PROPERTY_REVIEWS_GO);
        // feature declares NO query for the read -> opaque @fn only.
        let feature = lower("feature trust\n  resource Review\n    rating: Integer required\n");
        let findings = check(&feature, dir.path());
        assert_eq!(findings.len(), 1, "expected one ESC-RAWSQL finding");
        assert_eq!(findings[0].read_name, "list_property_reviews");
        assert_eq!(Finding::CODE, "ESC-RAWSQL-IN-HANDLER-001");
        let msg = findings[0].message();
        assert!(msg.contains("invisible to a cold"));
        assert!(msg.contains("Convert this read"), "must teach convert-not-silence");
        assert!(!findings[0].waived);
    }

    #[test]
    fn declared_query_sql_silent() {
        let dir = tempfile::tempdir().unwrap();
        write_handler(dir.path(), "list_property_reviews", LIST_PROPERTY_REVIEWS_GO);
        // feature DOES declare a query.sql for the read -> visible.
        let feature = lower(
            r#"
feature trust
  resource Review
    rating: Integer required
  query.sql list_property_reviews
    returns Review
    sql "./queries/list_property_reviews.sql"
    policy @policy.public
"#,
        );
        let findings = check(&feature, dir.path());
        assert!(findings.is_empty(), "declared query.sql should clear the finding");
    }

    #[test]
    fn rawsql_allow_records_debt_not_fix() {
        let waived = LIST_PROPERTY_REVIEWS_GO.replace(
            "func ListPropertyReviews(ctx lazuli.Ctx) ([]PropertyReview, error) {",
            "func ListPropertyReviews(ctx lazuli.Ctx) ([]PropertyReview, error) {\n    # doctor:allow ESC-RAWSQL-IN-HANDLER-001 legacy, migrate in 0013",
        );
        let dir = tempfile::tempdir().unwrap();
        write_handler(dir.path(), "list_property_reviews", &waived);
        let feature = lower("feature trust\n  resource Review\n    rating: Integer required\n");
        let findings = check(&feature, dir.path());
        assert_eq!(findings.len(), 1, "waiver must NOT silence ESC-RAWSQL");
        assert!(findings[0].waived);
        assert!(findings[0].message().contains("recorded as debt"));
    }

    #[test]
    fn one_line_imperative_statement_does_not_fire() {
        let go = r#"
package handlers

func TouchRow(ctx lazuli.Ctx) error {
    _, err := lazuli.DB().Query(`UPDATE rows SET seen = true WHERE id = $1`, ctx.ID())
    return err
}
"#;
        let dir = tempfile::tempdir().unwrap();
        write_handler(dir.path(), "touch_row", go);
        let feature = lower("feature trust\n  resource Review\n    rating: Integer required\n");
        assert!(check(&feature, dir.path()).is_empty());
    }

    #[test]
    fn has_multiline_raw_sql_read_detects_db_query() {
        assert!(has_multiline_raw_sql_read(LIST_PROPERTY_REVIEWS_GO));
        assert!(!has_multiline_raw_sql_read("x := db.Query(`SELECT 1`)"));
        assert!(!has_multiline_raw_sql_read("// just a comment about db.Query("));
    }
}
