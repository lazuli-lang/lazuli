//! Authentication block lowering.
//!
//! ## Role in the pipeline
//!
//! Lifts the `auth { ... }` block (identity field, password/sessions
//! /MFA/OAuth sub-blocks) from `syntax::Auth` onto `ir::Auth`. This
//! lowering is mostly structural — every leaf is a verbatim field
//! copy — with one non-trivial duty: splitting `<Resource>.<field>`
//! into a typed `FieldRef`, rejecting empty or multi-dotted forms
//! that the parser somehow lets through.
//!
//! `auth.identity` is the only slot the analyzer validates beyond
//! field copying. Everything else (`password`, `sessions`,
//! `sessions.rotation`, `mfa`, `oauth`) is lowered structurally and
//! left for doctor / codegen to enforce richer rules (TTL parsing,
//! provider catalog, MFA verifier shape).
//!
//! ## Cross-references
//!
//! * Input: `lazuli_syntax::ast::Auth`, `AuthIdentity`, `AuthPassword`,
//!   `AuthSessions`, `AuthSessionRotation`, `AuthTheftDetectionAction`,
//!   `AuthMfa`, `AuthOAuthProvider`.
//! * Output: `lazuli_ir::Auth`, `AuthIdentity`, `AuthPassword`,
//!   `AuthSessions`, `RotationConfig`, `TheftAction`, `AuthMfa`,
//!   `AuthOAuthProvider`, `FieldRef`.
//! * Diagnostic: `InvalidAuthIdentity` (raised on missing dot,
//!   multi-dot, or empty segments).
//!
//! ## ABI guarantee
//!
//! `lower_auth` is `pub` (consumed by `lazuli_cli` via the canonical
//! `lazuli_analyzer::lower_auth` path); per-sub-block helpers stay
//! `pub(crate)` because the in-crate `tests` module exercises
//! `lower_auth_identity` directly.

use crate::helpers::span_of;
use crate::resource::lower_rate_limit_spec;
use crate::{AnalyzeError, lower_public_contract, qualified_name_local};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Phase L — lower a canonical-indent `auth` block into the IR `Auth`
/// shape. The translation is mostly structural; the analyzer's only
/// non-trivial duty is splitting `Customer.email` into `FieldRef`.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::lower_auth;
/// use lazuli_syntax::Auth;
///
/// let auth: Auth = unimplemented!("from canonical-indent parse");
/// let lowered = lower_auth(&auth)?;
/// assert!(!lowered.identity.field.field.is_empty());
/// # Ok::<(), lazuli_analyzer::AnalyzeError>(())
/// ```
pub fn lower_auth(auth: &syntax::Auth) -> Result<ir::Auth, AnalyzeError> {
    Ok(ir::Auth {
        identity: lower_auth_identity(&auth.identity)?,
        password: auth.password.as_ref().map(lower_auth_password),
        sessions: auth.sessions.as_ref().map(lower_auth_sessions),
        mfa: auth.mfa.as_ref().map(lower_auth_mfa),
        oauth: auth.oauth.iter().map(lower_auth_oauth).collect(),
        span_ref: Some(span_of(auth.span)),
    })
}

pub(crate) fn lower_auth_identity(
    identity: &syntax::AuthIdentity,
) -> Result<ir::AuthIdentity, AnalyzeError> {
    let (resource, field) =
        identity
            .field
            .split_once('.')
            .ok_or_else(|| AnalyzeError::InvalidAuthIdentity {
                reference: identity.field.clone(),
            })?;
    if resource.is_empty() || field.is_empty() || field.contains('.') {
        return Err(AnalyzeError::InvalidAuthIdentity {
            reference: identity.field.clone(),
        });
    }
    Ok(ir::AuthIdentity {
        field: ir::FieldRef {
            resource: qualified_name_local(resource),
            field: field.to_owned(),
        },
        public_contract: lower_public_contract(&identity.public_contract),
    })
}

