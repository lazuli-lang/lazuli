//! `.lzx` app-surface lowering — document, app, routes, experiences.
//!
//! ## Why this slot exists
//!
//! `.lzx` files carry the experience layer: the app manifest, route
//! table, experience definitions (view groupings + extension points),
//! and platform surfaces (web / mobile audiences). The lowering here
//! is mechanical projection — `syntax::Lzx*` AST → `ir::Experience*`
//! shapes — because the parser already enforces structural shape and
//! the doctor cells run cross-module reasoning later.
//!
//! Compared to `feature.rs` (which carries the `.lzi` ViewModel
//! surface lowering: `lower_surface` / `lower_view_ast`), the
//! functions here never validate against feature scope. Their entire
//! domain is the `.lzx` document tree.
//!
//! ## Public API
//!
//! Only `lower_lzx_document` is exported. Everything else is
//! `pub(crate)` and used only by the document walker.
//!
//! Source AST shapes: `lazuli_syntax::LzxDocument` and friends.
//! Destination IR shapes: `lazuli_ir::ExperienceModule` family.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

use crate::helpers::span_of;

/// Lower a parsed `.lzx` document into its [`ir::ExperienceModule`]
/// counterpart.
///
/// One-shot entry point used by the analyzer pipeline. Walks the
/// document's app / routes / experiences / surfaces slots; each child
/// lowerer is private because nothing outside the walker has a use for
/// the partial shapes.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::lzx::lower_lzx_document;
/// use lazuli_syntax::LzxDocument;
///
/// let doc: LzxDocument = unimplemented!("parse from a .lzx file");
/// let module = lower_lzx_document(&doc);
/// assert!(module.routes.iter().count() >= 0);
/// ```
pub fn lower_lzx_document(document: &syntax::LzxDocument) -> ir::ExperienceModule {
    ir::ExperienceModule {
        app: document.app.as_ref().map(lower_lzx_app),
        routes: document.routes.iter().map(lower_lzx_route).collect(),
        experiences: document.experiences.iter().map(lower_experience).collect(),
        surfaces: document
            .surfaces
            .iter()
            .map(lower_platform_surface)
            .collect(),
    }
}

/// G-A2 — lower the experience-dialect surfaces of a `.lzx` document into the
/// rich per-feature [`ir::Surface`] shape (the one carrying [`ir::ViewUx`] /
/// [`ir::FilterDecl`]) so the §7a surface-UX doctor rules (`ux_rules`,
/// `date_range_filter`) can walk real `.lzx` usage.
///
/// `lower_lzx_document` above projects surfaces into the lean
/// [`ir::PlatformSurface`] shape, which drops the §7a `ViewUxAst`
/// (`wizard_steps` / `tab_group` / `view_mode` / `view.board` /
/// `repeatable input`) the parser captured. This sibling lowering keeps that
/// UX payload by reusing the surface-dialect's `lower_view_ux` /
/// `lower_audience_ux` helpers — so the §7a `ViewUxAst -> ir::ViewUx`
/// projection is not reimplemented.
///
/// The experience dialect carries no `source <feature>.query.<name>` line and
/// no list/detail/create kind (every view is `view <name> <type>`), so:
/// * the view kind is inferred from `<type>` (`Form`/`Create` -> Create,
///   `Detail`/`SidePanel`/`Panel` -> Detail, else List);
/// * each list/detail view's `source` is synthesized as a `query.list`
///   reference whose `feature` is the surface's owning feature — single-resource
///   features then resolve unambiguously (matching `ux_rules::resolve_resource`),
///   and multi-resource features are suppressed (the rules return `None`).
///
/// Views whose `view_modes` reference an unknown render mode are still emitted;
/// the analyzer normally rejects those at lowering (`LzxUnknownRenderMode`), so
/// here an unknown mode is simply dropped (doctor's other passes own that).
pub fn lower_lzx_feature_surfaces(document: &syntax::LzxDocument) -> Vec<ir::Surface> {
    document
        .surfaces
        .iter()
        .map(lower_feature_surface_from_experience)
        .collect()
}

