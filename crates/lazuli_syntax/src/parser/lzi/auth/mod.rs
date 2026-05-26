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

mod mfa;
mod oauth;
mod password;
mod sessions;

use super::super::common::{SourceLine, is_trivia, line_error};
use super::super::error::ParseError;
use super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD};
use crate::ast::{
    Auth, AuthIdentity, AuthMfa, AuthOAuthProvider, AuthPassword, AuthSessions,
    PublicContractDeclAst, Span,
};

use mfa::parse_auth_mfa;
use oauth::parse_auth_oauth;
use password::parse_auth_password;
use sessions::parse_auth_sessions;

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


// =============================================================================
// Phase L — `auth` block parser slice tests.
// =============================================================================
#[cfg(test)]
mod auth_parser_tests {
    use super::super::parse_feature_skeletons;

    #[test]
    fn auth_minimal_identity_only_parses() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let auth = features[0].auth.as_ref().expect("auth block");
        assert_eq!(auth.identity.field, "Customer.email");
        assert!(auth.password.is_none());
        assert!(auth.sessions.is_none());
        assert!(auth.mfa.is_none());
        assert!(auth.oauth.is_empty());
    }

    #[test]
    fn auth_full_block_parses() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"

    oauth google
      adapter @adapter.google_oauth

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp

    sessions
      resource CustomerSession
      ttl "7 days"
      refresh false
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let auth = features[0].auth.as_ref().expect("auth block");

        assert_eq!(auth.identity.field, "Customer.email");

        let password = auth.password.as_ref().expect("password");
        assert_eq!(password.algorithm, "argon2id");
        assert_eq!(password.hash, "@fn.hash_customer_password");
        assert_eq!(password.verify, "@fn.verify_customer_password");
        let rate_limit = password.rate_limit.as_ref().expect("password rate_limit");
        assert_eq!(rate_limit.default.as_deref(), Some("5 per 10 minutes"));
        assert!(rate_limit.by_env.is_empty());

        assert_eq!(auth.oauth.len(), 1);
        assert_eq!(auth.oauth[0].provider, "google");
        assert_eq!(auth.oauth[0].adapter, "@adapter.google_oauth");

        let mfa = auth.mfa.as_ref().expect("mfa");
        assert_eq!(mfa.method, "totp");
        assert_eq!(mfa.enroll, "@fn.enroll_customer_totp");
        assert_eq!(mfa.verify, "@validator.verify_customer_totp");

        let sessions = auth.sessions.as_ref().expect("sessions");
        assert_eq!(sessions.resource, "CustomerSession");
        assert_eq!(sessions.ttl, "7 days");
        assert!(!sessions.refresh);
    }

    #[test]
    fn auth_without_identity_errors() {
        let source = r#"
feature customer_auth
  auth
    password
      algorithm argon2id
      hash @fn.h
      verify @fn.v
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("identity"),
            "error should require identity: {message}"
        );
    }

    #[test]
    fn auth_password_without_algorithm_errors() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      hash @fn.h
      verify @fn.v
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("algorithm"),
            "error should require algorithm: {message}"
        );
    }

    #[test]
    fn auth_mfa_without_enroll_errors() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    mfa totp
      verify @validator.totp
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("enroll"),
            "error should require enroll: {message}"
        );
    }

    #[test]
    fn auth_duplicate_block_errors() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

  auth
    identity Customer.email
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("at most one"),
            "error should mention duplicate: {message}"
        );
    }

    #[test]
    fn auth_identity_without_dot_errors() {
        let source = r#"
feature customer_auth
  auth
    identity customeremail
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("dot-qualified"),
            "error should require Resource.field: {message}"
        );
    }

    // Per `docs/proposals/auth-lowering-scope.md` Route A: dedicated
    // happy-path tests for each `auth` child block so a regression in
    // `parse_auth_password` / `parse_auth_sessions` / `parse_auth_oauth` /
    // `parse_auth_mfa` is pinned to the specific child instead of being
    // detected only by the multi-child happy-path test.

    #[test]
    fn auth_password_child_parses_with_rate_limit() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let password = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .password
            .as_ref()
            .expect("password child");
        assert_eq!(password.algorithm, "argon2id");
        assert_eq!(password.hash, "@fn.hash_customer_password");
        assert_eq!(password.verify, "@fn.verify_customer_password");
        let rate_limit = password.rate_limit.as_ref().expect("password rate_limit");
        assert_eq!(rate_limit.default.as_deref(), Some("5 per 10 minutes"));
        assert!(rate_limit.by_env.is_empty());
    }

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

    #[test]
    fn auth_oauth_child_parses_multiple_providers() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    oauth google
      adapter @adapter.google_oauth

    oauth github
      adapter @adapter.github_oauth
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let oauth = &features[0].auth.as_ref().expect("auth").oauth;
        assert_eq!(oauth.len(), 2);
        assert_eq!(oauth[0].provider, "google");
        assert_eq!(oauth[0].adapter, "@adapter.google_oauth");
        assert_eq!(oauth[1].provider, "github");
        assert_eq!(oauth[1].adapter, "@adapter.github_oauth");
    }

    #[test]
    fn auth_mfa_child_parses_with_validator_verify() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let mfa = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .mfa
            .as_ref()
            .expect("mfa child");
        assert_eq!(mfa.method, "totp");
        assert_eq!(mfa.enroll, "@fn.enroll_customer_totp");
        assert_eq!(mfa.verify, "@validator.verify_customer_totp");
        assert!(mfa.adapter.is_none());
    }
}
