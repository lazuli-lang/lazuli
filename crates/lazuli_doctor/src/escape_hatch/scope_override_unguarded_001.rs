//! `ESC-SCOPE-OVERRIDE-UNGUARDED-001` — a `query.sql` with no tenant
//! predicate AND no `@actor.<privileged>` guard.
//!
//! ## What it checks
//!
//! For each `query.sql`, read its `.sql` file (at `sql_path`, relative to
//! the feature dir) and fire when BOTH are true: the SQL has no tenant
//! predicate (no `tenant_id`/`org_id`/`account_id`/`workspace_id` in a
//! WHERE/AND/ON/JOIN clause) AND the query carries no `@actor.<privileged>`
//! policy (`@actor.super_admin`, `@actor.admin`, `@actor.platform_*`, or
//! the `@policy.`/`@role.` equivalents). An unscoped read with no
//! privileged guard is a silent tenant-bypass; a SQL comment is NOT a
//! guard.
//!
//! ## Severity
//!
//! `Warning` — an unguarded scope override is a contract bug to fix;
//! ordinary waivable warning.
//!
//! ## Trigger cue / fixture
//!
//! Warns when a cross-tenant read drops its guard (a `list_all_*.sql`
//! with no tenant filter and no `@actor.super_admin`); see
//! `no_tenant_no_guard_fires`.
//!
//! ## Example silent
//!
//! The canonical pass is pauta `admin_panel/list_all_agencies.sql`: no
//! tenant filter, but guarded by `@actor.super_admin` -> a legitimate,
//! visible scope override -> does NOT fire. See `no_tenant_with_actor_silent`.

use std::path::Path;

use lazuli_ir::{Feature, PolicyRef, Query};

/// One unguarded-scope-override finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Feature owning the query.
    pub feature: String,
    /// Query name.
    pub query_name: String,
}

impl Finding {
    /// Rule code.
    pub const CODE: &'static str = "ESC-SCOPE-OVERRIDE-UNGUARDED-001";

    /// Human-readable diagnostic message.
    pub fn message(&self) -> String {
        format!(
            "query.sql `{}` in feature `{}` has no tenant predicate and no `@actor.<privileged>` \
guard — an unscoped read may bypass tenancy. Either add a tenant predicate (e.g. \
`WHERE org_id = :org_id`) or, if this is an intentional cross-tenant read, guard it with an \
`@actor.<privileged>` policy (a SQL comment is not a guard). \
See docs/lazuli_way/escape-hatch-decision-tree.md.",
            self.query_name, self.feature,
        )
    }
}

const TENANT_TOKENS: &[&str] = &["tenant_id", "org_id", "account_id", "workspace_id"];
/// SQL clause keywords after which a tenant token counts as a predicate.
const PREDICATE_KEYWORDS: &[&str] = &["where", "and", "on", "join"];

/// True if a tenant column appears in a predicate clause (after one of the
/// predicate keywords) anywhere in the SQL.
pub fn has_tenant_predicate(sql: &str) -> bool {
    let lower = sql.to_lowercase();
    for token in TENANT_TOKENS {
        let mut from = 0;
        while let Some(rel) = lower[from..].find(token) {
            let pos = from + rel;
            let before = &lower[..pos];
            if PREDICATE_KEYWORDS.iter().any(|kw| contains_word(before, kw)) {
                return true;
            }
            from = pos + token.len();
        }
    }
    false
}

