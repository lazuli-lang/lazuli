//! Feature-level `policies` block parser.
//!
//! A `policies` block is the single declarative source of truth for
//! who can do what in a feature. The grammar is closed and indent-
//! sensitive (every block here lives under a `feature <name>` at
//! `AGENT_INDENT_FEATURE_CHILD`):
//!
//! ```text
//! policies
//!   create: @actor.staff, @actor.owner
//!     when_denied @translation.errors.forbidden
//!     when_denied_route
//!       unauthenticated -> view sign_in
//!       role_mismatch staff -> path "/dashboard"
//!       default -> view forbidden
//!   update: @policy.owner
//!   fields Customer
//!     email
//!       read: @actor.staff, @actor.owner
//!       write: @actor.owner
//! ```
//!
//! Two child shapes are accepted under `policies`:
//!
//! - **Policy categories** — `<name>: <@atom>, <@atom>` lines. Each
//!   may carry an optional `when_denied` translation reference and/or
//!   a `when_denied_route` redirect block (used by `command` audit
//!   diagnostics — `ERR-VOCAB-WHEN-DENIED-*`).
//! - **Field policy blocks** — `fields <Resource>` headers carrying
//!   per-field `read:` / `write:` clauses. These lower to typed
//!   `FieldPoliciesDecl` records so the analyzer can cross-check the
//!   resource shape.
//!
//! Closed-catalog rules (single-`when_denied`, single-`when_denied_route`,
//! at-most-one-default, unique `role_mismatch <role>`) are enforced
//! here so downstream consumers can trust the AST. Vocabulary closure
//! (which policy `@atom` namespaces exist) lives analyzer-side.
//!
//! Visibility: the entry `parse_policies_decl` is `pub(super)` so the
//! feature-skeleton walker in `mod.rs` can call it.

use super::super::common::{
    SourceLine, is_lzx_bare_ident, is_lzx_resume_ref, is_trivia, line_error, split_lzx_arrow,
    unquote_lzx_value,
};
use super::super::error::ParseError;
use super::is_policy_identifier;
use super::parse_translation_key_token;

use crate::ast::{
    FieldPoliciesDecl, FieldPolicyDecl, PoliciesDecl, PolicyCategoryDecl, RoleMismatchArmAst,
    RouteRedirectTargetAst, Span, TranslationKeyRefAst, WhenDeniedRouteAst,
};

