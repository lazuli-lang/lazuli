//! Auth HTTP route auto-mounts (`/auth/login`, `/auth/signup`,
//! `/auth/logout`, OAuth provider routes).
//!
//! The route shape lives here because it is a self-contained sub-concern of
//! the auth emitter — the orchestrator decides whether to emit, this module
//! decides which routes exist and how each one is laid out as a
//! `lazuli.RegisterApi(...)` call.

use lazuli_ir::{Auth, AuthOAuthProvider, AuthSessions, Feature, SessionCookie};

use super::super::module::EmitContext;
use super::super::patterns::{PATTERN_AUTH_LOGIN, PATTERN_AUTH_REFRESH, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::format::{
    escape_route_segment, escape_string, write_aligned_kv_rows, write_section_banner,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AuthRoute {
    pub(super) name_suffix: String,
    pub(super) method: &'static str,
    pub(super) path: String,
    pub(super) handler: &'static str,
    pub(super) verify_handler_name: bool,
}

pub(super) fn has_auth_routes(auth_block: &Auth) -> bool {
    auth_block.password.is_some() || auth_block.sessions.is_some() || !auth_block.oauth.is_empty()
}

pub(super) fn auth_routes(auth_block: &Auth) -> Vec<AuthRoute> {
    let mut routes = Vec::new();

    if auth_block.password.is_some() {
        routes.push(AuthRoute {
            name_suffix: "login".to_owned(),
            method: "lazuli.MethodPost",
            path: "/auth/login".to_owned(),
            handler: "auth.LoginHandler",
            verify_handler_name: false,
        });
        routes.push(AuthRoute {
            name_suffix: "signup".to_owned(),
            method: "lazuli.MethodPost",
            path: "/auth/signup".to_owned(),
            handler: "auth.SignupHandler",
            verify_handler_name: false,
        });
    }

    if auth_block.password.is_some() || auth_block.sessions.is_some() {
        routes.push(AuthRoute {
            name_suffix: "logout".to_owned(),
            method: "lazuli.MethodPost",
            path: "/auth/logout".to_owned(),
            handler: "auth.LogoutHandler",
            verify_handler_name: false,
        });
    }

    let mut oauth: Vec<&AuthOAuthProvider> = auth_block.oauth.iter().collect();
    oauth.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then_with(|| a.adapter.cmp(&b.adapter))
    });
    for provider in oauth {
        let provider_path = escape_route_segment(&provider.provider);
        routes.push(AuthRoute {
            name_suffix: format!("oauth.{}", provider.provider),
            method: "lazuli.MethodGet",
            path: format!("/auth/oauth/{provider_path}"),
            handler: "auth.OAuthHandler",
            verify_handler_name: true,
        });
        routes.push(AuthRoute {
            name_suffix: format!("oauth.{}.callback", provider.provider),
            method: "lazuli.MethodGet",
            path: format!("/auth/oauth/{provider_path}/callback"),
            handler: "auth.OAuthCallbackHandler",
            verify_handler_name: true,
        });
    }

    routes
}

/// Emit an `init()` block that registers the feature's
/// `auth.sessions` contract with the runtime session resolver.
/// Wires the production session middleware to this feature's table
/// so `Authorization: Bearer <token>` and the `lazuli_session`
/// cookie populate `Ctx.User` automatically.
pub(super) fn emit_session_resolver_register(
    p: &mut GoPrinter,
    feature: &Feature,
    feature_pascal: &str,
    sessions: &AuthSessions,
) {
    write_section_banner(
        p,
        &[
            format!("Auth session resolver: {}", feature.name),
            "  wires the production session middleware to this feature".to_owned(),
        ],
    );
    let pattern = if sessions.is_rotation_enabled() {
        PATTERN_AUTH_REFRESH
    } else {
        PATTERN_AUTH_LOGIN
    };
    emit_pattern_header(p, pattern);
    p.line("func init() {");
    p.indent();
    p.line(&format!(
        "auth.RegisterSessionContract({feature_pascal}AuthSessions)"
    ));
    if sessions.is_rotation_enabled() {
        p.line(&format!(
            "auth.RegisterRefreshContract({feature_pascal}AuthSessions)"
        ));
    }
    if let Some(cookie) = sessions.cookie.as_ref() {
        emit_session_cookie_config(p, cookie);
    }
    p.dedent();
    p.line("}");
}

