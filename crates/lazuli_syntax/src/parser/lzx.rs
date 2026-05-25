//! `.lzx` source-text parser — both the app-surface dialect and the
//! per-feature ViewModel dialect.
//!
//! ## What lives here
//!
//! - **App-surface dialect** (`parse_lzx_document`): top-level `app` /
//!   `route` / `experience` / `resume` / `error_page` blocks. One file
//!   per project — `app.lzx`.
//! - **Feature-surface dialect** (`parse_surface_document` + the
//!   `parse_view_*`, `parse_drawer_*`, `parse_filter_*`,
//!   `parse_view_settings_*` family): one file per feature surface —
//!   `features/<feat>/<feat>.{web,mobile}.lzx`. Walks
//!   `surface <feature> web|mobile` + `audience` + `view list|detail|create`
//!   + `drawer` + `filters` + `search` + `sort` + `settings` + `on_success`.
//! - **Policy expression machinery**: `parse_policy_atom`,
//!   `try_parse_policy_expr`, `looks_like_policy_expr`,
//!   `PolicyExprParser`, and the `is_valid_permission_ref` shape check.
//!   These bridge `.lzx` audience `requires` lines and every `.lzi`
//!   `policy` payload, so they are `pub(super)` and used from
//!   `parser/lzi.rs` (and the still-resident parsers in `mod.rs`).
//!
//! ## What stays out
//!
//! - The shared infra (`SourceLine`, error ctors, ident validators,
//!   token scanners) is in `common.rs`.
//! - The `.lzi` feature parsers (`parse_feature_skeletons` and friends)
//!   live in `lzi.rs` (pending extraction) — but they consume the
//!   policy machinery published here.
//! - Cross-module helpers `parse_invalidates_entry` and
//!   `parse_translation_key_token` are imported from the parent module
//!   (`super::`) — they are defined alongside the `.lzi` parsers but
//!   the feature-surface `on_success` block needs them.
//!
//! ## Grammar source-of-truth
//!
//! Hand-rolled two-space indentation. No Pest grammar — each
//! `parse_*` function IS the spec. The proposal references
//! `docs/proposals/lzx-integration-codegen.md` §5 for the
//! feature-surface dialect and `docs/canonical-semantics.md` for the
//! app-surface dialect.

use crate::ast::{
    AudienceAst, BindingRefAst, CellBindingAst, DrawerBindingSourceAst, DrawerRouteBindingAst,
    DrawerSubViewAst, DrawerTriggerAst, FilterCardinalityAst, FilterDeclAst, FlashSpecAst,
    InvalidatesDecl, LzxAction, LzxApp, LzxAudience, LzxDocument, LzxErrorPage, LzxExperience,
    LzxExperienceView, LzxExtensionOrder, LzxExtensionSlot, LzxPlatform, LzxPlatformView,
    LzxRequiresLifecycle, LzxResumeArm, LzxResumeArmKind, LzxResumeRouter, LzxRoute,
    LzxRouteGuardDefaults, LzxSurface, LzxViewExtension, LzxViewGuard, LzxViewTestAssertion,
    OnSuccessSpecAst, PolicyAtomAst, PolicyExprAst, RouteParamAst, SearchDeclAst, SearchFieldAst,
    SearchModeAst, SelectionDeclAst, SelectionModeAst, SettingDeclAst, SettingPersistenceAst,
    SettingValueSpaceAst, SortDeclAst, SortDirAst, Span, SurfaceAst, SurfaceTargetAst, ViewAst,
    ViewCreateAst, ViewDetailAst, ViewListAst,
};

use super::common::{
    SourceLine, find_top_level_token, is_kebab_or_snake_ident, is_lzx_bare_ident,
    is_lzx_resume_ref, is_trivia, line_error, line_error_owned, parse_lzx_bool, source_lines,
    split_lzx_arrow, split_lzx_list, strip_inline_comment, unquote_lzx_value,
};
use super::error::ParseError;
use super::lzi::{parse_invalidates_entry, parse_translation_key_token};

pub fn parse_lzx_document(source: &str) -> Result<LzxDocument, ParseError> {
    let lines = source_lines(source);
    let mut app = None;
    let mut routes = Vec::new();
    let mut experiences = Vec::new();
    let mut surfaces = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent != 0 {
            return Err(line_error(
                line,
                "top-level `.lzx` declarations are not indented",
            ));
        }

        if trimmed.starts_with("app ") {
            if app.is_some() {
                return Err(line_error(
                    line,
                    "`.lzx` files can declare only one `app` manifest",
                ));
            }
            let (parsed_app, next) = parse_lzx_app(&lines, index)?;
            app = Some(parsed_app);
            index = next;
        } else if trimmed.starts_with("route ") {
            let (route, next) = parse_lzx_route(&lines, index)?;
            routes.push(route);
            index = next;
        } else if trimmed.starts_with("experience ") {
            let (experience, next) = parse_lzx_experience(&lines, index)?;
            experiences.push(experience);
            index = next;
        } else if trimmed.starts_with("surface ") {
            let (surface, next) = parse_lzx_surface(&lines, index)?;
            surfaces.push(surface);
            index = next;
        } else {
            return Err(line_error(
                line,
                "expected `app <name>`, `route <name>`, `experience <name>`, or `surface <experience> <platform>`",
            ));
        }
    }

    Ok(LzxDocument {
        app,
        routes,
        experiences,
        surfaces,
        span: Span::new(0, source.len()),
    })
}

