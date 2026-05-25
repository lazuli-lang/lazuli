//! `auth` block parser — feature-level identity / password / sessions /
//! mfa / oauth declarations.
//!
//! The `auth` header sits at the feature child indent (2 spaces). Its
//! direct children — `identity`, `password`, `sessions`, `mfa`, `oauth` —
//! live at the agent child indent (4 spaces). Grandchildren (the named
//! options inside `password` / `sessions` / `mfa` / `oauth`) live at six
//! spaces; rotation grandchildren reach eight spaces. The block mirrors
//! `parse_agent` so an LLM authoring auth has the same indentation
//! contract as authoring an agent.
//!
//! ## Sub-parsers (private to this module)
//!
//! - `parse_auth_password` — algorithm / hash / verify / rate_limit
//! - `parse_auth_sessions` — resource / ttl / refresh / access_ttl /
//!   rotation
//! - `parse_auth_session_rotation` — refresh_ttl / grace /
//!   theft_detection_action
//! - `parse_auth_theft_detection_action` — closed catalog
//!   (revoke_session_family / revoke_user)
//! - `parse_auth_mfa` — enroll / verify / adapter
//! - `parse_auth_oauth` — adapter
//!
//! ## Cross-feature contract bridge
//!
//! `parse_auth_identity_contract_line` recognises a
//! `public contract identity as v<N>` line immediately above the
//! `identity <Resource>.<field>` declaration. It is reachable from
//! `parse_feature_skeleton` via `pub(super)` so the cross-feature
//! contract validator in mod.rs can short-circuit when an auth-specific
//! contract form is seen instead of the generic `public contract <sym>`
//! shape.
//!
//! ## See also
//!
//! - `docs/proposals/cross-feature-contracts.md` §3.5 + §5.3.
//! - `lazuli_ir::nodes::auth` — typed lowering target.

use super::super::common::{
    SourceLine, is_trivia, line_error, line_error_owned, parse_lzx_bool, strip_inline_comment,
    unquote_lzx_value,
};
use super::super::error::ParseError;
use super::numerics::{fold_rate_limit_line, parse_rate_limit_line_body};
use super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD,
    AGENT_INDENT_GREAT_GRANDCHILD,
};
use crate::ast::{
    Auth, AuthDurationClause, AuthIdentity, AuthMfa, AuthOAuthProvider, AuthPassword,
    AuthSessionRotation, AuthSessions, AuthTheftDetectionAction, AuthTheftDetectionActionClause,
    PublicContractDeclAst, RateLimitSpecAst, Span,
};

pub(super) fn parse_auth_identity_contract_line(
    line: &SourceLine<'_>,
) -> Result<Option<PublicContractDeclAst>, ParseError> {
    let trimmed = line.text.trim_start();
    let Some(rest) = trimmed.strip_prefix("public contract identity ") else {
        // Not the identity-contract form. Bare `public contract identity`
        // with no trailing tokens is also rejected (caught by the
        // general parse_public_contract_line which would error on the
        // missing `as` suffix). Return None to let the regular flow run.
        return Ok(None);
    };

    let mut parts = rest.split_whitespace();
    let as_kw = parts
        .next()
        .ok_or_else(|| line_error(line, "`public contract identity` requires `as v<N>` suffix"))?;
    if as_kw != "as" {
        return Err(line_error(
            line,
            "`public contract identity` requires `as v<N>` suffix",
        ));
    }
    let version_token = parts.next().ok_or_else(|| {
        line_error(
            line,
            "`public contract identity as` requires a version `v<N>`",
        )
    })?;
    let Some(version_digits) = version_token.strip_prefix('v') else {
        return Err(line_error(line, "version must start with `v`, e.g. `v1`"));
    };
    let version: u16 = version_digits
        .parse()
        .map_err(|_| line_error(line, "version must be a positive integer (u16)"))?;
    if version == 0 {
        return Err(line_error(line, "version must be a positive integer (u16)"));
    }
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`public contract identity as v<N>` admits no trailing tokens",
        ));
    }
    Ok(Some(PublicContractDeclAst {
        version,
        span: Span::new(line.start, line.end),
    }))
}

