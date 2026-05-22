use lazuli_analyzer::lower_feature_skeleton;
use lazuli_ir::RouteRedirectTarget;
use lazuli_syntax::parse_feature_skeletons;

#[test]
fn policy_when_denied_route_lowers_to_ir() {
    let source = r#"
feature account
  policies
    host_only: @scope.authenticated, @role.host
      when_denied_route
        unauthenticated -> view sign_in
        role_mismatch traveler -> view explore
        default -> path "/welcome"
"#;

    let skeletons = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&skeletons[0]).expect("lowers");
    let policy = feature
        .policies
        .categories
        .iter()
        .find(|category| category.name == "host_only")
        .expect("host_only policy");
    let route = policy
        .when_denied_route
        .as_ref()
        .expect("when_denied_route lowers");

    assert_eq!(
        route.unauthenticated,
        Some(RouteRedirectTarget::View("sign_in".to_string()))
    );
    assert_eq!(route.role_mismatch[0].role, "traveler");
    assert_eq!(
        route.role_mismatch[0].target,
        RouteRedirectTarget::View("explore".to_string())
    );
    assert_eq!(
        route.default,
        Some(RouteRedirectTarget::Path("/welcome".to_string()))
    );
}
