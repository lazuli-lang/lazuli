//! Top-level package skeleton + RBAC catalog parser (RB.A vocabulary).
//!
//! A `.lzi` package file can carry, alongside its `feature` blocks, a
//! catalog of `permission` and `role` declarations at indent 0. These
//! describe authorization vocabulary scoped to the package — they're
//! siblings of `feature` / `app` / `workspace`, never nested inside one.
//!
//! ## Entry points
//!
//! - `parse_package_skeleton` — the canonical package-level entry. Returns
//!   features (delegating to `parse_feature_skeletons`) plus the RBAC
//!   catalog (`permission` list + `role` list).
//!
//! The parse is additive: it never touches the existing per-callable
//! struct literals. Existing callers using `parse_feature_skeletons`
//! directly continue to work — `parse_package_skeleton` is the
//! superset.
//!
//! ## RBAC grammar (closed)
//!
//! ```text
//! permission <segment>:<segment>[:<segment>[:<segment>]]   # 2-4 segs
//! role <name>
//!   inherits <single-parent>                  # at most one
//!   grants                                    # XOR with grants_all
//!     <permission-ref-1>
//!     <permission-ref-2>
//!     ...
//!   grants_all                                # XOR with grants
//! ```
//!
//! Multi-parent inheritance is intentionally rejected in v0.1 — declare
//! a single parent role per `role`.
//!
//! ## See also
//!
//! - `docs/proposals/rbac-catalog-vocab.md` — the RB.A vocabulary spec.
//! - `lazuli_ir::nodes::rbac` — the typed lowering target.

use super::super::common::{SourceLine, is_trivia, line_error, source_lines};
use super::super::error::ParseError;
use super::parse_feature_skeletons;

use crate::ast::{PackageSkeleton, PermissionDeclAst, RoleDeclAst, RoleGrantsAst, Span};

// =============================================================================
// RB.A — top-level RBAC catalog parser (`permission` / `role`).
// -----------------------------------------------------------------------------
// Package-scoped vocab declared at indent 0, sibling to `feature` /
// `app` / `workspace`. The parse is additive: existing top-level
// constructs are untouched. See `docs/proposals/rbac-catalog-vocab.md`.
// =============================================================================

/// Parse a `.lzi` source for the full top-level package skeleton —
/// features (delegating to `parse_feature_skeletons`) plus RBAC
/// catalog declarations (`permission <ident>`, `role <name>`).
///
/// This is the new canonical entry point for package-level parsing.
/// Existing callers using `parse_feature_skeletons` directly continue
/// to work (the function is unchanged); new callers wanting the
/// catalog go through `parse_package_skeleton`.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_syntax::parse_package_skeleton;
///
/// // Fixture string elided — the real authoring shape requires the
/// // full indent + sibling structure across feature / permission /
/// // role blocks. See the `parse_package_skeleton` unit tests for
/// // ready-to-paste inputs.
/// let src = include_str!("../fixtures/package.lzi");
/// let pkg = parse_package_skeleton(src).expect("parses");
/// assert!(!pkg.features.is_empty());
/// ```
pub fn parse_package_skeleton(source: &str) -> Result<PackageSkeleton, ParseError> {
    let features = parse_feature_skeletons(source)?;
    let (permissions, roles) = parse_rbac_catalog_decls(source)?;
    Ok(PackageSkeleton {
        features,
        permissions,
        roles,
    })
}

/// Walk `source` and harvest top-level `permission <ident>` and
/// `role <name>` declarations. Skips lines inside any indented block
/// (features, agents, etc.); recognises only indent-0 directives.
fn parse_rbac_catalog_decls(
    source: &str,
) -> Result<(Vec<PermissionDeclAst>, Vec<RoleDeclAst>), ParseError> {
    let lines = source_lines(source);
    let mut permissions = Vec::new();
    let mut roles = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent == 0 {
            if let Some(rest) = trimmed.strip_prefix("permission ") {
                let name = rest.trim().to_owned();
                let decl = parse_permission_decl(line, name)?;
                permissions.push(decl);
                i += 1;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("role ") {
                let header_name = rest.trim().to_owned();
                if header_name.is_empty() {
                    return Err(line_error(line, "`role` requires a name: `role <name>`"));
                }
                if !is_rbac_role_ident(&header_name) {
                    return Err(line_error(line, "role name must match `[a-z][a-z0-9_]*`"));
                }
                let (decl, next) = parse_role_decl(&lines, i, header_name)?;
                roles.push(decl);
                i = next;
                continue;
            }
        }

        i += 1;
    }

    Ok((permissions, roles))
}

