//! RB.B — analyzer pass for the RBAC catalog.
//!
//! Consumes `PackageSkeleton.permissions` / `.roles` (surface AST) and
//! produces `ir::RbacCatalog` with closures computed per role. Cycle
//! detection runs here. Unknown grants / inherits refs become
//! `RbacAnalyzeIssue` records that doctor surfaces as
//! `RBAC-*` diagnostics; the analyzer itself does NOT abort lowering
//! on these — the catalog is still produced (partial) so codegen has a
//! deterministic baseline.
//!
//! See `docs/proposals/rbac-catalog-vocab.md` §Analyzer.
//!
//! Wire-thin: the closure computation is a DFS chain walk with a
//! visited set. Linear in `roles * permissions`. No external crates.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir as ir;
use lazuli_syntax::{PackageSkeleton, RoleGrantsAst};

/// Non-fatal issue surfaced by the analyzer. Doctor renders these to
/// `RBAC-*-001` diagnostics with the exact code listed below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbacIssue {
    /// Doctor code (e.g., `RBAC-PERM-DUPLICATE-001`).
    pub code: &'static str,
    /// Human message (Doctor wraps with role/permission name).
    pub message: String,
    /// Source-byte span if known (from the offending AST node).
    pub span: Option<(usize, usize)>,
}

