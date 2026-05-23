use lazuli_codegen_ts::GeneratedFile;
use lazuli_codegen_ts::routes::{RoutesTarget, emit_routes_artifacts};
use lazuli_ir::{AppRoute, Experience, Platform, PlatformSurface};

fn fixture_routes() -> (Vec<AppRoute>, Vec<PlatformSurface>, Vec<Experience>) {
    (
        vec![
            AppRoute {
                name: "host_home".to_string(),
                path: Some("/host".to_string()),
                routes: Vec::new(),
                to: Some("host.view.host_home".to_string()),
                surface: Some("host web".to_string()),
                audience: Some("host".to_string()),
                lazy: None,
                prerender: None,
                guard: None,
                loaders: Vec::new(),
                span_ref: None,
            },
            AppRoute {
                name: "host_booking_detail".to_string(),
                path: Some("/host/bookings/:booking_id".to_string()),
                routes: Vec::new(),
                to: Some("host.view.host_booking_detail(id: route.booking_id)".to_string()),
                surface: Some("host web".to_string()),
                audience: Some("host".to_string()),
                lazy: None,
                prerender: None,
                guard: None,
                loaders: Vec::new(),
                span_ref: None,
            },
        ],
        vec![PlatformSurface {
            experience: "host".to_string(),
            platform: Platform::Web,
            uses_experience: None,
            audiences: Vec::new(),
            span_ref: None,
        }],
        vec![Experience {
            name: "host".to_string(),
            imports: Vec::new(),
            views: Vec::new(),
            resume_routers: Vec::new(),
            extensions: Vec::new(),
            span_ref: None,
        }],
    )
}

fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("missing generated file {path}; got {files:#?}"))
        .contents
        .as_str()
}

#[test]
fn routes_gen_tsx_emit_matches_golden() {
    let (routes, surfaces, experiences) = fixture_routes();
    let files = emit_routes_artifacts(None, &routes, &surfaces, &experiences, &[], RoutesTarget::Web);

    assert_eq!(files.len(), 1, "{files:#?}");
    assert_eq!(
        file(&files, "dist/ts-web/host/routes.gen.tsx"),
        include_str!("golden/routes/host.web.host.routes.gen.tsx")
    );
}
