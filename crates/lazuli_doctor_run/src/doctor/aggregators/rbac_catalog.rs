//! Rbac (Role-Based Access Control) aggregator.
//!
//! Owns the package-level RBAC catalog diagnostics:
//!
//! * `RBAC-CATALOG-MISSING-001` (info) — `@role.*` references exist
//!   but no catalog declared.
//! * `RBAC-ROLE-UNDECLARED-001` (error) — `@role.X` references a
//!   role not in the catalog (when a catalog IS declared).
//! * `RBAC-MISSING-POLICY-001` (warning) — a feature with two or
//!   more policied commands has a sibling command/query without an
//!   explicit `policy`.
//!
//! Helpers:
//!
//! * `collect_known_roles` — gather first-class `@role.<name>`
//!   declarations from `policies` blocks and `policy_for ...:
//!   @role.<name>` defaults. Intentionally scoped: scanning every
//!   `@role.X` reference would self-resolve the very `by @role.X`
//!   line we're trying to validate.
//! * `collect_package_rbac_catalog` — re-parse each `.lzi` via
//!   `parse_package_skeleton`, concatenate `permission` / `role`
//!   declarations, and run them through `analyze_rbac_catalog` for
//!   closure + cycle issues.
//! * `extract_role_atoms` / `flush_missing_policy` — internal
//!   helpers folded into the same surface so the rule body and its
//!   support stay in one file.
//! * `line_col_for_offset_from_files` — locate the source position
//!   of a `RbacIssue.decl_offset` against the same files the
//!   analyzer read.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::doctor::parsers::is_lzi_path;
use crate::doctor::scanners::leading_spaces;
use crate::doctor::{DoctorDiagnostic, DoctorFile, DoctorSeverity, line_col_for_offset};

/// Collect every role name declared by a feature's `policies` block
/// (children at indent 4 referencing `@role.<name>`) or by an
/// `app.lzi` `policy_for ...: @role.<name>` default. Used by
/// `approval_role_unresolved_diagnostics`.
///
/// Intentionally scoped: scanning every `@role.X` reference in the
/// file would self-resolve the very `by @role.X` line we're trying
/// to validate. Only first-class declarations count.
pub(crate) fn collect_known_roles(files: &[DoctorFile]) -> BTreeSet<String> {
    let mut roles = BTreeSet::new();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            // Feature-level `policies` block at indent 2.
            if leading_spaces(line) == 2 && trimmed == "policies" {
                let mut j = i + 1;
                while j < lines.len() {
                    let inner = lines[j];
                    let inner_trim = inner.trim_start();
                    if inner_trim.is_empty() || inner_trim.starts_with('#') {
                        j += 1;
                        continue;
                    }
                    if leading_spaces(inner) <= 2 {
                        break;
                    }
                    // `<name>: @role.x, @scope.y, ...` — harvest only
                    // the @role.<name> entries.
                    if let Some((_, refs)) = inner_trim.split_once(':') {
                        extract_role_atoms(refs, &mut roles);
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            // Top-level `app.lzi` `policy_for <kinds>: @role.x, ...`
            // (or feature-level `policy_for` inside `defaults`).
            if let Some(rest) = trimmed.strip_prefix("policy_for ")
                && let Some((_, refs)) = rest.split_once(':')
            {
                extract_role_atoms(refs, &mut roles);
            }
            i += 1;
        }
    }
    roles
}

pub(crate) fn extract_role_atoms(refs: &str, roles: &mut BTreeSet<String>) {
    for token in refs.split(',') {
        let token = token.trim();
        if let Some(name) = token.strip_prefix("@role.") {
            let end = name
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(name.len());
            if end > 0 {
                roles.insert(name[..end].to_owned());
            }
        }
    }
}

/// Aggregate the package-level RBAC catalog by re-parsing each `.lzi`
/// file and concatenating `permission` / `role` decls. Cross-file
/// duplicates are caught by the analyzer's per-package pass.
pub(crate) fn collect_package_rbac_catalog(
    files: &[DoctorFile],
) -> (
    Option<lazuli_ir::RbacCatalog>,
    Vec<(PathBuf, lazuli_analyzer::rbac::RbacIssue)>,
) {
    use lazuli_syntax::{PackageSkeleton, PermissionDeclAst, RoleDeclAst, parse_package_skeleton};

    let mut all_permissions: Vec<PermissionDeclAst> = Vec::new();
    let mut all_roles: Vec<RoleDeclAst> = Vec::new();
    let mut file_of_decl: BTreeMap<usize, PathBuf> = BTreeMap::new();

    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let Ok(pkg) = parse_package_skeleton(&file.source) else {
            continue;
        };
        for p in pkg.permissions {
            file_of_decl.insert(all_permissions.len(), file.path.clone());
            all_permissions.push(p);
        }
        for r in pkg.roles {
            // Use a disjoint key space (roles indexed by 1_000_000 + i)
            // to avoid collision with permission indices.
            file_of_decl.insert(1_000_000 + all_roles.len(), file.path.clone());
            all_roles.push(r);
        }
    }

    if all_permissions.is_empty() && all_roles.is_empty() {
        return (None, Vec::new());
    }

    let pkg = PackageSkeleton {
        features: Vec::new(),
        permissions: all_permissions,
        roles: all_roles,
    };
    let (catalog, issues) = lazuli_analyzer::rbac::analyze_rbac_catalog(&pkg);
    // For now, attach the first .lzi file with rbac decls to each issue.
    let representative = files
        .iter()
        .find(|f| {
            is_lzi_path(&f.path) && f.source.contains("\nrole ") || f.source.starts_with("role ")
        })
        .or_else(|| files.iter().find(|f| is_lzi_path(&f.path)))
        .map(|f| f.path.clone())
        .unwrap_or_default();
    let issues_with_path: Vec<(PathBuf, _)> = issues
        .into_iter()
        .map(|i| (representative.clone(), i))
        .collect();
    (catalog, issues_with_path)
}

