//! Auth HTTP route auto-mounts (`/auth/login`, `/auth/signup`,
//! `/auth/logout`, OAuth provider routes).
//!
//! The route shape lives here because it is a self-contained sub-concern of
//! the auth emitter — the orchestrator decides whether to emit, this module
//! decides which routes exist and how each one is laid out as a
//! `lazuli.RegisterApi(...)` call.

use lazuli_ir::{Auth, AuthOAuthProvider, Feature};

use super::super::module::EmitContext;
use super::super::patterns::{PATTERN_AUTH_LOGIN, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::format::{escape_route_segment, escape_string, write_aligned_kv_rows, write_section_banner};

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