fn lower_feature_surface_from_experience(surface: &syntax::LzxSurface) -> ir::Surface {
    let owning_feature = surface
        .uses_experience
        .clone()
        .unwrap_or_else(|| surface.experience.clone());
    ir::Surface {
        feature: owning_feature.clone(),
        target: match surface.platform {
            syntax::LzxPlatform::Web => ir::SurfaceTarget::Web,
            syntax::LzxPlatform::Mobile => ir::SurfaceTarget::Mobile,
        },
        audiences: surface
            .audiences
            .iter()
            .map(|audience| lower_feature_audience_from_experience(audience, &owning_feature))
            .collect(),
        span_ref: Some(span_of(surface.span)),
    }
}

fn lower_feature_audience_from_experience(
    audience: &syntax::LzxAudience,
    owning_feature: &str,
) -> ir::Audience {
    ir::Audience {
        name: audience.name.clone(),
        requires: Vec::new(),
        views: audience
            .views
            .iter()
            .map(|view| lower_feature_view_from_experience(view, owning_feature))
            .collect(),
        ux: crate::surface::crate_lower_audience_ux(&audience.ux),
        span_ref: Some(span_of(audience.span)),
    }
}

fn lower_feature_view_from_experience(
    view: &syntax::LzxPlatformView,
    owning_feature: &str,
) -> ir::View {
    let synthetic_source = ir::QueryRef {
        feature: owning_feature.to_owned(),
        kind: ir::QueryKind::List,
        name: String::new(),
    };
    // `lower_view_ux` only fails on an unknown render-mode keyword; drop the
    // UX payload to empty in that case rather than aborting the whole surface
    // (doctor's render-mode pass owns that diagnostic independently).
    let ux = crate::surface::crate_lower_view_ux(&view.ux, owning_feature, &view.name)
        .unwrap_or_default();

    match classify_experience_view_kind(&view.view_type) {
        ExperienceViewKind::Create => ir::View::Create(ir::ViewCreate {
            name: view.name.clone(),
            route: None,
            submit: ir::CommandRef {
                feature: owning_feature.to_owned(),
                name: view.submit.clone().unwrap_or_default(),
            },
            on_success: None,
            fields: view.fields.clone(),
            cells: Vec::new(),
            redacted_fields: Vec::new(),
            span_ref: Some(span_of(view.span)),
        }),
        ExperienceViewKind::Detail => ir::View::Detail(ir::ViewDetail {
            name: view.name.clone(),
            route: None,
            source: synthetic_source,
            route_params: Vec::new(),
            sections: view.sections.clone(),
            cells: Vec::new(),
            actions: Vec::new(),
            redacted_fields: Vec::new(),
            ux,
            span_ref: Some(span_of(view.span)),
        }),
        ExperienceViewKind::List => ir::View::List(ir::ViewList {
            name: view.name.clone(),
            route: None,
            source: synthetic_source,
            render: ir::ListRender::Table {
                columns: view.columns.clone(),
            },
            search: None,
            filter: view
                .filter
                .iter()
                .map(|name| ir::FilterDecl {
                    name: name.clone(),
                    type_ref: String::new(),
                    cardinality: ir::FilterCardinality::Single,
                    url_sync: false,
                    span_ref: None,
                })
                .collect(),
            cells: Vec::new(),
            actions: Vec::new(),
            drawer: None,
            sort: None,
            selection: None,
            settings: Vec::new(),
            redacted_fields: Vec::new(),
            ux,
            span_ref: Some(span_of(view.span)),
        }),
    }
}

enum ExperienceViewKind {
    List,
    Detail,
    Create,
}

/// Map the experience-dialect `view <name> <type>` `<type>` keyword onto the
/// closed `ir::View` kind. The §7a UX primitives the rules validate
/// (`tab_group` / `wizard_steps` / `view.board` / `view_mode`) are list-only,
/// so the exact Detail-vs-List split only matters for form views — those carry
/// `submit`/`fields` and never the list-only UX.
fn classify_experience_view_kind(view_type: &str) -> ExperienceViewKind {
    match view_type.trim().to_ascii_lowercase().as_str() {
        "form" | "create" | "wizard" => ExperienceViewKind::Create,
        "detail" | "sidepanel" | "side_panel" | "panel" | "drawer" => ExperienceViewKind::Detail,
        _ => ExperienceViewKind::List,
    }
}