pub(super) fn parse_auth(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Auth, usize), ParseError> {
    let header = &lines[start];
    let mut identity: Option<AuthIdentity> = None;
    let mut password: Option<AuthPassword> = None;
    let mut sessions: Option<AuthSessions> = None;
    let mut mfa: Option<AuthMfa> = None;
    let mut oauth: Vec<AuthOAuthProvider> = Vec::new();
    let mut pending_identity_contract: Option<PublicContractDeclAst> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`auth` body children use four-space indentation",
            ));
        }

        // Cross-feature contract: `public contract identity as v<N>`
        // immediately above the `identity <Resource>.<field>` line per
        // docs/proposals/cross-feature-contracts.md §3.5 + §5.3.
        if let Some(contract) = parse_auth_identity_contract_line(line)? {
            if pending_identity_contract.is_some() {
                return Err(line_error(
                    line,
                    "duplicate `public contract identity` line; only one may precede each `identity` declaration",
                ));
            }
            if identity.is_some() {
                return Err(line_error(
                    line,
                    "`public contract identity` must appear ABOVE the `identity` line, not below",
                ));
            }
            pending_identity_contract = Some(contract);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("identity ") {
            if identity.is_some() {
                return Err(line_error(
                    line,
                    "`auth identity` may be declared at most once",
                ));
            }
            let field = rest.trim();
            if field.is_empty() {
                return Err(line_error(
                    line,
                    "`auth identity` requires `<Resource>.<field>`",
                ));
            }
            if !field.contains('.') {
                return Err(line_error(
                    line,
                    "`auth identity` requires `<Resource>.<field>` (dot-qualified)",
                ));
            }
            identity = Some(AuthIdentity {
                field: field.to_owned(),
                public_contract: pending_identity_contract.take(),
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
        } else if trimmed == "password" {
            if password.is_some() {
                return Err(line_error(
                    line,
                    "`auth password` may be declared at most once",
                ));
            }
            let (parsed, next) = parse_auth_password(lines, i)?;
            password = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "sessions" {
            if sessions.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions` may be declared at most once",
                ));
            }
            let (parsed, next) = parse_auth_sessions(lines, i)?;
            sessions = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("mfa ") {
            if mfa.is_some() {
                return Err(line_error(line, "`auth mfa` may be declared at most once"));
            }
            let method = rest.trim();
            if method.is_empty() {
                return Err(line_error(
                    line,
                    "`auth mfa` requires a method id (`totp`, `sms`, `webauthn`, ...)",
                ));
            }
            let (parsed, next) = parse_auth_mfa(lines, i, method.to_owned())?;
            mfa = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("oauth ") {
            let provider = rest.trim();
            if provider.is_empty() {
                return Err(line_error(
                    line,
                    "`auth oauth` requires a provider id (`google`, `github`, ...)",
                ));
            }
            let (parsed, next) = parse_auth_oauth(lines, i, provider.to_owned())?;
            oauth.push(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "`auth` children are `identity`, `password`, `sessions`, `mfa`, or `oauth`",
            ));
        }
    }

    let identity = identity.ok_or_else(|| {
        line_error(
            header,
            "`auth` requires an `identity <Resource>.<field>` declaration",
        )
    })?;

    Ok((
        Auth {
            identity,
            password,
            sessions,
            mfa,
            oauth,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_password(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthPassword, usize), ParseError> {
    let header = &lines[start];
    let mut algorithm: Option<String> = None;
    let mut hash: Option<String> = None;
    let mut verify: Option<String> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

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
                "`auth password` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("algorithm ") {
            algorithm = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("hash ") {
            hash = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("verify ") {
            verify = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
        } else {
            return Err(line_error(
                line,
                "`auth password` children are `algorithm`, `hash`, `verify`, or `rate_limit`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let algorithm = algorithm.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires an `algorithm <name>` declaration",
        )
    })?;
    let hash = hash.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires a `hash @fn.<name>` declaration",
        )
    })?;
    let verify = verify.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires a `verify @fn.<name>` declaration",
        )
    })?;

    Ok((
        AuthPassword {
            algorithm,
            hash,
            verify,
            rate_limit,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_sessions(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthSessions, usize), ParseError> {
    let header = &lines[start];
    let mut resource: Option<String> = None;
    let mut ttl: Option<String> = None;
    let mut refresh: Option<bool> = None;
    let mut access_ttl: Option<AuthDurationClause> = None;
    let mut rotation: Option<AuthSessionRotation> = None;
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
        } else {
            return Err(line_error(
                line,
                "`auth sessions` children are `resource`, `ttl`, `refresh`, `access_ttl`, or `rotation`",
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

fn parse_auth_mfa(
    lines: &[SourceLine<'_>],
    start: usize,
    method: String,
) -> Result<(AuthMfa, usize), ParseError> {
    let header = &lines[start];
    let mut enroll: Option<String> = None;
    let mut verify: Option<String> = None;
    let mut adapter: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

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
                "`auth mfa` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("enroll ") {
            enroll = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("verify ") {
            verify = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("adapter ") {
            adapter = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`auth mfa` children are `enroll`, `verify`, or `adapter`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let enroll = enroll.ok_or_else(|| {
        line_error(
            header,
            "`auth mfa` requires an `enroll @fn.<name>` declaration",
        )
    })?;
    let verify = verify.ok_or_else(|| {
        line_error(
            header,
            "`auth mfa` requires a `verify @validator.<name>` declaration",
        )
    })?;

    Ok((
        AuthMfa {
            method,
            enroll,
            verify,
            adapter,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_oauth(
    lines: &[SourceLine<'_>],
    start: usize,
    provider: String,
) -> Result<(AuthOAuthProvider, usize), ParseError> {
    let header = &lines[start];
    let mut adapter: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

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
                "`auth oauth` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("adapter ") {
            adapter = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(line, "`auth oauth` children are `adapter`"));
        }

        last_end = line.end;
        i += 1;
    }

    let adapter = adapter.ok_or_else(|| {
        line_error(
            header,
            "`auth oauth` requires an `adapter @adapter.<name>` declaration",
        )
    })?;

    Ok((
        AuthOAuthProvider {
            provider,
            adapter,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
