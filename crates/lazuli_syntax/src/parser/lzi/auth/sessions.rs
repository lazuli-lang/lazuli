//! `auth sessions` sub-block — resource / ttl / refresh / access_ttl /
//! `rotation` (with its `refresh_ttl` / `grace` /
//! `theft_detection_action` children).
//!
//! Extracted from the original monolithic `auth.rs`.

use super::super::super::common::{
    SourceLine, is_trivia, line_error, line_error_owned, parse_lzx_bool, strip_inline_comment,
    unquote_lzx_value,
};
use super::super::super::error::ParseError;
use super::super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD, AGENT_INDENT_GREAT_GRANDCHILD,
};
use crate::ast::{
    AuthDurationClause, AuthSessionCookie, AuthSessionRotation, AuthSessions,
    AuthTheftDetectionAction, AuthTheftDetectionActionClause, Span,
};

pub(super) fn parse_auth_sessions(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthSessions, usize), ParseError> {
    let header = &lines[start];
    let mut resource: Option<String> = None;
    let mut ttl: Option<String> = None;
    let mut refresh: Option<bool> = None;
    let mut access_ttl: Option<AuthDurationClause> = None;
    let mut rotation: Option<AuthSessionRotation> = None;
    let mut cookie: Option<AuthSessionCookie> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = strip_inline_comment(line.text.trim_start()).trim_end();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth sessions` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("resource ") {
            resource = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("ttl ") {
            ttl = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("refresh ") {
            refresh = Some(
                parse_lzx_bool(rest.trim())
                    .ok_or_else(|| line_error(line, "`refresh` must be `true` or `false`"))?,
            );
        } else if let Some(rest) = trimmed.strip_prefix("access_ttl ") {
            if access_ttl.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions` may declare `access_ttl` at most once",
                ));
            }
            access_ttl = Some(AuthDurationClause {
                value: unquote_lzx_value(rest.trim()).to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if trimmed == "rotation" {
            if rotation.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions` may declare `rotation` at most once",
                ));
            }
            let (parsed, next) = parse_auth_session_rotation(lines, i)?;
            rotation = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        } else if trimmed == "cookie" {
            if cookie.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions` may declare `cookie` at most once",
                ));
            }
            let (parsed, next) = parse_auth_session_cookie(lines, i)?;
            cookie = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        } else {
            return Err(line_error(
                line,
                "`auth sessions` children are `resource`, `ttl`, `refresh`, `access_ttl`, `rotation`, or `cookie`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let resource = resource.ok_or_else(|| {
        line_error(
            header,
            "`auth sessions` requires a `resource <Name>` declaration",
        )
    })?;
    let ttl = ttl.ok_or_else(|| {
        line_error(
            header,
            "`auth sessions` requires a `ttl \"<duration>\"` declaration",
        )
    })?;

    Ok((
        AuthSessions {
            resource,
            ttl,
            refresh: refresh.unwrap_or(false),
            access_ttl,
            rotation,
            cookie,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_session_rotation(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthSessionRotation, usize), ParseError> {
    let header = &lines[start];
    let mut refresh_ttl: Option<AuthDurationClause> = None;
    let mut grace: Option<AuthDurationClause> = None;
    let mut theft_detection_action: Option<AuthTheftDetectionActionClause> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = strip_inline_comment(line.text.trim_start()).trim_end();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_GRANDCHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GREAT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth sessions rotation` children use eight-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("refresh_ttl ") {
            if refresh_ttl.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions rotation` may declare `refresh_ttl` at most once",
                ));
            }
            refresh_ttl = Some(AuthDurationClause {
                value: unquote_lzx_value(rest.trim()).to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("grace ") {
            if grace.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions rotation` may declare `grace` at most once",
                ));
            }
            grace = Some(AuthDurationClause {
                value: unquote_lzx_value(rest.trim()).to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("theft_detection_action ") {
            if theft_detection_action.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions rotation` may declare `theft_detection_action` at most once",
                ));
            }
            theft_detection_action = Some(AuthTheftDetectionActionClause {
                action: parse_auth_theft_detection_action(line, rest)?,
                span: Span::new(line.start, line.end),
            });
        } else {
            return Err(line_error(
                line,
                "`auth sessions rotation` children are `refresh_ttl`, `grace`, or `theft_detection_action`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        AuthSessionRotation {
            refresh_ttl,
            grace,
            theft_detection_action,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse a `cookie` block under `auth sessions` — the closed-catalog
/// transport attributes (`name` / `same_site` / `secure` / `http_only` /
/// `domain` / `path`). Mirrors [`parse_auth_session_rotation`]: 8-space
/// children, each declarable at most once. Unknown children and unknown
/// `same_site` values are rejected with a clear error.
fn parse_auth_session_cookie(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthSessionCookie, usize), ParseError> {
    let header = &lines[start];
    let mut name: Option<String> = None;
    let mut same_site: Option<String> = None;
    let mut secure: Option<bool> = None;
    let mut http_only: Option<bool> = None;
    let mut domain: Option<String> = None;
    let mut path: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = strip_inline_comment(line.text.trim_start()).trim_end();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_GRANDCHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GREAT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth sessions cookie` children use eight-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("name ") {
            if name.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions cookie` may declare `name` at most once",
                ));
            }
            name = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("same_site ") {
            if same_site.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions cookie` may declare `same_site` at most once",
                ));
            }
            same_site = Some(parse_same_site(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("secure ") {
            if secure.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions cookie` may declare `secure` at most once",
                ));
            }
            secure = Some(
                parse_lzx_bool(rest.trim())
                    .ok_or_else(|| line_error(line, "`secure` must be `true` or `false`"))?,
            );
        } else if let Some(rest) = trimmed.strip_prefix("http_only ") {
            if http_only.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions cookie` may declare `http_only` at most once",
                ));
            }
            http_only = Some(
                parse_lzx_bool(rest.trim())
                    .ok_or_else(|| line_error(line, "`http_only` must be `true` or `false`"))?,
            );
        } else if let Some(rest) = trimmed.strip_prefix("domain ") {
            if domain.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions cookie` may declare `domain` at most once",
                ));
            }
            domain = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("path ") {
            if path.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions cookie` may declare `path` at most once",
                ));
            }
            path = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`auth sessions cookie` children are `name`, `same_site`, `secure`, `http_only`, `domain`, or `path`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        AuthSessionCookie {
            name,
            same_site,
            secure,
            http_only,
            domain,
            path,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Validate a `same_site` value against the closed catalog
/// `lax | strict | none`. The same three values the app-wide `cookie`
/// block accepts (`Context::Cookie` / `constant.language.cookie.lazuli`).
fn parse_same_site(line: &SourceLine<'_>, rest: &str) -> Result<String, ParseError> {
    let mut parts = rest.split_whitespace();
    let value = parts
        .next()
        .ok_or_else(|| line_error(line, "`same_site` requires `lax`, `strict`, or `none`"))?;
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`same_site` must be a single closed-catalog value",
        ));
    }
    match value {
        "lax" | "strict" | "none" => Ok(value.to_owned()),
        other => Err(line_error_owned(
            line,
            format!("unknown `same_site` `{other}` - closed catalog is `lax`, `strict`, or `none`"),
        )),
    }
}

fn parse_auth_theft_detection_action(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<AuthTheftDetectionAction, ParseError> {
    let mut parts = rest.split_whitespace();
    let action = parts.next().ok_or_else(|| {
        line_error(
            line,
            "`theft_detection_action` requires `revoke_session_family` or `revoke_user`",
        )
    })?;
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`theft_detection_action` must be a single closed-catalog verb",
        ));
    }
    match action {
        "revoke_session_family" => Ok(AuthTheftDetectionAction::RevokeSessionFamily),
        "revoke_user" => Ok(AuthTheftDetectionAction::RevokeUser),
        other => Err(line_error_owned(
            line,
            format!(
                "unknown `theft_detection_action` `{other}` - closed catalog is `revoke_session_family` or `revoke_user`"
            ),
        )),
    }
}

#[cfg(test)]
mod sessions_tests {
    include!("sessions_tests.rs");
}