pub(super) fn parse_policies_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(PoliciesDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;
    let greatgrand_indent = header_indent + 6;

    let mut categories: Vec<PolicyCategoryDecl> = Vec::new();
    let mut field_blocks: Vec<FieldPoliciesDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`policies` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("fields ") {
            let resource = rest.trim().to_owned();
            if resource.is_empty() {
                return Err(line_error(
                    line,
                    "`fields` requires a resource name (`fields <Resource>`)",
                ));
            }
            let (block, next) = parse_field_policies_block(
                lines,
                i,
                resource,
                grandchild_indent,
                greatgrand_indent,
            )?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            field_blocks.push(block);
            i = next;
            continue;
        }

        if let Some((name, atoms_text)) = trimmed.split_once(':') {
            let name = name.trim();
            if !is_policy_identifier(name) {
                i += 1;
                continue;
            }
            let atoms = atoms_text
                .split(',')
                .map(str::trim)
                .filter(|atom| atom.starts_with('@'))
                .map(str::to_owned)
                .collect();
            let category_header_line = line;
            let category_header_end = line.end;
            let mut category_last_end = category_header_end;
            // IR Error-Vocab (Cell PARSE-1) — consume the optional
            // `when_denied @translation.<key>` child(ren) at
            // grandchild_indent (6 spaces under a feature). Zero-or-one
            // per category; duplicate is a parse error.
            let mut when_denied: Option<TranslationKeyRefAst> = None;
            let mut when_denied_route: Option<WhenDeniedRouteAst> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let inner = &lines[j];
                let inner_trim = inner.text.trim_start();
                if is_trivia(inner_trim) {
                    j += 1;
                    continue;
                }
                if inner.indent <= child_indent {
                    break;
                }
                if inner.indent != grandchild_indent {
                    return Err(line_error(
                        inner,
                        "policy category children use one indentation level deeper than the category line",
                    ));
                }
                if let Some(rest) = inner_trim.strip_prefix("when_denied ") {
                    if when_denied.is_some() {
                        return Err(line_error(
                            inner,
                            "policy category may declare at most one `when_denied` child (ERR-VOCAB-MULTIPLE-WHEN-DENIED)",
                        ));
                    }
                    when_denied = Some(parse_translation_key_token(inner, rest)?);
                    category_last_end = inner.end;
                    j += 1;
                    continue;
                }
                if inner_trim == "when_denied_route" {
                    if when_denied_route.is_some() {
                        return Err(line_error(
                            inner,
                            "policy category may declare at most one `when_denied_route` child",
                        ));
                    }
                    let (parsed, next) = parse_when_denied_route_block(
                        lines,
                        j,
                        grandchild_indent,
                        greatgrand_indent,
                    )?;
                    category_last_end = lines[next.saturating_sub(1).max(j)].end;
                    when_denied_route = Some(parsed);
                    j = next;
                    continue;
                }
                return Err(line_error(
                    inner,
                    "policy category children are `when_denied @translation.<key>` or `when_denied_route` only",
                ));
            }
            categories.push(PolicyCategoryDecl {
                name: name.to_owned(),
                atoms,
                when_denied,
                when_denied_route,
                span: Span::new(category_header_line.start, category_last_end),
            });
            last_end = category_last_end;
            i = j;
            continue;
        }

        return Err(line_error(
            line,
            "`policies` children are `<name>: <atom>, ...` or `fields <Resource>` headers",
        ));
    }

    Ok((
        PoliciesDecl {
            categories,
            fields: field_blocks,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_when_denied_route_block(
    lines: &[SourceLine<'_>],
    start: usize,
    header_indent: usize,
    arm_indent: usize,
) -> Result<(WhenDeniedRouteAst, usize), ParseError> {
    let header = &lines[start];
    if header.indent != header_indent || header.text.trim_start() != "when_denied_route" {
        return Err(line_error(
            header,
            "policy route denial blocks use `when_denied_route`",
        ));
    }

    let mut unauthenticated: Option<RouteRedirectTargetAst> = None;
    let mut role_mismatch: Vec<RoleMismatchArmAst> = Vec::new();
    let mut default: Option<RouteRedirectTargetAst> = None;
    let mut seen_roles = std::collections::BTreeSet::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != arm_indent {
            return Err(line_error(
                line,
                "`when_denied_route` arms use one indentation level deeper than the block",
            ));
        }
        let Some((left, right)) = split_lzx_arrow(trimmed) else {
            return Err(line_error(
                line,
                "`when_denied_route` arms use `<case> -> view <name>` or `<case> -> path \"...\"`",
            ));
        };
        let left = left.trim();
        let target = parse_route_redirect_target(line, right.trim())?;
        if left == "unauthenticated" {
            if unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares `unauthenticated` at most once",
                ));
            }
            unauthenticated = Some(target);
        } else if left == "default" {
            if default.is_some() {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares `default` at most once",
                ));
            }
            default = Some(target);
        } else if let Some(role) = left.strip_prefix("role_mismatch ") {
            let role = role.trim();
            if !is_lzx_bare_ident(role) {
                return Err(line_error(
                    line,
                    "`role_mismatch` requires a bare role identifier",
                ));
            }
            if !seen_roles.insert(role.to_owned()) {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares each `role_mismatch <role>` at most once",
                ));
            }
            role_mismatch.push(RoleMismatchArmAst {
                role: role.to_owned(),
                target,
                span: Span::new(line.start, line.end),
            });
        } else {
            return Err(line_error(
                line,
                "`when_denied_route` arms are `unauthenticated`, `role_mismatch <role>`, or `default`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    if unauthenticated.is_none() && role_mismatch.is_empty() && default.is_none() {
        return Err(line_error(
            header,
            "`when_denied_route` requires at least one arm",
        ));
    }

    Ok((
        WhenDeniedRouteAst {
            unauthenticated,
            role_mismatch,
            default,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_route_redirect_target(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<RouteRedirectTargetAst, ParseError> {
    if let Some(view) = value.strip_prefix("view ") {
        let view = view.trim();
        if !is_lzx_resume_ref(view) {
            return Err(line_error(
                line,
                "`view` redirect targets use `<view>` or `<feature>.<view>`",
            ));
        }
        return Ok(RouteRedirectTargetAst::View(view.to_owned()));
    }
    if let Some(path) = value.strip_prefix("path ") {
        let path = path.trim();
        if !(path.starts_with('"') && path.ends_with('"')) {
            return Err(line_error(
                line,
                "`path` redirect targets must be quoted string literals",
            ));
        }
        return Ok(RouteRedirectTargetAst::Path(
            unquote_lzx_value(path).to_owned(),
        ));
    }
    Err(line_error(
        line,
        "`when_denied_route` targets use `view <name>` or `path \"...\"`",
    ))
}

fn parse_field_policies_block(
    lines: &[SourceLine<'_>],
    start: usize,
    resource: String,
    field_indent: usize,
    clause_indent: usize,
) -> Result<(FieldPoliciesDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let mut fields: Vec<FieldPolicyDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != field_indent {
            return Err(line_error(
                line,
                "`fields` children use one indentation level deeper than the header",
            ));
        }

        // Bare field name at field_indent (`email`); read/write at
        // clause_indent below.
        let field_name = trimmed.to_owned();
        if field_name.is_empty() || !is_policy_identifier(&field_name) {
            return Err(line_error(
                line,
                "field policy entry must be a bare identifier",
            ));
        }
        let field_header_end = line.end;
        let mut read: Option<Vec<String>> = None;
        let mut write: Option<Vec<String>> = None;
        let mut last_field_end = field_header_end;
        let mut j = i + 1;
        while j < lines.len() {
            let inner = &lines[j];
            let inner_trim = inner.text.trim_start();
            if is_trivia(inner_trim) {
                j += 1;
                continue;
            }
            if inner.indent <= field_indent {
                break;
            }
            if inner.indent != clause_indent {
                return Err(line_error(
                    inner,
                    "field policy clauses use one indentation level deeper than the field name",
                ));
            }
            let parsed_atoms = |rest: &str| -> Vec<String> {
                rest.split(',')
                    .map(str::trim)
                    .filter(|atom| atom.starts_with('@'))
                    .map(str::to_owned)
                    .collect()
            };
            if let Some(rest) = inner_trim.strip_prefix("read:") {
                read = Some(parsed_atoms(rest));
                last_field_end = inner.end;
                j += 1;
                continue;
            }
            if let Some(rest) = inner_trim.strip_prefix("write:") {
                write = Some(parsed_atoms(rest));
                last_field_end = inner.end;
                j += 1;
                continue;
            }
            return Err(line_error(
                inner,
                "field policy clauses are `read:` or `write:` followed by atoms",
            ));
        }
        fields.push(FieldPolicyDecl {
            field: field_name,
            read,
            write,
            span: Span::new(line.start, last_field_end),
        });
        last_end = last_field_end;
        i = j;
    }

    Ok((
        FieldPoliciesDecl {
            resource,
            fields,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