/// Analyzer output: the lowered `RbacCatalog` + any non-fatal issues
/// for doctor to surface. When the source has no `permission` or
/// `role` decls, returns `(None, vec![])` and the caller passes
/// `None` into `Module.rbac`.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::analyze_rbac_catalog;
/// use lazuli_syntax::PackageSkeleton;
///
/// let pkg: PackageSkeleton = unimplemented!("from parser");
/// let (catalog, issues) = analyze_rbac_catalog(&pkg);
/// assert!(catalog.is_some() || pkg.permissions.is_empty());
/// assert!(issues.iter().all(|i| !i.code.is_empty()));
/// ```
pub fn analyze_rbac_catalog(pkg: &PackageSkeleton) -> (Option<ir::RbacCatalog>, Vec<RbacIssue>) {
    if pkg.permissions.is_empty() && pkg.roles.is_empty() {
        return (None, Vec::new());
    }

    let mut issues: Vec<RbacIssue> = Vec::new();

    // -- Permission catalog -----------------------------------------------
    let mut perm_entries: Vec<ir::PermissionEntry> = Vec::new();
    let mut perm_names: BTreeSet<String> = BTreeSet::new();
    for p in &pkg.permissions {
        if !perm_names.insert(p.name.clone()) {
            issues.push(RbacIssue {
                code: "RBAC-PERM-DUPLICATE-001",
                message: format!("duplicate permission `{}` in catalog", p.name),
                span: Some((p.span.start, p.span.end)),
            });
            continue;
        }
        perm_entries.push(ir::PermissionEntry {
            name: p.name.clone(),
            segments: p.segments.clone(),
            span_ref: Some(ir::SpanRef {
                start: p.span.start,
                end: p.span.end,
            }),
        });
    }

    // -- Role catalog -----------------------------------------------------
    // First pass: dedupe roles + validate references.
    let mut role_entries: Vec<ir::RoleEntry> = Vec::new();
    let mut role_names: BTreeSet<String> = BTreeSet::new();
    for r in &pkg.roles {
        if !role_names.insert(r.name.clone()) {
            issues.push(RbacIssue {
                code: "RBAC-ROLE-DUPLICATE-001",
                message: format!("duplicate role `{}` in catalog", r.name),
                span: Some((r.span.start, r.span.end)),
            });
            continue;
        }
        let grants = match &r.grants {
            RoleGrantsAst::Explicit(list) => {
                // Validate each ref against catalog.
                for g in list {
                    if !perm_names.contains(g) {
                        issues.push(RbacIssue {
                            code: "RBAC-PERM-UNKNOWN-001",
                            message: format!(
                                "role `{}` grants undeclared permission `{}`",
                                r.name, g
                            ),
                            span: Some((r.span.start, r.span.end)),
                        });
                    }
                }
                ir::RoleGrants::Explicit(list.clone())
            }
            RoleGrantsAst::All => ir::RoleGrants::All,
            RoleGrantsAst::InheritedOnly => ir::RoleGrants::InheritedOnly,
        };
        role_entries.push(ir::RoleEntry {
            name: r.name.clone(),
            inherits: r.inherits.clone(),
            grants,
            closure: Vec::new(), // computed below
            span_ref: Some(ir::SpanRef {
                start: r.span.start,
                end: r.span.end,
            }),
        });
    }

    // Validate `inherits` refs.
    let role_names_for_inherit: BTreeSet<String> =
        role_entries.iter().map(|r| r.name.clone()).collect();
    for r in &pkg.roles {
        if let Some(parent) = &r.inherits {
            if !role_names_for_inherit.contains(parent) {
                issues.push(RbacIssue {
                    code: "RBAC-ROLE-INHERIT-UNKNOWN-001",
                    message: format!("role `{}` inherits from unknown role `{}`", r.name, parent),
                    span: Some((r.span.start, r.span.end)),
                });
            }
        }
    }

    // Cycle detection — DFS visited set, returns first cycle edge found.
    let role_parent: BTreeMap<String, Option<String>> = role_entries
        .iter()
        .map(|r| (r.name.clone(), r.inherits.clone()))
        .collect();
    let mut cycles_reported: BTreeSet<String> = BTreeSet::new();
    for r in &role_entries {
        let mut chain: Vec<String> = Vec::new();
        let mut cur = Some(r.name.clone());
        while let Some(name) = cur {
            if chain.contains(&name) {
                // Cycle. Report once per cycle (sorted-by-name).
                let mut key = chain.clone();
                key.push(name.clone());
                key.sort();
                let key_str = key.join(",");
                if cycles_reported.insert(key_str) {
                    issues.push(RbacIssue {
                        code: "RBAC-CYCLE-001",
                        message: format!(
                            "role inheritance cycle detected: {} -> {}",
                            chain.join(" -> "),
                            name
                        ),
                        span: r.span_ref.map(|s| (s.start, s.end)),
                    });
                }
                break;
            }
            chain.push(name.clone());
            cur = role_parent.get(&name).cloned().flatten();
        }
    }

    // Closure computation. For each role, walk chain via inherits,
    // collecting grants. `All` short-circuits to full permission list.
    let all_perm_names: Vec<String> = perm_entries.iter().map(|p| p.name.clone()).collect();
    let grants_by_role: BTreeMap<String, ir::RoleGrants> = role_entries
        .iter()
        .map(|r| (r.name.clone(), r.grants.clone()))
        .collect();
    for r in role_entries.iter_mut() {
        let mut acc: BTreeSet<String> = BTreeSet::new();
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<String> = vec![r.name.clone()];
        let mut bail_all = false;
        while let Some(name) = stack.pop() {
            if !visited.insert(name.clone()) {
                continue; // cycle guard
            }
            match grants_by_role.get(&name) {
                Some(ir::RoleGrants::All) => {
                    bail_all = true;
                    break;
                }
                Some(ir::RoleGrants::Explicit(list)) => {
                    for g in list {
                        if all_perm_names.contains(g) {
                            acc.insert(g.clone());
                        }
                    }
                }
                Some(ir::RoleGrants::InheritedOnly) => {}
                None => {}
            }
            if let Some(Some(parent)) = role_parent.get(&name) {
                stack.push(parent.clone());
            }
        }
        let closure: Vec<String> = if bail_all {
            all_perm_names.clone()
        } else {
            acc.into_iter().collect()
        };
        r.closure = closure;
    }

    // Unused permissions (warning) — only when no role has `grants_all`,
    // because `grants_all` transitively covers every permission.
    let any_grants_all = role_entries
        .iter()
        .any(|r| matches!(r.grants, ir::RoleGrants::All));
    if !any_grants_all {
        let used: BTreeSet<String> = role_entries
            .iter()
            .flat_map(|r| r.closure.iter().cloned())
            .collect();
        for p in &perm_entries {
            if !used.contains(&p.name) {
                issues.push(RbacIssue {
                    code: "RBAC-PERM-UNUSED-001",
                    message: format!("permission `{}` is declared but no role grants it", p.name),
                    span: p.span_ref.map(|s| (s.start, s.end)),
                });
            }
        }
    }

    (
        Some(ir::RbacCatalog {
            permissions: perm_entries,
            roles: role_entries,
        }),
        issues,
    )
}

