//! Parser for `app.lzi` — the operational app manifest.
//!
//! This is the single largest entry point in the `app_manifest` sub-tree
//! because `app.lzi` is the widest contract in Lazuli: a single block
//! covers identity (`title`, `version`, `lazuli_version`), targets,
//! environment list, URL templates, CORS, security headers (HSTS / CSP),
//! cookies, proxy trust, request limits, route guards, env vars (with
//! environment scoping + grouping), integrations + credential bindings,
//! capability slots, architecture + services, communication defaults,
//! runtime units (with i18n locale negotiation), deploy topology + roll
//! strategy, logging / tracing / observability, locale defaults, and
//! encryption key bindings. Each section is dispatched by `app_child` at
//! indent-2; indent-4 / indent-6 / indent-8 lines fill the open block.
//!
//! The body is one large state machine on purpose — the cross-section
//! invariants (`current_integration` cleared by indent-2 breaks,
//! `state.in_headers_hsts` resets on outer transitions, etc.) cannot be
//! cleanly split without leaking state across functions. The cost is the
//! file length; the benefit is no surprise side-effects between
//! adjacent sections.
//!
//! See: `lazuli_ir::nodes::app_manifest::AppManifest`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

use lazuli_ir::{AppManifest, ErrorPage, RouteGuardDefaults};

use super::manifest_indent::{ManifestParseState, handle_indent6, handle_indent8};
use super::manifest_indent4::handle_indent4;
use super::parsers::{app_child, leading_spaces, line_span_ref, line_start_offsets, split_items, unquote};

/// Drive the indented state machine that parses an `app.lzi` body into
/// an [`AppManifest`].
///
/// Returns `None` only when the source does not contain a top-level
/// `app <Name>` header — everything else (unknown indent-2 children,
/// malformed indent-4 fields, etc.) is silently dropped so doctor can
/// surface shape errors with full context downstream. The four indent
/// tiers (2 / 4 / 6 / 8) are dispatched through the handlers in
/// `manifest_indent` / `manifest_indent4`; cross-section cursors live on
/// `ManifestParseState`.
///
/// If `route_guard` was never declared but `auth_failed_redirect` was,
/// the function synthesizes a guard with `on_unauthenticated` set to
/// preserve the legacy redirect contract.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::app_manifest::manifest::parse_app_manifest;
///
/// let src = "app Hostpoint\n  title \"Hostpoint\"\n  version \"1.0\"\n";
/// let manifest = parse_app_manifest(src).expect("app header");
/// assert_eq!(manifest.name, "Hostpoint");
/// assert_eq!(manifest.title.as_deref(), Some("Hostpoint"));
/// ```
pub fn parse_app_manifest(source: &str) -> Option<AppManifest> {
    let lines: Vec<_> = source.lines().collect();
    let line_starts = line_start_offsets(source);
    let start = lines
        .iter()
        .position(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("app "))?;
    let header = lines[start].trim_start();
    let name = header.split_whitespace().nth(1)?.to_owned();

    let mut app = AppManifest {
        name,
        title: None,
        version: None,
        lazuli_version: None,
        targets: Vec::new(),
        default_locale: None,
        default_timezone: None,
        auth_failed_redirect: None,
        not_found: None,
        error_pages: Vec::new(),
        uses: Vec::new(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
        headers: None,
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        logging: None,
        tracing: None,
        observability: None,
        locale: None,
        encryption_bindings: Vec::new(),
        cookie: None,
        proxy: None,
        limits: None,
        route_guard: None,
        actor_query: None,
        span_ref: None,
    };
    // All cross-section cursors live on `ManifestParseState` so the
    // indent-6 + indent-8 handlers (in `manifest_indent.rs`) can share
    // them with this indent-2 / indent-4 driver. See the struct for
    // per-field semantics.
    let mut state = ManifestParseState::new();

    for (line_index, line) in lines.iter().enumerate().skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            break;
        }

        match leading_spaces(line) {
            2 => {
                state.reset_for_indent2();
                if let Some(rest) = trimmed.strip_prefix("title ") {
                    app.title = Some(unquote(rest.trim()).to_owned());
                    state.current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("version ") {
                    app.version = Some(unquote(rest.trim()).to_owned());
                    state.current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("lazuli_version ") {
                    app.lazuli_version = Some(unquote(rest.trim()).to_owned());
                    state.current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("default_locale ") {
                    app.default_locale = Some(unquote(rest.trim()).to_owned());
                    state.current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("default_timezone ") {
                    app.default_timezone = Some(unquote(rest.trim()).to_owned());
                    state.current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("auth_failed_redirect ") {
                    app.auth_failed_redirect = Some(rest.trim().to_owned());
                    state.current_child = None;
                } else if trimmed == "route_guard" {
                    app.route_guard = Some(RouteGuardDefaults {
                        default_policy: None,
                        on_unauthenticated: None,
                        on_unauthorized: None,
                        skeleton: None,
                        span_ref: Some(line_span_ref(&line_starts, line_index, line)),
                    });
                    state.current_child = Some("route_guard");
                } else if let Some(rest) = trimmed.strip_prefix("actor_query ") {
                    app.actor_query = Some(unquote(rest.trim()).to_owned());
                    state.current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("not_found ") {
                    app.not_found = Some(rest.trim().to_owned());
                    state.current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("error_page ") {
                    if let Ok(status) = rest.trim().parse::<u16>() {
                        app.error_pages.push(ErrorPage {
                            status,
                            template: String::new(),
                            audience: None,
                        });
                        state.current_error_page = app.error_pages.len().checked_sub(1);
                        state.current_child = Some("error_page");
                    } else {
                        state.current_child = None;
                    }
                } else if let Some(rest) = trimmed.strip_prefix("uses ") {
                    app.uses.extend(split_items(rest));
                    state.current_child = None;
                } else if let Some(child) = app_child(trimmed) {
                    state.current_child = Some(child);
                } else {
                    state.current_child = None;
                }
            }
            4 => handle_indent4(trimmed, line, line_index, &line_starts, &mut app, &mut state),
            6 => handle_indent6(trimmed, &mut app, &mut state),
            8 => handle_indent8(trimmed, &mut app, &state),
            _ => {}
        }
    }

    if app.route_guard.is_none() {
        if let Some(redirect) = app.auth_failed_redirect.clone() {
            app.route_guard = Some(RouteGuardDefaults {
                default_policy: None,
                on_unauthenticated: Some(redirect),
                on_unauthorized: None,
                skeleton: None,
                span_ref: Some(line_span_ref(&line_starts, start, lines[start])),
            });
        }
    }

    Some(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_when_no_app_header() {
        assert!(parse_app_manifest("  not_an_app Foo\n").is_none());
    }

    #[test]
    fn captures_identity_fields() {
        let src = "app Hostpoint\n  title \"Hostpoint\"\n  version \"1.0\"\n";
        let manifest = parse_app_manifest(src).expect("app header");
        assert_eq!(manifest.name, "Hostpoint");
        assert_eq!(manifest.title.as_deref(), Some("Hostpoint"));
        assert_eq!(manifest.version.as_deref(), Some("1.0"));
    }
}