/// Convert analyzer-emitted RBAC issues into doctor diagnostics.
pub(crate) fn rbac_catalog_diagnostics(
    files: &[DoctorFile],
) -> (Vec<DoctorDiagnostic>, Option<lazuli_ir::RbacCatalog>) {
    let (catalog, issues) = collect_package_rbac_catalog(files);
    let mut out: Vec<DoctorDiagnostic> = Vec::new();
    for (path, issue) in issues {
        let line = if let Some((start, _)) = issue.span {
            line_col_for_offset_from_files(files, &path, start).0
        } else {
            1
        };
        let severity = match issue.code {
            "RBAC-PERM-UNUSED-001" => DoctorSeverity::Warning,
            _ => DoctorSeverity::Error,
        };
        out.push(DoctorDiagnostic {
            path,
            line,
            column: 1,
            severity,
            code: issue.code.to_owned(),
            message: issue.message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    (out, catalog)
}

/// Resolve a byte offset within a given file path to (line, column).
pub(crate) fn line_col_for_offset_from_files(
    files: &[DoctorFile],
    path: &Path,
    offset: usize,
) -> (usize, usize) {
    for f in files {
        if f.path == path {
            return line_col_for_offset(&f.source, offset);
        }
    }
    (1, 1)
}

/// RBAC-ROLE-UNDECLARED-001 — when a catalog IS declared, every
/// `@role.X` mention in `policies` / `policy_for` must resolve to a
/// catalog role. Returns one diagnostic per orphan reference (deduped).
pub(crate) fn rbac_role_undeclared_diagnostics(
    files: &[DoctorFile],
    catalog: &lazuli_ir::RbacCatalog,
) -> Vec<DoctorDiagnostic> {
    let mut out = Vec::new();
    let catalog_roles: BTreeSet<String> = catalog.roles.iter().map(|r| r.name.clone()).collect();
    let mentioned = collect_known_roles(files);
    for role in mentioned.difference(&catalog_roles) {
        // Find the first file that mentions this role.
        for file in files {
            if !is_lzi_path(&file.path) {
                continue;
            }
            let needle = format!("@role.{}", role);
            if let Some(idx) = file.source.find(&needle) {
                let (line, _) = line_col_for_offset(&file.source, idx);
                out.push(DoctorDiagnostic {
                    path: file.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "RBAC-ROLE-UNDECLARED-001".to_owned(),
                    message: format!(
                        "`@role.{}` references a role not declared in the RBAC catalog (declare `role {}` at top level or remove the reference).",
                        role, role
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                break;
            }
        }
    }
    out
}

/// RBAC-CATALOG-MISSING-001 (info advisory) — fires when the legacy
/// implicit-role-set has entries but no `role` / `permission` blocks
/// were authored at top level. Migration hint per
/// `docs/proposals/rbac-catalog-vocab.md` §Backwards compatibility.
pub(crate) fn rbac_catalog_missing_diagnostics(
    files: &[DoctorFile],
    catalog_present: bool,
) -> Vec<DoctorDiagnostic> {
    if catalog_present {
        return Vec::new();
    }
    let implicit = collect_known_roles(files);
    if implicit.is_empty() {
        return Vec::new();
    }
    // Surface a single hint on the first `.lzi` file that mentions a
    // role. Severity is `Info` mapped to LSP Hint.
    let role_names: Vec<String> = implicit.into_iter().collect();
    // 2026-05-27 — honor `# doctor:allow RBAC-CATALOG-MISSING-001` in
    // ANY .lzi file. Pilots that legitimately rely on the legacy
    // implicit-role-set (no RBAC catalog yet, migration tracked
    // elsewhere) can opt out of the migration-hint advisory.
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        if lazuli_doctor::allow_comment::source_contains_doctor_allow(
            &file.source,
            "RBAC-CATALOG-MISSING-001",
        ) {
            return Vec::new();
        }
    }
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let needle = format!("@role.{}", role_names[0]);
        if let Some(idx) = file.source.find(&needle) {
            let (line, _) = line_col_for_offset(&file.source, idx);
            return vec![DoctorDiagnostic {
                path: file.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Info,
                code: "RBAC-CATALOG-MISSING-001".to_owned(),
                message: format!(
                    "package uses `@role.*` references ({}) but declares no `role` / `permission` catalog. Consider migrating to a top-level RBAC catalog (see docs/proposals/rbac-catalog-vocab.md).",
                    role_names.join(", ")
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }];
        }
    }
    Vec::new()
}

/// RBAC-MISSING-POLICY-001 — feature mixes policied + unpoliced
/// commands/queries. Suspicious gap; explicit `policy @scope.public`
/// opts out. Warning level. Per-feature; scans for indent-2 `command`/
/// `query.*` blocks and checks if their indent-4 children include a
/// `policy ` line.
pub(crate) fn rbac_missing_policy_diagnostics(files: &[DoctorFile]) -> Vec<DoctorDiagnostic> {
    let mut out = Vec::new();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut feature: Option<String> = None;
        let mut feature_line: usize = 0;
        // For each feature, count callables with/without `policy`.
        let mut policied: Vec<String> = Vec::new();
        let mut unpoliced: Vec<(String, usize)> = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
                // Flush prior feature.
                if let Some(_fname) = feature.take() {
                    flush_missing_policy(&mut out, &file.path, &policied, &unpoliced);
                }
                feature = trimmed
                    .strip_prefix("feature ")
                    .map(|n| n.trim().to_owned());
                feature_line = i + 1;
                policied.clear();
                unpoliced.clear();
                i += 1;
                continue;
            }
            let _ = feature_line;
            if leading_spaces(line) == 2
                && (trimmed.starts_with("command ")
                    || trimmed.starts_with("query.list ")
                    || trimmed.starts_with("query.lookup ")
                    || trimmed.starts_with("query.sql ")
                    || trimmed.starts_with("query.view ")
                    || trimmed.starts_with("api "))
            {
                let name = trimmed.split_whitespace().nth(1).unwrap_or("").to_owned();
                // Scan body at indent 4 for a `policy ` line.
                let mut has_policy = false;
                let mut j = i + 1;
                while j < lines.len() {
                    let inner = lines[j];
                    let inner_trim = inner.trim_start();
                    if inner_trim.is_empty() || inner_trim.starts_with('#') {
                        j += 1;
                        continue;
                    }
                    if leading_spaces(inner) <= 2 {
                        break;
                    }
                    if leading_spaces(inner) == 4 && inner_trim.starts_with("policy ") {
                        has_policy = true;
                        break;
                    }
                    j += 1;
                }
                if has_policy {
                    policied.push(name);
                } else {
                    unpoliced.push((name, i + 1));
                }
            }
            i += 1;
        }
        if let Some(_fname) = feature.take() {
            flush_missing_policy(&mut out, &file.path, &policied, &unpoliced);
        }
    }
    out
}

pub(crate) fn flush_missing_policy(
    out: &mut Vec<DoctorDiagnostic>,
    path: &Path,
    policied: &[String],
    unpoliced: &[(String, usize)],
) {
    if policied.len() < 2 || unpoliced.is_empty() {
        return;
    }
    for (name, line) in unpoliced {
        out.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line: *line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "RBAC-MISSING-POLICY-001".to_owned(),
            message: format!(
                "`{}` declares no explicit `policy` while sibling callables do; add `policy <atoms>` (or `policy @scope.public` to opt out) for visibility.",
                name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}
