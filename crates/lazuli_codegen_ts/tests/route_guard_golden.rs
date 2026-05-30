use lazuli_codegen_ts::GeneratedFile;
use lazuli_codegen_ts::lzx_audience_slot::{RouteGuardTarget, emit_route_guard_artifacts};
use lazuli_ir::{AppManifest, AppRoute, Experience, Feature, PlatformSurface};

fn fixture_feature() -> Feature {
    let source = r#"
feature host
  policies
    host_only: @scope.authenticated, @role.host
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
    lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("feature lowers")
}

fn fixture_app() -> AppManifest {
    serde_json::from_value(serde_json::json!({
        "name": "Acme",
        "route_guard": {
            "default_policy": "@scope.authenticated",
            "on_unauthenticated": "/sign-in",
            "on_unauthorized": "/explore",
            "skeleton": "@client.route_guard_skeleton"
        },
        "actor_query": "account.query.me"
    }))
    .expect("valid app manifest")
}

fn fixture_routes() -> Vec<AppRoute> {
    serde_json::from_value(serde_json::json!([
        {
            "name": "host_home",
            "path": "/host",
            "to": "host.view.home",
            "surface": "host web",
            "audience": "host"
        },
        {
            "name": "host_property_create",
            "path": "/host/properties/new",
            "to": "host.view.property_create",
            "surface": "host web",
            "audience": "host"
        },
        {
            "name": "explore",
            "path": "/explore",
            "to": "host.view.explore",
            "surface": "host web",
            "audience": "public"
        }
    ]))
    .expect("valid routes")
}

fn fixture_surfaces() -> Vec<PlatformSurface> {
    serde_json::from_value(serde_json::json!([
        {
            "experience": "host",
            "platform": "web",
            "audiences": [
                {
                    "name": "host",
                    "guard": {
                        "policy": ["@policy.host_only"],
                        "on_unauthenticated": "/host/sign-in",
                        "on_unauthorized": "/host"
                    },
                    "views": [
                        { "name": "home", "view_type": "Detail" },
                        {
                            "name": "property_create",
                            "view_type": "Form",
                            "guard": {
                                "policy": ["@policy.host_only"]
                            }
                        }
                    ]
                },
                {
                    "name": "public",
                    "views": [
                        { "name": "explore", "view_type": "List" }
                    ]
                }
            ]
        }
    ]))
    .expect("valid surfaces")
}

fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("missing generated file {path}"))
        .contents
        .as_str()
}

fn policy_list_golden(golden: &str) -> String {
    golden
        .replace("  policy: {\n", "  policy: [{\n")
        .replace(
            "\n  },\n  onUnauthenticated",
            "\n  }],\n  onUnauthenticated",
        )
        .replace("    policy: {\n", "    policy: [{\n")
        .replace(
            "\n    },\n    unauthenticated",
            "\n    }],\n    unauthenticated",
        )
}

#[test]
fn route_guard_emit_matches_golden() {
    let app = fixture_app();
    let routes = fixture_routes();
    let surfaces = fixture_surfaces();
    let feature = fixture_feature();

    let files = emit_route_guard_artifacts(
        Some(&app),
        &routes,
        &surfaces,
        &[] as &[Experience],
        &[feature],
        RouteGuardTarget::Web,
    );

    assert_eq!(files.len(), 3, "{files:#?}");
    assert_eq!(
        file(&files, "dist/ts-web/host/host.web.host.gen.ts"),
        policy_list_golden(include_str!("golden/route-guard/host.web.host.gen.ts"))
    );
    assert_eq!(
        file(&files, "dist/ts-web/host/host.web.public.gen.ts"),
        policy_list_golden(include_str!("golden/route-guard/host.web.public.gen.ts"))
    );
    assert_eq!(
        file(&files, "dist/ts-web/app/route-guards.gen.ts"),
        policy_list_golden(include_str!("golden/route-guard/route-guards.gen.ts"))
    );
}

#[test]
fn route_guard_emit_noops_without_guards_or_app_defaults() {
    let app: AppManifest = serde_json::from_value(serde_json::json!({
        "name": "Acme",
        "actor_query": "account.query.me"
    }))
    .expect("valid app manifest");
    let routes = fixture_routes();

    let files = emit_route_guard_artifacts(
        Some(&app),
        &routes,
        &[],
        &[] as &[Experience],
        &[fixture_feature()],
        RouteGuardTarget::Web,
    );

    assert!(files.is_empty());
}
