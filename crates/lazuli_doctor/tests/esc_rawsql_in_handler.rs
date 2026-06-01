//! Integration coverage for `ESC-RAWSQL-IN-HANDLER-001` driving the rule
//! through the public crate API. Grounded on the hostpoint
//! `trust/handlers/list_property_reviews.go` shape: a multi-JOIN read
//! declared in the `.lzi` only as an opaque `@fn`.

use std::path::Path;

use lazuli_doctor::escape_hatch::rawsql_in_handler_001::{Finding, check};
use lazuli_ir::Feature;

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

fn lower(source: &str) -> Feature {
    let skeletons = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
    lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
}

fn write_handler(dir: &Path, stem: &str, src: &str) {
    let handlers = dir.join("handlers");
    std::fs::create_dir_all(&handlers).unwrap();
    std::fs::write(handlers.join(format!("{stem}.go")), src).unwrap();
}

#[test]
fn rawsql_hidden_in_handler_fires() {
    let dir = tempfile::tempdir().unwrap();
    write_handler(dir.path(), "list_property_reviews", LIST_PROPERTY_REVIEWS_GO);
    let feature = lower("feature trust\n  resource Review\n    rating: Integer required\n");
    let findings = check(&feature, dir.path());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].read_name, "list_property_reviews");
    assert_eq!(Finding::CODE, "ESC-RAWSQL-IN-HANDLER-001");
    let msg = findings[0].message();
    assert!(msg.contains("invisible to a cold"));
    assert!(msg.contains("Convert this read"));
}

#[test]
fn declared_query_sql_silent() {
    let dir = tempfile::tempdir().unwrap();
    write_handler(dir.path(), "list_property_reviews", LIST_PROPERTY_REVIEWS_GO);
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
    assert!(check(&feature, dir.path()).is_empty());
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