pub(crate) fn lower_auth_password(password: &syntax::AuthPassword) -> ir::AuthPassword {
    ir::AuthPassword {
        algorithm: password.algorithm.clone(),
        hash: password.hash.clone(),
        verify: password.verify.clone(),
        rate_limit: password.rate_limit.as_ref().map(lower_rate_limit_spec),
    }
}

pub(crate) fn lower_auth_sessions(sessions: &syntax::AuthSessions) -> ir::AuthSessions {
    ir::AuthSessions {
        resource: qualified_name_local(&sessions.resource),
        ttl: sessions.ttl.clone(),
        refresh: sessions.refresh,
        // Populated in S3 when the orchestrator wires resource FieldSpec lookup.
        extra_columns: vec![],
        access_ttl: sessions.access_ttl.as_ref().map(|ttl| ttl.value.clone()),
        rotation: sessions.rotation.as_ref().map(lower_auth_session_rotation),
        cookie: sessions.cookie.as_ref().map(lower_auth_session_cookie),
    }
}

/// Lower a parsed `cookie` block into [`ir::SessionCookie`]. Mirrors how
/// [`lower_auth_session_rotation`] carries each `Option<_>` slot straight
/// through — `None` slots stay `None` so the runtime keeps its hardcoded
/// literal for that axis. The closed `same_site` value rides as its raw
/// string (parse-time validated).
pub(crate) fn lower_auth_session_cookie(cookie: &syntax::AuthSessionCookie) -> ir::SessionCookie {
    ir::SessionCookie {
        name: cookie.name.clone(),
        same_site: cookie.same_site.clone(),
        secure: cookie.secure,
        http_only: cookie.http_only,
        domain: cookie.domain.clone(),
        path: cookie.path.clone(),
        span_ref: Some(span_of(cookie.span)),
    }
}

pub(crate) fn lower_auth_session_rotation(
    rotation: &syntax::AuthSessionRotation,
) -> ir::RotationConfig {
    ir::RotationConfig {
        refresh_ttl: rotation.refresh_ttl.as_ref().map(|ttl| ttl.value.clone()),
        grace: rotation.grace.as_ref().map(|grace| grace.value.clone()),
        theft_detection_action: rotation
            .theft_detection_action
            .as_ref()
            .map(|action| lower_auth_theft_action(action.action)),
        span_ref: Some(span_of(rotation.span)),
    }
}

pub(crate) fn lower_auth_theft_action(action: syntax::AuthTheftDetectionAction) -> ir::TheftAction {
    match action {
        syntax::AuthTheftDetectionAction::RevokeSessionFamily => {
            ir::TheftAction::RevokeSessionFamily
        }
        syntax::AuthTheftDetectionAction::RevokeUser => ir::TheftAction::RevokeUser,
    }
}

pub(crate) fn lower_auth_mfa(mfa: &syntax::AuthMfa) -> ir::AuthMfa {
    ir::AuthMfa {
        method: mfa.method.clone(),
        enroll: mfa.enroll.clone(),
        verify: mfa.verify.clone(),
        adapter: mfa.adapter.clone(),
    }
}

pub(crate) fn lower_auth_oauth(oauth: &syntax::AuthOAuthProvider) -> ir::AuthOAuthProvider {
    ir::AuthOAuthProvider {
        provider: oauth.provider.clone(),
        adapter: oauth.adapter.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_auth_identity_splits_dotted_pair() {
        let identity = syntax::AuthIdentity {
            field: "Customer.email".into(),
            public_contract: None,
            span: syntax::Span { start: 0, end: 0 },
        };
        let lowered = lower_auth_identity(&identity).expect("dotted pair lowers");
        assert_eq!(lowered.field.resource.name, "Customer");
        assert_eq!(lowered.field.field, "email");
    }

    #[test]
    fn lower_auth_identity_rejects_missing_dot() {
        let identity = syntax::AuthIdentity {
            field: "no_dot_here".into(),
            public_contract: None,
            span: syntax::Span { start: 0, end: 0 },
        };
        assert!(matches!(
            lower_auth_identity(&identity),
            Err(AnalyzeError::InvalidAuthIdentity { .. })
        ));
    }
}