fn lower_lzx_app(app: &syntax::LzxApp) -> ir::AppManifest {
    let route_guard = app
        .route_guard
        .as_ref()
        .map(lower_route_guard_defaults)
        .or_else(|| {
            app.auth_failed_redirect
                .as_ref()
                .map(|redirect| ir::RouteGuardDefaults {
                    default_policy: None,
                    on_unauthenticated: Some(redirect.clone()),
                    on_unauthorized: None,
                    skeleton: None,
                    span_ref: Some(span_of(app.span)),
                })
        });

    ir::AppManifest {
        name: app.name.clone(),
        title: app.title.clone(),
        version: app.version.clone(),
        lazuli_version: None,
        targets: app.targets.clone(),
        default_locale: app.default_locale.clone(),
        default_timezone: app.default_timezone.clone(),
        auth_failed_redirect: app.auth_failed_redirect.clone(),
        not_found: app.not_found.clone(),
        error_pages: app
            .error_pages
            .iter()
            .map(|page| ir::ErrorPage {
                status: page.status,
                template: page.template.clone(),
                audience: page.audience.clone(),
            })
            .collect(),
        uses: app.uses.clone(),
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
        // ir-route-guards Cell IR-1 — slots wired by Cell PARSE-1.
        route_guard,
        actor_query: app.actor_query.clone(),
        span_ref: Some(span_of(app.span)),
    }
}

fn lower_lzx_route(route: &syntax::LzxRoute) -> ir::AppRoute {
    ir::AppRoute {
        name: route.name.clone(),
        path: route.path.clone(),
        routes: route.routes.clone(),
        route_params: route
            .route_params
            .iter()
            .map(|p| ir::RouteParam {
                name: p.name.clone(),
                type_ref: p.type_ref.clone(),
            })
            .collect(),
        to: route.to.clone(),
        surface: route.surface.clone(),
        audience: route.audience.clone(),
        lazy: route.lazy,
        prerender: route.prerender.clone(),
        // ir-route-guards Cell IR-1 — guard slot wired by Cell PARSE-1.
        guard: route.guard.as_ref().map(lower_view_guard),
        // router-w5 — loader declarations.
        loaders: route
            .loaders
            .iter()
            .map(|l| ir::RouteLoader {
                feature: l.feature.clone(),
                query: l.query.clone(),
                span_ref: Some(span_of(l.span)),
            })
            .collect(),
        pending_view: route.pending_view.clone(),
        error_view: route.error_view.clone(),
        parent: route.parent.clone(),
        span_ref: Some(span_of(route.span)),
    }
}

fn lower_experience(experience: &syntax::LzxExperience) -> ir::Experience {
    ir::Experience {
        name: experience.name.clone(),
        imports: experience.imports.clone(),
        views: experience.views.iter().map(lower_experience_view).collect(),
        resume_routers: experience
            .resume_routers
            .iter()
            .map(lower_resume_router)
            .collect(),
        extensions: experience
            .extensions
            .iter()
            .map(lower_view_extension)
            .collect(),
        span_ref: Some(span_of(experience.span)),
    }
}

fn lower_resume_router(router: &syntax::LzxResumeRouter) -> ir::ResumeRouter {
    ir::ResumeRouter {
        name: router.name.clone(),
        source_query: router.source_query.clone(),
        arms: router.arms.iter().map(lower_resume_arm).collect(),
        span_ref: Some(span_of(router.span)),
    }
}

fn lower_resume_arm(arm: &syntax::LzxResumeArm) -> ir::ResumeArm {
    ir::ResumeArm {
        kind: match &arm.kind {
            syntax::LzxResumeArmKind::State(state) => ir::ResumeArmKind::State(state.clone()),
            syntax::LzxResumeArmKind::None => ir::ResumeArmKind::None,
            syntax::LzxResumeArmKind::Wildcard => ir::ResumeArmKind::Wildcard,
        },
        substep: arm.substep.clone(),
        target_view: arm.target_view.clone(),
        span_ref: Some(span_of(arm.span)),
    }
}

fn lower_experience_view(view: &syntax::LzxExperienceView) -> ir::ExperienceView {
    ir::ExperienceView {
        name: view.name.clone(),
        anchor: view.anchor.clone(),
        routes: view.routes.clone(),
        extensible_by: view.extensible_by.clone(),
        source: view.source.clone(),
        submit: view.submit.clone(),
        blocks: view.blocks.clone(),
        actions: view.actions.iter().map(lower_experience_action).collect(),
        opens: view.opens.clone(),
        // Wave 4 — typed view test assertions. Surface AST and IR shapes
        // are 1:1 isomorphic, so this lowering preserves the closed
        // catalog (`allows extension` / `denies extension`) verbatim while
        // forwarding the source span for diagnostic surfaces.
        tests: view.tests.iter().map(lower_view_test_assertion).collect(),
        // ir-route-guards Cell IR-1 — guard slot wired by Cell PARSE-1.
        guard: view.guard.as_ref().map(lower_view_guard),
        resolved_guard_policy: None,
        resolved_lifecycle_gate: None,
        span_ref: Some(span_of(view.span)),
    }
}