fn parse_lzx_app(lines: &[SourceLine<'_>], start: usize) -> Result<(LzxApp, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "app manifests use `app <name>`"));
    }

    let mut title = None;
    let mut version = None;
    let mut targets = Vec::new();
    let mut default_locale = None;
    let mut default_timezone = None;
    let mut auth_failed_redirect = None;
    let mut route_guard = None;
    let mut actor_query = None;
    let mut not_found = None;
    let mut error_pages = Vec::new();
    let mut uses = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "app manifest children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("title ") {
            title = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("version ") {
            version = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if trimmed == "targets" {
            index += 1;
            while index < lines.len() {
                let target_line = &lines[index];
                let target_trimmed = target_line.text.trim_start();
                if is_trivia(target_trimmed) {
                    index += 1;
                    continue;
                }
                if target_line.indent <= 2 {
                    break;
                }
                if target_line.indent != 4 {
                    return Err(line_error(
                        target_line,
                        "app targets use four-space indentation",
                    ));
                }
                targets.push(target_trimmed.to_owned());
                index += 1;
            }
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("targets ") {
            targets.extend(split_lzx_list(rest));
        } else if let Some(rest) = trimmed.strip_prefix("default_locale ") {
            default_locale = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("default_timezone ") {
            default_timezone = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("auth_failed_redirect ") {
            auth_failed_redirect = Some(rest.trim().to_owned());
        } else if trimmed == "route_guard" {
            if route_guard.is_some() {
                return Err(line_error(
                    line,
                    "app manifest declares `route_guard` at most once",
                ));
            }
            let (parsed, next) = parse_lzx_route_guard_defaults(lines, index, 2)?;
            route_guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("actor_query ") {
            actor_query = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("not_found ") {
            not_found = Some(rest.trim().to_owned());
        } else if trimmed.starts_with("error_page ") {
            let (error_page, next) = parse_lzx_error_page(lines, index)?;
            error_pages.push(error_page);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("uses ") {
            uses = split_lzx_list(rest);
        } else {
            return Err(line_error(
                line,
                "app manifest children are `title`, `version`, `targets`, `default_locale`, `default_timezone`, `auth_failed_redirect`, `route_guard`, `actor_query`, `not_found`, `error_page <status>`, or `uses` declarations",
            ));
        }
        index += 1;
    }

    Ok((
        LzxApp {
            name: parts[1].to_owned(),
            title,
            version,
            targets,
            default_locale,
            default_timezone,
            auth_failed_redirect,
            route_guard,
            actor_query,
            not_found,
            error_pages,
            uses,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_route_guard_defaults(
    lines: &[SourceLine<'_>],
    start: usize,
    guard_indent: usize,
) -> Result<(LzxRouteGuardDefaults, usize), ParseError> {
    let header = &lines[start];
    if header.text.trim_start() != "route_guard" {
        return Err(line_error(header, "route guard blocks use `route_guard`"));
    }

    let child_indent = guard_indent + 2;
    let mut default_policy = None;
    let mut on_unauthenticated = None;
    let mut on_unauthorized = None;
    let mut skeleton = None;
    let mut index = start + 1;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            index += 1;
            continue;
        }
        if line.indent <= guard_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`route_guard` children use one indentation level deeper than `route_guard`",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("default_policy ") {
            if default_policy.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `default_policy` at most once",
                ));
            }
            default_policy = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthenticated ") {
            if on_unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `on_unauthenticated` at most once",
                ));
            }
            on_unauthenticated = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthorized ") {
            if on_unauthorized.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `on_unauthorized` at most once",
                ));
            }
            on_unauthorized = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("skeleton ") {
            if skeleton.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `skeleton` at most once",
                ));
            }
            skeleton = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`route_guard` children are `default_policy`, `on_unauthenticated redirect`, `on_unauthorized redirect`, or `skeleton @client.<name>` declarations",
            ));
        }

        last_end = line.end;
        index += 1;
    }

    Ok((
        LzxRouteGuardDefaults {
            default_policy,
            on_unauthenticated,
            on_unauthorized,
            skeleton,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

fn parse_lzx_view_guard(
    lines: &[SourceLine<'_>],
    start: usize,
    policy_indent: usize,
) -> Result<(LzxViewGuard, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let Some(rest) = trimmed.strip_prefix("policy ") else {
        return Err(line_error(
            header,
            "view guard blocks use `policy <policy>`",
        ));
    };
    let policy = parse_lzx_view_guard_policy(header, rest.trim())?;

    let child_indent = policy_indent + 2;
    let mut on_unauthenticated = None;
    let mut on_unauthorized = None;
    let mut requires_lifecycle = None;
    let mut on_lifecycle_pending = None;
    let mut forbid_when: Vec<crate::ast::LzxForbidWhen> = Vec::new();
    let mut index = start + 1;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            index += 1;
            continue;
        }
        if line.indent <= policy_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`policy` redirect children use one indentation level deeper than the `policy` line",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("on_unauthenticated ") {
            if on_unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_unauthenticated` at most once",
                ));
            }
            on_unauthenticated = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthorized ") {
            if on_unauthorized.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_unauthorized` at most once",
                ));
            }
            on_unauthorized = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("requires_lifecycle ") {
            // router-w3 Tier 2 — lifecycle gate. Format:
            //   requires_lifecycle <Resource> = <state>[ on substep <tag>]
            if requires_lifecycle.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `requires_lifecycle` at most once",
                ));
            }
            requires_lifecycle = Some(parse_lzx_requires_lifecycle(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("on_lifecycle_pending ") {
            // router-w3 Tier 2 — optional resume router reference. v1
            // codegen prefers the W4 lifecycle_routes table on the
            // resource; this slot is kept for back-compat with apps
            // that authored resume routers.
            if on_lifecycle_pending.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_lifecycle_pending` at most once",
                ));
            }
            on_lifecycle_pending = Some(parse_lzx_on_lifecycle_pending(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("forbid_when ") {
            // router-w3 Tier 3 — positive-state redirect. Format:
            //   forbid_when <atom> dispatch_to "<url>"
            // where <atom> is `@<ns>.<name>` (e.g. `@role.host`).
            forbid_when.push(parse_lzx_forbid_when(line, rest.trim())?);
        } else {
            return Err(line_error(
                line,
                "`policy` children are `on_unauthenticated redirect \"<path>\"`, `on_unauthorized redirect \"<path>\"`, `requires_lifecycle <Resource> = <state>`, `on_lifecycle_pending @resume <name>`, or `forbid_when <atom> dispatch_to \"<path>\"`",
            ));
        }

        last_end = line.end;
        index += 1;
    }

    Ok((
        LzxViewGuard {
            policy,
            on_unauthenticated,
            on_unauthorized,
            requires_lifecycle,
            on_lifecycle_pending,
            forbid_when,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

/// router-w3 Tier 3 — parse `forbid_when <atom> dispatch_to "<url>"`.
fn parse_lzx_forbid_when(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<crate::ast::LzxForbidWhen, ParseError> {
    let (atom_part, url_part) = text.split_once("dispatch_to").ok_or_else(|| {
        line_error(
            line,
            "`forbid_when` uses `forbid_when <atom> dispatch_to \"<url>\"`",
        )
    })?;
    let atom_ref = atom_part.trim().to_owned();
    if !atom_ref.starts_with('@') {
        return Err(line_error(
            line,
            "`forbid_when` atom must be a policy atom like `@role.host` or `@scope.X`",
        ));
    }
    let url_text = url_part.trim();
    if !url_text.starts_with('"') || !url_text.ends_with('"') || url_text.len() < 2 {
        return Err(line_error(
            line,
            "`forbid_when` URL must be a double-quoted string",
        ));
    }
    let dispatch_to = url_text[1..url_text.len() - 1].to_owned();
    Ok(crate::ast::LzxForbidWhen {
        atom_ref,
        dispatch_to,
        span: Span::new(line.start, line.end),
    })
}

fn parse_lzx_view_guard_policy(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<Vec<String>, ParseError> {
    if value.is_empty() {
        return Err(line_error(line, "`policy` requires a policy reference"));
    }

    if !value.starts_with('[') {
        return Ok(vec![value.to_owned()]);
    }

    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Err(line_error(
            line,
            "`policy` list form is `policy [@policy.a, @policy.b]`",
        ));
    };
    if inner.trim().is_empty() {
        return Err(line_error(
            line,
            "`policy` list requires at least one policy reference",
        ));
    }

    let mut policies = Vec::new();
    for atom in inner.split(',') {
        let atom = atom.trim();
        if atom.is_empty() {
            return Err(line_error(
                line,
                "`policy` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        policies.push(atom.to_owned());
    }
    Ok(policies)
}

fn parse_lzx_redirect_clause(line: &SourceLine<'_>, value: &str) -> Result<String, ParseError> {
    let Some(rest) = value.strip_prefix("redirect ") else {
        return Err(line_error(
            line,
            "route guard redirect clauses use `redirect \"<path>\"`",
        ));
    };
    let target = rest.trim();
    if !target.starts_with('"') || !target.ends_with('"') {
        return Err(line_error(
            line,
            "route guard redirect targets must be quoted strings",
        ));
    }
    Ok(unquote_lzx_value(target).to_owned())
}

fn parse_lzx_requires_lifecycle(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LzxRequiresLifecycle, ParseError> {
    let Some((resource, state)) = rest.split_once('=') else {
        return Err(line_error(
            line,
            "`requires_lifecycle` uses `requires_lifecycle <Resource> = <state>`",
        ));
    };
    let resource = resource.trim();
    let state = state.trim();
    let (state, substep) = parse_lzx_optional_substep_tail(line, state, "`requires_lifecycle`")?;
    if resource.is_empty()
        || !resource
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        || !is_lzx_bare_ident(resource)
    {
        return Err(line_error(
            line,
            "`requires_lifecycle` resource must be an upper-case resource identifier",
        ));
    }
    if !is_lzx_bare_ident(state) {
        return Err(line_error(
            line,
            "`requires_lifecycle` state must be a bare lifecycle state identifier",
        ));
    }
    Ok(LzxRequiresLifecycle {
        resource: resource.to_owned(),
        state: state.to_owned(),
        substep,
        span: Span::new(line.start, line.end),
    })
}

fn parse_lzx_optional_substep_tail<'a>(
    line: &SourceLine<'_>,
    value: &'a str,
    context: &str,
) -> Result<(&'a str, Option<String>), ParseError> {
    let parts: Vec<_> = value.split_whitespace().collect();
    match parts.as_slice() {
        [state] => Ok((*state, None)),
        [state, "substep", substep] => {
            if !is_lzx_bare_ident(substep) {
                return Err(line_error_owned(
                    line,
                    format!("{context} substep must be a bare identifier"),
                ));
            }
            Ok((*state, Some((*substep).to_owned())))
        }
        _ => Err(line_error_owned(
            line,
            format!("{context} accepts an optional `substep <name>` tail"),
        )),
    }
}

fn parse_lzx_on_lifecycle_pending(line: &SourceLine<'_>, rest: &str) -> Result<String, ParseError> {
    let target = if let Some(target) = rest.trim().strip_prefix("@resume ") {
        target.trim()
    } else if let Some(target) = rest.trim().strip_prefix("@resume.") {
        target.trim()
    } else {
        return Err(line_error(
            line,
            "`on_lifecycle_pending` uses `on_lifecycle_pending @resume <name>`",
        ));
    };
    if !is_lzx_resume_ref(target) {
        return Err(line_error(
            line,
            "`on_lifecycle_pending` resume reference must be `<name>` or `<feature>.<name>`",
        ));
    }
    Ok(target.to_owned())
}

fn attach_lzx_requires_lifecycle(
    line: &SourceLine<'_>,
    guard: &mut LzxViewGuard,
    parsed: LzxRequiresLifecycle,
) -> Result<(), ParseError> {
    if guard.requires_lifecycle.is_some() {
        return Err(line_error(
            line,
            "view declares `requires_lifecycle` at most once",
        ));
    }
    guard.span.end = guard.span.end.max(parsed.span.end);
    guard.requires_lifecycle = Some(parsed);
    Ok(())
}

fn attach_lzx_on_lifecycle_pending(
    line: &SourceLine<'_>,
    guard: &mut LzxViewGuard,
    parsed: String,
    span_end: usize,
) -> Result<(), ParseError> {
    if guard.on_lifecycle_pending.is_some() {
        return Err(line_error(
            line,
            "view declares `on_lifecycle_pending` at most once",
        ));
    }
    guard.span.end = guard.span.end.max(span_end);
    guard.on_lifecycle_pending = Some(parsed);
    Ok(())
}

fn parse_lzx_error_page(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxErrorPage, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 || parts[0] != "error_page" {
        return Err(line_error(header, "error pages use `error_page <status>`"));
    }
    let status = parts[1]
        .parse::<u16>()
        .map_err(|_| line_error(header, "error page status must be an HTTP status code"))?;

    let mut template = None;
    let mut audience = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            index += 1;
            continue;
        }
        if line.indent <= 2 {
            break;
        }
        if line.indent != 4 {
            return Err(line_error(
                line,
                "error_page children use four-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("template ") {
            template = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("audience ") {
            audience = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "error_page children are `template \"./...\"` or `audience <name>` declarations",
            ));
        }
        index += 1;
    }

    let template = template.ok_or_else(|| {
        line_error(
            header,
            "`error_page` requires a `template \"./...\"` declaration",
        )
    })?;

    Ok((
        LzxErrorPage {
            status,
            template,
            audience,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_route(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxRoute, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "routes use `route <name>`"));
    }

    let mut path = None;
    let mut routes = Vec::new();
    let mut to = None;
    let mut surface = None;
    let mut audience = None;
    let mut lazy = None;
    let mut prerender = None;
    let mut guard = None;
    let mut loaders: Vec<crate::ast::LzxRouteLoader> = Vec::new();
    let mut pending_view: Option<String> = None;
    let mut error_view: Option<String> = None;
    let mut parent: Option<String> = None;
    let mut route_params: Vec<crate::ast::RouteParamAst> = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(line, "route children use two-space indentation"));
        }

        if let Some(rest) = trimmed.strip_prefix("path ") {
            path = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            // Wave §2 (2026-05-24) — disambiguate two child shapes:
            //   `route <name>: <Type>`  →  typed path-param declaration
            //   `route <name>[, <name>]` → legacy routes-list (still
            //                              accepted, drops to the
            //                              vestigial `routes: Vec<String>`
            //                              field; no live consumer).
            let rest = rest.trim();
            if let Some((name_part, type_part)) = rest.split_once(':') {
                let name = name_part.trim();
                let type_ref = type_part.trim();
                if name.is_empty() || type_ref.is_empty() {
                    return Err(line_error(
                        line,
                        "route param declaration uses `route <name>: <Type>` (both name and type must be non-empty)",
                    ));
                }
                route_params.push(crate::ast::RouteParamAst {
                    name: name.to_owned(),
                    type_ref: type_ref.to_owned(),
                    span: Span::new(line.start, line.end),
                });
            } else {
                routes.extend(split_lzx_list(rest));
            }
        } else if let Some(rest) = trimmed.strip_prefix("to ") {
            to = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("surface ") {
            surface = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("audience ") {
            audience = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("lazy ") {
            lazy =
                Some(parse_lzx_bool(rest.trim()).ok_or_else(|| {
                    line_error(line, "route lazy uses `lazy true` or `lazy false`")
                })?);
        } else if let Some(rest) = trimmed.strip_prefix("prerender ") {
            prerender = Some(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "route declares `policy` at most once"));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 2)?;
            guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("loader ") {
            // router-w5 — `loader <feature>.<query>`. Multiple
            // declarations supported (codegen emits Promise.all).
            let qualified = rest.trim();
            let (feature, query) = qualified.split_once('.').ok_or_else(|| {
                line_error(
                    line,
                    "`loader` references must be `<feature>.<query>` (e.g. `loader host.lookup_my_host`)",
                )
            })?;
            loaders.push(crate::ast::LzxRouteLoader {
                feature: feature.trim().to_owned(),
                query: query.trim().to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("pending_view ") {
            // router-w6 — `pending_view <component_key>`.
            if pending_view.is_some() {
                return Err(line_error(
                    line,
                    "route declares `pending_view` at most once",
                ));
            }
            pending_view = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("error_view ") {
            // router-w6 — `error_view <component_key>`.
            if error_view.is_some() {
                return Err(line_error(line, "route declares `error_view` at most once"));
            }
            error_view = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("parent ") {
            // router-w8 — `parent <route_name>` nests this route
            // under another route's tree.
            if parent.is_some() {
                return Err(line_error(line, "route declares `parent` at most once"));
            }
            parent = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "route children are `path`, `route <name>: <Type>`, `to`, `surface`, `audience`, `lazy`, `prerender`, `loader <feature>.<query>`, `pending_view <key>`, `error_view <key>`, or `policy` declarations",
            ));
        }
        index += 1;
    }

    Ok((
        LzxRoute {
            name: parts[1].to_owned(),
            path,
            routes,
            to,
            surface,
            audience,
            lazy,
            prerender,
            guard,
            loaders,
            pending_view,
            error_view,
            parent,
            route_params,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_experience(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExperience, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "`experience` uses `experience <name>`"));
    }

    let mut imports = Vec::new();
    let mut views = Vec::new();
    let mut resume_routers = Vec::new();
    let mut extensions = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "experience children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("imports ") {
            imports.extend(split_lzx_list(rest));
            index += 1;
        } else if trimmed.starts_with("view ") {
            let (view, next) = parse_lzx_experience_view(lines, index)?;
            views.push(view);
            index = next;
        } else if trimmed.starts_with("resume ") {
            let (resume, next) = parse_lzx_resume_router(lines, index)?;
            resume_routers.push(resume);
            index = next;
        } else if trimmed.starts_with("extends @anchor.") {
            let (extension, next) = parse_lzx_view_extension(lines, index)?;
            extensions.push(extension);
            index = next;
        } else {
            return Err(line_error(
                line,
                "experience children are `imports`, `view`, `resume`, or `extends @anchor.*` declarations",
            ));
        }
    }

    Ok((
        LzxExperience {
            name: parts[1].to_owned(),
            imports,
            views,
            resume_routers,
            extensions,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_resume_router(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxResumeRouter, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "resume blocks use `resume <name>`"));
    }
    if !is_lzx_bare_ident(parts[1]) {
        return Err(line_error(
            header,
            "`resume` name must be a bare identifier",
        ));
    }

    let mut source_query = None;
    let mut arms = Vec::new();
    let mut index = start + 1;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(
                line,
                "resume children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source query.lookup ") {
            if source_query.is_some() {
                return Err(line_error(
                    line,
                    "resume declares `source query.lookup` at most once",
                ));
            }
            let query = rest.trim();
            if !is_lzx_resume_ref(query) {
                return Err(line_error(
                    line,
                    "`source query.lookup` requires `<query>` or `<feature>.<query>`",
                ));
            }
            source_query = Some(query.to_owned());
        } else {
            arms.push(parse_lzx_resume_arm(line, trimmed)?);
        }

        last_end = line.end;
        index += 1;
    }

    let Some(source_query) = source_query else {
        return Err(line_error(
            header,
            "resume blocks require `source query.lookup <query>`",
        ));
    };
    if arms.is_empty() {
        return Err(line_error(header, "resume blocks require at least one arm"));
    }

    Ok((
        LzxResumeRouter {
            name: parts[1].to_owned(),
            source_query,
            arms,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

fn parse_lzx_resume_arm(line: &SourceLine<'_>, trimmed: &str) -> Result<LzxResumeArm, ParseError> {
    let Some((left, right)) = split_lzx_arrow(trimmed) else {
        return Err(line_error(
            line,
            "resume arms use `<state> -> view <name>` or `<state> → view <name>`",
        ));
    };
    let arm = left.trim();
    let (arm, substep) = parse_lzx_optional_substep_tail(line, arm, "resume arm")?;
    let kind = match arm {
        "none" => LzxResumeArmKind::None,
        "*" => LzxResumeArmKind::Wildcard,
        state if is_lzx_bare_ident(state) => LzxResumeArmKind::State(state.to_owned()),
        _ => {
            return Err(line_error(
                line,
                "resume arm state must be a lifecycle state, `none`, or `*`",
            ));
        }
    };
    if substep.is_some() && matches!(kind, LzxResumeArmKind::None | LzxResumeArmKind::Wildcard) {
        return Err(line_error(
            line,
            "resume arm `substep` is only valid on lifecycle state arms",
        ));
    }

    let Some(target_view) = right.trim().strip_prefix("view ") else {
        return Err(line_error(
            line,
            "resume arms target views with `view <name>`",
        ));
    };
    let target_view = target_view.trim();
    if !is_lzx_bare_ident(target_view) {
        return Err(line_error(
            line,
            "resume arm target view must be a bare identifier",
        ));
    }

    Ok(LzxResumeArm {
        kind,
        substep,
        target_view: target_view.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

fn parse_lzx_experience_view(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExperienceView, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 && !(parts.len() == 4 && parts[2] == "id") {
        return Err(line_error(
            header,
            "experience views use `view <name>` or `view <name> id @anchor.<name>`",
        ));
    }

    let mut anchor = (parts.len() == 4).then(|| parts[3].to_owned());
    let mut source = None;
    let mut submit = None;
    let mut routes = Vec::new();
    let mut extensible_by = Vec::new();
    let mut blocks = Vec::new();
    let mut actions = Vec::new();
    let mut opens = Vec::new();
    let mut tests = Vec::new();
    let mut guard = None;
    let mut pending_requires_lifecycle: Option<(usize, LzxRequiresLifecycle)> = None;
    let mut pending_on_lifecycle_pending: Option<(usize, String)> = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(line, "view children use four-space indentation"));
        }

        if let Some(rest) = trimmed.strip_prefix("route ") {
            routes.push(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("anchor ") {
            anchor = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("source ") {
            source = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            submit = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("extensible_by ") {
            extensible_by = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("action ") {
            let Some((name, target)) = rest.split_once(" -> ") else {
                return Err(line_error(line, "actions use `action <name> -> <target>`"));
            };
            actions.push(LzxAction {
                name: name.trim().to_owned(),
                target: target.trim().to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("opens ") {
            opens.push(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "view declares `policy` at most once"));
            }
            let (mut parsed, next) = parse_lzx_view_guard(lines, index, 4)?;
            if let Some((pending_index, pending)) = pending_requires_lifecycle.take() {
                attach_lzx_requires_lifecycle(&lines[pending_index], &mut parsed, pending)?;
            }
            if let Some((pending_index, pending)) = pending_on_lifecycle_pending.take() {
                let pending_line = &lines[pending_index];
                attach_lzx_on_lifecycle_pending(
                    pending_line,
                    &mut parsed,
                    pending,
                    pending_line.end,
                )?;
            }
            guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("requires_lifecycle ") {
            let parsed = parse_lzx_requires_lifecycle(line, rest.trim())?;
            if let Some(guard) = guard.as_mut() {
                attach_lzx_requires_lifecycle(line, guard, parsed)?;
            } else {
                if pending_requires_lifecycle.is_some() {
                    return Err(line_error(
                        line,
                        "view declares `requires_lifecycle` at most once",
                    ));
                }
                pending_requires_lifecycle = Some((index, parsed));
            }
        } else if let Some(rest) = trimmed.strip_prefix("on_lifecycle_pending ") {
            let parsed = parse_lzx_on_lifecycle_pending(line, rest.trim())?;
            if let Some(guard) = guard.as_mut() {
                attach_lzx_on_lifecycle_pending(line, guard, parsed, line.end)?;
            } else {
                if pending_on_lifecycle_pending.is_some() {
                    return Err(line_error(
                        line,
                        "view declares `on_lifecycle_pending` at most once",
                    ));
                }
                pending_on_lifecycle_pending = Some((index, parsed));
            }
        } else if trimmed == "tests" {
            index += 1;
            while index < lines.len() {
                let test_line = &lines[index];
                let test_trimmed = test_line.text.trim_start();
                if is_trivia(test_trimmed) {
                    index += 1;
                    continue;
                }
                if test_line.indent <= 4 {
                    break;
                }
                if test_line.indent != 6 {
                    return Err(line_error(
                        test_line,
                        "test assertions inside experience views use six-space indentation",
                    ));
                }
                // Wave 4 — view tests are an extensibility vocabulary,
                // not policy/predicate. Closed catalog: `accepted by
                // <feature>` / `rejected by <feature>`. Anything else is
                // a hard parse error (no silent acceptance).
                let assertion = if let Some(rest) = test_trimmed.strip_prefix("accepted by ") {
                    let feature = rest.trim().to_owned();
                    if feature.is_empty() {
                        return Err(line_error(
                            test_line,
                            "view test `accepted by` requires a feature name",
                        ));
                    }
                    LzxViewTestAssertion::AcceptedBy {
                        feature,
                        span: Span::new(test_line.start, test_line.end),
                    }
                } else if let Some(rest) = test_trimmed.strip_prefix("rejected by ") {
                    let feature = rest.trim().to_owned();
                    if feature.is_empty() {
                        return Err(line_error(
                            test_line,
                            "view test `rejected by` requires a feature name",
                        ));
                    }
                    LzxViewTestAssertion::RejectedBy {
                        feature,
                        span: Span::new(test_line.start, test_line.end),
                    }
                } else {
                    return Err(line_error(
                        test_line,
                        "view test assertion must start with `accepted by` or `rejected by` \
                             (extensibility vocabulary only — policy / predicate testing lives \
                             on commands, rules, and transitions)",
                    ));
                };
                tests.push(assertion);
                index += 1;
            }
            continue;
        } else {
            return Err(line_error(
                line,
                "view children are `route`, `anchor`, `source`, `submit`, `extensible_by`, `block`, `action`, `opens`, `policy`, `requires_lifecycle`, `on_lifecycle_pending`, or `tests`",
            ));
        }
        index += 1;
    }

    if let Some((pending_index, _)) = pending_requires_lifecycle.as_ref() {
        return Err(line_error(
            &lines[*pending_index],
            "`requires_lifecycle` currently requires a sibling `policy` guard",
        ));
    }
    if let Some((pending_index, _)) = pending_on_lifecycle_pending.as_ref() {
        return Err(line_error(
            &lines[*pending_index],
            "`on_lifecycle_pending` currently requires a sibling `policy` guard",
        ));
    }

    Ok((
        LzxExperienceView {
            name: parts[1].to_owned(),
            anchor,
            routes,
            extensible_by,
            source,
            submit,
            blocks,
            actions,
            opens,
            tests,
            guard,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_view_extension(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxViewExtension, usize), ParseError> {
    let header = &lines[start];
    let anchor = header
        .text
        .trim_start()
        .strip_prefix("extends ")
        .ok_or_else(|| line_error(header, "view extensions use `extends @anchor.<name>`"))?
        .trim()
        .to_owned();
    let mut blocks = Vec::new();
    let mut slots = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(
                line,
                "view extension children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if trimmed.starts_with("slot ") {
            let (slot, next) = parse_lzx_extension_slot(lines, index)?;
            slots.push(slot);
            index = next;
            continue;
        } else {
            return Err(line_error(
                line,
                "view extension children are `slot` declarations or legacy `block` declarations",
            ));
        }

        index += 1;
    }

    Ok((
        LzxViewExtension {
            anchor,
            blocks,
            slots,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_extension_slot(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExtensionSlot, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();

    if parts.len() != 2 && parts.len() != 4 {
        return Err(line_error(
            header,
            "extension slots use `slot <name>` or `slot <name> before|after <block>`",
        ));
    }

    let order = if parts.len() == 4 {
        if !matches!(parts[2], "before" | "after") {
            return Err(line_error(
                header,
                "extension slot ordering uses `before` or `after`",
            ));
        }
        Some(LzxExtensionOrder {
            relation: parts[2].to_owned(),
            target: parts[3].to_owned(),
        })
    } else {
        None
    };

    let mut blocks = Vec::new();
    let mut platforms = Vec::new();
    let mut audiences = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let child = line.text.trim_start();

        if is_trivia(child) {
            index += 1;
            continue;
        }

        if line.indent <= 4 {
            break;
        }

        if line.indent != 6 {
            return Err(line_error(
                line,
                "extension slot children use six-space indentation",
            ));
        }

        if let Some(rest) = child.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if let Some(rest) = child.strip_prefix("platforms ") {
            platforms = split_lzx_list(rest);
        } else if let Some(rest) = child.strip_prefix("audience ") {
            audiences = split_lzx_list(rest);
        } else {
            return Err(line_error(
                line,
                "extension slot children are `block`, `platforms`, or `audience` declarations",
            ));
        }

        index += 1;
    }

    if blocks.is_empty() {
        return Err(line_error(
            header,
            "extension slots must declare at least one `block`",
        ));
    }

    Ok((
        LzxExtensionSlot {
            name: parts[1].to_owned(),
            order,
            blocks,
            platforms,
            audiences,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_surface(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxSurface, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 3 {
        return Err(line_error(
            header,
            "surfaces use `surface <experience> web|mobile`",
        ));
    }

    let platform = match parts[2] {
        "web" => LzxPlatform::Web,
        "mobile" => LzxPlatform::Mobile,
        _ => {
            return Err(line_error(
                header,
                "surface platform must be `web` or `mobile`",
            ));
        }
    };

    let mut uses_experience = None;
    let mut audiences = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "surface children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("uses experience ") {
            uses_experience = Some(rest.trim().to_owned());
            index += 1;
        } else if trimmed.starts_with("audience ") {
            let (audience, next) = parse_lzx_audience(lines, index)?;
            audiences.push(audience);
            index = next;
        } else if trimmed.starts_with("view ") {
            return Err(line_error(
                line,
                "concrete platform views live under `audience ...` blocks",
            ));
        } else {
            return Err(line_error(
                line,
                "surface children are `uses experience` or `audience` declarations",
            ));
        }
    }

    Ok((
        LzxSurface {
            experience: parts[1].to_owned(),
            platform,
            uses_experience,
            audiences,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_audience(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxAudience, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() < 2 {
        return Err(line_error(header, "audience blocks use `audience <name>`"));
    }

    let mut views = Vec::new();
    let mut guard = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(
                line,
                "audience children are complete `view <name> <type>` declarations or `policy <policy>`",
            ));
        }

        if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "audience declares `policy` at most once"));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 4)?;
            guard = Some(parsed);
            index = next;
        } else if trimmed.starts_with("view ") {
            let (view, next) = parse_lzx_platform_view(lines, index)?;
            views.push(view);
            index = next;
        } else {
            return Err(line_error(
                line,
                "audience children are complete `view <name> <type>` declarations or `policy <policy>`",
            ));
        }
    }

    Ok((
        LzxAudience {
            name: parts[1].to_owned(),
            qualifiers: parts[2..].iter().map(|part| (*part).to_owned()).collect(),
            views,
            guard,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_platform_view(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxPlatformView, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 3 {
        return Err(line_error(
            header,
            "platform views use `view <name> <type>`",
        ));
    }

    let mut columns = Vec::new();
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut search = Vec::new();
    let mut filter = Vec::new();
    let mut cells = Vec::new();
    let mut actions = Vec::new();
    let mut submit = None;
    let mut blocks = Vec::new();
    let mut guard = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 4 {
            break;
        }

        if line.indent != 6 {
            return Err(line_error(
                line,
                "platform view children use six-space indentation",
            ));
        }

        if trimmed.contains("+=") || trimmed.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole view",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("columns ") {
            columns = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("fields ") {
            fields = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("sections ") {
            sections = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("search ") {
            search = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("filter ") {
            filter = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("cells ") {
            cells = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("actions ") {
            actions = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            submit = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(
                    line,
                    "platform view declares `policy` at most once",
                ));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 6)?;
            guard = Some(parsed);
            index = next;
            continue;
        } else {
            return Err(line_error(
                line,
                "platform view children are `columns`, `fields`, `sections`, `search`, `filter`, `cells`, `actions`, `submit`, `block`, or `policy`",
            ));
        }

        index += 1;
    }

    Ok((
        LzxPlatformView {
            name: parts[1].to_owned(),
            view_type: parts[2].to_owned(),
            columns,
            fields,
            sections,
            search,
            filter,
            cells,
            actions,
            submit,
            blocks,
            guard,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}
// =============================================================================
// L0 #3 — lzx ViewModel surface parser.
// -----------------------------------------------------------------------------
// Hand-written line-walker for `features/<feat>/<feat>.{web,mobile}.lzx`
// per `docs/proposals/lzx-integration-codegen.md` §5. Mirrors the
// `parse_design_decl` pattern (L0 #2 Cell A) and the legacy
// `parse_lzx_*` helpers. Indentation is two spaces per level.
//
// Top-level entry point is `parse_surface_document` (source text) which
// dispatches to `parse_surface_decl` (line slice). The helper is `pub`
// so the analyzer and CLI can drive it from already-loaded
// `SourceLine` slices when needed.
// =============================================================================

/// Parse a full `.lzx` ViewModel file. Expects exactly one
/// `surface <feature> web|mobile` declaration at indent 0.
pub fn parse_surface_document(source: &str) -> Result<SurfaceAst, ParseError> {
    let lines = source_lines(source);
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent != 0 {
            return Err(line_error(
                line,
                "top-level `surface` declaration must start at indent 0",
            ));
        }
        if trimmed.starts_with("surface ") {
            let (parsed, _next) = parse_surface_decl(&lines, i)?;
            return Ok(parsed);
        }
        return Err(line_error(
            line,
            "`.lzx` ViewModel files must begin with `surface <feature> web|mobile`",
        ));
    }
    Err(ParseError::Expected {
        expected: "surface <feature> web|mobile declaration",
    })
}

/// Parse a `surface <feature> web|mobile` block starting at `lines[start]`.
/// Returns the AST + the index of the first line not consumed. Module-private
/// to match `SourceLine`'s scope; callers use the `parse_surface_document`
/// source-text entry point.
fn parse_surface_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(SurfaceAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let parts: Vec<_> = header_text.split_whitespace().collect();
    if parts.len() != 3 || parts[0] != "surface" {
        return Err(line_error(
            header,
            "surface header is `surface <feature> web|mobile`",
        ));
    }
    let feature = parts[1].to_owned();
    let target = match parts[2] {
        "web" => SurfaceTargetAst::Web,
        "mobile" => SurfaceTargetAst::Mobile,
        _ => {
            return Err(line_error(
                header,
                "surface target must be `web` or `mobile`",
            ));
        }
    };
    let header_indent = header.indent;
    let body_indent = header_indent + 2;

    let mut uses_feature: Option<String> = None;
    let mut audiences: Vec<AudienceAst> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != body_indent {
            return Err(line_error(
                line,
                "surface body lines use one indentation level deeper than the `surface` header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("uses feature ") {
            let value = rest.trim();
            if value.is_empty() {
                return Err(line_error(line, "`uses feature` requires a feature name"));
            }
            uses_feature = Some(value.to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("audience ") || trimmed == "audience" {
            let (audience, next) = parse_lzx_audience_block(lines, i, body_indent)?;
            audiences.push(audience);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "surface body lines are `uses feature <feature>` or `audience <name>` declarations",
            ));
        }
    }

    Ok((
        SurfaceAst {
            feature,
            target,
            uses_feature,
            audiences,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse an `audience <name>` block. `requires @scope.<name>` lines may
/// appear at the same indentation as `view` children; both are captured.
fn parse_lzx_audience_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(AudienceAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let parts: Vec<_> = header_text.split_whitespace().collect();
    if parts.len() != 2 || parts[0] != "audience" {
        return Err(line_error(header, "audience header is `audience <name>`"));
    }
    let name = parts[1].to_owned();
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error(
            header,
            "audience names use kebab-case or snake_case identifiers",
        ));
    }
    let body_indent = parent_indent + 2;
    let view_indent = body_indent;

    let mut requires: Vec<PolicyAtomAst> = Vec::new();
    let mut views: Vec<ViewAst> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != view_indent {
            return Err(line_error(
                line,
                "audience body lines use one indentation level deeper than the `audience` header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("requires ") {
            let atom = parse_policy_atom(line, rest.trim())?;
            requires.push(atom);
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("view list ")
            || trimmed.starts_with("view detail ")
            || trimmed.starts_with("view create ")
        {
            let (view, next) = parse_view_block(lines, i, view_indent)?;
            views.push(view);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "audience body lines are `requires @scope.<name>` or `view list|detail|create <name>` declarations",
            ));
        }
    }

    Ok((
        AudienceAst {
            name,
            requires,
            views,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse one of `view list`, `view detail`, `view create` blocks.
fn parse_view_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(ViewAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let (kind, after_kind) = if let Some(rest) = header_text.strip_prefix("view list ") {
        ("list", rest)
    } else if let Some(rest) = header_text.strip_prefix("view detail ") {
        ("detail", rest)
    } else if let Some(rest) = header_text.strip_prefix("view create ") {
        ("create", rest)
    } else {
        return Err(line_error(
            header,
            "view header is `view list|detail|create <name> [at \"<path>\"]`",
        ));
    };

    let (name, route) = parse_view_header_tail(header, after_kind)?;
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            header,
            format!("view name `{}` must be kebab-case or snake_case", name),
        ));
    }
    let body_indent = parent_indent + 2;

    // Collect raw children; dispatch into the kind-specific builder.
    let mut state = ViewBodyState::default();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != body_indent {
            return Err(line_error(
                line,
                "view body lines use one indentation level deeper than the `view` header",
            ));
        }
        if raw.contains("+=") || raw.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole view",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();

        if let Some(rest) = trimmed.strip_prefix("drawer ") {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`drawer` is only valid in `view list` bodies",
                ));
            }
            if state.drawer.is_some() {
                return Err(line_error(
                    line,
                    "view list declares at most one `drawer` block",
                ));
            }
            let (drawer, next) = parse_drawer_block(lines, i, body_indent, rest.trim())?;
            last_end = drawer.span.end;
            state.drawer = Some(drawer);
            i = next;
            continue;
        }

        if trimmed == "filters" {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`filters` block is only valid in `view list`",
                ));
            }
            let (next, block_end) = parse_filters_block(lines, i, body_indent, &mut state)?;
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed.starts_with("filters ") {
            return Err(line_error(
                line,
                "`filters` is a block keyword and does not accept inline content",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("search ") {
            if state.search.is_some() {
                return Err(line_error(line, "view declares `search` at most once"));
            }
            let (search, next) = parse_view_search_decl(lines, i, rest.trim(), body_indent)?;
            state.search = Some(search);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "sort" {
            if state.sort.is_some() {
                return Err(line_error(line, "view declares `sort` at most once"));
            }
            let (sort, next, block_end) = parse_view_sort_block(lines, i, body_indent)?;
            state.sort = Some(sort);
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed == "settings" {
            if !state.settings.is_empty() {
                return Err(line_error(line, "view declares `settings` at most once"));
            }
            let (settings, next, block_end) = parse_view_settings_block(lines, i, body_indent)?;
            state.settings = settings;
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed.starts_with("persist ") {
            return Err(line_error(
                line,
                "`persist` is valid only as a child of a `settings` declaration",
            ));
        }

        if trimmed == "on_success" {
            if state.on_success.is_some() {
                return Err(line_error(line, "view declares `on_success` at most once"));
            }
            let (on_success, next) = parse_on_success_block(lines, i, body_indent)?;
            last_end = on_success.span.end;
            state.on_success = Some(on_success);
            i = next;
            continue;
        }
        if trimmed.starts_with("on_success ") {
            return Err(line_error(
                line,
                "`on_success` is a block keyword and does not accept inline content",
            ));
        }

        let mut matched = false;
        for (prefix, handler) in view_body_handlers() {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                handler(line, rest.trim(), &mut state)?;
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(line_error_owned(
                line,
                format!(
                    "view body lines are `source`, `submit`, `on_success`, `columns`, `fields`, `search`, `filter`, `sections`, `cells`, `route`, or `actions` declarations (got `{}`)",
                    trimmed
                ),
            ));
        }
        last_end = line.end;
        i += 1;
    }

    let span = Span::new(header.start, last_end);
    let view = match kind {
        "list" => {
            if state.on_success.is_some() {
                return Err(line_error(
                    header,
                    "`on_success` is valid only in submit-backed `view create` bodies",
                ));
            }
            let selection = assemble_selection_decl(&state, span);
            ViewAst::List(ViewListAst {
                name,
                route,
                source: state.source.ok_or_else(|| {
                    line_error(
                        header,
                        "view list requires a `source <feature>.query.<name>` line",
                    )
                })?,
                columns: state.columns,
                search: state.search,
                filter: state.filter,
                filters: state.filters,
                cells_slot: state.cells_slot,
                cells: state.cells,
                drawer: state.drawer,
                sort: state.sort,
                selection,
                settings: state.settings,
                actions: state.actions,
                redacted_fields: state.redacted_fields,
                span,
            })
        }
        "detail" => {
            reject_list_only_view_body(header, &state, "view detail")?;
            if state.on_success.is_some() {
                return Err(line_error(
                    header,
                    "`on_success` is valid only in submit-backed `view create` bodies",
                ));
            }
            ViewAst::Detail(ViewDetailAst {
                name,
                route,
                source: state.source.ok_or_else(|| {
                    line_error(
                        header,
                        "view detail requires a `source <feature>.query.<name>` line",
                    )
                })?,
                route_params: state.route_params,
                sections: state.sections,
                cells: state.cells,
                actions: state.actions,
                redacted_fields: state.redacted_fields,
                span,
            })
        }
        "create" => {
            reject_list_only_view_body(header, &state, "view create")?;
            ViewAst::Create(ViewCreateAst {
                name,
                route,
                submit: state.submit.ok_or_else(|| {
                    line_error(
                        header,
                        "view create requires a `submit <feature>.command.<name>` line",
                    )
                })?,
                on_success: state.on_success,
                fields: state.fields,
                cells: state.cells,
                redacted_fields: state.redacted_fields,
                span,
            })
        }
        _ => unreachable!(),
    };
    Ok((view, i))
}

#[derive(Default)]
struct ViewBodyState {
    source: Option<String>,
    submit: Option<String>,
    columns: Vec<String>,
    search: Option<SearchDeclAst>,
    filter: Vec<String>,
    filters: Vec<FilterDeclAst>,
    has_filters_block: bool,
    fields: Vec<String>,
    sections: Vec<String>,
    cells_slot: Option<String>,
    cells: Vec<CellBindingAst>,
    actions: Vec<String>,
    route_params: Vec<RouteParamAst>,
    drawer: Option<DrawerSubViewAst>,
    on_success: Option<OnSuccessSpecAst>,
    sort: Option<SortDeclAst>,
    selection: Option<SelectionDeclAst>,
    bulk_actions: Vec<String>,
    bulk_actions_seen: bool,
    settings: Vec<SettingDeclAst>,
    redacted_fields: Vec<String>,
}

type ViewBodyLineHandler =
    for<'a> fn(&SourceLine<'a>, &str, &mut ViewBodyState) -> Result<(), ParseError>;

fn view_body_handlers() -> &'static [(&'static str, ViewBodyLineHandler)] {
    &[
        ("source ", parse_view_source_line),
        ("submit ", parse_view_submit_line),
        ("columns ", parse_view_columns_line),
        ("fields ", parse_view_fields_line),
        ("filter ", parse_view_filter_line),
        ("sections ", parse_view_sections_line),
        ("selection ", parse_view_selection_line),
        ("bulk_actions ", parse_view_bulk_actions_line),
        ("actions ", parse_view_actions_line),
        ("cells ", parse_view_cells_line),
        ("route ", parse_view_route_line),
    ]
}

fn parse_view_source_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.source.is_some() {
        return Err(line_error(line, "view declares `source` at most once"));
    }
    state.source = Some(rest.to_owned());
    Ok(())
}

fn parse_view_submit_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.submit.is_some() {
        return Err(line_error(line, "view declares `submit` at most once"));
    }
    state.submit = Some(rest.to_owned());
    Ok(())
}

fn parse_on_success_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(OnSuccessSpecAst, usize), ParseError> {
    let header = &lines[start];
    let child_indent = parent_indent + 2;
    let mut back = false;
    let mut redirect: Option<String> = None;
    let mut flash: Option<FlashSpecAst> = None;
    let mut invalidates: Vec<InvalidatesDecl> = Vec::new();
    let mut replace = false;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`on_success` children use one indentation level deeper than the block header",
            ));
        }

        let trimmed = strip_inline_comment(raw).trim_end();
        if trimmed == "back" {
            if back {
                return Err(line_error(
                    line,
                    "`on_success.back` is declared at most once",
                ));
            }
            back = true;
        } else if let Some(rest) = trimmed.strip_prefix("redirect ") {
            if redirect.is_some() {
                return Err(line_error(
                    line,
                    "`on_success.redirect` is declared at most once",
                ));
            }
            redirect = Some(parse_on_success_redirect(line, rest)?);
        } else if let Some(rest) = trimmed.strip_prefix("flash ") {
            if flash.is_some() {
                return Err(line_error(
                    line,
                    "`on_success.flash` is declared at most once",
                ));
            }
            flash = Some(parse_on_success_flash(line, rest)?);
        } else if let Some(rest) = trimmed.strip_prefix("invalidates ") {
            invalidates.push(parse_invalidates_entry(line, rest)?);
        } else if trimmed == "replace" {
            if replace {
                return Err(line_error(
                    line,
                    "`on_success.replace` is declared at most once",
                ));
            }
            replace = true;
        } else {
            return Err(line_error(
                line,
                "`on_success` children are `back`, `redirect \"<path>\"`, `flash <success|error|info> @translation.<key>`, `invalidates query.<name>`, or `replace`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    Ok((
        OnSuccessSpecAst {
            back,
            redirect,
            flash,
            invalidates,
            replace,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_on_success_redirect(line: &SourceLine<'_>, rest: &str) -> Result<String, ParseError> {
    let trimmed = rest.trim();
    let Some(after_open) = trimmed.strip_prefix('"') else {
        return Err(line_error(
            line,
            "`on_success.redirect` target must be a quoted string",
        ));
    };
    let Some(close_idx) = after_open.find('"') else {
        return Err(line_error(
            line,
            "`on_success.redirect` target is missing the closing quote",
        ));
    };
    let value = after_open[..close_idx].to_owned();
    if !after_open[close_idx + 1..].trim().is_empty() {
        return Err(line_error(
            line,
            "`on_success.redirect` accepts exactly one quoted string",
        ));
    }
    Ok(value)
}

fn parse_on_success_flash(line: &SourceLine<'_>, rest: &str) -> Result<FlashSpecAst, ParseError> {
    let mut parts = rest.trim().splitn(2, char::is_whitespace);
    let kind = parts.next().unwrap_or("");
    if !matches!(kind, "success" | "error" | "info") {
        return Err(line_error(
            line,
            "`on_success.flash` kind must be `success`, `error`, or `info`",
        ));
    }
    let message_key = parse_translation_key_token(line, parts.next().unwrap_or(""))?;
    Ok(FlashSpecAst {
        kind: kind.to_owned(),
        message_key,
        span: Span::new(line.start, line.end),
    })
}

fn parse_view_columns_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.columns.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_fields_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if let Some(fields) = rest.strip_suffix(" redacted") {
        let fields = split_lzx_list(fields);
        state.redacted_fields.extend(fields.iter().cloned());
        state.fields.extend(fields);
    } else {
        state.fields.extend(split_lzx_list(rest));
    }
    Ok(())
}

fn parse_view_filter_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.filter.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_sections_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.sections.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_actions_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.actions.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_cells_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    if let Some(slot_rest) = rest.strip_prefix("@client.") {
        let slot = slot_rest.trim();
        if slot.is_empty() {
            return Err(line_error(
                line,
                "`cells @client.<slot>` requires a slot identifier after `@client.`",
            ));
        }
        if slot.split_whitespace().count() > 1 {
            return Err(line_error_owned(
                line,
                format!(
                    "`cells @client.<slot>` accepts only one slot identifier (got `{}`); per-column form is `cells <field> @client.<slot>` and binds a single field",
                    slot
                ),
            ));
        }
        if state.cells_slot.is_some() {
            return Err(line_error(
                line,
                "view declares `cells @client.<slot>` (grid form) at most once",
            ));
        }
        if !is_kebab_or_snake_ident(slot) {
            return Err(line_error_owned(
                line,
                format!("cell slot `{}` must be a kebab/snake identifier", slot),
            ));
        }
        state.cells_slot = Some(slot.to_owned());
        Ok(())
    } else {
        let binding = parse_cell_binding(line, rest)?;
        state.cells.push(binding);
        Ok(())
    }
}

fn parse_view_route_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    let param = parse_route_param(line, rest)?;
    state.route_params.push(param);
    Ok(())
}

fn parse_drawer_block(
    lines: &[SourceLine<'_>],
    start: usize,
    drawer_indent: usize,
    header_rest: &str,
) -> Result<(DrawerSubViewAst, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header_rest.split_whitespace().collect();
    if parts.len() != 3 || parts[1] != "on" {
        return Err(line_error(
            header,
            "drawer blocks use `drawer <name> on select|open`",
        ));
    }
    let name = parts[0].to_owned();
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            header,
            format!("drawer name `{}` must be kebab/snake identifier", name),
        ));
    }
    let trigger = match parts[2] {
        "select" => DrawerTriggerAst::Select,
        "open" => DrawerTriggerAst::ManualOpen,
        _ => {
            return Err(line_error(
                header,
                "drawer trigger must be `select` or `open`",
            ));
        }
    };

    let child_indent = drawer_indent + 2;
    let mut state = ViewBodyState::default();
    let mut route_binding = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= drawer_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "drawer body lines use one indentation level deeper than the `drawer` header",
            ));
        }
        if raw.contains("+=") || raw.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole drawer",
            ));
        }

        let trimmed = strip_inline_comment(raw).trim_end();
        if trimmed.starts_with("drawer ") {
            return Err(line_error(line, "drawer cannot be nested"));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            parse_view_source_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            if route_binding.is_some() {
                return Err(line_error(line, "drawer declares `route` at most once"));
            }
            route_binding = Some(parse_drawer_route_binding(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("sections ") {
            parse_view_sections_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("cells ") {
            parse_drawer_cells_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("actions ") {
            parse_view_actions_line(line, rest.trim(), &mut state)?;
        } else {
            return Err(line_error_owned(
                line,
                format!(
                    "drawer body lines are `source`, `route <key> from selection`, `sections`, `cells <field> @client.<slot>`, or `actions` declarations (got `{}`)",
                    trimmed
                ),
            ));
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        DrawerSubViewAst {
            name,
            trigger,
            source: state.source.ok_or_else(|| {
                line_error(
                    header,
                    "drawer requires a `source <feature>.query.<name>` line",
                )
            })?,
            route_binding,
            sections: state.sections,
            cells: state.cells,
            actions: state.actions,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_drawer_cells_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if rest.split_whitespace().count() != 2 {
        return Err(line_error(
            line,
            "drawer cells use `cells <field> @client.<slot>`",
        ));
    }
    parse_view_cells_line(line, rest, state)
}

fn parse_drawer_route_binding(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<DrawerRouteBindingAst, ParseError> {
    let (target, source) = value.rsplit_once(" from ").ok_or_else(|| {
        line_error(
            line,
            "drawer route binding must be `route <key> from selection`",
        )
    })?;
    let target = target.trim();
    if target.is_empty() {
        return Err(line_error(
            line,
            "drawer route binding requires a target key",
        ));
    }
    if !is_kebab_or_snake_ident(target) {
        return Err(line_error_owned(
            line,
            format!(
                "drawer route target `{}` must be kebab/snake identifier",
                target
            ),
        ));
    }
    if source.trim() != "selection" {
        return Err(line_error(
            line,
            "drawer route binding source must be `from selection`",
        ));
    }
    Ok(DrawerRouteBindingAst {
        target: target.to_owned(),
        source: DrawerBindingSourceAst::Selection,
    })
}

fn parse_filters_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
    state: &mut ViewBodyState,
) -> Result<(usize, usize), ParseError> {
    let header = &lines[start];
    if state.has_filters_block {
        return Err(line_error(
            header,
            "view list declares `filters` at most once",
        ));
    }
    state.has_filters_block = true;

    let child_indent = body_indent + 2;
    let mut block_filters = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "filters declarations use one indentation level deeper than the `filters` header",
            ));
        }

        let trimmed = strip_inline_comment(raw).trim_end();
        let filter = parse_filter_decl(line, trimmed)?;
        if block_filters
            .iter()
            .any(|existing: &FilterDeclAst| existing.name == filter.name)
        {
            return Err(line_error_owned(
                line,
                format!("duplicate filter `{}` in `filters` block", filter.name),
            ));
        }
        last_end = line.end;
        block_filters.push(filter);
        i += 1;
    }

    if block_filters.is_empty() {
        return Err(line_error(
            header,
            "filters block requires at least one filter declaration",
        ));
    }

    state.filters.extend(block_filters);
    Ok((i, last_end))
}

fn parse_filter_decl(line: &SourceLine<'_>, value: &str) -> Result<FilterDeclAst, ParseError> {
    let (name_raw, type_raw) = value.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "filter declaration must be `<name>: [list of] <Type> [from query]`",
        )
    })?;
    let name = name_raw.trim().to_owned();
    if !is_lzx_bare_ident(&name) {
        return Err(line_error_owned(
            line,
            format!(
                "filter name `{}` must start with a letter and contain only letters, digits, or `_`",
                name
            ),
        ));
    }

    let mut rest = type_raw.trim();
    let mut url_sync = false;
    if let Some((head, source)) = rest.rsplit_once(" from ") {
        if source.trim() != "query" {
            return Err(line_error(line, "filter URL source must be `from query`"));
        }
        rest = head.trim();
        url_sync = true;
    }

    let (cardinality, type_ref) = if let Some(type_ref) = rest.strip_prefix("list of ") {
        (FilterCardinalityAst::Multi, type_ref.trim())
    } else {
        (FilterCardinalityAst::Single, rest)
    };
    if type_ref.is_empty() {
        return Err(line_error(line, "filter declaration requires a type"));
    }
    if !is_lzx_bare_ident(type_ref) {
        return Err(line_error_owned(
            line,
            format!("filter type `{}` must be a bare identifier", type_ref),
        ));
    }

    Ok(FilterDeclAst {
        name,
        type_ref: type_ref.to_owned(),
        cardinality,
        url_sync,
        span: Span::new(line.start, line.end),
    })
}

fn parse_view_search_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    body_indent: usize,
) -> Result<(SearchDeclAst, usize), ParseError> {
    let header = &lines[start];
    if rest == "segmented" {
        parse_view_segmented_search(lines, start, body_indent)
    } else if rest.starts_with("segmented ") {
        Err(line_error(
            header,
            "the `segmented` form takes no inline list — use child `field` declarations",
        ))
    } else {
        Ok((
            SearchDeclAst {
                mode: SearchModeAst::Columns(split_lzx_list(rest)),
                fields: Vec::new(),
                free_text_target: None,
                span: Span::new(header.start, header.end),
            },
            start + 1,
        ))
    }
}

fn parse_view_segmented_search(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
) -> Result<(SearchDeclAst, usize), ParseError> {
    let header = &lines[start];
    let child_indent = body_indent + 2;
    let mut fields: Vec<SearchFieldAst> = Vec::new();
    let mut free_text_target = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`search segmented` child lines use one indentation level deeper than `search segmented`",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("field ") {
            let field = parse_view_search_field(line, rest.trim())?;
            if fields.iter().any(|existing| existing.key == field.key) {
                return Err(line_error_owned(
                    line,
                    format!(
                        "`search segmented` declares field `{}` more than once",
                        field.key
                    ),
                ));
            }
            fields.push(field);
        } else if let Some(rest) = trimmed.strip_prefix("free text into ") {
            if free_text_target.is_some() {
                return Err(line_error(
                    line,
                    "`search segmented` declares `free text into` at most once",
                ));
            }
            free_text_target = Some(parse_binding_ref(line, rest.trim())?);
        } else {
            return Err(line_error(
                line,
                "`search segmented` children are `field <key> binds <BindingRef>` or `free text into <BindingRef>`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    Ok((
        SearchDeclAst {
            mode: SearchModeAst::Segmented,
            fields,
            free_text_target,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_view_search_field(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<SearchFieldAst, ParseError> {
    let Some((key, target)) = rest.split_once(" binds ") else {
        return Err(line_error(
            line,
            "`search segmented` fields use `field <key> binds <BindingRef>`",
        ));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(line_error(
            line,
            "`search segmented` field key cannot be empty",
        ));
    }
    Ok(SearchFieldAst {
        key: key.to_owned(),
        binds_to: parse_binding_ref(line, target.trim())?,
        span: Span::new(line.start, line.end),
    })
}

fn parse_binding_ref(line: &SourceLine<'_>, raw: &str) -> Result<BindingRefAst, ParseError> {
    if raw == "selection" {
        return Ok(BindingRefAst::SelectionScalar);
    }
    if let Some(name) = raw.strip_prefix("filters.") {
        if !name.is_empty() {
            return Ok(BindingRefAst::Filter {
                name: name.to_owned(),
            });
        }
    }
    if let Some(name) = raw.strip_prefix("source.") {
        if !name.is_empty() {
            return Ok(BindingRefAst::SourceInput {
                name: name.to_owned(),
            });
        }
    }
    Err(line_error(
        line,
        "binding references are `filters.<name>`, `source.<name>`, or `selection`",
    ))
}

fn parse_view_selection_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.selection.is_some() {
        return Err(line_error(line, "view declares `selection` at most once"));
    }
    let mode = match rest {
        "single" => SelectionModeAst::Single,
        "multi" => SelectionModeAst::Multi,
        "none" => {
            return Err(line_error(
                line,
                "`selection none` is not valid; omit the line for no selection",
            ));
        }
        _ => {
            return Err(line_error(
                line,
                "`selection` must be `selection single` or `selection multi`",
            ));
        }
    };
    state.selection = Some(SelectionDeclAst {
        mode,
        bulk_actions: Vec::new(),
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

fn parse_view_bulk_actions_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.bulk_actions_seen {
        return Err(line_error(
            line,
            "view declares `bulk_actions` at most once",
        ));
    }
    let actions = split_lzx_list(rest);
    if actions.is_empty() {
        return Err(line_error(
            line,
            "`bulk_actions` requires at least one command name",
        ));
    }
    state.bulk_actions = actions;
    state.bulk_actions_seen = true;
    Ok(())
}

fn assemble_selection_decl(state: &ViewBodyState, view_span: Span) -> Option<SelectionDeclAst> {
    if let Some(mut selection) = state.selection.clone() {
        selection.bulk_actions = state.bulk_actions.clone();
        Some(selection)
    } else if state.bulk_actions_seen {
        Some(SelectionDeclAst {
            mode: SelectionModeAst::None,
            bulk_actions: state.bulk_actions.clone(),
            span: view_span,
        })
    } else {
        None
    }
}

fn reject_list_only_view_body(
    header: &SourceLine<'_>,
    state: &ViewBodyState,
    kind: &str,
) -> Result<(), ParseError> {
    if state.sort.is_some()
        || state.selection.is_some()
        || state.bulk_actions_seen
        || !state.settings.is_empty()
    {
        return Err(line_error_owned(
            header,
            format!(
                "`sort`, `selection`, `bulk_actions`, and `settings` are valid only in `view list`, not `{}`",
                kind
            ),
        ));
    }
    Ok(())
}

/// Split the `<name> [at "<path>"]` tail of a view header. The optional
/// `at "<...>"` clause carries a quoted route path.
fn parse_view_header_tail(
    header: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<String>), ParseError> {
    let rest = rest.trim();
    if let Some(at_idx) = find_top_level_token(rest, " at ") {
        let name = rest[..at_idx].trim().to_owned();
        if name.is_empty() {
            return Err(line_error(header, "view header requires a name"));
        }
        let after = rest[at_idx + " at ".len()..].trim();
        if !after.starts_with('"') {
            return Err(line_error(
                header,
                "`at` route must be a quoted string (e.g. `at \"/slugs\"`)",
            ));
        }
        let route = unquote_lzx_value(after).to_owned();
        if !route.starts_with('/') {
            return Err(line_error(header, "`at` route path must begin with `/`"));
        }
        Ok((name, Some(route)))
    } else {
        let name = rest.trim().to_owned();
        if name.is_empty() {
            return Err(line_error(header, "view header requires a name"));
        }
        Ok((name, None))
    }
}

/// Parse `cells <field> @client.<slot>` — `value` is the text after the
/// `cells ` prefix.
fn parse_cell_binding(line: &SourceLine<'_>, value: &str) -> Result<CellBindingAst, ParseError> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(
            line,
            "cell bindings use `cells <field> @client.<slot>`",
        ));
    }
    let field = parts[0].to_owned();
    let slot = parts[1]
        .strip_prefix("@client.")
        .ok_or_else(|| line_error(line, "cell slot must be `@client.<slot>`"))?
        .to_owned();
    if !is_kebab_or_snake_ident(&field) {
        return Err(line_error_owned(
            line,
            format!("cell field `{}` must be a kebab/snake identifier", field),
        ));
    }
    if !is_kebab_or_snake_ident(&slot) {
        return Err(line_error_owned(
            line,
            format!("cell slot `{}` must be a kebab/snake identifier", slot),
        ));
    }
    Ok(CellBindingAst {
        field,
        slot,
        span: Span::new(line.start, line.end),
    })
}

/// Parse `route <name>: <Type> from path` — the path-source clause is
/// mandatory; the lzx grammar reserves `route ... from path` for typed
/// path parameters.
fn parse_route_param(line: &SourceLine<'_>, value: &str) -> Result<RouteParamAst, ParseError> {
    // Pattern: `<name>: <Type> from path`. Split on `from` first so
    // any `:` inside `<Type>` is preserved.
    let (head, source) = value
        .rsplit_once(" from ")
        .ok_or_else(|| line_error(line, "route param must be `route <name>: <Type> from path`"))?;
    if source.trim() != "path" {
        return Err(line_error(line, "route param source must be `from path`"));
    }
    let (name_raw, type_raw) = head
        .split_once(':')
        .ok_or_else(|| line_error(line, "route param must be `route <name>: <Type> from path`"))?;
    let name = name_raw.trim().to_owned();
    let type_ref = type_raw.trim().to_owned();
    if name.is_empty() || type_ref.is_empty() {
        return Err(line_error(
            line,
            "route param requires both a name and a type",
        ));
    }
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            line,
            format!("route param name `{}` must be kebab/snake case", name),
        ));
    }
    Ok(RouteParamAst {
        name,
        type_ref,
        span: Span::new(line.start, line.end),
    })
}

fn parse_view_sort_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
) -> Result<(SortDeclAst, usize, usize), ParseError> {
    let header = &lines[start];
    let child_indent = body_indent + 2;
    let mut index = start + 1;
    let mut allowed: Option<Vec<String>> = None;
    let mut default: Option<(String, SortDirAst)> = None;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            index += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`sort` children use one indentation level deeper than `sort`",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("by ") {
            if allowed.is_some() {
                return Err(line_error(line, "`sort` declares `by` at most once"));
            }
            let fields = split_lzx_list(rest);
            if fields.is_empty() {
                return Err(line_error(line, "`sort by` requires at least one field"));
            }
            allowed = Some(fields);
        } else if let Some(rest) = trimmed.strip_prefix("default ") {
            if default.is_some() {
                return Err(line_error(line, "`sort` declares `default` at most once"));
            }
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() != 2 {
                return Err(line_error(
                    line,
                    "`sort default` uses `default <field> <asc|desc>`",
                ));
            }
            default = Some((parts[0].to_owned(), parse_sort_dir(line, parts[1])?));
        } else {
            return Err(line_error(
                line,
                "`sort` children are `by <field>, ...` or `default <field> <asc|desc>`",
            ));
        }
        last_end = line.end;
        index += 1;
    }

    let allowed = allowed.ok_or_else(|| line_error(header, "`sort` requires a `by` line"))?;
    let (default_field, default_dir) =
        default.ok_or_else(|| line_error(header, "`sort` requires a `default` line"))?;
    if !allowed.iter().any(|field| field == &default_field) {
        return Err(line_error_owned(
            header,
            format!(
                "`sort default` field `{}` must be listed in `sort by`",
                default_field
            ),
        ));
    }

    Ok((
        SortDeclAst {
            allowed,
            default_field,
            default_dir,
            span: Span::new(header.start, last_end),
        },
        index,
        last_end,
    ))
}

fn parse_sort_dir(line: &SourceLine<'_>, value: &str) -> Result<SortDirAst, ParseError> {
    match value {
        "asc" => Ok(SortDirAst::Asc),
        "desc" => Ok(SortDirAst::Desc),
        _ => Err(line_error(
            line,
            "`sort default` dir must be `asc` or `desc`",
        )),
    }
}

fn parse_view_settings_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
) -> Result<(Vec<SettingDeclAst>, usize, usize), ParseError> {
    let header = &lines[start];
    let setting_indent = body_indent + 2;
    let persist_indent = body_indent + 4;
    let mut index = start + 1;
    let mut settings = Vec::new();
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            index += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != setting_indent {
            return Err(line_error(
                line,
                "`settings` children use one indentation level deeper than `settings`",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if trimmed.starts_with("persist ") {
            return Err(line_error(
                line,
                "`persist` is valid only as a child of a setting declaration",
            ));
        }
        let mut setting = parse_setting_decl_line(line, trimmed)?;
        if settings
            .iter()
            .any(|existing: &SettingDeclAst| existing.name == setting.name)
        {
            return Err(line_error_owned(
                line,
                format!("duplicate setting `{}`", setting.name),
            ));
        }
        last_end = line.end;
        index += 1;

        let mut persistence_seen = false;
        while index < lines.len() {
            let child = &lines[index];
            let child_raw = child.text.trim_start();
            if is_trivia(child_raw) {
                index += 1;
                continue;
            }
            if child.indent <= setting_indent {
                break;
            }
            if child.indent != persist_indent {
                return Err(line_error(
                    child,
                    "setting children use one indentation level deeper than the setting declaration",
                ));
            }
            let child_trimmed = strip_inline_comment(child_raw).trim_end();
            if let Some(rest) = child_trimmed.strip_prefix("persist ") {
                if persistence_seen {
                    return Err(line_error(child, "setting declares `persist` at most once"));
                }
                persistence_seen = true;
                setting.persistence = parse_setting_persistence(child, rest.trim())?;
            } else {
                return Err(line_error(
                    child,
                    "setting children are `persist local`, `persist workspace`, or `persist none`",
                ));
            }
            setting.span = Span::new(setting.span.start, child.end);
            last_end = child.end;
            index += 1;
        }

        settings.push(setting);
    }

    if settings.is_empty() {
        return Err(line_error(
            header,
            "`settings` requires at least one setting",
        ));
    }
    Ok((settings, index, last_end))
}

fn parse_setting_decl_line(
    line: &SourceLine<'_>,
    trimmed: &str,
) -> Result<SettingDeclAst, ParseError> {
    let (name_raw, rest_raw) = trimmed.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "setting declarations use `<name>: <Type> [constraints] default <value>`",
        )
    })?;
    let name = name_raw.trim().to_owned();
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            line,
            format!("setting name `{}` must be kebab/snake case", name),
        ));
    }
    let rest = rest_raw.trim();
    let (value_space, default) = if let Some(after_enum) = rest.strip_prefix("Enum ") {
        parse_enum_setting(line, after_enum.trim())?
    } else if let Some(after_bool) = rest.strip_prefix("Bool ") {
        parse_bool_setting(line, after_bool.trim())?
    } else if let Some(after_int) = rest.strip_prefix("Int ") {
        parse_int_setting(line, after_int.trim())?
    } else {
        return Err(line_error(
            line,
            "setting type must be `Enum [...]`, `Bool`, or `Int`",
        ));
    };

    Ok(SettingDeclAst {
        name,
        value_space,
        default,
        persistence: SettingPersistenceAst::None,
        span: Span::new(line.start, line.end),
    })
}

fn parse_enum_setting(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(SettingValueSpaceAst, String), ParseError> {
    if !rest.starts_with('[') {
        return Err(line_error(line, "enum settings use `Enum [value, ...]`"));
    }
    let values_end = rest.find(']').ok_or_else(|| {
        line_error(
            line,
            "enum settings use `Enum [value, ...] default <value>`",
        )
    })?;
    let values = split_lzx_list(&rest[1..values_end]);
    if values.is_empty() {
        return Err(line_error(line, "enum settings require at least one value"));
    }
    let default = parse_required_default(line, rest[values_end + 1..].trim())?;
    if !values.iter().any(|value| value == &default) {
        return Err(line_error_owned(
            line,
            format!(
                "enum setting default `{}` is not in the enum values",
                default
            ),
        ));
    }
    Ok((SettingValueSpaceAst::Enum(values), default))
}

fn parse_bool_setting(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(SettingValueSpaceAst, String), ParseError> {
    let default = parse_required_default(line, rest)?;
    if !matches!(default.as_str(), "true" | "false") {
        return Err(line_error(
            line,
            "bool setting default must be `true` or `false`",
        ));
    }
    Ok((SettingValueSpaceAst::Bool, default))
}

fn parse_int_setting(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(SettingValueSpaceAst, String), ParseError> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut min = None;
    let mut max = None;
    let mut default = None;
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "min" => {
                index += 1;
                let value = parts.get(index).ok_or_else(|| {
                    line_error(line, "int setting `min` requires an integer value")
                })?;
                min = Some(parse_i64_token(line, value, "min")?);
            }
            "max" => {
                index += 1;
                let value = parts.get(index).ok_or_else(|| {
                    line_error(line, "int setting `max` requires an integer value")
                })?;
                max = Some(parse_i64_token(line, value, "max")?);
            }
            "default" => {
                index += 1;
                let value = parts.get(index).ok_or_else(|| {
                    line_error(line, "int setting `default` requires an integer value")
                })?;
                if default.is_some() {
                    return Err(line_error(line, "setting declares `default` at most once"));
                }
                default = Some((*value).to_owned());
            }
            _ => {
                return Err(line_error(
                    line,
                    "int settings use `Int [min N] [max N] default V`",
                ));
            }
        }
        index += 1;
    }
    let default = default.ok_or_else(|| line_error(line, "setting requires `default <value>`"))?;
    let default_value = default.parse::<i64>().map_err(|_| {
        line_error(
            line,
            "int setting default must be an integer within the declared range",
        )
    })?;
    if let Some(min) = min {
        if default_value < min {
            return Err(line_error(
                line,
                "int setting default is below the declared `min`",
            ));
        }
    }
    if let Some(max) = max {
        if default_value > max {
            return Err(line_error(
                line,
                "int setting default is above the declared `max`",
            ));
        }
    }
    Ok((SettingValueSpaceAst::Int { min, max }, default))
}

fn parse_required_default(line: &SourceLine<'_>, rest: &str) -> Result<String, ParseError> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 2 || parts[0] != "default" {
        return Err(line_error(line, "setting requires `default <value>`"));
    }
    Ok(parts[1].to_owned())
}

fn parse_i64_token(
    line: &SourceLine<'_>,
    value: &str,
    label: &'static str,
) -> Result<i64, ParseError> {
    value
        .parse::<i64>()
        .map_err(|_| line_error_owned(line, format!("int setting `{}` must be an integer", label)))
}

fn parse_setting_persistence(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<SettingPersistenceAst, ParseError> {
    match value {
        "local" => Ok(SettingPersistenceAst::Local),
        "workspace" => Ok(SettingPersistenceAst::Workspace),
        "none" => Ok(SettingPersistenceAst::None),
        _ => Err(line_error(
            line,
            "`persist` must be `persist local`, `persist workspace`, or `persist none`",
        )),
    }
}

/// Parse a `@<namespace>.<name>` policy atom, with an optional raw
/// parenthesized argument suffix for step-up atoms such as
/// `@mfa.required(within:15m)`.
pub(super) fn parse_policy_atom(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<PolicyAtomAst, ParseError> {
    let atom = value.trim();
    let body = atom.strip_prefix('@').ok_or_else(|| {
        line_error(
            line,
            "policy atoms start with `@` (e.g. `@scope.workspace_admin`)",
        )
    })?;
    let (body, args) = if let Some((head, tail)) = body.split_once('(') {
        if !tail.ends_with(')') {
            return Err(line_error(
                line,
                "policy atom arguments must be closed with `)`",
            ));
        }
        let args = tail[..tail.len() - 1].trim();
        if args.is_empty() {
            return Err(line_error(line, "policy atom arguments cannot be empty"));
        }
        (head.trim(), Some(args.to_owned()))
    } else {
        (body.trim(), None)
    };
    let (namespace, name) = body.split_once('.').ok_or_else(|| {
        line_error(
            line,
            "policy atom must include a namespace and name (`@<ns>.<name>`)",
        )
    })?;
    if !matches!(
        namespace,
        "scope" | "role" | "actor" | "mfa" | "session" | "rate_budget" | "time"
    ) {
        return Err(line_error_owned(
            line,
            format!(
                "policy atom namespace `{}` is not in the closed catalog (`scope` | `role` | `actor` | `mfa` | `session` | `rate_budget` | `time`)",
                namespace
            ),
        ));
    }
    if !is_kebab_or_snake_ident(name) {
        return Err(line_error_owned(
            line,
            format!("policy atom name `{}` must be kebab/snake case", name),
        ));
    }
    Ok(PolicyAtomAst {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        args,
        span: Span::new(line.start, line.end),
    })
}

/// RB.S6 — recognize the new `has_role` / `has_permission` /
/// `authenticated` predicates within a `policy <expr>` payload. The
/// caller passes the raw payload (`rest.trim()` from `policy <rest>`);
/// the helper returns:
///
/// - `Ok(Some(expr))` when the payload is a structured expression
///   (contains `has_role` / `has_permission` / `authenticated` /
///   `and` / `or` / `not` / parens).
/// - `Ok(None)` when the payload is a bare legacy atom
///   (`@policy.<name>` / `@role.<name>` / etc.) — back-compat path,
///   caller keeps the raw string and skips the expression form.
/// - `Err(_)` when the payload looks expression-shaped but is
///   malformed (unknown predicate, bad permission ref, etc.).
pub(super) fn try_parse_policy_expr(
    line: &SourceLine<'_>,
    payload: &str,
) -> Result<Option<PolicyExprAst>, ParseError> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    // Back-compat fast path: bare atom (no spaces, no parens, no keyword
    // boundaries). Examples: `@policy.create`, `@role.admin`,
    // `@scope.same_company`. The caller keeps the raw string for the
    // legacy single-atom rendering.
    if !looks_like_policy_expr(trimmed) {
        return Ok(None);
    }
    let mut parser = PolicyExprParser::new(trimmed, line);
    let expr = parser.parse_or()?;
    if !parser.is_at_end() {
        return Err(line_error_owned(
            line,
            format!(
                "unexpected trailing input in policy expression: `{}`",
                parser.remaining()
            ),
        ));
    }
    Ok(Some(expr))
}

/// Cheap surface heuristic: does the payload contain any of the closed
/// expression keywords or grouping punctuation?
pub(super) fn looks_like_policy_expr(payload: &str) -> bool {
    if payload.contains('(') || payload.contains(')') {
        return true;
    }
    // Tokenize on whitespace; any token equal to a reserved keyword
    // qualifies as expression-shaped.
    for tok in payload.split_whitespace() {
        match tok {
            "authenticated" | "has_role" | "has_permission" | "and" | "or" | "not" => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Hand-rolled recursive-descent parser for the closed policy
/// expression grammar:
///
/// ```text
/// or_expr   := and_expr ("or" and_expr)*
/// and_expr  := unary_expr ("and" unary_expr)*
/// unary_expr := "not" unary_expr | atom_expr
/// atom_expr := "(" or_expr ")"
///            | "authenticated"
///            | "has_role" <ident>
///            | "has_permission" <perm_ref>
///            | <policy_atom>     # @<ns>.<name>
/// ```
struct PolicyExprParser<'a, 'src> {
    input: &'a str,
    pos: usize,
    line: &'a SourceLine<'src>,
}

impl<'a, 'src> PolicyExprParser<'a, 'src> {
    fn new(input: &'a str, line: &'a SourceLine<'src>) -> Self {
        Self {
            input,
            pos: 0,
            line,
        }
    }

    fn is_at_end(&self) -> bool {
        self.skip_ws_peek();
        self.pos >= self.input.len()
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    fn skip_ws_peek(&self) -> usize {
        let bytes = self.input.as_bytes();
        let mut p = self.pos;
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        p
    }

    fn skip_ws(&mut self) {
        self.pos = self.skip_ws_peek();
    }

    /// Consume the literal `kw` if it appears next (followed by
    /// whitespace, `(`, or end). Returns true on success.
    fn consume_keyword(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let rest = &self.input[self.pos..];
        if !rest.starts_with(kw) {
            return false;
        }
        let after = &rest[kw.len()..];
        if !after.is_empty() {
            let c = after.as_bytes()[0];
            if !(c.is_ascii_whitespace() || c == b'(' || c == b')') {
                return false;
            }
        }
        self.pos += kw.len();
        true
    }

    fn consume_char(&mut self, c: char) -> bool {
        self.skip_ws();
        let rest = &self.input[self.pos..];
        if rest.starts_with(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    /// Read a bare ident token (lowercase + digits + `_`). Used for
    /// `has_role <ident>`.
    fn read_ident(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let start = self.pos;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase() || c == b'_' || (self.pos > start && c.is_ascii_digit()) {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(self.input[start..self.pos].to_owned())
        }
    }

    /// Read a permission ref: 2-4 colon-separated lowercase segments.
    /// Mirrors `parse_permission_decl` validation; centralised here so
    /// `has_permission` malformed args raise a parse error
    /// (RBAC-POLICY-PREDICATE-FORM-001 spec).
    fn read_permission_ref(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let start = self.pos;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase()
                || c == b'_'
                || c == b':'
                || (self.pos > start && c.is_ascii_digit())
            {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(self.input[start..self.pos].to_owned())
        }
    }

    /// Read a `@<ns>.<name>` atom token, including one optional
    /// parenthesized argument suffix.
    fn read_atom_token(&mut self) -> Option<&str> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        if self.pos >= bytes.len() || bytes[self.pos] != b'@' {
            return None;
        }
        let start = self.pos;
        self.pos += 1;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-' || c == b'.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start + 1 {
            // Just `@` with nothing after.
            self.pos = start;
            return None;
        }
        if self.pos < bytes.len() && bytes[self.pos] == b'(' {
            self.pos += 1;
            while self.pos < bytes.len() && bytes[self.pos] != b')' {
                self.pos += 1;
            }
            if self.pos < bytes.len() && bytes[self.pos] == b')' {
                self.pos += 1;
            }
        }
        Some(&self.input[start..self.pos])
    }

    fn parse_or(&mut self) -> Result<PolicyExprAst, ParseError> {
        let mut terms = vec![self.parse_and()?];
        while self.consume_keyword("or") {
            terms.push(self.parse_and()?);
        }
        Ok(if terms.len() == 1 {
            terms.into_iter().next().unwrap()
        } else {
            PolicyExprAst::Or(terms)
        })
    }

    fn parse_and(&mut self) -> Result<PolicyExprAst, ParseError> {
        let mut terms = vec![self.parse_unary()?];
        while self.consume_keyword("and") {
            terms.push(self.parse_unary()?);
        }
        Ok(if terms.len() == 1 {
            terms.into_iter().next().unwrap()
        } else {
            PolicyExprAst::And(terms)
        })
    }

    fn parse_unary(&mut self) -> Result<PolicyExprAst, ParseError> {
        if self.consume_keyword("not") {
            let inner = self.parse_unary()?;
            return Ok(PolicyExprAst::Not(Box::new(inner)));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<PolicyExprAst, ParseError> {
        self.skip_ws();
        if self.consume_char('(') {
            let inner = self.parse_or()?;
            if !self.consume_char(')') {
                return Err(line_error(
                    self.line,
                    "unbalanced parens in policy expression (expected `)`)",
                ));
            }
            return Ok(inner);
        }
        if self.consume_keyword("authenticated") {
            return Ok(PolicyExprAst::Authenticated);
        }
        if self.consume_keyword("has_role") {
            let name = self.read_ident().ok_or_else(|| {
                line_error(
                    self.line,
                    "`has_role` requires an identifier (e.g. `has_role manager`)",
                )
            })?;
            return Ok(PolicyExprAst::HasRole(name));
        }
        if self.consume_keyword("has_permission") {
            let perm = self.read_permission_ref().ok_or_else(|| {
                line_error(
                    self.line,
                    "`has_permission` requires a permission ref (e.g. `has_permission users:read`)",
                )
            })?;
            // Validate shape: 2-4 colon-separated lowercase segments,
            // each non-empty. Mirrors the RBAC catalog grammar.
            if !is_valid_permission_ref(&perm) {
                return Err(line_error_owned(
                    self.line,
                    format!(
                        "`has_permission` argument `{}` must be 2-4 colon-separated lowercase segments",
                        perm
                    ),
                ));
            }
            return Ok(PolicyExprAst::HasPermission(perm));
        }
        if let Some(tok) = self.read_atom_token() {
            // Re-parse via parse_policy_atom to enforce the closed
            // namespace catalog. `tok` includes the leading `@`.
            let owned = tok.to_owned();
            let atom = parse_policy_atom(self.line, &owned)?;
            return Ok(PolicyExprAst::Atom(atom));
        }
        Err(line_error_owned(
            self.line,
            format!(
                "expected `authenticated`, `has_role`, `has_permission`, `not`, `(`, or `@<ns>.<name>` in policy expression; found `{}`",
                self.remaining()
            ),
        ))
    }
}

/// Permission ref shape: 2-4 colon-separated lowercase segments, each
/// non-empty, alphanumeric + `_`, first char lowercase. Mirrors the
/// `permission <ref>` catalog grammar (`parse_permission_decl`).
fn is_valid_permission_ref(s: &str) -> bool {
    let segments: Vec<&str> = s.split(':').collect();
    if segments.len() < 2 || segments.len() > 4 {
        return false;
    }
    for seg in segments {
        if seg.is_empty() {
            return false;
        }
        let mut chars = seg.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() {
            return false;
        }
        for c in chars {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod surface_parser_tests {
    use super::parse_surface_document;
    use crate::{
        BindingRefAst, DrawerBindingSourceAst, DrawerTriggerAst, FilterCardinalityAst,
        SearchModeAst, SelectionModeAst, SettingPersistenceAst, SettingValueSpaceAst, SortDirAst,
        SurfaceTargetAst, ViewAst,
    };

    #[test]
    fn minimal_surface_one_audience_one_view_list() {
        let source = r#"
surface slug web
  audience admin
    view list slug_list
      source slug.query.mine
      columns key, title
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, SurfaceTargetAst::Web);
        assert_eq!(surface.uses_feature, None);
        assert_eq!(surface.audiences.len(), 1);
        let audience = &surface.audiences[0];
        assert_eq!(audience.name, "admin");
        assert_eq!(audience.requires.len(), 0);
        assert_eq!(audience.views.len(), 1);
        let view = match &audience.views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected ViewAst::List, got {:?}", other),
        };
        assert_eq!(view.name, "slug_list");
        assert_eq!(view.route, None);
        assert_eq!(view.source, "slug.query.mine");
        assert_eq!(view.columns, vec!["key", "title"]);
    }

    #[test]
    fn parses_full_section_13_1_demo_fixture() {
        // Section 13.1 verbatim from
        // `docs/proposals/lzx-integration-codegen.md`.
        let source = r#"surface slug web
  uses feature slug

  audience admin
    requires @scope.workspace_admin

    view list slug_list at "/slugs"
      source slug.query.mine
      columns key, title, tags, created_at
      search key, title
      filter tags
      cells tags @client.type_badge
      actions create, update, delete

    view detail slug_detail at "/slugs/:key"
      source slug.query.by_key
      route key: Text from path
      sections header, metadata, related_items
      cells tags @client.type_badge
      actions update, delete

    view create slug_create at "/slugs/new"
      submit slug.command.create
      fields key, title, description, tags
      cells tags @client.type_badge

  audience public
    requires @scope.workspace_member

    view list public_slug_list at "/browse"
      source slug.query.mine
      columns key, title
      search key, title
"#;
        let surface = parse_surface_document(source).expect("parses §13.1 fixture");
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, SurfaceTargetAst::Web);
        assert_eq!(surface.uses_feature.as_deref(), Some("slug"));
        assert_eq!(surface.audiences.len(), 2);

        // admin audience.
        let admin = &surface.audiences[0];
        assert_eq!(admin.name, "admin");
        assert_eq!(admin.requires.len(), 1);
        assert_eq!(admin.requires[0].namespace, "scope");
        assert_eq!(admin.requires[0].name, "workspace_admin");
        assert_eq!(admin.views.len(), 3);

        let list = match &admin.views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(list.name, "slug_list");
        assert_eq!(list.route.as_deref(), Some("/slugs"));
        assert_eq!(list.columns, vec!["key", "title", "tags", "created_at"]);
        match &list.search.as_ref().expect("search").mode {
            SearchModeAst::Columns(columns) => assert_eq!(columns, &vec!["key", "title"]),
            other => panic!("expected columns search, got {other:?}"),
        }
        assert_eq!(list.filter, vec!["tags"]);
        assert_eq!(list.cells.len(), 1);
        assert_eq!(list.cells[0].field, "tags");
        assert_eq!(list.cells[0].slot, "type_badge");
        assert_eq!(list.actions, vec!["create", "update", "delete"]);

        let detail = match &admin.views[1] {
            ViewAst::Detail(v) => v,
            other => panic!("expected detail, got {:?}", other),
        };
        assert_eq!(detail.name, "slug_detail");
        assert_eq!(detail.route.as_deref(), Some("/slugs/:key"));
        assert_eq!(detail.source, "slug.query.by_key");
        assert_eq!(detail.route_params.len(), 1);
        assert_eq!(detail.route_params[0].name, "key");
        assert_eq!(detail.route_params[0].type_ref, "Text");
        assert_eq!(detail.sections, vec!["header", "metadata", "related_items"]);
        assert_eq!(detail.actions, vec!["update", "delete"]);

        let create = match &admin.views[2] {
            ViewAst::Create(v) => v,
            other => panic!("expected create, got {:?}", other),
        };
        assert_eq!(create.name, "slug_create");
        assert_eq!(create.route.as_deref(), Some("/slugs/new"));
        assert_eq!(create.submit, "slug.command.create");
        assert_eq!(create.fields, vec!["key", "title", "description", "tags"]);

        // public audience.
        let public = &surface.audiences[1];
        assert_eq!(public.name, "public");
        assert_eq!(public.requires.len(), 1);
        assert_eq!(public.requires[0].name, "workspace_member");
        assert_eq!(public.views.len(), 1);
    }

    #[test]
    fn search_segmented_block_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key
      search segmented
        field slug binds filters.slug
        field type binds filters.type
        field tag binds filters.tags
        free text into source.q
"#;
        let surface = parse_surface_document(source).expect("parses segmented search");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {other:?}"),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, SearchModeAst::Segmented);
        assert_eq!(search.fields.len(), 3);
        assert_eq!(search.fields[0].key, "slug");
        assert_eq!(
            search.fields[0].binds_to,
            BindingRefAst::Filter {
                name: "slug".to_owned()
            }
        );
        assert_eq!(
            search.free_text_target,
            Some(BindingRefAst::SourceInput {
                name: "q".to_owned()
            })
        );
    }

    #[test]
    fn search_columns_v1_form_still_parses() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search key, title\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        match &search.mode {
            SearchModeAst::Columns(columns) => assert_eq!(columns, &vec!["key", "title"]),
            other => panic!("expected columns search, got {other:?}"),
        }
        assert!(search.fields.is_empty());
        assert!(search.free_text_target.is_none());
    }

    #[test]
    fn search_segmented_rejects_inline_content() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented foo\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("takes no inline list"));
    }

    #[test]
    fn search_at_most_once() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search key
      search segmented
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn search_field_rejects_duplicate_key() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search segmented
        field slug binds filters.slug
        field slug binds source.slug
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("more than once"));
    }

    #[test]
    fn search_free_text_at_most_once() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search segmented
        free text into source.q
        free text into source.query
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("free text into"));
    }

    #[test]
    fn search_binding_ref_filter_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field slug binds filters.slug\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::Filter {
                name: "slug".to_owned()
            }
        );
    }

    #[test]
    fn search_binding_ref_source_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field q binds source.q\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::SourceInput {
                name: "q".to_owned()
            }
        );
    }

    #[test]
    fn search_binding_ref_selection_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field selected binds selection\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::SelectionScalar
        );
    }

    #[test]
    fn search_binding_ref_invalid() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field slug binds foo.bar\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("binding references"));
    }

    #[test]
    fn search_segmented_empty_block() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n";
        let surface = parse_surface_document(source).expect("parses empty segmented search");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, SearchModeAst::Segmented);
        assert!(search.fields.is_empty());
        assert!(search.free_text_target.is_none());
    }

    #[test]
    fn view_list_requires_source() {
        let source = "surface slug web\n  audience admin\n    view list bad\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("view list requires"));
    }

    #[test]
    fn view_list_no_columns_is_not_parse_time_error() {
        let source =
            "surface slug web\n  audience admin\n    view list bad\n      source slug.query.mine\n";
        let surface = parse_surface_document(source).expect("parses without columns");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert!(view.columns.is_empty());
        assert!(view.cells_slot.is_none());
    }

    #[test]
    fn view_create_requires_submit() {
        let source = "surface slug web\n  audience admin\n    view create bad\n      fields key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("view create requires"));
    }

    #[test]
    fn view_create_parses_on_success_block() {
        let source = r#"surface host web
  audience admin
    view create edit_host
      submit host.command.update_host_basic_details
      fields title
      on_success
        back
        redirect "/host/property/{result.id}"
        flash success @translation.saved
        invalidates query.lookup_my_host
        replace
"#;
        let surface = parse_surface_document(source).expect("parses on_success");
        let create = match &surface.audiences[0].views[0] {
            ViewAst::Create(v) => v,
            other => panic!("expected create, got {other:?}"),
        };
        let on_success = create.on_success.as_ref().expect("on_success");
        assert!(on_success.back);
        assert_eq!(
            on_success.redirect.as_deref(),
            Some("/host/property/{result.id}")
        );
        let flash = on_success.flash.as_ref().expect("flash");
        assert_eq!(flash.kind, "success");
        assert_eq!(flash.message_key.key, "saved");
        assert_eq!(on_success.invalidates.len(), 1);
        assert_eq!(on_success.invalidates[0].query, "query.lookup_my_host");
        assert!(on_success.replace);
    }

    #[test]
    fn view_create_parses_on_success_redirect_only() {
        let source = r#"surface host web
  audience admin
    view create create_property
      submit host.command.create_property
      fields title
      on_success
        redirect "/host/property/{result.id}"
"#;
        let surface = parse_surface_document(source).expect("parses redirect-only on_success");
        let create = match &surface.audiences[0].views[0] {
            ViewAst::Create(v) => v,
            other => panic!("expected create, got {other:?}"),
        };
        let on_success = create.on_success.as_ref().expect("on_success");
        assert!(!on_success.back);
        assert_eq!(
            on_success.redirect.as_deref(),
            Some("/host/property/{result.id}")
        );
        assert!(on_success.flash.is_none());
        assert!(on_success.invalidates.is_empty());
        assert!(!on_success.replace);
    }

    #[test]
    fn on_success_rejects_invalid_flash_kind() {
        let source = r#"surface host web
  audience admin
    view create edit_host
      submit host.command.update_host_basic_details
      fields title
      on_success
        flash warning @translation.saved
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("kind must be"));
    }

    #[test]
    fn mobile_target_recognised() {
        let source = "surface item mobile\n  audience kiosk\n    view list item_list\n      source item.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses mobile");
        assert_eq!(surface.target, SurfaceTargetAst::Mobile);
    }

    #[test]
    fn rejects_unknown_target() {
        let source = "surface slug desktop\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("surface target must be"));
    }

    #[test]
    fn rejects_top_level_indentation() {
        let source = "  surface slug web\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("top-level"));
    }

    #[test]
    fn cells_binding_parses() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      columns tags\n      cells tags @client.type_badge\n";
        let surface = parse_surface_document(source).expect("parses cells");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.cells.len(), 1);
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn view_list_accepts_cells_at_client_slot_grid_form() {
        let source = "surface item web\n  audience admin\n    view list foo at \"/\"\n      source f.query.q\n      cells @client.item_card\n";
        let surface = parse_surface_document(source).expect("parses cells grid form");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot.as_deref(), Some("item_card"));
        assert!(view.columns.is_empty());
        assert!(view.cells.is_empty());
    }

    #[test]
    fn view_list_rejects_cells_at_client_slot_with_trailing_tokens() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.foo extra\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("accepts only one slot identifier"));
    }

    #[test]
    fn view_list_rejects_double_cells_grid_form() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.item_card\n      cells @client.other_card\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn view_list_v1_per_column_cells_still_works() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      cells tags @client.type_badge\n      columns key, title\n";
        let surface = parse_surface_document(source).expect("parses per-column cells");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot, None);
        assert_eq!(view.columns, vec!["key", "title"]);
        assert_eq!(view.cells.len(), 1);
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn view_list_no_longer_requires_columns_if_cells_slot_present() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.item_card\n";
        let surface = parse_surface_document(source).expect("parses without columns");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot.as_deref(), Some("item_card"));
        assert!(view.columns.is_empty());
    }

    #[test]
    fn view_list_empty_grid_and_no_columns_does_not_error_at_parse_time() {
        let source =
            "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n";
        let surface = parse_surface_document(source).expect("parses without render declaration");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert!(view.cells_slot.is_none());
        assert!(view.columns.is_empty());
    }

    #[test]
    fn cells_binding_requires_at_client_prefix() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      columns tags\n      cells tags @server.type_badge\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("cell slot must be `@client."));
    }

    #[test]
    fn view_list_with_drawer_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key, title
      drawer item_detail on select
        source item.query.by_id
        route key from selection
        sections header, content, metadata
        cells related @client.related_items
        actions update, delete
