//! `actor_query` validation for route-guarded apps (ROUTE-GUARD-004).

use lazuli_ir::{AppManifest, ExperienceModule, Feature, Query, SpanRef, TypeRef};

use super::{RouteGuardDiagnostic, RouteGuardOrigin, RouteGuardSeverity, parse_query_ref};

pub(super) fn check_actor_query(
    app: Option<&AppManifest>,
    module: &ExperienceModule,
    features: &[Feature],
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    let guarded = app.and_then(|a| a.route_guard.as_ref()).is_some()
        || module.routes.iter().any(|r| r.guard.is_some())
        || module.surfaces.iter().any(|s| {
            s.audiences
                .iter()
                .any(|a| a.guard.is_some() || a.views.iter().any(|v| v.guard.is_some()))
        })
        || module
            .experiences
            .iter()
            .any(|e| e.views.iter().any(|v| v.guard.is_some()));
    let Some(app) = app.filter(|_| guarded) else {
        return;
    };
    let Some(actor_query) = app.actor_query.as_deref() else {
        return push_004(
            "app declares route guards but no `actor_query`.",
            app.span_ref,
            out,
        );
    };
    let Some((feature, name)) = parse_query_ref(actor_query, "") else {
        return push_004(
            "`actor_query` must be `<feature>.query.<name>`.",
            app.span_ref,
            out,
        );
    };
    match find_query(features, &feature, &name) {
        Some(Query::Sql(q)) if !actor_type(&q.returns) => push_004(
            "`actor_query` should return `LazuliActor | null` compatible data.",
            q.span_ref.or(app.span_ref),
            out,
        ),
        Some(_) => {}
        None => push_004(
            "`actor_query` references a query that does not exist.",
            app.span_ref,
            out,
        ),
    }
}

pub(super) fn push_004(message: &str, span: Option<SpanRef>, out: &mut Vec<RouteGuardDiagnostic>) {
    out.push(RouteGuardDiagnostic {
        code: "ROUTE-GUARD-004",
        severity: RouteGuardSeverity::Warning,
        origin: RouteGuardOrigin::App,
        span,
        message: message.to_owned(),
    });
}

fn find_query<'a>(features: &'a [Feature], feature: &str, name: &str) -> Option<&'a Query> {
    features
        .iter()
        .find(|f| f.name == feature)?
        .queries
        .iter()
        .find(|q| q.name() == name)
}

fn actor_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::UserDefined(q) => matches!(q.name.as_str(), "LazuliActor" | "Actor"),
        TypeRef::Unresolved(s) => s.contains("LazuliActor") || s.contains("Actor"),
        _ => false,
    }
}
