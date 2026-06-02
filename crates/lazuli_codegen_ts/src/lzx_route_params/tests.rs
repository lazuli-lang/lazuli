//! Route-param emitter tests — kept verbatim from the original
//! `lzx_route_params.rs` inline test module.

use super::*;

#[test]
fn emits_mixed_route_params_golden() {
    let feature = feature_fixture();
    let module = ir::Module {
        workspace: None,
        contracts: vec![],
        app: None,
        registry: None,
        profiles: vec![],
        design: None,
        rbac: None,
        doctor_allows: Vec::new(),
        features: vec![feature.clone()],
    };
    let out = emit_route_params_ts(&feature, &module, "ts-web").expect("routes generated");

    assert_eq!(
        out,
        include_str!("../../tests/golden/route-params/host.routes.gen.ts")
    );
}

fn feature_fixture() -> ir::Feature {
    ir::Feature {
        name: "host".to_owned(),
        purpose: None,
        non_goals: vec![],
        context_path: None,
        knowledge: None,
        defaults: ir::Defaults::default(),
        uses: vec![],
        uses_spans: vec![],
        uses_versions: vec![],
        requirements: vec![],
        enums: vec![ir::EnumDecl {
            name: "ServiceKind".to_owned(),
            public_contract: None,
            variants: vec![
                ir::EnumVariant {
                    name: "Cleaning".to_owned(),
                    storage_value: Some(ir::StorageValue::String("cleaning".to_owned())),
                    previous_names: vec![],
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                },
                ir::EnumVariant {
                    name: "Maintenance".to_owned(),
                    storage_value: None,
                    previous_names: vec![],
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                },
            ],
            previous_names: vec![],
            span_ref: None,
        }],
        resources: vec![],
        events: vec![],
        rules: vec![],
        policies: ir::Policies::default(),
        errors: None,
        commands: vec![],
        apis: vec![],
        records: vec![],
        queries: vec![],
        resume_routers: vec![],
        workflows: vec![],
        jobs: vec![],
        webhooks: vec![],
        notifications: vec![],
        event_groups: vec![],
        tenant_migrations: vec![],
        translation: None,
        pollers: vec![],
        auth: None,
        surfaces: vec![surface_fixture()],
        extensions: vec![],
        escape_routes: vec![],
        agents: vec![],
        reports: vec![],
        channels: vec![],
        caches: vec![],
        aggregates: vec![],
        mcp_servers: vec![],
        previous_names: vec![],
        span_ref: None,
        synth_origins: BTreeMap::new(),
    }
}

fn surface_fixture() -> ir::Surface {
    ir::Surface {
        feature: "host".to_owned(),
        target: ir::SurfaceTarget::Web,
        audiences: vec![ir::Audience {
            name: "host".to_owned(),
            requires: vec![],
            views: vec![ir::View::Detail(ir::ViewDetail {
                name: "host_service_edit".to_owned(),
                route: Some(
                    "/host/property/:property_id/service/:service_id/:kind/:starts_at".to_owned(),
                ),
                source: ir::QueryRef {
                    feature: "host".to_owned(),
                    kind: ir::QueryKind::Lookup,
                    name: "by_service".to_owned(),
                },
                route_params: vec![
                    route_param("property_id", "Host.ID"),
                    route_param("service_id", "ID"),
                    route_param("kind", "ServiceKind"),
                    route_param("starts_at", "DateTime"),
                    route_param("day", "Date"),
                    route_param("page", "Integer"),
                    route_param("featured", "Boolean"),
                    route_param("amount", "Decimal"),
                    route_param("slug", "Text"),
                ],
                sections: vec![],
                cells: vec![],
                actions: vec![],
                redacted_fields: vec![],
                ux: Default::default(),
                span_ref: None,
            })],
            ux: Default::default(),
            span_ref: None,
        }],
        span_ref: None,
    }
}

fn route_param(name: &str, type_ref: &str) -> ir::RouteParam {
    ir::RouteParam {
        name: name.to_owned(),
        type_ref: type_ref.to_owned(),
    }
}
