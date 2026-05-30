//! POLICY-CATEGORY-SHADOWS-EFFECT-001 — a `policies` category named after a
//! command effect verb.
//!
//! Severity: error (iron-hand + production — the CRUD/effect collision must
//! not ship); warning under tdd-strict for the migration window.
//!
//! Fires when a feature-level `policies` block declares a CATEGORY whose name
//! shadows a command effect verb: the singular `create` / `read` / `update` /
//! `delete` or the plural effect spellings `creates` / `updates` / `deletes` /
//! `reads`. At a `policy @policy.create` reference site the name reads as a
//! write EFFECT, not an authorization category — the very near-collision the
//! `@policy.` prefix used to paper over (SPEC-07 C kills it at source). The fix
//! is a semantic rename: `create`→`author`, `read`→`view`, `update`→`edit`,
//! `delete`→`remove`.
//!
//! ## Scope — category position only
//!
//! A policy CATEGORY is a `<name>:` line at the direct-child indent of a
//! `policies` block. The per-field `read:` / `write:` access directions under a
//! `fields <Resource>` sub-block are a DIFFERENT closed catalog (access
//! direction, not a user-named category) and are deeper than the category
//! indent, so they are never flagged.

use std::path::{Path, PathBuf};

/// The closed set of effect-verb spellings a policy category must not shadow:
/// the singular CRUD verbs (which read as effect intent at a `policy <X>` site)
/// and the plural command-effect spellings.
const EFFECT_SHADOWS: &[&str] = &[
    "create", "read", "update", "delete", "creates", "reads", "updates", "deletes",
];

/// The semantic, non-shadowing rename the fix-it suggests for each CRUD verb.
fn suggested_rename(name: &str) -> &'static str {
    match name {
        "create" | "creates" => "author",
        "read" | "reads" => "view",
        "update" | "updates" => "edit",
        "delete" | "deletes" => "remove",
        _ => "manage",
    }
}

/// One POLICY-CATEGORY-SHADOWS-EFFECT-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source path (`.lzi`).
    pub path: PathBuf,
    /// 1-indexed source line of the offending category declaration.
    pub line: usize,
    /// The offending category name (e.g. `create`).
    pub category: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "POLICY-CATEGORY-SHADOWS-EFFECT-001";

    /// Render the user-facing diagnostic body with the semantic-rename fix-it.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::policy_category_shadows_effect_001::Finding;
    ///
    /// let f = Finding { path: PathBuf::from("f.lzi"), line: 5, category: "create".into() };
    /// assert!(f.message().contains("author"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "policy category `{cat}` shadows a command effect verb — at a `policy @policy.{cat}` \
             site it reads as a write effect, not an authorization category (SPEC-07 C). Rename to \
             a semantic name, e.g. `{cat}` → `{rename}` (and its `@policy.{cat}` references).",
            cat = self.category,
            rename = suggested_rename(&self.category),
        )
    }
}

/// Run POLICY-CATEGORY-SHADOWS-EFFECT-001 over a `.lzi` source. One finding per
/// effect-verb-named category at a `policies` block's direct-child indent.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::correctness::policy_category_shadows_effect_001::check;
///
/// let src = "feature f\n  policies\n    create: @role.admin\n";
/// assert_eq!(check(src, Path::new("f.lzi")).len(), 1);
///
/// // Field-level read:/write: under `fields` are access directions, not flagged.
/// let ok = "feature f\n  policies\n    fields C\n      email\n        read: @role.admin\n";
/// assert!(check(ok, Path::new("f.lzi")).is_empty());
/// ```
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Indent of the open `policies` header, and the category child indent
    // (header + 2). `None` ⇒ not inside a `policies` block.
    let mut policies_indent: Option<usize> = None;
    for (idx, raw) in source.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        // Close the policies block on dedent to its level or shallower.
        if let Some(p_indent) = policies_indent {
            if indent <= p_indent {
                policies_indent = None;
            }
        }
        if trimmed == "policies" {
            policies_indent = Some(indent);
            continue;
        }
        let Some(p_indent) = policies_indent else {
            continue;
        };
        // Only the direct-child indent carries CATEGORY declarations. `fields
        // <Resource>` sits at the same indent but has no `:`; the per-field
        // `read:`/`write:` directions live deeper and are skipped here.
        if indent != p_indent + 2 {
            continue;
        }
        if let Some(name) = category_name(trimmed) {
            if EFFECT_SHADOWS.contains(&name) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    line: idx + 1,
                    category: name.to_string(),
                });
            }
        }
    }
    findings
}

/// The category name in a `<name>: …` line, or `None` when the line is not a
/// `<lower_ident>:` category declaration (e.g. `fields Customer`).
fn category_name(trimmed: &str) -> Option<&str> {
    let colon = trimmed.find(':')?;
    let name = &trimmed[..colon];
    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        Some(name)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_each_crud_category() {
        let src = "\
feature f
  policies
    create: @role.admin
    read: @scope.same_org
    update: @role.admin
    delete: @role.admin
";
        let f = check(src, Path::new("f.lzi"));
        assert_eq!(f.len(), 4);
        assert_eq!(f[0].category, "create");
        assert!(f[0].message().contains("author"));
        assert!(f[1].message().contains("view"));
    }

    #[test]
    fn fires_on_plural_effect_spellings() {
        let src = "feature f\n  policies\n    creates: @role.admin\n    deletes: @role.admin\n";
        assert_eq!(check(src, Path::new("f.lzi")).len(), 2);
    }

    #[test]
    fn silent_on_semantic_category_names() {
        let src = "\
feature f
  policies
    author: @role.admin
    view: @scope.same_org
    edit: @role.admin
    remove: @role.admin
    manage: @role.admin
";
        assert!(check(src, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn silent_on_field_level_read_write() {
        // `read:`/`write:` under a `fields <Resource>` sub-block are access
        // directions (deeper than the category indent), never flagged.
        let src = "\
feature f
  policies
    view: @scope.same_org
    fields Customer
      name
        read: @scope.same_org
        write: @role.admin
      email
        read: @scope.same_org
        write: @role.admin
";
        assert!(check(src, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn fires_only_in_category_position_not_after_block_closes() {
        // A `read:` after the policies block dedents (e.g. a different
        // construct) must not be misread as a category.
        let src = "\
feature f
  policies
    view: @scope.same_org
  query.list q
    filters
      read: something
";
        assert!(check(src, Path::new("f.lzi")).is_empty());
    }
}
