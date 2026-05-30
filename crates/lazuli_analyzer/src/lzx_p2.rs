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