"#;
        let surface = parse_surface_document(source).expect("parses drawer");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        let drawer = view.drawer.as_ref().expect("drawer populated");
        assert_eq!(drawer.name, "item_detail");
        assert_eq!(drawer.trigger, DrawerTriggerAst::Select);
        assert_eq!(drawer.source, "item.query.by_id");
        let route = drawer.route_binding.as_ref().expect("route binding");
        assert_eq!(route.target, "key");
        assert_eq!(route.source, DrawerBindingSourceAst::Selection);
        assert_eq!(drawer.sections, vec!["header", "content", "metadata"]);
        assert_eq!(drawer.cells.len(), 1);
        assert_eq!(drawer.cells[0].field, "related");
        assert_eq!(drawer.cells[0].slot, "related_items");
        assert_eq!(drawer.actions, vec!["update", "delete"]);
    }

    #[test]
    fn drawer_rejects_unknown_trigger() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on hover\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer trigger must be `select` or `open`")
        );
    }

    #[test]
    fn drawer_rejects_columns_inside() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        columns a, b\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer body lines are"));
    }

    #[test]
    fn drawer_rejects_filters_inside() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        filters status\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer body lines are"));
    }

    #[test]
    fn drawer_rejects_nested() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        drawer bar on select\n          source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer cannot be nested"));
    }

    #[test]
    fn view_list_at_most_one_drawer() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n      drawer bar on open\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most one `drawer`"));
    }

    #[test]
    fn drawer_grid_form_cells_rejected() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        cells @client.item_card\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer cells use `cells <field> @client.<slot>`")
        );
    }

    #[test]
    fn view_detail_rejects_drawer() {
        let source = "surface item web\n  audience admin\n    view detail item_detail\n      source item.query.by_id\n      route key: Text from path\n      drawer foo on select\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("`drawer` is only valid in `view list` bodies")
        );
    }

    #[test]
    fn route_key_from_selection_parses() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        route key from selection\n";
        let surface = parse_surface_document(source).expect("parses drawer route");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let route = view
            .drawer
            .as_ref()
            .and_then(|drawer| drawer.route_binding.as_ref())
            .expect("route binding");
        assert_eq!(route.target, "key");
        assert_eq!(route.source, DrawerBindingSourceAst::Selection);
    }

    #[test]
    fn route_key_from_path_inside_drawer_rejected() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        route key from path\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer route binding source must be `from selection`")
        );
    }

    #[test]
    fn view_list_filters_block_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key
      filters
        type: ItemType
        status: ItemStatus
        confidence: Confidence
        tags: list of Text
        slug: Text from query
