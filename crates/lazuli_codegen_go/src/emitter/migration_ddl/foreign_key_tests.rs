//! Foreign-key DDL tests + topological ordering invariants. Cross-
//! feature FK lowering, same-feature FK regression (bug #9), and the
//! topo-sort safety net that keeps FK targets ahead of their
//! referencers (WAR-RUNTIME-MIGRATION-03).

#![cfg(test)]

use super::emit_migrations;
use super::test_support::{base_feature, base_module, builtin, field, resource};
use lazuli_ir::{BuiltinType, QualifiedName, TypeRef};

#[test]
fn emits_foreign_key_for_cross_feature_resource_ref() {
    let mut customer = base_feature("customer");
    customer.resources.push(resource(
        "Customer",
        vec![builtin("email", BuiltinType::SemanticEmail, true)],
    ));
    let mut orders = base_feature("orders");
    orders.resources.push(resource(
        "Order",
        vec![field(
            "customer",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            }),
            true,
        )],
    ));

    let files = emit_migrations(&base_module(vec![orders, customer]), "shop");
    let order_sql = files
        .iter()
        .find(|file| file.path == "migrations/002_orders_order.sql")
        .map(|file| file.contents.as_str())
        .unwrap();

    assert!(order_sql.contains("customer BIGINT NOT NULL,"));
    // FK target must match the table name emitted by
    // `emit_resource_migration` (bare `"customer"`, not the legacy
    // `<feature>_<resource>` form that produced broken migrations).
    // See migration_ddl.rs::foreign_key_constraints for the contract.
    assert!(
        order_sql.contains("FOREIGN KEY (customer) REFERENCES \"customer\" (id)"),
        "FK target must match CREATE TABLE name; got:\n{order_sql}"
    );
    assert!(
        !order_sql.contains("customer_customer"),
        "legacy `<feature>_<resource>` FK target leaked back in:\n{order_sql}"
    );
}

#[test]
fn emits_foreign_key_for_same_feature_resource_ref() {
    // Regression for the same-feature FK gap (discovered during
    // bug #9 cross-feature work, 2026-05-15). A resource referencing
    // another resource in the SAME feature (e.g. `Category.parent:
    // Category` parent-child link, or `Membership.workspace:
    // Workspace` when both live in the `org` feature) must emit a
    // FOREIGN KEY constraint just like the cross-feature case.
    // Previously `foreign_key_owner` returned `None` for these and
    // the constraint silently disappeared.
    let mut org = base_feature("org");
    org.resources.push(resource(
        "Workspace",
        vec![builtin("name", BuiltinType::Text, true)],
    ));
    org.resources.push(resource(
        "Membership",
        vec![field(
            "workspace",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Workspace".to_owned(),
            }),
            true,
        )],
    ));

    let files = emit_migrations(&base_module(vec![org]), "saas");
    // Topo-sorted: Workspace (no FK deps) emits first; Membership
    // (depends on Workspace) follows. Look up by `_membership.sql`
    // suffix so the test isn't coupled to the migration index.
    let membership_sql = files
        .iter()
        .find(|file| file.path.ends_with("_org_membership.sql"))
        .map(|file| file.contents.as_str())
        .unwrap();

    assert!(
        membership_sql.contains("workspace BIGINT NOT NULL,"),
        "expected workspace FK column; got:\n{membership_sql}"
    );
    assert!(
        membership_sql.contains("FOREIGN KEY (workspace) REFERENCES \"workspace\" (id)"),
        "same-feature FK must reference the actual table; got:\n{membership_sql}"
    );

    // Topo invariant: the FK target's CREATE TABLE migration index
    // must be smaller than the referencing one (otherwise applying
    // the second fails with `relation "workspace" does not exist`).
    let workspace_idx = files
        .iter()
        .position(|file| file.path.ends_with("_org_workspace.sql"))
        .unwrap();
    let membership_idx = files
        .iter()
        .position(|file| file.path.ends_with("_org_membership.sql"))
        .unwrap();
    assert!(
        workspace_idx < membership_idx,
        "FK target migration must precede the referencing migration: workspace at {workspace_idx}, membership at {membership_idx}"
    );
}

