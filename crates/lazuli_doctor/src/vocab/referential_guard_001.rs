//! `SUGGEST-REFERENTIAL-GUARD-001` — hand-written referential guard that
//! should be the `restrict on_delete` primitive.
//!
//! ## What it checks
//!
//! For each `features/<f>/handlers/*.go`, recognize the invariant
//! "referential guard" shape the pilots hand-write 15× across 11 features:
//! a `SELECT COUNT(*) FROM <rel> WHERE <fk> = $… …` or `SELECT EXISTS
//! (SELECT 1 FROM <rel> WHERE <fk> = $… …)` read, followed by a `> 0` /
//! truthy reject that returns an `ErrReferencedInUse` / `Err*InUse` /
//! `*_IN_USE` sentinel. When both halves are present, suggest declaring the
//! guard on the protected resource instead.
//!
//! ## Severity
//!
//! `Warning` — a *suggestion* toward vocabulary (mirrors
//! `VOCAB-CRUD-SYNTH-AVAILABLE-001`). The hand-written guard still works;
//! the rule names the declarative replacement and points at the idiom doc.
//!
//! ## Trigger cue / fixture
//!
//! Fires when a handler body matches the COUNT/EXISTS-then-reject shape
//! returning an in-use error. The canonical fixture is grounded on pauta
//! `features/billing_config/handlers/guard_billing_type_in_use.go` (COUNT
//! variant) and `features/workflow_templates/handlers/guard_template_not_in_use.go`
//! (EXISTS variant); see `count_guard_fires` / `exists_guard_fires`.

use std::path::{Path, PathBuf};

/// One hand-written-referential-guard finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path to the Go handler that hand-writes the guard.
    pub handler_path: PathBuf,
    /// Feature owning the handler.
    pub feature: String,
    /// The referencing relation named in the guard's `FROM <rel>`, when
    /// it could be extracted (used to render the suggested `restrict` line).
    pub relation: Option<String>,
    /// The foreign-key column from the `WHERE <fk> = $…` clause, when found.
    pub fk: Option<String>,
}

impl Finding {
    /// Rule code.
    pub const CODE: &'static str = "SUGGEST-REFERENTIAL-GUARD-001";

    /// Human-readable hint. Names the primitive and the idiom doc.
    pub fn message(&self) -> String {
        let decl = match (&self.relation, &self.fk) {
            (Some(rel), Some(fk)) => {
                format!("restrict on_delete references {rel} via {fk}")
            }
            (Some(rel), None) => {
                format!("restrict on_delete references {rel} via <fk>")
            }
            _ => "restrict on_delete references <relation> via <fk>".to_owned(),
        };
        format!(
            "handler `{}` (feature `{}`) hand-writes a referential guard \
(COUNT/EXISTS live references → reject with an in-use error). Declare it on the protected \
resource instead:\n        {decl}\nThe generated guard is tenant-scoped and soft-delete-aware \
by construction. See docs/lazuli_way/referential-guards.md.",
            self.handler_path.display(),
            self.feature,
        )
    }
}

/// True if `source` reads as a referential guard: a COUNT/EXISTS-then-reject
/// over a single FK, returning an in-use sentinel. Both halves must be
/// present so an ordinary read or an ordinary delete handler does not fire.
pub fn looks_like_referential_guard(source: &str) -> bool {
    has_count_or_exists_probe(source) && rejects_with_in_use(source)
}

/// The probe half: a `SELECT COUNT(*) FROM …` or `SELECT EXISTS (SELECT 1
/// FROM …)` over a `WHERE <col> = $…` clause. Whitespace-insensitive (Go
/// raw-string SQL is multi-line and indented).
fn has_count_or_exists_probe(source: &str) -> bool {
    let flat = flatten_ws(source).to_lowercase();
    let has_count =
        flat.contains("select count(*) from ") || flat.contains("select count( *) from ");
    // After whitespace-flattening, `EXISTS (\n SELECT 1` becomes
    // `exists ( select 1`. Match the `EXISTS (SELECT 1 FROM ...)` core
    // regardless of what precedes it (`SELECT EXISTS`, or an `ELSE EXISTS`
    // wrapped in a `to_regclass` CASE guard — the real pauta variant),
    // tolerating the optional space after the paren.
    let has_exists =
        flat.contains("exists ( select 1 from ") || flat.contains("exists (select 1 from ");
    (has_count || has_exists) && flat.contains(" where ") && flat.contains(" = $")
}

/// The reject half: the guard returns a "still-referenced" sentinel after a
/// truthy/`> 0` probe. Accepts the runtime sentinel (`ErrReferencedInUse`),
/// the `Err*InUse` / `*_IN_USE` spelling, and the `Has{Active,Open,…}` /
/// `*_HAS_*` family the subset guards use (`ErrJobHasOpenActivities` /
/// `JOB_HAS_OPEN_ACTIVITIES`) — the spellings the pilots actually use. A
/// bare `return err` / `return nil` (a stub) does NOT count.
fn rejects_with_in_use(source: &str) -> bool {
    source.contains("ErrReferencedInUse")
        || source.contains("InUse")
        || source.contains("_HAS_ACTIVE")
        || source.contains("_HAS_OPEN")
        || source.contains("HasActive")
        || source.contains("HasOpen")
        || source.contains("_IN_USE")
}

/// Collapse all runs of ASCII whitespace (incl. newlines/tabs) to a single
/// space so multi-line indented SQL matches a flat marker.
fn flatten_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

