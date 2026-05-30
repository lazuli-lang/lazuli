
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
    fn auth_sessions_child_parses_cookie_block() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        name "lazuli_session"
        same_site lax
        secure true
        http_only true
        domain ".example.com"
        path "/"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let sessions = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child");
        let cookie = sessions.cookie.as_ref().expect("cookie block");
        assert!(cookie.span.end > cookie.span.start);
        assert_eq!(cookie.name.as_deref(), Some("lazuli_session"));
        assert_eq!(cookie.same_site.as_deref(), Some("lax"));
        assert_eq!(cookie.secure, Some(true));
        assert_eq!(cookie.http_only, Some(true));
        assert_eq!(cookie.domain.as_deref(), Some(".example.com"));
        assert_eq!(cookie.path.as_deref(), Some("/"));
    }

    #[test]
    fn auth_sessions_child_parses_partial_cookie_block() {
        // Only `same_site` declared — every other axis stays `None` so the
        // runtime keeps its hardcoded literal for the rest.
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        same_site strict
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let cookie = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child")
            .cookie
            .as_ref()
            .expect("cookie block");
        assert_eq!(cookie.same_site.as_deref(), Some("strict"));
        assert!(cookie.name.is_none());
        assert!(cookie.secure.is_none());
        assert!(cookie.http_only.is_none());
        assert!(cookie.domain.is_none());
        assert!(cookie.path.is_none());
    }

    #[test]
    fn auth_sessions_cookie_rejects_unknown_same_site() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        same_site whenever
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("unknown `same_site`"),
            "error should mention closed-catalog same_site: {message}"
        );
    }

    #[test]
    fn auth_sessions_cookie_rejects_unknown_child() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        partitioned true
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("`auth sessions cookie` children are"),
            "error should reject unknown cookie child: {message}"
        );
    }

    #[test]
    fn auth_sessions_child_keeps_cookie_none_when_omitted() {
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
        assert!(sessions.cookie.is_none());
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
