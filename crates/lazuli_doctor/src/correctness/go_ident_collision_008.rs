//! CODEGEN-GO-IDENT-COLLISION-008 (`@correctness.go_ident_collision`) —
//! two different DSL constructs lower to the SAME emitted Go identifier
//! inside one feature's generated package.
//!
//! Severity: error.
//!
//! Fires when ≥2 IR constructs in a feature collapse onto the same
//! exported Go identifier under the acronym-aware caser the Go emitter
//! uses (`lazuli_codegen_go::emitter::casing::pascal_case`). Every
//! feature lowers into a single `<feature>gen` Go package, so two
//! constructs sharing an emitted name produce a Go double-declaration
//! that `go build` only catches AFTER codegen has already written broken
//! source. This rule moves the failure forward to `lazuli check` /
//! `doctor`.
//!
//! Tracked construct families and the exported identifier each emits:
//!
//! - `enum <Name>` → the typed alias `pascal_case(Name)`.
//! - `query.* <name>` → the wrapper func / args type `pascal_case(name)`.
//! - `command <name>` → the `Handle<Name>` handler + input/var family,
//!   keyed by `pascal_case(name)`.
//!
//! Example: a feature with `enum status` AND `query.lookup status by …`
//! both emit an exported `Status` (enum alias vs. query wrapper func) in
//! the same package — a Go `Status redeclared in this block` error. This
//! rule fires before codegen and names BOTH constructs plus the shared
//! identifier so the author can rename one.
//!
//! ## Deliberately NOT tracked
//!
//! - **Lifecycle transitions.** A `transition <name>` lowers to a *string
//!   literal* (`{Name: "<name>", …}`) inside the resource's
//!   `lifecycle.New[…]` machine, not to a package-level Go identifier. A
//!   command named the same as a transition it drives (the canonical
//!   `command activate … triggers transition activate` pattern) is correct
//!   and compiles cleanly — flagging it would be a false positive.
//! - **Lifecycle-generated enums.** The enum a `lifecycle <field>`
//!   generates is allowed (and often expected) to reuse an authored
//!   `enum` of the same name; codegen emits the alias once. That exact
//!   collision is owned by `LIFECYCLE-ENUM-DUPLICATE`, which understands
//!   the reuse semantics; double-claiming it here would double-report.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lazuli_codegen_go::emitter::casing::pascal_case;
use lazuli_ir::Feature;

/// The construct family a colliding identifier originates from. Carried
/// on the finding so the diagnostic can name *what kind* of declaration
/// each side of the collision is (an `enum` vs. a `query`, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConstructKind {
    /// An authored `enum <Name>` block.
    Enum,
    /// A `query.list` / `query.lookup` / `query.sql` block.
    Query,
    /// A `command <name>` block.
    Command,
}

impl ConstructKind {
    /// Human-facing label used in the diagnostic message.
    pub fn label(self) -> &'static str {
        match self {
            ConstructKind::Enum => "enum",
            ConstructKind::Query => "query",
            ConstructKind::Command => "command",
        }
    }
}

/// One construct that contributes a name to the emitted-identifier scope.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Construct {
    kind: ConstructKind,
    /// The author-facing DSL name (e.g. `status`, `begin_publishing`).
    dsl_name: String,
}

/// One CODEGEN-GO-IDENT-COLLISION-008 finding — two or more constructs
/// in a feature lower to the same emitted Go identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending feature lives in.
    pub path: PathBuf,
    /// Feature name carrying the collision.
    pub feature: String,
    /// The shared emitted Go identifier (e.g. `Status`).
    pub emitted_ident: String,
    /// The colliding constructs, sorted for deterministic output.
    /// Each entry is `(kind label, dsl name)`.
    pub colliding: Vec<(&'static str, String)>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "CODEGEN-GO-IDENT-COLLISION-008";
    /// Author-facing rule ID used in `# doctor:allow` and prose docs.
    pub const ID: &'static str = "@correctness.go_ident_collision";

    /// Render the collision message. Names the shared emitted identifier,
    /// every colliding construct (kind + DSL name), and how to fix it.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::go_ident_collision_008::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("catalog.lzi"),
    ///     feature: "catalog".into(),
    ///     emitted_ident: "Status".into(),
    ///     colliding: vec![("enum", "status".into()), ("query", "status".into())],
    /// };
    /// assert!(f.message().contains("Status"));
    /// ```
    pub fn message(&self) -> String {
        let parts: Vec<String> = self
            .colliding
            .iter()
            .map(|(kind, name)| format!("{kind} `{name}`"))
            .collect();
        format!(
            "feature '{}' lowers {} to the same Go identifier `{}` in the generated `{}gen` package — `go build` would reject it as a double declaration. Rename one of them so each construct lowers to a distinct identifier.",
            self.feature,
            parts.join(" and "),
            self.emitted_ident,
            self.feature,
        )
    }
}

