//! `ESC-SQL-TENANCY-CONTRACT-001` — a `query.sql` with an incoherent
//! binding contract.
//!
//! ## What it checks
//!
//! For each `query.sql`, read the `.sql` file at the block's `sql_path`
//! (relative to the feature dir) and parse its binding markers. Fire when
//! the body MIXES named (`:x`) and positional (`$N`) binding styles, OR
//! references a param the `.lzi` block does not declare (a named `:param`
//! absent from the `params` slots, or a positional `$N` whose index has no
//! declared slot).
//!
//! The check encodes the project's *current* contract read from the block
//! itself (its declared `params`), not a hardcoded preference — so a future
//! tenancy auto-inject build doesn't make the rule lie.
//!
//! ## Severity
//!
//! `Warning` — a mis-declared binding is a contract bug to fix; ordinary
//! waivable warning (the detector is precise).
//!
//! ## Trigger cue / fixture
//!
//! Fires when a `query.sql` binds `WHERE org_id = :org_id AND status = $2`
//! (mixed styles) or references a `$2` with no second declared param. The
//! canonical fire is pauta `notifications/queries/unread_count.sql`
//! (`:user_id`/`:org_id` against a positional-`$N` project). See
//! `mixed_binding_fires` / `undeclared_param_fires`.
//!
//! ## Example silent
//!
//! A `$N`-only query with every positional slot declared, or a `:named`
//! query with every name declared; see `positional_clean_silent`.

use std::path::Path;

use lazuli_ir::{Feature, Query};

/// What kind of contract violation a finding records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// The SQL mixes `:named` and `$positional` binding styles.
    MixedBinding,
    /// A named `:param` is referenced but not declared in `params`.
    UndeclaredNamed(String),
    /// A positional `$N` is referenced with no matching declared slot.
    UndeclaredPositional(usize),
}

/// One binding-contract finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Feature owning the query.
    pub feature: String,
    /// Query name.
    pub query_name: String,
    /// What went wrong.
    pub violation: Violation,
}

impl Finding {
    /// Rule code.
    pub const CODE: &'static str = "ESC-SQL-TENANCY-CONTRACT-001";

    /// Human-readable diagnostic message.
    pub fn message(&self) -> String {
        match &self.violation {
            Violation::MixedBinding => format!(
                "query.sql `{}` in feature `{}` mixes named (`:`) and positional (`$N`) binding \
styles — a `query.sql` must use ONE binding contract (this was the spec 0011 binding bug)",
                self.query_name, self.feature,
            ),
            Violation::UndeclaredNamed(name) => format!(
                "query.sql `{}` in feature `{}` references named param `:{}` not declared in `params`",
                self.query_name, self.feature, name,
            ),
            Violation::UndeclaredPositional(n) => format!(
                "query.sql `{}` in feature `{}` references positional param `${}` with no matching \
slot in `params`",
                self.query_name, self.feature, n,
            ),
        }
    }
}

/// Extract the distinct named bindings (`:name`) referenced in `sql`,
/// skipping `::` casts (Postgres `value::type`).
pub fn named_refs(sql: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                i += 2;
                continue;
            }
            let prev_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            if prev_ok {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && is_ident_byte(bytes[j]) {
                    j += 1;
                }
                if j > start {
                    let name = sql[start..j].to_string();
                    if !out.contains(&name) {
                        out.push(name);
                    }
                    i = j;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Extract the distinct positional indices (`$N`) referenced in `sql`.
pub fn positional_refs(sql: &str) -> Vec<usize> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > start
                && let Ok(n) = sql[start..j].parse::<usize>()
                && !out.contains(&n)
            {
                out.push(n);
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Analyze one SQL body string against its declared param names.
pub fn analyze(sql: &str, declared: &[&str]) -> Vec<Violation> {
    let named = named_refs(sql);
    let positional = positional_refs(sql);
    let mut out = Vec::new();

    if !named.is_empty() && !positional.is_empty() {
        out.push(Violation::MixedBinding);
    }
    for n in &named {
        if !declared.iter().any(|d| d == n) {
            out.push(Violation::UndeclaredNamed(n.clone()));
        }
    }
    for n in &positional {
        if *n == 0 || *n > declared.len() {
            out.push(Violation::UndeclaredPositional(*n));
        }
    }
    out
}

/// Check one feature's `query.sql` blocks for binding-contract violations.
///
/// `feature_dir` is the on-disk feature dir (each `query.sql`'s `sql_path`
/// is resolved relative to it). When a `.sql` file can't be read the block
/// is skipped silently.
pub fn check(feature: &Feature, feature_dir: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for query in &feature.queries {
        let Query::Sql(sq) = query else {
            continue;
        };
        let Some(sql) = read_sql(feature_dir, &sq.sql_path) else {
            continue;
        };
        let declared: Vec<&str> = sq.params.iter().map(|p| p.name.as_str()).collect();
        for violation in analyze(&sql, &declared) {
            findings.push(Finding {
                feature: feature.name.clone(),
                query_name: sq.name.clone(),
                violation,
            });
        }
    }
    findings
}

fn read_sql(feature_dir: &Path, sql_path: &str) -> Option<String> {
    let p = feature_dir.join(sql_path);
    std::fs::read_to_string(p).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_mixed_binding() {
        let v = analyze(
            "SELECT id FROM invoices WHERE org_id = :org_id AND status = $2",
            &["org_id"],
        );
        assert!(v.contains(&Violation::MixedBinding), "{v:?}");
    }

    #[test]
    fn analyze_undeclared_named() {
        let v = analyze(
            "SELECT count(*) FROM n WHERE user_id = :user_id AND org_id = :org_id",
            &["user_id"],
        );
        assert!(v.contains(&Violation::UndeclaredNamed("org_id".into())), "{v:?}");
    }

    #[test]
    fn analyze_undeclared_positional() {
        let v = analyze("SELECT id FROM i WHERE org_id = $1 AND status = $2", &["org_id"]);
        assert!(v.contains(&Violation::UndeclaredPositional(2)), "{v:?}");
    }

    #[test]
    fn analyze_positional_clean() {
        let v = analyze(
            "SELECT id FROM i WHERE org_id = $1 AND status = $2",
            &["org_id", "status"],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn analyze_named_clean_with_cast() {
        // `created::date` must not be parsed as a `:date` binding.
        let v = analyze(
            "SELECT id, created::date FROM i WHERE org_id = :org_id",
            &["org_id"],
        );
        assert!(v.is_empty(), "::cast must not be read as a binding: {v:?}");
    }

    #[test]
    fn check_reads_sql_file_and_fires() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("queries")).unwrap();
        std::fs::write(
            dir.path().join("queries/unread_count.sql"),
            "SELECT count(*) FROM n WHERE org_id = :org_id AND status = $2",
        )
        .unwrap();
        let feature = lower(
            r#"
feature notifications
  resource Note
    body: Text required
  query.sql unread_count
    returns Note
    sql "./queries/unread_count.sql"
    policy @policy.public
"#,
        );
        let findings = check(&feature, dir.path());
        assert!(findings.iter().any(|f| f.violation == Violation::MixedBinding));
        assert_eq!(Finding::CODE, "ESC-SQL-TENANCY-CONTRACT-001");
    }

    fn lower(source: &str) -> Feature {
        let skeletons =
            lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
        lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
    }
}
