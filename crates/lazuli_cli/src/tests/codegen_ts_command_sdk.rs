    // Codegen-TS command/SDK metadata tests — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use super::test_support::*;
    use crate::emit_feature_sdk_ts;

    #[test]
    fn command_sdk_emits_policy_rate_limit_audit_metadata() {
        // Regression for review bug #7 (2026-05-15): the TS SDK
        // previously emitted only `invalidates:` on `defineCommand`,
        // losing the Go-side Policy/RateLimit/Audit. Clients had to
        // call a separate metadata RPC (which didn't exist) to drive
        // policy-aware affordances or rate-limit-aware backoff.
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.policies = lazuli_ir::Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@role.admin".to_owned(), "@role.sales".to_owned()],
                conditional_atoms: vec![],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            fields: vec![],
            span_ref: None,
        };
        feature.commands.push(lazuli_ir::Command {
            name: "update_item".to_owned(),
            public_contract: None,
            kind: lazuli_ir::CommandKind::Update,
            route: vec![],
            input: lazuli_ir::CommandInput::Typed(vec![]),
            target: None,
            lets: vec![],
            effect: lazuli_ir::CommandEffect::None,
            policy: lazuli_ir::PolicyRef::Atom("policy.update".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: Some(lazuli_ir::RateLimitSpec::from_default(
                "30 per hour per user".to_owned(),
            )),
            audit: Some(lazuli_ir::AuditSpec {
                subjects: vec![],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
                materialize: None,
            }),
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        });
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("policy: { name: \"@policy.update\", atoms: ["),
            "policy name must qualify with @policy. prefix; got:\n{output}"
        );
        assert!(
            output.contains("{ namespace: \"role\", name: \"admin\" }"),
            "policy atoms must resolve via feature.policies dictionary; got:\n{output}"
        );
        assert!(
            output.contains("{ namespace: \"role\", name: \"sales\" }"),
            "all atoms from the matching category must be emitted; got:\n{output}"
        );
        assert!(
            output.contains("rateLimit: \"30 per hour per user\""),
            "rateLimit must surface to the TS SDK; got:\n{output}"
        );
        assert!(
            output.contains("audit: \"default\""),
            "empty-subject AuditSpec must lower to the \"default\" sentinel; got:\n{output}"
        );
    }

    #[test]
    fn command_sdk_omits_metadata_when_absent() {
        // Counterpoint: when the DSL omits a piece of metadata the SDK
        // must omit the property entirely rather than emit it as
        // `undefined` (TS `exactOptionalPropertyTypes` discipline).
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.commands.push(lazuli_ir::Command {
            name: "bare".to_owned(),
            public_contract: None,
            kind: lazuli_ir::CommandKind::Update,
            route: vec![],
            input: lazuli_ir::CommandInput::Typed(vec![]),
            target: None,
            lets: vec![],
            effect: lazuli_ir::CommandEffect::None,
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        });
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            !output.contains("policy:"),
            "expected no policy line; got:\n{output}"
        );
        assert!(
            !output.contains("rateLimit:"),
            "expected no rateLimit line; got:\n{output}"
        );
        assert!(
            !output.contains("audit:"),
            "expected no audit line; got:\n{output}"
        );
        // invalidates is always emitted even when empty — that's the
        // existing contract that this test does not change.
        assert!(output.contains("invalidates: []"));
    }

    #[test]
    fn cap_file_request_upload_emits_command_spec_for_react_hook() {
        // Wave C.2 upload hooks call request_*_upload through
        // useLazuliCommand because minting a signed PUT URL is an
        // imperative upload step, not a cacheable read. The get-url
        // command remains query-shaped so the hook can expose photoUri
        // from TanStack Query state.
        let source = r#"feature host
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
      profile_photo: @cap.File(max_size:5mb,accept:image/jpeg,visibility:signed,signed_ttl:1h) optional
"#;
        let parsed = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
        let feature = lazuli_analyzer::lower_feature_skeleton(&parsed[0]).expect("feature lowers");
        let module = lazuli_ir::Module {
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

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains(
                "export const requestHostProfilePhotoUpload = defineCommand<RequestHostProfilePhotoUploadInput, ProfilePhotoUploadIntent>(\"host.request_profile_photo_upload\", {"
            ),
            "request upload must remain a CommandSpec for useLazuliCommand; got:\n{output}"
        );
        assert!(
            output.contains(
                "export const getHostProfilePhotoURL = defineQuery<GetHostProfilePhotoURLInput, ProfilePhotoDisplayUrl>(\"host.get_profile_photo_url\");"
            ),
            "get-url stays query-shaped for photoUri cache state; got:\n{output}"
        );
    }