"#;
        let surface = parse_surface_document(source).expect("parses filters");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters.len(), 5);
        assert_eq!(view.filters[0].name, "type");
        assert_eq!(view.filters[0].type_ref, "ItemType");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Single);
        assert!(!view.filters[0].url_sync);
        assert_eq!(view.filters[3].name, "tags");
        assert_eq!(view.filters[3].cardinality, FilterCardinalityAst::Multi);
        assert!(!view.filters[3].url_sync);
        assert_eq!(view.filters[4].name, "slug");
        assert_eq!(view.filters[4].cardinality, FilterCardinalityAst::Single);
        assert!(view.filters[4].url_sync);
    }

    #[test]
    fn filters_single_from_query() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from query\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters[0].name, "slug");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Single);
        assert!(view.filters[0].url_sync);
    }

    #[test]
    fn filters_multi_from_query() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        tags: list of Text from query\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters[0].name, "tags");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Multi);
        assert!(view.filters[0].url_sync);
    }

    #[test]
    fn filters_rejects_from_path() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from path\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("from query"));
    }

    #[test]
    fn filters_rejects_duplicate_name() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        tags: list of Text\n        tags: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("duplicate filter `tags`"));
    }

    #[test]
    fn filters_rejects_empty_block() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n      actions update\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires at least one"));
    }

    #[test]
    fn view_detail_rejects_filters() {
        let source = "surface item web\n  audience admin\n    view detail a\n      source item.query.by_id\n      filters\n        slug: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("only valid in `view list`"));
    }

    #[test]
    fn view_create_rejects_filters() {
        let source = "surface item web\n  audience admin\n    view create a\n      submit item.command.create\n      fields key\n      filters\n        slug: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("only valid in `view list`"));
    }

    #[test]
    fn view_list_at_most_one_filters_block() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text\n      filters\n        tags: list of Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn filters_missing_type_ref() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug:\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires a type"));
    }

    #[test]
    fn multiple_audiences_per_surface() {
        let source = r#"surface slug web
  audience admin
    requires @scope.workspace_admin
    view list a
      source slug.query.mine
      columns key

  audience public
    requires @scope.workspace_member
    view list b
      source slug.query.mine
      columns key
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.audiences.len(), 2);
        assert_eq!(surface.audiences[0].name, "admin");
        assert_eq!(surface.audiences[1].name, "public");
    }

    #[test]
    fn multiple_views_per_audience() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
    view list b
      source slug.query.mine
      columns key
    view detail c at "/x/:id"
      source slug.query.by_key
      route id: Text from path
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.audiences[0].views.len(), 3);
    }

    #[test]
    fn empty_audience_parses_cleanly() {
        let source = "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n";
        let surface = parse_surface_document(source).expect("parses empty audience");
        assert_eq!(surface.audiences.len(), 1);
        assert_eq!(surface.audiences[0].views.len(), 0);
        assert_eq!(surface.audiences[0].requires.len(), 1);
    }

    #[test]
    fn actions_comma_separated_list() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions create, update, delete\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions, vec!["create", "update", "delete"]);
    }

    #[test]
    fn at_path_optional() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.route, None);
    }

    #[test]
    fn rejects_partial_overrides() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      columns += score\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("partial overrides"));
    }

    #[test]
    fn route_param_captures_type_text() {
        let source = "surface slug web\n  audience admin\n    view detail d at \"/s/:id\"\n      source slug.query.by_key\n      route id: Customer.ID from path\n";
        let surface = parse_surface_document(source).expect("parses");
        let detail = match &surface.audiences[0].views[0] {
            ViewAst::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.route_params[0].name, "id");
        assert_eq!(detail.route_params[0].type_ref, "Customer.ID");
    }

    #[test]
    fn uses_feature_override_captured() {
        let source = "surface slug web\n  uses feature slug\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.uses_feature.as_deref(), Some("slug"));
    }

    #[test]
    fn requires_scope_atom_captured() {
        let source = "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        let atom = &surface.audiences[0].requires[0];
        assert_eq!(atom.namespace, "scope");
        assert_eq!(atom.name, "workspace_admin");
    }

    #[test]
    fn requires_rejects_unknown_namespace() {
        let source = "surface slug web\n  audience admin\n    requires @group.workspace_admin\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("namespace"));
    }

    #[test]
    fn rejects_blank_document() {
        let source = "\n\n# comment only\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(matches!(err, super::ParseError::Expected { .. }));
    }

    #[test]
    fn comments_and_blank_lines_skipped() {
        let source = r#"# header comment

surface slug web
  # mid comment
  audience admin

    view list a
      # explanatory
      source slug.query.mine
      columns key
"#;
        let surface = parse_surface_document(source).expect("parses with comments");
        assert_eq!(surface.audiences[0].views.len(), 1);
    }

    #[test]
    fn at_path_requires_leading_slash() {
        let source = "surface slug web\n  audience admin\n    view list a at \"slugs\"\n      source slug.query.mine\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("must begin with `/`"));
    }

    #[test]
    fn view_create_with_route_at() {
        let source = "surface slug web\n  audience admin\n    view create new at \"/slugs/new\"\n      submit slug.command.create\n      fields key\n";
        let surface = parse_surface_document(source).expect("parses");
        let create = match &surface.audiences[0].views[0] {
            ViewAst::Create(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(create.route.as_deref(), Some("/slugs/new"));
        assert_eq!(create.submit, "slug.command.create");
    }

    #[test]
    fn sort_block_parses() {
        let source = r#"surface item web
  audience admin
    view list terminal
      source item.query.search
      columns title
      sort
        by title, type, priority, updated
        default updated desc
"#;
        let surface = parse_surface_document(source).expect("parses sort");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let sort = list.sort.as_ref().expect("sort");
        assert_eq!(sort.allowed, vec!["title", "type", "priority", "updated"]);
        assert_eq!(sort.default_field, "updated");
        assert_eq!(sort.default_dir, SortDirAst::Desc);
    }

    #[test]
    fn sort_requires_by_line() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        default title asc\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires a `by`"));
    }

    #[test]
    fn sort_default_field_must_be_allowed() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title\n        default updated desc\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("must be listed"));
    }

    #[test]
    fn sort_default_requires_dir() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title\n        default title\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("default <field>"));
    }

    #[test]
    fn selection_single_and_multi_parse() {
        let source = r#"surface item web
  audience admin
    view list single_view
      source item.query.search
      columns title
      selection single
    view list multi_view
      source item.query.search
      columns title
      selection multi
"#;
        let surface = parse_surface_document(source).expect("parses selection");
        let single = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        let multi = match &surface.audiences[0].views[1] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(single.mode, SelectionModeAst::Single);
        assert_eq!(multi.mode, SelectionModeAst::Multi);
    }

    #[test]
    fn selection_none_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      selection none\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("omit the line"));
    }

    #[test]
    fn selection_unknown_mode_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      selection foo\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("selection single"));
    }

    #[test]
    fn bulk_actions_single_and_multi_parse() {
        let source = r#"surface item web
  audience admin
    view list one
      source item.query.search
      columns title
      selection multi
      bulk_actions delete
    view list many
      source item.query.search
      columns title
      selection multi
      bulk_actions delete, archive
"#;
        let surface = parse_surface_document(source).expect("parses bulk actions");
        let one = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        let many = match &surface.audiences[0].views[1] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(one.bulk_actions, vec!["delete"]);
        assert_eq!(many.bulk_actions, vec!["delete", "archive"]);
    }

    #[test]
    fn bulk_actions_duplicate_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      bulk_actions delete\n      bulk_actions archive\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("bulk_actions"));
    }

    #[test]
    fn bulk_actions_without_selection_is_not_parser_error() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      bulk_actions delete\n";
        let surface = parse_surface_document(source).expect("bulk-only parses");
        let selection = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(selection.mode, SelectionModeAst::None);
        assert_eq!(selection.bulk_actions, vec!["delete"]);
    }

    #[test]
    fn settings_full_example_parses() {
        let source = r#"surface item web
  audience admin
    view list terminal
      source item.query.search
      columns title
      settings
        grid_size: Enum [sm, md, lg] default sm
          persist local
        show_metadata: Bool default true
        page_size: Int min 10 max 200 default 25
          persist workspace
"#;
        let surface = parse_surface_document(source).expect("parses settings");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(list.settings.len(), 3);
        assert_eq!(list.settings[0].name, "grid_size");
        assert_eq!(
            list.settings[0].value_space,
            SettingValueSpaceAst::Enum(vec!["sm".into(), "md".into(), "lg".into()])
        );
        assert_eq!(list.settings[0].default, "sm");
        assert_eq!(list.settings[0].persistence, SettingPersistenceAst::Local);
        assert_eq!(list.settings[1].value_space, SettingValueSpaceAst::Bool);
        assert_eq!(
            list.settings[2].value_space,
            SettingValueSpaceAst::Int {
                min: Some(10),
                max: Some(200)
            }
        );
        assert_eq!(
            list.settings[2].persistence,
            SettingPersistenceAst::Workspace
        );
    }

    #[test]
    fn persist_outside_setting_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      persist local\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("persist"));
    }

    #[test]
    fn duplicate_setting_name_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        grid_size: Bool default true\n        grid_size: Bool default false\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("duplicate setting"));
    }

    #[test]
    fn enum_default_must_be_member() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        grid_size: Enum [sm, md] default lg\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("not in the enum"));
    }

    #[test]
    fn int_default_must_be_in_range() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        page_size: Int min 10 max 200 default 5\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("below"));
    }

    #[test]
    fn settings_empty_block_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at least one setting"));
    }

    #[test]
    fn list_only_keywords_rejected_in_detail_and_create() {
        let detail = "surface item web\n  audience admin\n    view detail terminal\n      source item.query.by_id\n      sort\n        by title\n        default title asc\n";
        let create = "surface item web\n  audience admin\n    view create terminal\n      submit item.command.create\n      selection multi\n";
        let detail_err = parse_surface_document(detail).unwrap_err();
        let create_err = parse_surface_document(create).unwrap_err();
        assert!(detail_err.to_string().contains("valid only in `view list`"));
        assert!(create_err.to_string().contains("valid only in `view list`"));
    }
}