/// Run CODEGEN-GO-IDENT-COLLISION-008 over one feature.
///
/// Collects the emitted Go identifier for every tracked construct
/// (enums, lifecycle enums, queries, commands, transitions) and reports
/// each identifier claimed by two or more *distinct* constructs.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::go_ident_collision_008::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature");
/// let _ = check(&feature, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    // emitted-ident -> the distinct constructs that claim it.
    let mut buckets: BTreeMap<String, Vec<Construct>> = BTreeMap::new();

    let mut record = |kind: ConstructKind, dsl_name: &str| {
        let ident = pascal_case(dsl_name);
        if ident.is_empty() {
            return;
        }
        let construct = Construct {
            kind,
            dsl_name: dsl_name.to_owned(),
        };
        let entry = buckets.entry(ident).or_default();
        // Dedupe identical (kind, name) pairs so a construct never reports
        // against itself. Genuine same-name duplicates within a single
        // family are owned by the per-family rules (e.g.
        // DUPLICATE-QUERY-NAME-001); this rule targets CROSS-construct
        // collisions (enum vs query vs command).
        if !entry.contains(&construct) {
            entry.push(construct);
        }
    };

    for enum_decl in &feature.enums {
        record(ConstructKind::Enum, &enum_decl.name);
    }
    for query in &feature.queries {
        record(ConstructKind::Query, query.name());
    }
    for command in &feature.commands {
        record(ConstructKind::Command, &command.name);
    }

    let mut findings = Vec::new();
    for (ident, constructs) in buckets {
        // A collision needs ≥2 constructs AND at least two distinct
        // construct *families* OR distinct DSL names — i.e. genuinely
        // different declarations that the per-family dedupe rules don't
        // already own. Two constructs of the same kind+name were deduped
        // above, so any bucket with len ≥ 2 here is a real cross-cut.
        if constructs.len() < 2 {
            continue;
        }
        let colliding: Vec<(&'static str, String)> = constructs
            .iter()
            .map(|c| (c.kind.label(), c.dsl_name.clone()))
            .collect();
        findings.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            emitted_ident: ident,
            colliding,
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn enum_and_query_collide_on_pascal_ident() {
        // `enum status` -> type `Status`; `query.lookup status` -> wrapper
        // func `Status`. Same package, same identifier: a Go double
        // declaration that `go build` would reject.
        let feature = lower(
            r#"
feature catalog
  domain
    enum status
      active
      archived

  resource Item
    org: Org required
    name: Text required

  query.lookup status by id: ID
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));

        assert_eq!(findings.len(), 1, "one collision expected: {findings:?}");
        let f = &findings[0];
        assert_eq!(f.emitted_ident, "Status");
        assert_eq!(f.feature, "catalog");
        // Both families are named.
        let labels: Vec<&str> = f.colliding.iter().map(|(k, _)| *k).collect();
        assert!(labels.contains(&"enum"), "enum side missing: {labels:?}");
        assert!(labels.contains(&"query"), "query side missing: {labels:?}");
        assert_eq!(Finding::CODE, "CODEGEN-GO-IDENT-COLLISION-008");
        assert_eq!(Finding::ID, "@correctness.go_ident_collision");
        assert!(f.message().contains("Status"));
        assert!(f.message().contains("double declaration"));
    }

    #[test]
    fn snake_vs_pascal_name_collision_fires() {
        // Acronym-aware caser collapses `id_lookup` and `IdLookup`-shaped
        // names; here a command `archive_item` and an enum `archive_item`
        // both pascal to `ArchiveItem`.
        let feature = lower(
            r#"
feature catalog
  domain
    enum archive_item
      yes
      no

  resource Item
    org: Org required
    name: Text required

  command archive_item
    updates Item
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));
        assert_eq!(findings.len(), 1, "expected one collision: {findings:?}");
        assert_eq!(findings[0].emitted_ident, "ArchiveItem");
    }

    #[test]
    fn distinct_idents_do_not_fire() {
        let feature = lower(
            r#"
feature catalog
  domain
    enum status
      active
      archived

  resource Item
    org: Org required
    name: Text required

  query.lookup find_item by id: ID

  command rename_item
    updates Item
"#,
        );

        assert!(
            check(&feature, Path::new("catalog.lzi")).is_empty(),
            "distinct names must not collide"
        );
    }

    #[test]
    fn same_family_duplicate_is_not_double_counted() {
        // Two queries with the same name are owned by DUPLICATE-QUERY-NAME-001,
        // not this rule. They dedupe to a single (Query, name) entry here, so
        // this cross-construct rule stays silent.
        let feature = lower(
            r#"
feature catalog
  query.list list_items
  query.list list_items
"#,
        );

        assert!(
            check(&feature, Path::new("catalog.lzi")).is_empty(),
            "intra-family duplicate is another rule's job"
        );
    }
}
