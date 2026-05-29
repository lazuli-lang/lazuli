//! Tests for `parse_app_profiles` — `profiles.lzi` deploy / URL /
//! integration overrides per environment. Lives alongside
//! `profiles.rs`.

#![cfg(test)]

use super::parse_app_profiles;

#[test]
fn parses_app_profiles() {
    let source = r#"
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  bindings
    customer_import.crm = integrations.fake_crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
    migrations before_deploy

profile production
  urls
    web "https://app.acme.example"
  integrations
    crm environment production
  deploy
    topology split_services
"#;

    let profiles = parse_app_profiles(source);

    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].name, "local");
    assert_eq!(profiles[0].urls[0].target, "web");
    assert_eq!(profiles[0].bindings[0].target_feature, "customer_import");
    assert_eq!(profiles[0].integrations[0].name, "crm");
    assert_eq!(
        profiles[0].integrations[0].environment.as_deref(),
        Some("sandbox")
    );
    assert_eq!(
        profiles[0].integrations[0].adapter.as_deref(),
        Some("@adapter.fake_crm")
    );
    assert_eq!(
        profiles[0].integrations[0].adapter_provenance.as_deref(),
        Some("local")
    );
    assert_eq!(
        profiles[0]
            .deploy
            .as_ref()
            .and_then(|deploy| deploy.topology.as_deref()),
        Some("monolith")
    );
    assert_eq!(profiles[1].name, "production");
    assert_eq!(
        profiles[1]
            .deploy
            .as_ref()
            .and_then(|deploy| deploy.topology.as_deref()),
        Some("split_services")
    );
}
