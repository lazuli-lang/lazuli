
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, HandlerRef,
        Policies, PolicyRef, ReturnsEffect, TypeRef,
    };
    use std::fs;
    use tempfile::TempDir;

    fn mk_cmd_with_handler(name: &str, handler_name: &str) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(BuiltinType::Boolean),
            }),
            policy: PolicyRef::None,
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
            handler: Some(HandlerRef {
                namespace: "fn".into(),
                name: handler_name.to_owned(),
                span_ref: None,
            }),
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn mk_feature(name: &str, commands: Vec<Command>) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands,
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
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    /// Write a handler `.go` and a `command.gen.go` under the canonical
    /// `<app_root>/features/<feature>/handlers/` and
    /// `<dist_root>/go/<feature>/` paths.
    fn lay_out_files(
        app_root: &Path,
        dist_root: &Path,
        feature: &str,
        handler_name: &str,
        handler_source: &str,
        gen_source: &str,
    ) {
        let handler_dir = handler_path::handlers_dir(app_root, feature);
        fs::create_dir_all(&handler_dir).unwrap();
        fs::write(
            handler_dir.join(format!("{handler_name}.go")),
            handler_source,
        )
        .unwrap();

        let gen_dir = dist_root.join("go").join(feature);
        fs::create_dir_all(&gen_dir).unwrap();
        fs::write(gen_dir.join("command.gen.go"), gen_source).unwrap();
    }

    fn write_lzi(lzi_path: &Path, source: &str) {
        if let Some(parent) = lzi_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(lzi_path, source).unwrap();
    }

    const MATCHING_HANDLER: &str = r#"package accounthandlers

import (
    "lazuli.dev/runtime/lazuli"
    accountgen "github.com/example/account"
)

func Login(ctx *lazuli.Ctx, input accountgen.LoginInput) (string, error) {
    return "token", nil
}
"#;

    const MATCHING_GEN: &str = r#"package accountgen

import "lazuli.dev/runtime/lazuli"

var login = lazuli.Command[LoginInput, string]{
    Name: "account.login",
    Effect: lazuli.ReturnsFromRegistry[LoginInput, string]("account.login"),
}
"#;

    #[test]
    fn happy_path_matching_signatures_silent() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            MATCHING_HANDLER,
            MATCHING_GEN,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(
            findings.is_empty(),
            "expected no findings, got {:?}",
            findings
        );
        assert_eq!(Finding::CODE, "HANDLER-SIGNATURE-MISMATCH-001");
    }

    #[test]
    fn output_mismatch_string_vs_struct_fires() {
        // The hostpoint Google-OAuth bug verbatim: handler returns
        // (string, error) but codegen emitted Command[..., struct{}].
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
import "lazuli.dev/runtime/lazuli"
func LoginWithGoogle(ctx *lazuli.Ctx, input accountgen.LoginResultWithGoogleInput) (string, error) {
    return "token", nil
}
"#;
        let gen_src = r#"package accountgen
var loginResultWithGoogle = lazuli.Command[LoginResultWithGoogleInput, struct{}]{
    Name: "account.login_with_google",
}
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login_with_google",
            handler_src,
            gen_src,
        );

        let feature = mk_feature(
            "account",
            vec![mk_cmd_with_handler(
                "login_with_google",
                "login_with_google",
            )],
        );
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1, "got: {:?}", findings);
        match &findings[0].diff {
            Diff::OutputMismatch { expected, found } => {
                assert_eq!(expected, "struct{}");
                assert_eq!(found, "string");
            }
            other => panic!("expected OutputMismatch, got {:?}", other),
        }
        let msg = findings[0].message();
        assert!(msg.contains("handler_registry.go:89"), "msg = {msg}");
        assert!(msg.contains("login_with_google"), "msg = {msg}");
    }

    #[test]
    fn input_mismatch_fires() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
import "lazuli.dev/runtime/lazuli"
func Login(ctx *lazuli.Ctx, input OtherInput) (string, error) {
    return "x", nil
}
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[ExpectedInput, string]{
    Name: "account.login",
}
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1);
        match &findings[0].diff {
            Diff::InputMismatch { expected, found } => {
                assert_eq!(expected, "ExpectedInput");
                assert_eq!(found, "OtherInput");
            }
            other => panic!("expected InputMismatch, got {:?}", other),
        }
    }

    #[test]
    fn both_sides_drift_fires_with_both_variant() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
func Login(ctx *lazuli.Ctx, input WrongInput) (WrongOutput, error) {
    return WrongOutput{}, nil
}
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[RightInput, RightOutput]{
    Name: "account.login",
}
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1);
        match &findings[0].diff {
            Diff::Both {
                input_expected,
                input_found,
                output_expected,
                output_found,
            } => {
                assert_eq!(input_expected, "RightInput");
                assert_eq!(input_found, "WrongInput");
                assert_eq!(output_expected, "RightOutput");
                assert_eq!(output_found, "WrongOutput");
            }
            other => panic!("expected Both, got {:?}", other),
        }
    }
