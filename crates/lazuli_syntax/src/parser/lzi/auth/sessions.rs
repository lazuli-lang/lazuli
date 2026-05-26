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
    AuthDurationClause, AuthSessionRotation, AuthSessions, AuthTheftDetectionAction,
    AuthTheftDetectionActionClause, Span,
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

#[cfg(test)]
mod sessions_tests {
    use super::super::super::parse_feature_skeletons;

    #[test]
    fn auth_sessions_child_parses_with_refresh_true() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "30 days"
      refresh true
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let sessions = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child");
        assert_eq!(sessions.resource, "CustomerSession");
        assert_eq!(sessions.ttl, "30 days");
        assert!(sessions.refresh);
        assert!(sessions.access_ttl.is_none());
        assert!(sessions.rotation.is_none());
    }

    #[test]
    fn auth_sessions_child_defaults_legacy_refresh_false_when_omitted() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let sessions = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child");
        assert_eq!(sessions.resource, "CustomerSession");
        assert_eq!(sessions.ttl, "7 days");
        assert!(!sessions.refresh);
        assert!(sessions.access_ttl.is_none());
        assert!(sessions.rotation.is_none());
    }

    #[test]
    fn auth_sessions_child_parses_nested_rotation_block() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      access_ttl "15 minutes"
      rotation
        refresh_ttl "30 days"
        grace "30 seconds"
        theft_detection_action revoke_session_family
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let sessions = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child");
        assert_eq!(
            sessions.access_ttl.as_ref().map(|ttl| ttl.value.as_str()),
            Some("15 minutes")
        );
        assert!(sessions.access_ttl.as_ref().unwrap().span.end > 0);

        let rotation = sessions.rotation.as_ref().expect("rotation block");
        assert!(rotation.span.end > rotation.span.start);
        assert_eq!(
            rotation.refresh_ttl.as_ref().map(|ttl| ttl.value.as_str()),
            Some("30 days")
        );
        assert_eq!(
            rotation.grace.as_ref().map(|grace| grace.value.as_str()),
            Some("30 seconds")
        );
        assert_eq!(
            rotation
                .theft_detection_action
                .as_ref()
                .map(|action| action.action),
            Some(crate::AuthTheftDetectionAction::RevokeSessionFamily)
        );
    }

    #[test]
    fn auth_sessions_child_parses_empty_rotation_block() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      rotation
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let rotation = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child")
            .rotation
            .as_ref()
            .expect("rotation block");
        assert!(rotation.refresh_ttl.is_none());
        assert!(rotation.grace.is_none());
        assert!(rotation.theft_detection_action.is_none());
    }

    #[test]
    fn auth_sessions_rotation_rejects_unknown_theft_action() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      rotation
        theft_detection_action quarantine_device
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("unknown `theft_detection_action`"),
            "error should mention closed-catalog theft action: {message}"
        );
    }
}