fn lower_view_test_assertion(assertion: &syntax::LzxViewTestAssertion) -> ir::ViewTestAssertion {
    match assertion {
        syntax::LzxViewTestAssertion::AllowsExtension { feature, span } => {
            ir::ViewTestAssertion::AllowsExtension {
                feature: feature.clone(),
                span_ref: Some(span_of(*span)),
            }
        }
        syntax::LzxViewTestAssertion::DeniesExtension { feature, span } => {
            ir::ViewTestAssertion::DeniesExtension {
                feature: feature.clone(),
                span_ref: Some(span_of(*span)),
            }
        }
    }
}

fn lower_experience_action(action: &syntax::LzxAction) -> ir::ExperienceAction {
    ir::ExperienceAction {
        name: action.name.clone(),
        target: action.target.clone(),
        span_ref: Some(span_of(action.span)),
    }
}

fn lower_view_extension(extension: &syntax::LzxViewExtension) -> ir::ViewExtension {
    ir::ViewExtension {
        anchor: extension.anchor.clone(),
        blocks: extension.blocks.clone(),
        slots: extension
            .slots
            .iter()
            .map(lower_view_extension_slot)
            .collect(),
        span_ref: Some(span_of(extension.span)),
    }
}

fn lower_view_extension_slot(slot: &syntax::LzxExtensionSlot) -> ir::ViewExtensionSlot {
    ir::ViewExtensionSlot {
        name: slot.name.clone(),
        order: slot.order.as_ref().map(|order| ir::ViewExtensionOrder {
            relation: order.relation.clone(),
            target: order.target.clone(),
        }),
        blocks: slot.blocks.clone(),
        platforms: slot.platforms.clone(),
        audiences: slot.audiences.clone(),
        span_ref: Some(span_of(slot.span)),
    }
}

fn lower_platform_surface(surface: &syntax::LzxSurface) -> ir::PlatformSurface {
    ir::PlatformSurface {
        experience: surface.experience.clone(),
        platform: match surface.platform {
            syntax::LzxPlatform::Web => ir::Platform::Web,
            syntax::LzxPlatform::Mobile => ir::Platform::Mobile,
        },
        uses_experience: surface.uses_experience.clone(),
        audiences: surface
            .audiences
            .iter()
            .map(lower_audience_surface)
            .collect(),
        span_ref: Some(span_of(surface.span)),
    }
}

fn lower_audience_surface(audience: &syntax::LzxAudience) -> ir::AudienceSurface {
    ir::AudienceSurface {
        name: audience.name.clone(),
        qualifiers: audience.qualifiers.clone(),
        views: audience.views.iter().map(lower_platform_view).collect(),
        // ir-route-guards Cell IR-1 — guard slot wired by Cell PARSE-1.
        guard: audience.guard.as_ref().map(lower_view_guard),
        span_ref: Some(span_of(audience.span)),
    }
}

fn lower_platform_view(view: &syntax::LzxPlatformView) -> ir::PlatformView {
    ir::PlatformView {
        name: view.name.clone(),
        view_type: view.view_type.clone(),
        columns: view.columns.clone(),
        fields: view.fields.clone(),
        sections: view.sections.clone(),
        search: view.search.clone(),
        filter: view.filter.clone(),
        cells: view.cells.clone(),
        actions: view.actions.clone(),
        submit: view.submit.clone(),
        blocks: view.blocks.clone(),
        // ir-route-guards Cell IR-1 — guard slot wired by Cell PARSE-1.
        guard: view.guard.as_ref().map(lower_view_guard),
        resolved_guard_policy: None,
        span_ref: Some(span_of(view.span)),
    }
}

