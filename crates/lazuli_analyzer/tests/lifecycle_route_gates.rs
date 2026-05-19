use lazuli_analyzer::lower_lzx_document;
use lazuli_ir::ResumeArmKind;
use lazuli_syntax::parse_lzx_document;
use serde_json::json;

fn lower(source: &str) -> lazuli_ir::ExperienceModule {
    let document = parse_lzx_document(source).expect("lzx source parses");
    lower_lzx_document(&document)
}

#[test]
fn resume_router_and_lifecycle_guard_project_to_ir_json() {
    let module = lower(
        r#"
experience host
  imports host

  view host_home
    policy @policy.host_only
    requires_lifecycle Host = complete
    on_lifecycle_pending @resume host_onboarding
    source host.query.lookup.my_host

  view host_onboarding_intermediation
    policy @policy.host_only
    source host.query.lookup.my_host

  resume host_onboarding
    source query.lookup my_host
    none -> view host_onboarding_intermediation
    intermediation_terms_pending → view host_onboarding_intermediation
    complete -> view host_home
    * → view host_onboarding_intermediation
"#,
    );

    let experience = &module.experiences[0];
    let router = &experience.resume_routers[0];
    let guard = experience.views[0]
        .guard
        .as_ref()
        .expect("view guard lowers");

    assert_eq!(router.name, "host_onboarding");
    assert_eq!(router.source_query, "my_host");
    assert_eq!(router.arms[0].kind, ResumeArmKind::None);
    assert_eq!(
        router.arms[1].kind,
        ResumeArmKind::State("intermediation_terms_pending".to_string())
    );
    assert_eq!(router.arms[3].kind, ResumeArmKind::Wildcard);
    assert_eq!(
        guard.requires_lifecycle.as_ref().map(|requires| {
            json!({
                "resource": requires.resource.as_str(),
                "state": requires.state.as_str(),
            })
        }),
        Some(json!({ "resource": "Host", "state": "complete" }))
    );
    assert_eq!(
        guard.on_lifecycle_pending.as_deref(),
        Some("host_onboarding")
    );

    let json = serde_json::to_value(module).expect("ir serializes");
    assert_eq!(
        json["experiences"][0]["resume_routers"][0]["name"],
        "host_onboarding"
    );
    assert_eq!(
        json["experiences"][0]["views"][0]["guard"]["requires_lifecycle"]["resource"],
        "Host"
    );
    assert_eq!(
        json["experiences"][0]["views"][0]["guard"]["on_lifecycle_pending"],
        "host_onboarding"
    );
}

#[test]
fn unicode_and_ascii_resume_arrows_lower_to_same_ir() {
    let ascii = lower(
        r#"
experience host
  imports host

  view pending
    source host.query.lookup.my_host

  resume host_onboarding
    source query.lookup my_host
    none -> view pending
    complete -> view pending
    * -> view pending
"#,
    );
    let unicode = lower(
        r#"
experience host
  imports host

  view pending
    source host.query.lookup.my_host

  resume host_onboarding
    source query.lookup my_host
    none → view pending
    complete → view pending
    * → view pending
"#,
    );

    assert_eq!(resume_arm_semantics(&ascii), resume_arm_semantics(&unicode));
}

fn resume_arm_semantics(module: &lazuli_ir::ExperienceModule) -> Vec<(ResumeArmKind, String)> {
    module.experiences[0].resume_routers[0]
        .arms
        .iter()
        .map(|arm| (arm.kind.clone(), arm.target_view.clone()))
        .collect()
}