/// Emit the `lazuli.ConfigureSessionCookie(...)` call lowering a
/// `auth.sessions.cookie` block's transport axes into the runtime's
/// process-wide `SessionCookieConfig`. Each declared axis becomes an
/// addressable local then a `&local` field on the config literal; absent
/// axes leave the field nil so the runtime keeps its hardcoded default
/// (so a partial block only overrides the axes it names). When the whole
/// `cookie` child is absent the orchestrator never calls this — the
/// generated boot stays byte-identical to the pre-`cookie` runtime,
/// mirroring the `sessionCookieSecureFlag` precedent the cookie impl
/// deferred to wire here.
///
/// Wire, not logic: the runtime's `SessionCookieConfig.opts` does all the
/// overlay work (`runtime/go/lazuli/ctx.go`); codegen only emits the call.
fn emit_session_cookie_config(p: &mut GoPrinter, cookie: &SessionCookie) {
    // Bind each declared axis to a local so the config literal can take
    // its address (every SessionCookieConfig field is a pointer — a nil
    // field means "axis not declared"). gofmt keeps the `:=` block above
    // the struct literal; ordering follows the IR struct so output is
    // deterministic.
    if let Some(name) = cookie.name.as_ref() {
        p.line(&format!("cookieName := \"{}\"", escape_string(name)));
    }
    if let Some(same_site) = cookie.same_site.as_ref() {
        p.line(&format!("cookieSameSite := {}", same_site_expr(same_site)));
    }
    if let Some(secure) = cookie.secure {
        p.line(&format!("cookieSecure := {secure}"));
    }
    if let Some(http_only) = cookie.http_only {
        p.line(&format!("cookieHTTPOnly := {http_only}"));
    }
    if let Some(domain) = cookie.domain.as_ref() {
        p.line(&format!("cookieDomain := \"{}\"", escape_string(domain)));
    }
    if let Some(path) = cookie.path.as_ref() {
        p.line(&format!("cookiePath := \"{}\"", escape_string(path)));
    }

    p.line("lazuli.ConfigureSessionCookie(lazuli.SessionCookieConfig{");
    p.indent();
    let mut rows = Vec::new();
    if cookie.name.is_some() {
        rows.push(("Name:".to_owned(), "&cookieName,".to_owned()));
    }
    if cookie.same_site.is_some() {
        rows.push(("SameSite:".to_owned(), "&cookieSameSite,".to_owned()));
    }
    if cookie.secure.is_some() {
        rows.push(("Secure:".to_owned(), "&cookieSecure,".to_owned()));
    }
    if cookie.http_only.is_some() {
        rows.push(("HTTPOnly:".to_owned(), "&cookieHTTPOnly,".to_owned()));
    }
    if cookie.domain.is_some() {
        rows.push(("Domain:".to_owned(), "&cookieDomain,".to_owned()));
    }
    if cookie.path.is_some() {
        rows.push(("Path:".to_owned(), "&cookiePath,".to_owned()));
    }
    write_aligned_kv_rows(p, &rows);
    p.dedent();
    p.line("})");
}

/// Map the closed `same_site` catalog (`lax | strict | none`, validated
/// at parse time) onto the `net/http.SameSite` constant the runtime
/// `SessionCookieConfig.SameSite` pointer expects. Any out-of-catalog
/// value (only reachable if the analyzer regressed) falls back to the
/// runtime default `Lax` so the emitted Go still compiles.
fn same_site_expr(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "strict" => "http.SameSiteStrictMode",
        "none" => "http.SameSiteNoneMode",
        // "lax" and any unexpected value default to Lax (runtime default).
        _ => "http.SameSiteLaxMode",
    }
}

pub(super) fn emit_auth_routes(
    p: &mut GoPrinter,
    feature: &Feature,
    routes: &[AuthRoute],
    emit_ctx: &EmitContext<'_>,
) {
    write_section_banner(
        p,
        &[
            format!("Auth HTTP routes: {}", feature.name),
            "  canonical auth auto-mounts".to_owned(),
        ],
    );
    emit_pattern_header(p, PATTERN_AUTH_LOGIN);
    p.line("func init() {");
    p.indent();
    for route in routes {
        if route.verify_handler_name {
            p.line("// TODO(runtime): handler name verification");
        }
        p.line("lazuli.RegisterApi(&lazuli.Api[any, any]{");
        p.indent();
        write_aligned_kv_rows(
            p,
            &[
                (
                    "Name:".to_owned(),
                    format!(
                        "\"{}.auth.{}\",",
                        escape_string(&feature.name),
                        escape_string(&route.name_suffix)
                    ),
                ),
                (
                    "Feature:".to_owned(),
                    format!("\"{}\",", escape_string(&feature.name)),
                ),
                ("Method:".to_owned(), format!("{},", route.method)),
                (
                    "Path:".to_owned(),
                    format!("\"{}\",", escape_string(&route.path)),
                ),
                ("Handler:".to_owned(), format!("{},", route.handler)),
            ],
        );
        emit_ctx.emit_with_source_field(p, "auth", &route.name_suffix, None);
        p.dedent();
        p.line("})");
    }
    p.dedent();
    p.line("}");
}
