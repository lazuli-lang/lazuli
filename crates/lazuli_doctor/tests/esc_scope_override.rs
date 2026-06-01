//! Integration coverage for `ESC-SCOPE-OVERRIDE-UNGUARDED-001` — a
//! `query.sql` with no tenant predicate AND no `@actor.<privileged>`
//! guard. The canonical pass is the `list_all_agencies.sql` shape guarded
//! by `@policy.super_admin`.

use std::path::Path;

use lazuli_doctor::escape_hatch::scope_override_unguarded_001::{Finding, check};
use lazuli_ir::Feature;

fn lower(source: &str) -> Feature {
    let skeletons = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
    lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
}

fn write_sql(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

#[test]
fn no_tenant_no_guard_fires() {
    let dir = tempfile::tempdir().unwrap();
    write_sql(
        dir.path(),
        "list_everything.sql",
        "SELECT id, name FROM agencies ORDER BY created_at DESC",
    );
    let feature = lower(
        r#"
feature admin_panel
  resource Agency
    name: Text required
  query.sql list_everything
    returns Agency
    sql "./list_everything.sql"
    policy none
"#,
    );
    let findings = check(&feature, dir.path());
    assert_eq!(Finding::CODE, "ESC-SCOPE-OVERRIDE-UNGUARDED-001");
    assert_eq!(findings.len(), 1);
}

#[test]
fn no_tenant_with_actor_silent() {
    let dir = tempfile::tempdir().unwrap();
    write_sql(
        dir.path(),
        "list_all_agencies.sql",
        "SELECT id, name FROM agencies ORDER BY created_at DESC",
    );
    let feature = lower(
        r#"
feature admin_panel
  resource Agency
    name: Text required
  query.sql list_all_agencies
    returns Agency
    sql "./list_all_agencies.sql"
    policy @policy.super_admin
"#,
    );
    assert!(check(&feature, dir.path()).is_empty());
}

#[test]
fn tenant_predicate_silent() {
    let dir = tempfile::tempdir().unwrap();
    write_sql(
        dir.path(),
        "list_agencies.sql",
        "SELECT id, name FROM agencies WHERE org_id = :org_id",
    );
    let feature = lower(
        r#"
feature admin_panel
  resource Agency
    name: Text required
  query.sql list_agencies
    returns Agency
    sql "./list_agencies.sql"
    policy none
"#,
    );
    assert!(check(&feature, dir.path()).is_empty());
}
