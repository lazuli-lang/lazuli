use lazuli_codegen_ts::playwright::{
    LifecycleSeeder, PlaywrightFixtureConfig, PlaywrightFixtureHelperImports,
    emit_playwright_fixtures,
};
use lazuli_ir::{AppRoute, Experience, Module, PlatformSurface};

fn feature(source: &str) -> lazuli_ir::Feature {
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
    lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("feature lowers")
}

fn module_fixture() -> Module {
    Module {
        workspace: None,
        contracts: vec![],
        app: None,
        registry: None,
        profiles: vec![],
        design: None,
        rbac: None,
        features: vec![
            feature(
                r#"
feature host
  domain
    resource Host
      name: Text required
      lifecycle lifecycle_state
        state intermediation_terms_pending initial
        state complete terminal
        transition complete
          from intermediation_terms_pending
          to complete

  policies
    host_only: @scope.authenticated, @role.host
"#,
            ),
            feature(
                r#"
feature traveler
  domain
    resource Traveler
      name: Text required
      lifecycle lifecycle_state
        state basic_details_pending initial
        state complete terminal
        transition complete
          from basic_details_pending
          to complete

  policies
    traveler_only: @scope.authenticated, @role.traveler
"#,
            ),
            feature(
                r#"
feature operations
  policies
    operator_only: @scope.authenticated, @role.operator
"#,
            ),
        ],
    }
}

fn route_fixture() -> Vec<AppRoute> {
    serde_json::from_value(serde_json::json!([
        {
            "name": "host_home",
            "path": "/host",
            "to": "host.view.home",
            "surface": "host web",
            "audience": "host"
        },
        {
            "name": "traveler_explore",
            "path": "/explore",
            "to": "traveler.view.explore",
            "surface": "traveler web",
            "audience": "traveler",
            "guard": { "policy": ["@policy.traveler_only"] }
        },
        {
            "name": "operator_queue",
            "path": "/ops",
            "to": "operations.view.queue",
            "surface": "operations web",
            "audience": "operator",
            "guard": { "policy": ["@policy.operator_only"] }
        }
    ]))
    .expect("valid route fixture")
}

fn surface_fixture() -> Vec<PlatformSurface> {
    serde_json::from_value(serde_json::json!([
        {
            "experience": "host",
            "platform": "web",
            "audiences": [
                {
                    "name": "host",
                    "guard": { "policy": ["@policy.host_only"] },
                    "views": [{ "name": "home", "view_type": "Detail" }]
                }
            ]
        }
    ]))
    .expect("valid surface fixture")
}

#[test]
fn playwright_fixtures_emit_matches_golden() {
    let config = PlaywrightFixtureConfig {
        helpers: Some(PlaywrightFixtureHelperImports {
            api_import: "./api".to_owned(),
            session_import: "./session".to_owned(),
            lifecycle_import: Some("./onboarding-progress".to_owned()),
            lifecycle_seeders: vec![
                LifecycleSeeder {
                    role: "host".to_owned(),
                    function_name: "progressHostTo".to_owned(),
                },
                LifecycleSeeder {
                    role: "traveler".to_owned(),
                    function_name: "progressTravelerTo".to_owned(),
                },
            ],
        }),
    };

    let out = emit_playwright_fixtures(
        &module_fixture(),
        &route_fixture(),
        &surface_fixture(),
        &[] as &[Experience],
        &config,
    );

    assert_eq!(
        out,
        include_str!("golden/playwright-fixtures/fixtures.gen.ts")
    );
}