fn lower_view_guard(guard: &syntax::LzxViewGuard) -> ir::ViewGuard {
    ir::ViewGuard {
        policy: guard.policy.clone(),
        on_unauthenticated: guard.on_unauthenticated.clone(),
        on_unauthorized: guard.on_unauthorized.clone(),
        requires_lifecycle: guard
            .requires_lifecycle
            .as_ref()
            .map(lower_requires_lifecycle),
        on_lifecycle_pending: guard.on_lifecycle_pending.clone(),
        forbid_when: guard
            .forbid_when
            .iter()
            .filter_map(lower_forbid_when)
            .collect(),
        // ir-route-guard-escape-hatch-2026-05-28 §4.2 — Cell A IR slots.
        requires_lifecycle_in: guard.requires_lifecycle_in.as_ref().map(|rli| {
            ir::RequiresLifecycleIn {
                resource: rli.resource.clone(),
                allowed_states: rli.allowed_states.clone(),
                span_ref: Some(span_of(rli.span)),
            }
        }),
        requires_field: guard
            .requires_field
            .iter()
            .map(|rf| ir::RequiresField {
                feature: rf.feature.clone(),
                field: rf.field.clone(),
                expected: lower_scalar_literal(&rf.expected),
                on_unmet_redirect: rf.on_unmet_redirect.clone(),
                span_ref: Some(span_of(rf.span)),
            })
            .collect(),
        span_ref: Some(span_of(guard.span)),
    }
}

fn lower_requires_lifecycle(requires: &syntax::LzxRequiresLifecycle) -> ir::RequiresLifecycle {
    ir::RequiresLifecycle {
        resource: requires.resource.clone(),
        state: requires.state.clone(),
        substep: requires.substep.clone(),
        span_ref: Some(span_of(requires.span)),
    }
}

/// ir-route-guard-escape-hatch-2026-05-28 §4.2 — lift the parser
/// scalar-literal enum (`LzxScalarLiteral`) into the IR's
/// [`ir::DefaultValue`] envelope. Enum-literal defaults are not
/// emitted by route-guard parses (the surface admits only primitive
/// scalars per §4.1.1).
fn lower_scalar_literal(lit: &syntax::LzxScalarLiteral) -> ir::DefaultValue {
    match lit {
        syntax::LzxScalarLiteral::String(s) => ir::DefaultValue::String(s.clone()),
        syntax::LzxScalarLiteral::Integer(n) => ir::DefaultValue::Integer(*n),
        syntax::LzxScalarLiteral::Boolean(b) => ir::DefaultValue::Boolean(*b),
        syntax::LzxScalarLiteral::Null => ir::DefaultValue::Nil,
    }
}

/// router-w3 Tier 3 — lift `LzxForbidWhen` to `ir::ForbidWhen`,
/// resolving the `@<ns>.<name>` atom into a `PolicyAtom`. Returns
/// `None` for atoms that don't parse — silently drops; the analyzer's
/// existing atom validator passes will surface the invalid reference
/// against the source line.
fn lower_forbid_when(fw: &syntax::LzxForbidWhen) -> Option<ir::ForbidWhen> {
    let bare = fw.atom_ref.strip_prefix('@').unwrap_or(&fw.atom_ref);
    let (ns, rest) = bare.split_once('.')?;
    let name = rest.split('(').next().unwrap_or(rest);
    Some(ir::ForbidWhen {
        atom_ref: fw.atom_ref.clone(),
        atom: ir::PolicyAtom {
            namespace: ns.to_owned(),
            name: name.to_owned(),
            args: None,
        },
        dispatch_to: fw.dispatch_to.clone(),
        // ir-route-guard-escape-hatch-2026-05-28 §4.2 — Cell A IR slot.
        only_when_lifecycle: fw
            .only_when_lifecycle
            .as_ref()
            .map(lower_requires_lifecycle),
        span_ref: Some(span_of(fw.span)),
    })
}

fn lower_route_guard_defaults(defaults: &syntax::LzxRouteGuardDefaults) -> ir::RouteGuardDefaults {
    ir::RouteGuardDefaults {
        default_policy: defaults.default_policy.clone(),
        on_unauthenticated: defaults.on_unauthenticated.clone(),
        on_unauthorized: defaults.on_unauthorized.clone(),
        skeleton: defaults.skeleton.clone(),
        span_ref: Some(span_of(defaults.span)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_document_lowers_to_empty_module() {
        let doc = syntax::LzxDocument {
            app: None,
            routes: vec![],
            experiences: vec![],
            surfaces: vec![],
            span: syntax::Span { start: 0, end: 0 },
        };
        let module = lower_lzx_document(&doc);
        assert!(module.app.is_none());
        assert!(module.routes.is_empty());
        assert!(module.experiences.is_empty());
        assert!(module.surfaces.is_empty());
    }
}
