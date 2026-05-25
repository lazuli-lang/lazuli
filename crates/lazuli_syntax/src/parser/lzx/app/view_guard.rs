//! `.lzx` **policy / view-guard** family — access control + lifecycle gates.
//!
//! Two related blocks live here:
//!
//! 1. **`route_guard`** — the app-level defaults block:
//!
//!    ```text
//!    route_guard
//!      default_policy @policy.member
//!      on_unauthenticated redirect "/login"
//!      on_unauthorized redirect "/403"
//!      skeleton @client.app_shell
//!    ```
//!
//! 2. **`policy <ref>`** — the per-route / per-view guard:
//!
//!    ```text
//!    policy @policy.member
//!      on_unauthenticated redirect "/login"
//!      on_unauthorized redirect "/403"
//!      requires_lifecycle Customer = onboarded
//!      on_lifecycle_pending @resume onboarding
//!      forbid_when @role.host dispatch_to "/concierge"
//!    ```
//!
//! `policy <ref>` accepts a single atom (`@policy.member`) or a list
//! form (`[@policy.a, @policy.b]`). The redirect, lifecycle, and
//! forbid-when sub-clauses are parsed by dedicated helpers in this
//! module; the lifecycle pair travels with the guard so it stays
//! colocated with its consumers.
//!
//! The `attach_lzx_*` helpers are guard mutators used by
//! experience-view parsers that build the guard incrementally — they
//! enforce "declared at most once" and merge spans.

use crate::ast::{LzxForbidWhen, LzxRequiresLifecycle, LzxRouteGuardDefaults, LzxViewGuard, Span};

use super::super::super::common::{
    SourceLine, is_lzx_bare_ident, is_lzx_resume_ref, is_trivia, line_error, line_error_owned,
    unquote_lzx_value,
};
use super::super::super::error::ParseError;

