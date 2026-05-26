//! Redirect-target validation for route guards (ROUTE-GUARD-003).

use std::collections::BTreeSet;

use lazuli_ir::{RouteGuardDefaults, SpanRef, ViewGuard};

use super::{RouteGuardDiagnostic, RouteGuardOrigin, RouteGuardSeverity};

pub(super) fn check_default_redirects(
    defaults: &RouteGuardDefaults,
    route_names: &BTreeSet<String>,
    route_paths: &BTreeSet<String>,
    seen: &mut BTreeSet<(String, &'static str)>,
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    for (slot, target) in [
        ("on_unauthenticated", defaults.on_unauthenticated.as_deref()),
        ("on_unauthorized", defaults.on_unauthorized.as_deref()),
    ] {
        if let Some(target) = target {
            push_redirect_003(
                target,
                slot,
                defaults.span_ref,
                RouteGuardOrigin::App,
                route_names,
                route_paths,
                seen,
                out,
            );
        }
    }
}

pub(super) fn check_guard_redirects(
    guard: &ViewGuard,
    route_names: &BTreeSet<String>,
    route_paths: &BTreeSet<String>,
    seen: &mut BTreeSet<(String, &'static str)>,
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    for (slot, target) in [
        ("on_unauthenticated", guard.on_unauthenticated.as_deref()),
        ("on_unauthorized", guard.on_unauthorized.as_deref()),
    ] {
        if let Some(target) = target {
            push_redirect_003(
                target,
                slot,
                guard.span_ref,
                RouteGuardOrigin::Lzx,
                route_names,
                route_paths,
                seen,
                out,
            );
        }
    }
}

fn push_redirect_003(
    target: &str,
    slot: &'static str,
    span: Option<SpanRef>,
    origin: RouteGuardOrigin,
    route_names: &BTreeSet<String>,
    route_paths: &BTreeSet<String>,
    seen: &mut BTreeSet<(String, &'static str)>,
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    if route_paths.contains(target) || (!target.starts_with('/') && route_names.contains(target)) {
        return;
    }
    if !seen.insert((target.to_owned(), slot)) {
        return;
    }
    out.push(RouteGuardDiagnostic {
        code: "ROUTE-GUARD-003",
        severity: RouteGuardSeverity::Error,
        origin,
        span,
        message: format!(
            "route guard `{slot}` redirect target `{target}` does not resolve to a declared route."
        ),
    });
}
