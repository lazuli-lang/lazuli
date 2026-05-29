// Cookie-emission tests for the `auth.sessions.cookie` child, split from
// the inline `mod tests` in `auth/mod.rs` so the parent stays under the
// 500-LOC Rails budget. `include!`d into `mod tests`, so the shared
// helpers (`base_feature`, `auth_with_identity`, `emit`, `qname`) and the
// `SessionCookie` import resolve from the enclosing module.

fn sessions_with_cookie(cookie: Option<SessionCookie>) -> AuthSessions {
    AuthSessions {
        resource: qname("UserSession"),
        ttl: "7 days".to_owned(),
        refresh: false,
        extra_columns: vec![],
        access_ttl: None,
        rotation: None,
        cookie,
    }
}

#[test]
fn cookie_child_emits_configure_session_cookie_with_declared_axes() {
    // The task's canonical example: same_site strict + secure true +
    // name "myapp_session". The declared axes flow into the
    // ConfigureSessionCookie call as addressable locals; undeclared
    // axes leave the config field nil (runtime keeps its default).
    let mut feature = base_feature("portal");
    let mut auth = auth_with_identity("User", "email");
    auth.sessions = Some(sessions_with_cookie(Some(SessionCookie {
        name: Some("myapp_session".to_owned()),
        same_site: Some("strict".to_owned()),
        secure: Some(true),
        http_only: None,
        domain: None,
        path: None,
        span_ref: None,
    })));
    feature.auth = Some(auth);

    let out = emit(&feature).expect("must emit");
    // `net/http` is imported (same_site lowers to an http constant).
    assert!(out.contains("\"net/http\""));
    // Declared-axis locals.
    assert!(out.contains("cookieName := \"myapp_session\""));
    assert!(out.contains("cookieSameSite := http.SameSiteStrictMode"));
    assert!(out.contains("cookieSecure := true"));
    // The runtime configuration call sits inside the session resolver
    // init() alongside RegisterSessionContract.
    assert!(out.contains("auth.RegisterSessionContract(PortalAuthSessions)"));
    assert!(out.contains("lazuli.ConfigureSessionCookie(lazuli.SessionCookieConfig{"));
    assert!(out.contains("Name:     &cookieName,"));
    assert!(out.contains("SameSite: &cookieSameSite,"));
    assert!(out.contains("Secure:   &cookieSecure,"));
    assert!(out.contains("})"));
    // Undeclared axes never appear.
    assert!(!out.contains("HTTPOnly:"));
    assert!(!out.contains("cookieDomain"));
    assert!(!out.contains("cookiePath"));
}

#[test]
fn sessions_without_cookie_emit_no_configure_call() {
    // Back-compat: a `cookie`-less auth.sessions must not emit any
    // ConfigureSessionCookie call nor pull in `net/http`, so the
    // generated boot is byte-identical to the pre-`cookie` runtime.
    let mut feature = base_feature("portal");
    let mut auth = auth_with_identity("User", "email");
    auth.sessions = Some(sessions_with_cookie(None));
    feature.auth = Some(auth);

    let out = emit(&feature).expect("must emit");
    assert!(out.contains("auth.RegisterSessionContract(PortalAuthSessions)"));
    assert!(!out.contains("ConfigureSessionCookie"));
    assert!(!out.contains("SessionCookieConfig"));
    assert!(!out.contains("\"net/http\""));
    assert!(!out.contains("cookieName"));
}

#[test]
fn cookie_without_same_site_omits_http_import() {
    // A cookie that declares only string/bool axes lowers to plain
    // locals — `net/http` stays out of the import block.
    let mut feature = base_feature("portal");
    let mut auth = auth_with_identity("User", "email");
    auth.sessions = Some(sessions_with_cookie(Some(SessionCookie {
        name: None,
        same_site: None,
        secure: Some(false),
        http_only: Some(true),
        domain: Some(".example.com".to_owned()),
        path: Some("/app".to_owned()),
        span_ref: None,
    })));
    feature.auth = Some(auth);

    let out = emit(&feature).expect("must emit");
    assert!(!out.contains("\"net/http\""));
    assert!(out.contains("lazuli.ConfigureSessionCookie(lazuli.SessionCookieConfig{"));
    assert!(out.contains("cookieSecure := false"));
    assert!(out.contains("cookieHTTPOnly := true"));
    assert!(out.contains("cookieDomain := \".example.com\""));
    assert!(out.contains("cookiePath := \"/app\""));
    assert!(out.contains("Secure:   &cookieSecure,"));
    assert!(out.contains("HTTPOnly: &cookieHTTPOnly,"));
    assert!(out.contains("Domain:   &cookieDomain,"));
    assert!(out.contains("Path:     &cookiePath,"));
    // Undeclared axes absent.
    assert!(!out.contains("cookieName"));
    assert!(!out.contains("cookieSameSite"));
}

#[test]
fn cookie_same_site_catalog_maps_each_value() {
    for (raw, want) in [
        ("lax", "http.SameSiteLaxMode"),
        ("strict", "http.SameSiteStrictMode"),
        ("none", "http.SameSiteNoneMode"),
    ] {
        let mut feature = base_feature("portal");
        let mut auth = auth_with_identity("User", "email");
        auth.sessions = Some(sessions_with_cookie(Some(SessionCookie {
            name: None,
            same_site: Some(raw.to_owned()),
            secure: None,
            http_only: None,
            domain: None,
            path: None,
            span_ref: None,
        })));
        feature.auth = Some(auth);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains(&format!("cookieSameSite := {want}")),
            "same_site `{raw}` must map to `{want}`; got:\n{out}"
        );
    }
}
