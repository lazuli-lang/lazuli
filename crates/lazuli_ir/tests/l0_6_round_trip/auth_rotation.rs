//! ir-auth-refresh-rotation §3 — TheftAction + RotationConfig + AuthSessions
//! resolver methods. See docs/proposals/ir-auth-refresh-rotation.md.

use serde_json::json;

use lazuli_ir::{AuthSessions, QualifiedName, RotationConfig, SpanRef, TheftAction};

use super::round_trip;

fn user_session_qn() -> QualifiedName {
    QualifiedName {
        feature: Some("account".to_string()),
        name: "UserSession".to_string(),
    }
}

fn legacy_sessions() -> AuthSessions {
    AuthSessions {
        resource: user_session_qn(),
        ttl: "7 days".to_string(),
        refresh: false,
        extra_columns: Vec::new(),
        access_ttl: None,
        rotation: None,
        cookie: None,
    }
}

fn rotation_sessions(rotation: RotationConfig) -> AuthSessions {
    AuthSessions {
        resource: user_session_qn(),
        ttl: "7 days".to_string(),
        refresh: false,
        extra_columns: Vec::new(),
        access_ttl: None,
        rotation: Some(rotation),
        cookie: None,
    }
}

#[test]
fn theft_action_round_trip_and_default() {
    round_trip(&TheftAction::RevokeSessionFamily);
    round_trip(&TheftAction::RevokeUser);
    assert_eq!(TheftAction::default(), TheftAction::RevokeSessionFamily);

    // snake_case at the wire — required for parity with .lzi keyword.
    assert_eq!(
        serde_json::to_value(TheftAction::RevokeSessionFamily).unwrap(),
        json!("revoke_session_family")
    );
    assert_eq!(
        serde_json::to_value(TheftAction::RevokeUser).unwrap(),
        json!("revoke_user")
    );
}

#[test]
fn rotation_config_round_trips_with_all_slots() {
    round_trip(&RotationConfig {
        refresh_ttl: Some("30 days".to_string()),
        grace: Some("30 seconds".to_string()),
        theft_detection_action: Some(TheftAction::RevokeSessionFamily),
        span_ref: Some(SpanRef {
            start: 100,
            end: 200,
        }),
    });
}

#[test]
fn rotation_config_round_trips_with_all_slots_absent() {
    // Empty rotation block: presence = enabled, all defaults kick in.
    round_trip(&RotationConfig {
        refresh_ttl: None,
        grace: None,
        theft_detection_action: None,
        span_ref: None,
    });
}

#[test]
fn rotation_config_omits_none_fields_when_serialized() {
    let cfg = RotationConfig {
        refresh_ttl: None,
        grace: None,
        theft_detection_action: None,
        span_ref: None,
    };
    let v = serde_json::to_value(&cfg).unwrap();
    // Author wrote `rotation` block with no inner slots — the JSON
    // must be an empty object, not e.g. {"refresh_ttl": null, ...}.
    assert_eq!(v, json!({}));
}

#[test]
fn auth_sessions_legacy_back_compat_deserializes() {
    // Pre-this-cell fixtures lack access_ttl + rotation. Confirm they
    // still deserialize cleanly with the new fields defaulting to None.
    let legacy_json = json!({
        "resource": { "feature": "account", "name": "UserSession" },
        "ttl": "7 days",
        "refresh": false
    });
    let parsed: AuthSessions =
        serde_json::from_value(legacy_json).expect("legacy fixture must deserialize");
    assert_eq!(parsed.ttl, "7 days");
    assert!(!parsed.refresh);
    assert!(parsed.access_ttl.is_none());
    assert!(parsed.rotation.is_none());
    assert!(!parsed.is_rotation_enabled());
}

#[test]
fn auth_sessions_resolves_legacy_ttl_when_neither_set() {
    let s = legacy_sessions();
    assert_eq!(s.resolved_access_ttl(), "7 days");
    assert_eq!(s.resolved_refresh_ttl(), None);
    assert_eq!(s.resolved_rotation_grace(), None);
    assert_eq!(s.resolved_theft_action(), None);
}

#[test]
fn auth_sessions_resolves_framework_defaults_when_rotation_on() {
    let s = rotation_sessions(RotationConfig {
        refresh_ttl: None,
        grace: None,
        theft_detection_action: None,
        span_ref: None,
    });
    assert!(s.is_rotation_enabled());
    // access_ttl=None + rotation on => "15 minutes" framework default.
    assert_eq!(s.resolved_access_ttl(), "15 minutes");
    assert_eq!(s.resolved_refresh_ttl(), Some("30 days"));
    assert_eq!(s.resolved_rotation_grace(), Some("30 seconds"));
    assert_eq!(
        s.resolved_theft_action(),
        Some(TheftAction::RevokeSessionFamily)
    );
}

#[test]
fn auth_sessions_resolves_explicit_values_when_set() {
    let mut s = rotation_sessions(RotationConfig {
        refresh_ttl: Some("14 days".to_string()),
        grace: Some("60 seconds".to_string()),
        theft_detection_action: Some(TheftAction::RevokeUser),
        span_ref: None,
    });
    s.access_ttl = Some("10 minutes".to_string());

    assert_eq!(s.resolved_access_ttl(), "10 minutes");
    assert_eq!(s.resolved_refresh_ttl(), Some("14 days"));
    assert_eq!(s.resolved_rotation_grace(), Some("60 seconds"));
    assert_eq!(s.resolved_theft_action(), Some(TheftAction::RevokeUser));
}

#[test]
fn auth_sessions_resolves_access_ttl_falls_back_to_legacy_when_rotation_off() {
    let mut s = legacy_sessions();
    s.access_ttl = None;
    // Rotation off, access_ttl not set → legacy ttl.
    assert_eq!(s.resolved_access_ttl(), "7 days");

    s.access_ttl = Some("3 hours".to_string());
    // Rotation off, access_ttl explicit → explicit value wins.
    assert_eq!(s.resolved_access_ttl(), "3 hours");
}

#[test]
fn auth_sessions_round_trips_with_nested_rotation_block() {
    round_trip(&rotation_sessions(RotationConfig {
        refresh_ttl: Some("30 days".to_string()),
        grace: Some("30 seconds".to_string()),
        theft_detection_action: Some(TheftAction::RevokeSessionFamily),
        span_ref: None,
    }));
}
