//! Integration coverage for `ESC-SQL-TENANCY-CONTRACT-001` — binding-style
//! consistency + declared-param checks on `query.sql` (SQL read from the
//! `.sql` file at the block's `sql_path`).

use std::path::Path;

use lazuli_doctor::escape_hatch::sql_tenancy_contract_001::{Finding, Violation, check};
use lazuli_ir::Feature;

fn lower(source: &str) -> Feature {
    let skeletons = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
    lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
}

fn write_sql(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, body).unwrap();
}

/// Build a `feature` whose `query.sql <name>` declares the given param
/// slots. `params` is a child header; each slot is indented one more level
/// as `<name>: <Type>` (the grandchild grammar parsed by
/// `parse_query_params_block`).
fn feature_with_sql(name: &str, slots: &[&str]) -> Feature {
    let mut src = format!(
        "feature notifications\n  resource Note\n    body: Text required\n  query.sql {name}\n    returns Note\n    sql \"./queries/{name}.sql\"\n    policy @policy.public\n    params\n"
    );
    for slot in slots {
        src.push_str(&format!("      {slot}\n"));
    }
    lower(&src)
}

#[test]
fn mixed_binding_fires() {
    let dir = tempfile::tempdir().unwrap();
    write_sql(
        dir.path(),
        "queries/unread_count.sql",
        "SELECT count(*) FROM notifications WHERE org_id = :org_id AND status = $2",
    );
    let feature = feature_with_sql("unread_count", &["org_id: ID"]);
    let findings = check(&feature, dir.path());
    assert_eq!(Finding::CODE, "ESC-SQL-TENANCY-CONTRACT-001");
    assert!(findings.iter().any(|d| d.violation == Violation::MixedBinding));
}

#[test]
fn undeclared_param_fires() {
    let dir = tempfile::tempdir().unwrap();
    write_sql(
        dir.path(),
        "queries/unread_count.sql",
        "SELECT count(*) FROM notifications WHERE user_id = :user_id AND org_id = :org_id",
    );
    let feature = feature_with_sql("unread_count", &["user_id: ID"]);
    let findings = check(&feature, dir.path());
    assert!(
        findings
            .iter()
            .any(|d| d.violation == Violation::UndeclaredNamed("org_id".into()))
    );
}

#[test]
fn positional_clean_silent() {
    let dir = tempfile::tempdir().unwrap();
    write_sql(
        dir.path(),
        "queries/unread_count.sql",
        "SELECT count(*) FROM notifications WHERE org_id = $1 AND status = $2",
    );
    let feature = feature_with_sql("unread_count", &["org_id: ID", "status: Text"]);
    assert!(check(&feature, dir.path()).is_empty());
}
