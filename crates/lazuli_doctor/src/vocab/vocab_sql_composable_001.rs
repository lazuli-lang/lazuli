//! VOCAB-SQL-COMPOSABLE-001 — a checkable `query.sql` could be `query.compose`.
//!
//! ## Severity
//!
//! Severity: `warning` in both strict and production profiles. It NEVER
//! escalates to an error: the rule reads opaque SQL text, which the framework
//! deliberately does not fully parse, so it is a nudge, not a verdict.
//!
//! ## Rule statement
//!
//! Warns when a `query.sql` body is a pure FK-JOIN + scalar-sub-select read —
//! the shape `query.compose` was built to absorb and check. The symmetric
//! Prisma-trap pressure (`grading-rubric.md` C4): just as vocabulary must not
//! swallow everything, an author must not hide a *checkable* read inside opaque
//! `query.sql` where the tenant predicate, JOIN target, and projection are all
//! unverified. It is the `query.sql`-side counterpart to the `compose/`
//! `COMPOSE-DEMOTABLE-TO-LIST-001` / `COMPOSE-SUBSELECT-CATALOG-001` pressure.
//!
//! ## Heuristic (honest about what it can't parse)
//!
//! Fires when a SQL body, lower-cased, contains a `join` and DOES NOT contain
//! any token that lands the read outside the closed compose catalog:
//! `group by` (grouped sub-list → §6), a window function (`over (`), an ordered
//! comparison (`<` / `>`), or a geospatial function (`st_` / `earth_` /
//! `haversine`). The presence of any of those means the read genuinely needs
//! `query.sql`; their absence is the (necessarily heuristic) signal that the
//! read *could* be a checked `query.compose`. Because it cannot fully parse
//! SQL, a false negative (missing a composable read) is acceptable and a false
//! positive is only ever a warning. See proposal §7 + §8.
//!
//! ## Fixture example (warns)
//!
//! ```sql
//! SELECT p.id, h.name, (SELECT COUNT(*) FROM service s WHERE s.property = p.id)
//! FROM property p JOIN host h ON h.id = p.host WHERE p.org_id = $1
//! ```
//!
//! Genuinely-irreducible reads stay silent (a `GROUP BY` sub-list, a window
//! function, a geospatial `ST_Distance` sort, an ordered `>=` window).
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`VOCAB-SQL-COMPOSABLE-001`) + §1 + §6. Diagnostic ID / code constant:
//! `VOCAB-SQL-COMPOSABLE-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::SqlQuery;

/// One VOCAB-SQL-COMPOSABLE-001 finding — a `query.sql` that the heuristic
/// judges expressible as a checked `query.compose`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the query was authored in.
    pub path: PathBuf,
    /// Feature owning the query.
    pub feature: String,
    /// `query.sql <name>`.
    pub query_name: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-SQL-COMPOSABLE-001";

    /// Render the (warning-tier) nudge toward `query.compose`.
    pub fn message(&self) -> String {
        format!(
            "query.sql {} looks like a pure FK-JOIN + scalar sub-select read (no GROUP BY, window \
             fn, ordered comparison, or geo fn). Consider `query.compose`: it derives the tenant \
             predicate, JOIN target, and projection from the IR, so they become doctor-checked \
             instead of hand-written. (Heuristic — the framework does not fully parse SQL; this is \
             a nudge, never an error.)",
            self.query_name
        )
    }
}

/// Tokens whose presence means the SQL genuinely needs `query.sql` — outside
/// the closed compose catalog (grouped sub-list / window / ordered / geo).
const NON_COMPOSABLE_TOKENS: &[&str] = &[
    "group by", // grouped sub-list (§6)
    "over (",   // window function
    "over(",    // window function (no space)
    " < ",      // ordered comparison
    " > ",      // ordered comparison
    ">=",       // ordered comparison
    "<=",       // ordered comparison
    "st_",      // PostGIS geo function (st_distance, st_dwithin, ...)
    "earth_",   // earthdistance geo
    "haversine",
];