/// Returns true iff `perm` is in the closure of `role` in the given
/// catalog. Used by codegen and doctor.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::role_grants_permission;
/// use lazuli_ir::RbacCatalog;
///
/// let catalog: RbacCatalog = unimplemented!("from analyze_rbac_catalog");
/// let granted = role_grants_permission(&catalog, "Admin", "users.delete");
/// # let _ = granted;
/// ```
pub fn role_grants_permission(catalog: &ir::RbacCatalog, role: &str, perm: &str) -> bool {
    catalog
        .roles
        .iter()
        .find(|r| r.name == role)
        .map(|r| r.closure.iter().any(|p| p == perm))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_syntax::parse_package_skeleton;

    fn analyze(src: &str) -> (ir::RbacCatalog, Vec<RbacIssue>) {
        let pkg = parse_package_skeleton(src).expect("parses");
        let (cat, issues) = analyze_rbac_catalog(&pkg);
        (cat.expect("catalog"), issues)
    }

    #[test]
    fn closure_includes_inherited() {
        let (cat, issues) = analyze(
            r#"
permission users:read
permission users:create

role viewer
  grants
    users:read

role editor
  inherits viewer
  grants
    users:create
"#,
        );
        assert!(issues.is_empty(), "issues: {:?}", issues);
        let editor = cat.roles.iter().find(|r| r.name == "editor").unwrap();
        assert!(editor.closure.contains(&"users:read".to_string()));
        assert!(editor.closure.contains(&"users:create".to_string()));
    }

    #[test]
    fn grants_all_expands_to_full_set() {
        let (cat, _) = analyze(
            r#"
permission users:read
permission users:create
permission proposals:approve

role admin
  grants_all
"#,
        );
        let admin = cat.roles.iter().find(|r| r.name == "admin").unwrap();
        assert_eq!(admin.closure.len(), 3);
    }

    #[test]
    fn cycle_detected() {
        let (_cat, issues) = analyze(
            r#"
permission p:r
role a
  inherits b
  grants
    p:r
role b
  inherits a
  grants
    p:r
"#,
        );
        assert!(issues.iter().any(|i| i.code == "RBAC-CYCLE-001"));
    }

    #[test]
    fn unknown_grant_reported() {
        let (_cat, issues) = analyze(
            r#"
permission users:read

role viewer
  grants
    users:write
"#,
        );
        assert!(
            issues.iter().any(|i| i.code == "RBAC-PERM-UNKNOWN-001"),
            "{:?}",
            issues
        );
    }

    #[test]
    fn unused_permission_reported() {
        let (_cat, issues) = analyze(
            r#"
permission users:read
permission users:write

role viewer
  grants
    users:read
"#,
        );
        assert!(issues.iter().any(|i| i.code == "RBAC-PERM-UNUSED-001"));
    }

    #[test]
    fn grants_all_silences_unused() {
        let (_cat, issues) = analyze(
            r#"
permission users:read
permission users:write

role viewer
  grants
    users:read

role admin
  grants_all
"#,
        );
        assert!(!issues.iter().any(|i| i.code == "RBAC-PERM-UNUSED-001"));
    }
}
