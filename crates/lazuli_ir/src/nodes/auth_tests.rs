
    use super::*;

    #[test]
    fn theft_action_default_is_revoke_family() {
        assert_eq!(TheftAction::default(), TheftAction::RevokeSessionFamily);
    }

    #[test]
    fn rotation_disabled_falls_back_to_legacy_ttl() {
        let sessions = AuthSessions {
            resource: QualifiedName {
                feature: None,
                name: "CustomerSession".into(),
            },
            ttl: "7 days".into(),
            refresh: false,
            extra_columns: vec![],
            access_ttl: None,
            rotation: None,
            cookie: None,
        };
        assert!(!sessions.is_rotation_enabled());
        assert_eq!(sessions.resolved_access_ttl(), "7 days");
        assert_eq!(sessions.resolved_refresh_ttl(), None);
        assert_eq!(sessions.resolved_rotation_grace(), None);
        assert_eq!(sessions.resolved_theft_action(), None);
    }

    #[test]
    fn rotation_enabled_uses_framework_defaults_when_inner_slots_absent() {
        let sessions = AuthSessions {
            resource: QualifiedName {
                feature: None,
                name: "CustomerSession".into(),
            },
            ttl: "7 days".into(),
            refresh: false,
            extra_columns: vec![],
            access_ttl: None,
            rotation: Some(RotationConfig {
                refresh_ttl: None,
                grace: None,
                theft_detection_action: None,
                span_ref: None,
            }),
            cookie: None,
        };
        assert!(sessions.is_rotation_enabled());
        assert_eq!(sessions.resolved_access_ttl(), "15 minutes");
        assert_eq!(sessions.resolved_refresh_ttl(), Some("30 days"));
        assert_eq!(sessions.resolved_rotation_grace(), Some("30 seconds"));
        assert_eq!(
            sessions.resolved_theft_action(),
            Some(TheftAction::RevokeSessionFamily)
        );
    }

    #[test]
    fn explicit_rotation_values_override_defaults() {
        let sessions = AuthSessions {
            resource: QualifiedName {
                feature: None,
                name: "CustomerSession".into(),
            },
            ttl: "7 days".into(),
            refresh: false,
            extra_columns: vec![],
            access_ttl: Some("5 minutes".into()),
            rotation: Some(RotationConfig {
                refresh_ttl: Some("90 days".into()),
                grace: Some("10 seconds".into()),
                theft_detection_action: Some(TheftAction::RevokeUser),
                span_ref: None,
            }),
            cookie: None,
        };
        assert_eq!(sessions.resolved_access_ttl(), "5 minutes");
        assert_eq!(sessions.resolved_refresh_ttl(), Some("90 days"));
        assert_eq!(sessions.resolved_rotation_grace(), Some("10 seconds"));
        assert_eq!(
            sessions.resolved_theft_action(),
            Some(TheftAction::RevokeUser)
        );
    }

    #[test]
    fn cookie_absent_serializes_no_key() {
        // Back-compat: a `cookie`-less AuthSessions must not emit a
        // `cookie` key, so older IR consumers see byte-identical JSON.
        let sessions = AuthSessions {
            resource: QualifiedName {
                feature: None,
                name: "CustomerSession".into(),
            },
            ttl: "7 days".into(),
            refresh: false,
            extra_columns: vec![],
            access_ttl: None,
            rotation: None,
            cookie: None,
        };
        let json = serde_json::to_value(&sessions).unwrap();
        assert!(
            json.get("cookie").is_none(),
            "absent cookie must skip-serialize, got: {json}"
        );
    }

    #[test]
    fn session_cookie_round_trips_through_json() {
        // A fully-populated SessionCookie survives a serialize → deserialize
        // round-trip with every axis preserved (the closed `same_site`
        // catalog rides as its raw string).
        let cookie = SessionCookie {
            name: Some("lazuli_session".into()),
            same_site: Some("strict".into()),
            secure: Some(true),
            http_only: Some(true),
            domain: Some(".example.com".into()),
            path: Some("/".into()),
            span_ref: Some(SpanRef { start: 1, end: 2 }),
        };
        let json = serde_json::to_string(&cookie).unwrap();
        let back: SessionCookie = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cookie);
    }

    #[test]
    fn session_cookie_partial_skips_absent_axes() {
        // A partial cookie (only `secure`) skip-serializes the rest, so
        // the runtime keeps its hardcoded literal for every absent axis.
        let cookie = SessionCookie {
            name: None,
            same_site: None,
            secure: Some(false),
            http_only: None,
            domain: None,
            path: None,
            span_ref: None,
        };
        let json = serde_json::to_value(&cookie).unwrap();
        assert_eq!(json.get("secure"), Some(&serde_json::json!(false)));
        for absent in ["name", "same_site", "http_only", "domain", "path"] {
            assert!(
                json.get(absent).is_none(),
                "absent axis `{absent}` must skip-serialize, got: {json}"
            );
        }
        let back: SessionCookie = serde_json::from_value(json).unwrap();
        assert_eq!(back, cookie);
    }
