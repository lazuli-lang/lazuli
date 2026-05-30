use lazuli_ir as ir;

use lazuli_syntax::{parse_feature_skeletons, parse_lzx_document};

use crate::auth::lower_auth_identity;
use crate::query::parse_query_filter_line;
use crate::resource::lower_validate_line;
use crate::{
    AnalyzeError, lower_audit_block, lower_feature_skeleton, lower_lzx_document,
    lower_policy_atom_with_args, parse_cap_file_type, resolve_invalidates_targets,
    type_ref_from_syntax,
};

// -------------------------------------------------------------------------
// Phase L — `auth` block lowering
// -------------------------------------------------------------------------

#[test]
fn lower_auth_full_block_to_ir() {
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
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let auth = feature.auth.expect("auth lowered");

    assert_eq!(auth.identity.field.resource.name, "Customer");
    assert_eq!(auth.identity.field.field, "email");

    let password = auth.password.as_ref().expect("password");
    assert_eq!(password.algorithm, "argon2id");
    assert_eq!(password.hash, "@fn.hash_customer_password");
    assert_eq!(password.verify, "@fn.verify_customer_password");
    let rate_limit = password.rate_limit.as_ref().expect("rate_limit");
    assert_eq!(rate_limit.default, "5 per 10 minutes");
    assert!(rate_limit.by_env.is_empty());

    let mfa = auth.mfa.as_ref().expect("mfa");
    assert_eq!(mfa.method, "totp");
    assert_eq!(mfa.enroll, "@fn.enroll_customer_totp");
    assert_eq!(mfa.verify, "@validator.verify_customer_totp");

    let sessions = auth.sessions.as_ref().expect("sessions");
    assert_eq!(sessions.resource.name, "CustomerSession");
    assert_eq!(sessions.ttl, "7 days");
    assert!(!sessions.refresh);
    assert!(sessions.access_ttl.is_none());
    assert!(sessions.rotation.is_none());

    assert_eq!(auth.oauth.len(), 1);
    assert_eq!(auth.oauth[0].provider, "google");
    assert_eq!(auth.oauth[0].adapter, "@adapter.google_oauth");
}

#[test]
fn lower_auth_sessions_rotation_block_to_ir() {
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
        theft_detection_action revoke_user
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let sessions = feature
        .auth
        .as_ref()
        .expect("auth lowered")
        .sessions
        .as_ref()
        .expect("sessions lowered");

    assert_eq!(sessions.access_ttl.as_deref(), Some("15 minutes"));
    let rotation = sessions.rotation.as_ref().expect("rotation lowered");
    assert_eq!(rotation.refresh_ttl.as_deref(), Some("30 days"));
    assert_eq!(rotation.grace.as_deref(), Some("30 seconds"));
    assert_eq!(
        rotation.theft_detection_action,
        Some(ir::TheftAction::RevokeUser)
    );
    assert!(rotation.span_ref.is_some());
    // A rotation-only sessions block leaves the cookie slot None so the
    // runtime keeps its hardcoded cookie literals.
    assert!(sessions.cookie.is_none());
}

#[test]
fn lower_auth_sessions_cookie_block_to_ir() {
    let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        name "lazuli_session"
        same_site strict
        secure true
        http_only false
        domain ".example.com"
        path "/app"
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let sessions = feature
        .auth
        .as_ref()
        .expect("auth lowered")
        .sessions
        .as_ref()
        .expect("sessions lowered");

    let cookie = sessions.cookie.as_ref().expect("cookie lowered");
    assert_eq!(cookie.name.as_deref(), Some("lazuli_session"));
    assert_eq!(cookie.same_site.as_deref(), Some("strict"));
    assert_eq!(cookie.secure, Some(true));
    assert_eq!(cookie.http_only, Some(false));
    assert_eq!(cookie.domain.as_deref(), Some(".example.com"));
    assert_eq!(cookie.path.as_deref(), Some("/app"));
    assert!(cookie.span_ref.is_some());
}

#[test]
fn lower_auth_sessions_partial_cookie_keeps_absent_axes_none() {
    // Only `same_site` authored — every other axis lowers to None so the
    // runtime keeps its hardcoded literal for the rest.
    let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        same_site none
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let cookie = feature
        .auth
        .as_ref()
        .expect("auth lowered")
        .sessions
        .as_ref()
        .expect("sessions lowered")
        .cookie
        .as_ref()
        .expect("cookie lowered");

    assert_eq!(cookie.same_site.as_deref(), Some("none"));
    assert!(cookie.name.is_none());
    assert!(cookie.secure.is_none());
    assert!(cookie.http_only.is_none());
    assert!(cookie.domain.is_none());
    assert!(cookie.path.is_none());
}

#[test]
fn lower_auth_sessions_empty_rotation_block_uses_ir_defaults_later() {
    let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      rotation
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let sessions = feature
        .auth
        .as_ref()
        .expect("auth lowered")
        .sessions
        .as_ref()
        .expect("sessions lowered");

    let rotation = sessions.rotation.as_ref().expect("rotation lowered");
    assert!(rotation.refresh_ttl.is_none());
    assert!(rotation.grace.is_none());
    assert!(rotation.theft_detection_action.is_none());
    assert_eq!(sessions.resolved_access_ttl(), "15 minutes");
    assert_eq!(sessions.resolved_refresh_ttl(), Some("30 days"));
    assert_eq!(sessions.resolved_rotation_grace(), Some("30 seconds"));
    assert_eq!(
        sessions.resolved_theft_action(),
        Some(ir::TheftAction::RevokeSessionFamily)
    );
}

#[test]
fn lower_auth_sessions_without_legacy_refresh_keeps_rotation_none() {
    let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let sessions = feature
        .auth
        .as_ref()
        .expect("auth lowered")
        .sessions
        .as_ref()
        .expect("sessions lowered");

    assert!(!sessions.refresh);
    assert!(sessions.access_ttl.is_none());
    assert!(sessions.rotation.is_none());
}

#[test]
fn lower_auth_identity_with_empty_field_errors() {
    // Parser would already reject `identity .email` because the
    // dot-qualified contract requires both segments; this test
    // documents the analyzer's defensive guard for any future
    // parser shape that lets a stray dot through.
    let identity = lazuli_syntax::AuthIdentity {
        field: "Customer.".to_owned(),
        public_contract: None,
        span: lazuli_syntax::Span::new(0, 9),
    };
    let err = lower_auth_identity(&identity).unwrap_err();
    match err {
        AnalyzeError::InvalidAuthIdentity { reference } => {
            assert_eq!(reference, "Customer.");
        }
        other => panic!("expected InvalidAuthIdentity, got {other:?}"),
    }
}
