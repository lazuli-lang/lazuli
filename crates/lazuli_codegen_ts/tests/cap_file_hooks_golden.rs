use lazuli_codegen_ts::emit_cap_file_hooks_ts;

#[test]
fn cap_file_hook_emits_upload_orchestration_for_profile_photo() {
    let feature = lower_feature(
        r#"feature host
  defaults
    tenancy org

  uses org
  uses account

  policies
    host_only: @scope.authenticated, @role.host

  domain
    resource Host
      org: Org required
      user: User required unique
      profile_photo: @cap.File(max_size:5mb,accept:image/jpeg|image/png,visibility:signed,signed_ttl:1h) optional
"#,
    );

    let actual = emit_cap_file_hooks_ts(&feature).expect("cap file hook emitted");
    let expected = include_str!("golden/cap-file-hooks/host.react.gen.ts");
    assert_eq!(actual, expected);
}

fn lower_feature(source: &str) -> lazuli_ir::Feature {
    let parsed = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
    lazuli_analyzer::lower_feature_skeleton(&parsed[0]).expect("feature lowers")
}