/// Whole-word containment (so `band` does not match `and`).
fn contains_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(word) {
        let pos = from + rel;
        let before_ok = pos == 0 || !is_ident_byte(bytes[pos - 1]);
        let after = pos + word.len();
        let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
        if before_ok && after_ok {
            return true;
        }
        from = pos + word.len();
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// True if the query carries an `@actor.<privileged>` policy guard, via the
/// bare `policy` atom or a structured `policy_expr`.
pub fn has_privileged_guard(policy: &PolicyRef, policy_expr: &Option<lazuli_ir::PolicyExpr>) -> bool {
    if let PolicyRef::Atom(atom) = policy
        && atom_is_privileged(atom)
    {
        return true;
    }
    // Structured policy expr: scrape its debug form for a privileged atom.
    // (Conservative: any privileged atom anywhere in the expr clears it.)
    if let Some(expr) = policy_expr {
        let rendered = format!("{expr:?}").to_lowercase();
        if rendered.contains("super_admin")
            || rendered.contains("platform_")
            || rendered.contains("\"admin\"")
        {
            return true;
        }
    }
    false
}

/// True if a bare policy atom names a privileged role. Accepts the
/// `@actor.`, `@policy.`, `@role.`, `@scope.` namespaces and bare names.
fn atom_is_privileged(atom: &str) -> bool {
    let tail = atom.rsplit('.').next().unwrap_or(atom).to_lowercase();
    tail == "super_admin" || tail == "admin" || tail.starts_with("platform_")
}

/// Check one feature's `query.sql` blocks for unguarded scope overrides.
///
/// `feature_dir` resolves each block's `sql_path`. Unreadable `.sql` files
/// are skipped silently.
pub fn check(feature: &Feature, feature_dir: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for query in &feature.queries {
        let Query::Sql(sq) = query else {
            continue;
        };
        let Some(sql) = read_sql(feature_dir, &sq.sql_path) else {
            continue;
        };
        if has_tenant_predicate(&sql) {
            continue;
        }
        if has_privileged_guard(&sq.policy, &sq.policy_expr) {
            continue;
        }
        findings.push(Finding {
            feature: feature.name.clone(),
            query_name: sq.name.clone(),
        });
    }
    findings
}

fn read_sql(feature_dir: &Path, sql_path: &str) -> Option<String> {
    std::fs::read_to_string(feature_dir.join(sql_path)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::PolicyRef;

    #[test]
    fn has_tenant_predicate_detects_where_org_id() {
        assert!(has_tenant_predicate(
            "SELECT id FROM a WHERE org_id = :org_id"
        ));
        assert!(!has_tenant_predicate("SELECT id FROM agencies ORDER BY created_at"));
    }

    #[test]
    fn privileged_guard_recognizes_super_admin_atom() {
        assert!(has_privileged_guard(
            &PolicyRef::Atom("@actor.super_admin".into()),
            &None
        ));
        assert!(has_privileged_guard(
            &PolicyRef::Atom("@policy.super_admin".into()),
            &None
        ));
        assert!(!has_privileged_guard(
            &PolicyRef::Atom("@actor.org_member".into()),
            &None
        ));
        assert!(!has_privileged_guard(&PolicyRef::None, &None));
    }

    #[test]
    fn no_tenant_no_guard_fires() {
        let dir = write_sql("list_everything.sql", "SELECT id, name FROM agencies ORDER BY created_at DESC");
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
        assert_eq!(findings.len(), 1, "unscoped + unguarded must warn");
        assert_eq!(findings[0].query_name, "list_everything");
        assert_eq!(Finding::CODE, "ESC-SCOPE-OVERRIDE-UNGUARDED-001");
        drop(dir);
    }

    #[test]
    fn no_tenant_with_actor_silent() {
        // list_all_agencies shape: no tenant filter, but @actor.super_admin.
        let dir = write_sql(
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
        assert!(check(&feature, dir.path()).is_empty(), "super_admin guard clears the finding");
        drop(dir);
    }

    #[test]
    fn tenant_predicate_silent() {
        let dir = write_sql(
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
        assert!(check(&feature, dir.path()).is_empty(), "tenant predicate clears the finding");
        drop(dir);
    }

    fn write_sql(name: &str, body: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(name), body).unwrap();
        dir
    }

    fn lower(source: &str) -> Feature {
        let skeletons =
            lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
        lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
    }
}