/// Best-effort extraction of the referencing relation. Anchored on the
/// probe's own `FROM` (`COUNT(*) FROM <rel>` / `EXISTS (SELECT 1 FROM
/// <rel>`) so a stray ` from ` in a leading comment doesn't win.
fn extract_relation(source: &str) -> Option<String> {
    let flat = flatten_ws(source).to_lowercase();
    let anchors = [
        "exists ( select 1 from ",
        "exists (select 1 from ",
        "count(*) from ",
        "count( *) from ",
    ];
    let after = anchors.iter().find_map(|a| {
        flat.find(a).map(|idx| &flat[idx + a.len()..])
    })?;
    let rel: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!rel.is_empty()).then_some(rel)
}

/// Best-effort extraction of the FK column from the first `WHERE <fk> = $…`.
fn extract_fk(source: &str) -> Option<String> {
    let flat = flatten_ws(source).to_lowercase();
    let idx = flat.find(" where ")?;
    let after = flat[idx + " where ".len()..].trim_start();
    let fk: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    // The column is the leading identifier of the WHERE clause; the `= $N`
    // placeholder that normally follows is what makes it the FK predicate,
    // but we return the bare identifier regardless (best-effort hint).
    (!fk.is_empty()).then_some(fk)
}

/// Run `SUGGEST-REFERENTIAL-GUARD-001` over one feature's handler dir.
///
/// `feature_name` labels findings; `feature_dir` is the directory holding
/// the `.lzi`. Scans `<feature_dir>/handlers/*.go` (skipping `_test.go`).
pub fn check(feature_name: &str, feature_dir: &Path) -> Vec<Finding> {
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
        if !looks_like_referential_guard(&source) {
            continue;
        }
        findings.push(Finding {
            handler_path: path.clone(),
            feature: feature_name.to_owned(),
            relation: extract_relation(&source),
            fk: extract_fk(&source),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_handler(dir: &Path, stem: &str, src: &str) {
        let handlers = dir.join("handlers");
        std::fs::create_dir_all(&handlers).unwrap();
        std::fs::write(handlers.join(format!("{stem}.go")), src).unwrap();
    }

    // Grounded on pauta billing_config/handlers/guard_billing_type_in_use.go.
    const COUNT_GUARD_GO: &str = r#"
package handlers

var ErrBillingTypeInUse = errors.New("BILLING_TYPE_IN_USE")

func GuardBillingTypeInUse(ctx context.Context, db DBTX, id, tenantID string) error {
    const q = `SELECT COUNT(*) FROM invoice
        WHERE billing_type_id = $1 AND tenant_id = $2 AND deleted_at IS NULL`
    var n int
    if err := db.QueryRow(ctx, q, id, tenantID).Scan(&n); err != nil {
        return err
    }
    if n > 0 {
        return ErrBillingTypeInUse
    }
    return nil
}
"#;

    // Grounded on pauta workflow_templates/handlers/guard_template_not_in_use.go.
    const EXISTS_GUARD_GO: &str = r#"
package handlers

func GuardTemplateNotInUse(ctx context.Context, db DBTX, id string) error {
    const q = `SELECT EXISTS (
        SELECT 1 FROM job WHERE workflow_template_id = $1 AND deleted_at IS NULL
    )`
    var inUse bool
    if err := db.QueryRow(ctx, q, id).Scan(&inUse); err != nil {
        return err
    }
    if inUse {
        return runtime.ErrReferencedInUse
    }
    return nil
}
"#;

    #[test]
    fn count_guard_fires() {
        let dir = tempfile::tempdir().unwrap();
        write_handler(dir.path(), "guard_billing_type_in_use", COUNT_GUARD_GO);
        let findings = check("billing_config", dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "SUGGEST-REFERENTIAL-GUARD-001");
        assert_eq!(findings[0].relation.as_deref(), Some("invoice"));
        assert_eq!(findings[0].fk.as_deref(), Some("billing_type_id"));
        let msg = findings[0].message();
        assert!(msg.contains("restrict on_delete references invoice via billing_type_id"), "{msg}");
        assert!(msg.contains("docs/lazuli_way/referential-guards.md"), "{msg}");
    }

    #[test]
    fn exists_guard_fires() {
        let dir = tempfile::tempdir().unwrap();
        write_handler(dir.path(), "guard_template_not_in_use", EXISTS_GUARD_GO);
        let findings = check("workflow_templates", dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].relation.as_deref(), Some("job"));
        assert_eq!(findings[0].fk.as_deref(), Some("workflow_template_id"));
    }

    #[test]
    fn ordinary_read_does_not_fire() {
        // A plain list read: EXISTS-less, no in-use reject.
        let go = r#"
package handlers
func ListInvoices(ctx context.Context, db DBTX, id string) error {
    const q = `SELECT id, total FROM invoice WHERE billing_type_id = $1`
    _, err := db.Query(ctx, q, id)
    return err
}
"#;
        let dir = tempfile::tempdir().unwrap();
        write_handler(dir.path(), "list_invoices", go);
        assert!(check("billing_config", dir.path()).is_empty());
    }

    #[test]
    fn probe_without_reject_does_not_fire() {
        // Has the EXISTS probe but never returns an in-use error.
        let go = r#"
package handlers
func ProbeOnly(ctx context.Context, db DBTX, id string) (bool, error) {
    const q = `SELECT EXISTS (SELECT 1 FROM job WHERE workflow_template_id = $1)`
    var b bool
    err := db.QueryRow(ctx, q, id).Scan(&b)
    return b, err
}
"#;
        let dir = tempfile::tempdir().unwrap();
        write_handler(dir.path(), "probe_only", go);
        assert!(check("ops", dir.path()).is_empty());
    }

    #[test]
    fn detector_recognizes_both_variants() {
        assert!(looks_like_referential_guard(COUNT_GUARD_GO));
        assert!(looks_like_referential_guard(EXISTS_GUARD_GO));
        assert!(!looks_like_referential_guard("func F() {}"));
    }
}