pub(crate) fn parse_lzx_route_guard_defaults(
    lines: &[SourceLine<'_>],
    start: usize,
    guard_indent: usize,
) -> Result<(LzxRouteGuardDefaults, usize), ParseError> {
    let header = &lines[start];
    if header.text.trim_start() != "route_guard" {
        return Err(line_error(header, "route guard blocks use `route_guard`"));
    }

    let child_indent = guard_indent + 2;
    let mut default_policy = None;
    let mut on_unauthenticated = None;
    let mut on_unauthorized = None;
    let mut skeleton = None;
    let mut index = start + 1;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            index += 1;
            continue;
        }
        if line.indent <= guard_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`route_guard` children use one indentation level deeper than `route_guard`",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("default_policy ") {
            if default_policy.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `default_policy` at most once",
                ));
            }
            default_policy = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthenticated ") {
            if on_unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `on_unauthenticated` at most once",
                ));
            }
            on_unauthenticated = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthorized ") {
            if on_unauthorized.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `on_unauthorized` at most once",
                ));
            }
            on_unauthorized = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("skeleton ") {
            if skeleton.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `skeleton` at most once",
                ));
            }
            skeleton = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`route_guard` children are `default_policy`, `on_unauthenticated redirect`, `on_unauthorized redirect`, or `skeleton @client.<name>` declarations",
            ));
        }

        last_end = line.end;
        index += 1;
    }

    Ok((
        LzxRouteGuardDefaults {
            default_policy,
            on_unauthenticated,
            on_unauthorized,
            skeleton,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

pub(crate) fn parse_lzx_view_guard(
    lines: &[SourceLine<'_>],
    start: usize,
    policy_indent: usize,
) -> Result<(LzxViewGuard, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let Some(rest) = trimmed.strip_prefix("policy ") else {
        return Err(line_error(
            header,
            "view guard blocks use `policy <policy>`",
        ));
    };
    let policy = parse_lzx_view_guard_policy(header, rest.trim())?;

    let child_indent = policy_indent + 2;
    let mut on_unauthenticated = None;
    let mut on_unauthorized = None;
    let mut requires_lifecycle = None;
    let mut on_lifecycle_pending = None;
    let mut forbid_when: Vec<LzxForbidWhen> = Vec::new();
    let mut index = start + 1;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            index += 1;
            continue;
        }
        if line.indent <= policy_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`policy` redirect children use one indentation level deeper than the `policy` line",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("on_unauthenticated ") {
            if on_unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_unauthenticated` at most once",
                ));
            }
            on_unauthenticated = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthorized ") {
            if on_unauthorized.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_unauthorized` at most once",
                ));
            }
            on_unauthorized = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("requires_lifecycle ") {
            // router-w3 Tier 2 — lifecycle gate. Format:
            //   requires_lifecycle <Resource> = <state>[ on substep <tag>]
            if requires_lifecycle.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `requires_lifecycle` at most once",
                ));
            }
            requires_lifecycle = Some(parse_lzx_requires_lifecycle(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("on_lifecycle_pending ") {
            // router-w3 Tier 2 — optional resume router reference. v1
            // codegen prefers the W4 lifecycle_routes table on the
            // resource; this slot is kept for back-compat with apps
            // that authored resume routers.
            if on_lifecycle_pending.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_lifecycle_pending` at most once",
                ));
            }
            on_lifecycle_pending = Some(parse_lzx_on_lifecycle_pending(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("forbid_when ") {
            // router-w3 Tier 3 — positive-state redirect. Format:
            //   forbid_when <atom> dispatch_to "<url>"
            // where <atom> is `@<ns>.<name>` (e.g. `@role.host`).
            forbid_when.push(parse_lzx_forbid_when(line, rest.trim())?);
        } else {
            return Err(line_error(
                line,
                "`policy` children are `on_unauthenticated redirect \"<path>\"`, `on_unauthorized redirect \"<path>\"`, `requires_lifecycle <Resource> = <state>`, `on_lifecycle_pending @resume <name>`, or `forbid_when <atom> dispatch_to \"<path>\"`",
            ));
        }

        last_end = line.end;
        index += 1;
    }

    Ok((
        LzxViewGuard {
            policy,
            on_unauthenticated,
            on_unauthorized,
            requires_lifecycle,
            on_lifecycle_pending,
            forbid_when,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

/// router-w3 Tier 3 — parse `forbid_when <atom> dispatch_to "<url>"`.
fn parse_lzx_forbid_when(line: &SourceLine<'_>, text: &str) -> Result<LzxForbidWhen, ParseError> {
    let (atom_part, url_part) = text.split_once("dispatch_to").ok_or_else(|| {
        line_error(
            line,
            "`forbid_when` uses `forbid_when <atom> dispatch_to \"<url>\"`",
        )
    })?;
    let atom_ref = atom_part.trim().to_owned();
    if !atom_ref.starts_with('@') {
        return Err(line_error(
            line,
            "`forbid_when` atom must be a policy atom like `@role.host` or `@scope.X`",
        ));
    }
    let url_text = url_part.trim();
    if !url_text.starts_with('"') || !url_text.ends_with('"') || url_text.len() < 2 {
        return Err(line_error(
            line,
            "`forbid_when` URL must be a double-quoted string",
        ));
    }
    let dispatch_to = url_text[1..url_text.len() - 1].to_owned();
    Ok(LzxForbidWhen {
        atom_ref,
        dispatch_to,
        span: Span::new(line.start, line.end),
    })
}

fn parse_lzx_view_guard_policy(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<Vec<String>, ParseError> {
    if value.is_empty() {
        return Err(line_error(line, "`policy` requires a policy reference"));
    }

    if !value.starts_with('[') {
        return Ok(vec![value.to_owned()]);
    }

    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Err(line_error(
            line,
            "`policy` list form is `policy [@policy.a, @policy.b]`",
        ));
    };
    if inner.trim().is_empty() {
        return Err(line_error(
            line,
            "`policy` list requires at least one policy reference",
        ));
    }

    let mut policies = Vec::new();
    for atom in inner.split(',') {
        let atom = atom.trim();
        if atom.is_empty() {
            return Err(line_error(
                line,
                "`policy` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        policies.push(atom.to_owned());
    }
    Ok(policies)
}

fn parse_lzx_redirect_clause(line: &SourceLine<'_>, value: &str) -> Result<String, ParseError> {
    let Some(rest) = value.strip_prefix("redirect ") else {
        return Err(line_error(
            line,
            "route guard redirect clauses use `redirect \"<path>\"`",
        ));
    };
    let target = rest.trim();
    if !target.starts_with('"') || !target.ends_with('"') {
        return Err(line_error(
            line,
            "route guard redirect targets must be quoted strings",
        ));
    }
    Ok(unquote_lzx_value(target).to_owned())
}

pub(crate) fn parse_lzx_requires_lifecycle(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LzxRequiresLifecycle, ParseError> {
    let Some((resource, state)) = rest.split_once('=') else {
        return Err(line_error(
            line,
            "`requires_lifecycle` uses `requires_lifecycle <Resource> = <state>`",
        ));
    };
    let resource = resource.trim();
    let state = state.trim();
    let (state, substep) = parse_lzx_optional_substep_tail(line, state, "`requires_lifecycle`")?;
    if resource.is_empty()
        || !resource
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        || !is_lzx_bare_ident(resource)
    {
        return Err(line_error(
            line,
            "`requires_lifecycle` resource must be an upper-case resource identifier",
        ));
    }
    if !is_lzx_bare_ident(state) {
        return Err(line_error(
            line,
            "`requires_lifecycle` state must be a bare lifecycle state identifier",
        ));
    }
    Ok(LzxRequiresLifecycle {
        resource: resource.to_owned(),
        state: state.to_owned(),
        substep,
        span: Span::new(line.start, line.end),
    })
}

pub(crate) fn parse_lzx_optional_substep_tail<'a>(
    line: &SourceLine<'_>,
    value: &'a str,
    context: &str,
) -> Result<(&'a str, Option<String>), ParseError> {
    let parts: Vec<_> = value.split_whitespace().collect();
    match parts.as_slice() {
        [state] => Ok((*state, None)),
        [state, "substep", substep] => {
            if !is_lzx_bare_ident(substep) {
                return Err(line_error_owned(
                    line,
                    format!("{context} substep must be a bare identifier"),
                ));
            }
            Ok((*state, Some((*substep).to_owned())))
        }
        _ => Err(line_error_owned(
            line,
            format!("{context} accepts an optional `substep <name>` tail"),
        )),
    }
}

pub(crate) fn parse_lzx_on_lifecycle_pending(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<String, ParseError> {
    let target = if let Some(target) = rest.trim().strip_prefix("@resume ") {
        target.trim()
    } else if let Some(target) = rest.trim().strip_prefix("@resume.") {
        target.trim()
    } else {
        return Err(line_error(
            line,
            "`on_lifecycle_pending` uses `on_lifecycle_pending @resume <name>`",
        ));
    };
    if !is_lzx_resume_ref(target) {
        return Err(line_error(
            line,
            "`on_lifecycle_pending` resume reference must be `<name>` or `<feature>.<name>`",
        ));
    }
    Ok(target.to_owned())
}

pub(crate) fn attach_lzx_requires_lifecycle(
    line: &SourceLine<'_>,
    guard: &mut LzxViewGuard,
    parsed: LzxRequiresLifecycle,
) -> Result<(), ParseError> {
    if guard.requires_lifecycle.is_some() {
        return Err(line_error(
            line,
            "view declares `requires_lifecycle` at most once",
        ));
    }
    guard.span.end = guard.span.end.max(parsed.span.end);
    guard.requires_lifecycle = Some(parsed);
    Ok(())
}

pub(crate) fn attach_lzx_on_lifecycle_pending(
    line: &SourceLine<'_>,
    guard: &mut LzxViewGuard,
    parsed: String,
    span_end: usize,
) -> Result<(), ParseError> {
    if guard.on_lifecycle_pending.is_some() {
        return Err(line_error(
            line,
            "view declares `on_lifecycle_pending` at most once",
        ));
    }
    guard.span.end = guard.span.end.max(span_end);
    guard.on_lifecycle_pending = Some(parsed);
    Ok(())
}