#[test]
fn fk_target_table_matches_actual_create_table_name() {
    // Regression guard for the `<feature>_<resource>` FK drift —
    // every FOREIGN KEY emitted across the module must reference a
    // table that some `CREATE TABLE` statement actually creates in
    // the same module. Without this guard the codegen can produce
    // migrations that fail to apply with `relation "X" does not
    // exist`.
    //
    // Scope note: this test only covers **cross-feature** FKs.
    // Same-feature FKs are intentionally not emitted today
    // (`foreign_key_owner` filters out same-feature refs); when
    // that gap closes the test will need a same-feature case too.
    use std::collections::HashSet;

    // Two cross-feature references: `Membership.user → account.User`
    // and `Membership.workspace → org.Workspace`. Both are
    // cross-feature so both should produce FK constraints.
    let mut account = base_feature("account");
    account.resources.push(resource(
        "User",
        vec![builtin("email", BuiltinType::SemanticEmail, true)],
    ));
    let mut org = base_feature("org");
    org.resources.push(resource(
        "Workspace",
        vec![builtin("name", BuiltinType::Text, true)],
    ));
    let mut members = base_feature("members");
    members.resources.push(resource(
        "Membership",
        vec![
            field(
                "user",
                TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "User".to_owned(),
                }),
                true,
            ),
            field(
                "workspace",
                TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: "Workspace".to_owned(),
                }),
                true,
            ),
        ],
    ));

    let files = emit_migrations(&base_module(vec![account, org, members]), "saas");

    // Collect every quoted identifier that appears after `CREATE TABLE`
    // and every one that appears after `REFERENCES`. The second set
    // must be a subset of the first.
    let mut created: HashSet<String> = HashSet::new();
    let mut referenced: HashSet<String> = HashSet::new();
    for file in &files {
        for line in file.contents.lines() {
            if let Some(rest) = line
                .trim_start()
                .strip_prefix("CREATE TABLE IF NOT EXISTS ")
                && let Some(name) = rest
                    .split_whitespace()
                    .next()
                    .and_then(|tok| tok.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            {
                created.insert(name.to_owned());
            }
            if let Some(idx) = line.find("REFERENCES \"") {
                let after = &line[idx + "REFERENCES \"".len()..];
                if let Some(end) = after.find('"') {
                    referenced.insert(after[..end].to_owned());
                }
            }
        }
    }

    for table in &referenced {
        assert!(
            created.contains(table),
            "FK references {:?} but no CREATE TABLE emits it; created = {:?}",
            table,
            created
        );
    }
    assert!(
        referenced.contains("user") && referenced.contains("workspace"),
        "expected cross-feature FKs to `user` and `workspace`; got {:?}",
        referenced
    );
}

#[test]
fn topological_fk_order_emits_targets_before_referencers() {
    // WAR-RUNTIME-MIGRATION-03 regression. Build a cross-feature
    // dependency graph where lexical order would put the
    // referencing migration BEFORE its FK target — exercising the
    // failure mode that the topo sort closes.
    //
    // Resources:
    //   `zeta.Profile` (no deps)        — lexical last, topo any
    //   `alpha.Comment` (FK → Profile)  — lexical first, would fail
    //   `alpha.Vote`    (FK → Comment)  — chains a level deeper
    //
    // Pure lexical sort would write `001_alpha_comment.sql` first,
    // attempting to point at `zeta.profile` before its table
    // exists. Topo order must invert that.
    let mut zeta = base_feature("zeta");
    zeta.resources.push(resource(
        "Profile",
        vec![builtin("handle", BuiltinType::Text, true)],
    ));
    let mut alpha = base_feature("alpha");
    alpha.resources.push(resource(
        "Comment",
        vec![field(
            "author",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Profile".to_owned(),
            }),
            true,
        )],
    ));
    alpha.resources.push(resource(
        "Vote",
        vec![field(
            "comment",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Comment".to_owned(),
            }),
            true,
        )],
    ));

    let files = emit_migrations(&base_module(vec![alpha, zeta]), "topo");

    let pos = |suffix: &str| -> usize {
        files
            .iter()
            .position(|file| file.path.ends_with(suffix))
            .unwrap_or_else(|| {
                panic!(
                    "expected file ending with {suffix}; got {:#?}",
                    files.iter().map(|f| &f.path).collect::<Vec<_>>()
                )
            })
    };

    let profile = pos("_zeta_profile.sql");
    let comment = pos("_alpha_comment.sql");
    let vote = pos("_alpha_vote.sql");

    assert!(
        profile < comment,
        "profile (FK target) must precede comment (referencer); got profile={profile}, comment={comment}"
    );
    assert!(
        comment < vote,
        "comment (FK target) must precede vote (referencer); got comment={comment}, vote={vote}"
    );
}

#[test]
fn topological_fk_order_falls_back_to_lexical_when_independent() {
    // Two resources with no FK relationship between them — topo
    // imposes no constraint, so the lexical (feature, resource)
    // tiebreaker decides. Without this guarantee the output would
    // be non-deterministic across runs.
    let mut a = base_feature("alpha");
    a.resources.push(resource(
        "Account",
        vec![builtin("name", BuiltinType::Text, true)],
    ));
    let mut b = base_feature("beta");
    b.resources.push(resource(
        "Bucket",
        vec![builtin("label", BuiltinType::Text, true)],
    ));

    let files = emit_migrations(&base_module(vec![b, a]), "stable");

    let account = files
        .iter()
        .position(|file| file.path.ends_with("_alpha_account.sql"))
        .unwrap();
    let bucket = files
        .iter()
        .position(|file| file.path.ends_with("_beta_bucket.sql"))
        .unwrap();
    assert!(
        account < bucket,
        "lexical tiebreaker should put alpha.account before beta.bucket; got account={account}, bucket={bucket}"
    );
}