fn parse_permission_decl(
    line: &SourceLine<'_>,
    raw_name: String,
) -> Result<PermissionDeclAst, ParseError> {
    let name = raw_name.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(
            line,
            "`permission` requires an identifier: `permission <resource>:<action>`",
        ));
    }
    if name.contains(char::is_whitespace) {
        return Err(line_error(
            line,
            "`permission` accepts a single colon-separated identifier per line",
        ));
    }
    let segments: Vec<String> = name.split(':').map(str::to_owned).collect();
    if segments.len() < 2 || segments.len() > 4 {
        return Err(line_error(
            line,
            "permission identifier requires 2 to 4 colon-separated segments \
             (`<resource>:<action>` ... `<resource>:<action>:<scope>:<qualifier>`)",
        ));
    }
    for seg in &segments {
        if seg.is_empty() {
            return Err(line_error(
                line,
                "permission identifier cannot have empty segments (no leading, trailing, or doubled colons)",
            ));
        }
        if !is_rbac_segment_ident(seg) {
            return Err(line_error(
                line,
                "permission segments must match `[a-z][a-z0-9_]*`",
            ));
        }
    }
    Ok(PermissionDeclAst {
        name,
        segments,
        span: Span::new(line.start, line.end),
    })
}

fn parse_role_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    name: String,
) -> Result<(RoleDeclAst, usize), ParseError> {
    let header = &lines[start];
    let mut inherits: Option<String> = None;
    let mut grants: Option<Vec<String>> = None;
    let mut grants_all = false;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent == 0 {
            break;
        }
        if line.indent != 2 {
            return Err(line_error(
                line,
                "`role` children use two-space indentation (`inherits`, `grants`, `grants_all`)",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("inherits ") {
            if inherits.is_some() {
                return Err(line_error(
                    line,
                    "`role` may declare at most one `inherits` clause",
                ));
            }
            let parent = rest.trim();
            if parent.contains(',') {
                return Err(line_error(
                    line,
                    "multi-parent inheritance (`inherits A, B`) is not supported in v0.1; \
                     declare a single parent role per `role`",
                ));
            }
            if !is_rbac_role_ident(parent) {
                return Err(line_error(
                    line,
                    "`inherits` parent must be a single `[a-z][a-z0-9_]*` role name",
                ));
            }
            inherits = Some(parent.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "grants_all" {
            if grants_all {
                return Err(line_error(
                    line,
                    "`role` may declare `grants_all` at most once",
                ));
            }
            if grants.is_some() {
                return Err(line_error(
                    line,
                    "`role` may declare either `grants` or `grants_all`, not both",
                ));
            }
            grants_all = true;
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "grants" {
            if grants.is_some() {
                return Err(line_error(
                    line,
                    "`role` may declare at most one `grants` block",
                ));
            }
            if grants_all {
                return Err(line_error(
                    line,
                    "`role` may declare either `grants` or `grants_all`, not both",
                ));
            }
            let (entries, next) = parse_role_grants_block(lines, i)?;
            grants = Some(entries);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        return Err(line_error(
            line,
            "`role` children are `inherits <role>`, `grants` block, or `grants_all`",
        ));
    }

    let grants_kind = if grants_all {
        RoleGrantsAst::All
    } else if let Some(list) = grants {
        RoleGrantsAst::Explicit(list)
    } else {
        RoleGrantsAst::InheritedOnly
    };

    Ok((
        RoleDeclAst {
            name,
            inherits,
            grants: grants_kind,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_role_grants_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<String>, usize), ParseError> {
    let header = &lines[start];
    let mut entries = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header.indent {
            break;
        }
        if line.indent != header.indent + 2 {
            return Err(line_error(
                line,
                "`grants` entries use four-space indentation (one permission ref per line)",
            ));
        }
        let tok = trimmed;
        if tok.contains(char::is_whitespace) {
            return Err(line_error(
                line,
                "`grants` entries are bare permission identifiers (one per line)",
            ));
        }
        let segments: Vec<&str> = tok.split(':').collect();
        if segments.len() < 2 || segments.len() > 4 || segments.iter().any(|s| s.is_empty()) {
            return Err(line_error(
                line,
                "permission ref must be 2-4 non-empty colon-separated segments",
            ));
        }
        for seg in &segments {
            if !is_rbac_segment_ident(seg) {
                return Err(line_error(
                    line,
                    "permission ref segments must match `[a-z][a-z0-9_]*`",
                ));
            }
        }
        entries.push(tok.to_owned());
        i += 1;
    }
    if entries.is_empty() {
        return Err(line_error(
            header,
            "`grants` block requires at least one permission ref \
             (use `grants_all` or omit the block to inherit only)",
        ));
    }
    Ok((entries, i))
}

fn is_rbac_role_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_rbac_segment_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// =============================================================================
// RB.A — RBAC catalog parser smoke tests.
// =============================================================================
#[cfg(test)]
mod package_parser_tests {
    use super::parse_package_skeleton;
    use crate::RoleGrantsAst;

    #[test]
    fn parses_minimal_rbac_catalog() {
        let source = r#"
permission users:read
permission users:create
permission proposals:read:own

role viewer
  grants
    users:read
    proposals:read:own

role editor
  inherits viewer
  grants
    users:create

role admin
  grants_all
"#;
        let pkg = parse_package_skeleton(source).expect("parses");
        assert_eq!(pkg.permissions.len(), 3);
        assert_eq!(pkg.permissions[0].name, "users:read");
        assert_eq!(pkg.permissions[2].segments.len(), 3);

        assert_eq!(pkg.roles.len(), 3);
        assert_eq!(pkg.roles[0].name, "viewer");
        assert!(matches!(pkg.roles[0].grants, RoleGrantsAst::Explicit(_)));
        assert_eq!(pkg.roles[1].inherits.as_deref(), Some("viewer"));
        assert!(matches!(pkg.roles[2].grants, RoleGrantsAst::All));
    }

    #[test]
    fn rejects_invalid_permission_segments() {
        let source = "permission users\n";
        let err = parse_package_skeleton(source).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("2 to 4 colon-separated"));
    }

    #[test]
    fn rejects_too_many_permission_segments() {
        let source = "permission a:b:c:d:e\n";
        let err = parse_package_skeleton(source).unwrap_err();
        assert!(format!("{:?}", err).contains("2 to 4"));
    }

    #[test]
    fn rejects_empty_permission_segment() {
        let source = "permission users::read\n";
        let err = parse_package_skeleton(source).unwrap_err();
        assert!(format!("{:?}", err).contains("empty segments"));
    }

    #[test]
    fn rejects_multi_parent_inherits() {
        let source = r#"
role admin
  inherits a, b
"#;
        let err = parse_package_skeleton(source).unwrap_err();
        assert!(
            format!("{:?}", err).contains("Multi-parent")
                || format!("{:?}", err).contains("multi-parent")
        );
    }

    #[test]
    fn rejects_grants_and_grants_all() {
        let source = r#"
role admin
  grants_all
  grants
    users:read
"#;
        let err = parse_package_skeleton(source).unwrap_err();
        assert!(format!("{:?}", err).contains("either `grants` or `grants_all`"));
    }

    #[test]
    fn parses_inherited_only_role() {
        let source = r#"
role support_lead
  inherits support
"#;
        let pkg = parse_package_skeleton(source).expect("parses");
        assert_eq!(pkg.roles.len(), 1);
        assert!(matches!(pkg.roles[0].grants, RoleGrantsAst::InheritedOnly));
    }

    #[test]
    fn rbac_catalog_coexists_with_features() {
        let source = r#"
permission users:read

role viewer
  grants
    users:read

feature customer
  domain
    resource Customer
      email: Text required
"#;
        let pkg = parse_package_skeleton(source).expect("parses");
        assert_eq!(pkg.features.len(), 1);
        assert_eq!(pkg.features[0].name, "customer");
        assert_eq!(pkg.permissions.len(), 1);
        assert_eq!(pkg.roles.len(), 1);
    }
}