/// The pure heuristic: does this SQL body look like a checkable composite read?
///
/// `true` when the body has a `join` and contains none of the
/// [`NON_COMPOSABLE_TOKENS`] that push it outside the closed catalog.
/// Deliberately conservative — a read with no join is a `query.list`/`lookup`
/// concern, not this rule's.
///
/// ## Examples
///
/// ```
/// use lazuli_doctor::vocab::vocab_sql_composable_001::is_composable_sql;
///
/// assert!(is_composable_sql(
///     "SELECT p.id FROM property p JOIN host h ON h.id = p.host WHERE p.org_id = $1"
/// ));
/// // A GROUP BY sub-list genuinely needs query.sql.
/// assert!(!is_composable_sql(
///     "SELECT s.kind, COUNT(*) FROM service s JOIN property p ON p.id = s.property GROUP BY s.kind"
/// ));
/// ```
pub fn is_composable_sql(sql: &str) -> bool {
    let lower = sql.to_lowercase();
    if !lower.contains("join") {
        return false;
    }
    !NON_COMPOSABLE_TOKENS
        .iter()
        .any(|token| lower.contains(token))
}

/// Build a finding for one `query.sql` whose body has already been loaded.
///
/// Exposed (rather than a `check(feature, path)` that re-reads files) so the
/// pure heuristic stays unit-testable and the aggregator (W7) owns loading the
/// `.sql` file off `sql_path`. Returns `None` when the body is not composable.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_sql_composable_001::check_query;
/// use lazuli_ir::SqlQuery;
///
/// let q: SqlQuery = unimplemented!("a lowered query.sql");
/// let _ = check_query("catalog", &q, "SELECT ... JOIN ...", Path::new("catalog.lzi"));
/// ```
pub fn check_query(
    feature_name: &str,
    query: &SqlQuery,
    sql_body: &str,
    path: &Path,
) -> Option<Finding> {
    is_composable_sql(sql_body).then(|| Finding {
        path: path.to_path_buf(),
        feature: feature_name.to_owned(),
        query_name: query.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{QualifiedName, SqlQuery, SqlQueryKind, TypeRef};

    fn sample_sql_query(name: &str) -> SqlQuery {
        SqlQuery {
            name: name.to_owned(),
            sql_kind: SqlQueryKind::Sql,
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            returns: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Row".into(),
            }),
            sql_path: "./queries/x.sql".into(),
            cache: None,
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn pure_join_with_scalar_subselect_warns() {
        let q = sample_sql_query("property_cards");
        let sql = "SELECT p.id, h.name, \
                   (SELECT COUNT(*) FROM service s WHERE s.property = p.id) AS svc_count \
                   FROM property p JOIN host h ON h.id = p.host WHERE p.org_id = $1";
        let finding = check_query("catalog", &q, sql, Path::new("catalog.lzi"));
        assert!(finding.is_some(), "pure FK-JOIN + scalar sub-select should warn");
        let finding = finding.unwrap();
        assert_eq!(finding.query_name, "property_cards");
        assert_eq!(Finding::CODE, "VOCAB-SQL-COMPOSABLE-001");
    }

    #[test]
    fn group_by_sublist_does_not_warn() {
        let q = sample_sql_query("top_services");
        let sql = "SELECT s.kind, COUNT(*) FROM service s \
                   JOIN property p ON p.id = s.property GROUP BY s.kind ORDER BY 2 DESC LIMIT 5";
        assert!(check_query("catalog", &q, sql, Path::new("catalog.lzi")).is_none());
    }

    #[test]
    fn geospatial_read_does_not_warn() {
        let q = sample_sql_query("nearby");
        let sql = "SELECT p.id FROM property p JOIN host h ON h.id = p.host \
                   ORDER BY ST_Distance(p.geo, $1) LIMIT 20";
        assert!(check_query("catalog", &q, sql, Path::new("catalog.lzi")).is_none());
    }

    #[test]
    fn no_join_does_not_warn() {
        let q = sample_sql_query("plain");
        let sql = "SELECT id, name FROM property WHERE org_id = $1";
        assert!(check_query("catalog", &q, sql, Path::new("catalog.lzi")).is_none());
    }

    #[test]
    fn ordered_window_read_does_not_warn() {
        let q = sample_sql_query("windowed");
        let sql = "SELECT p.id FROM property p JOIN host h ON h.id = p.host WHERE p.score >= $1";
        assert!(check_query("catalog", &q, sql, Path::new("catalog.lzi")).is_none());
    }
}
