//! Cell CODEGEN-1 — Integration golden tests for IR Error-Vocab emission.
//!
//! Drives the full emitter pipeline end-to-end on a hand-rolled IR
//! module that exercises every layer of the error-resolver wire:
//!
//! - **Layer 1 (per-command)**: a command with
//!   `policy_when_denied: Some(TranslationKeyRef)` — emitter produces
//!   a `var <cmd>ErrorKeys = lazuli.ErrorKeys{...}` literal in
//!   `<feature>/command.gen.go` and references it via the new
//!   `ErrorKeys:` kv row inside the `lazuli.Command[I, O]{...}` value.
//!
//! - **Layer 3 (per-feature)**: a feature with `errors: Some(FeatureErrors)`
//!   — emitter produces `<feature>/errors.gen.go` carrying
//!   `var FeatureErrors = lazuli.FeatureErrorContract{...}`.
//!
//! - **App-level registry**: when ≥ 1 feature declares `errors`, emitter
//!   produces `app/error_resolution.gen.go` with one
//!   `lazuli.RegisterFeatureErrors(...)` call per feature.
//!
//! Files compared as strings (not byte-equivalence) — the assertions
//! lock the load-bearing identifiers and shape; whitespace/banner
//! drift inside the generated file does not fail the test. Determinism
//! across runs is already covered by the in-crate unit tests.
//!
//! Runtime caveat (per Cell CODEGEN-1 brief): the generated Go
//! references `lazuli.ErrorKeys`, `lazuli.FeatureErrorContract`,
//! `lazuli.RegisterFeatureErrors`, `lazuli.ExposureHide`, and friends —
//! all introduced by Cell RUNTIME-1 which is landing in parallel.
//! These tests compare STRINGS only; we never invoke `go build` here.
//! Smoke tests that drive `go build` will fail until RUNTIME-1 lands;
//! that is expected and out of scope for this cell.

use lazuli_codegen_go::{GoEmitOptions, generate_v1};
use lazuli_ir::{
    Command, CommandEffect, CommandInput, CommandKind, Defaults, ErrorExposureDefault, Feature,
    FeatureErrorMessage, FeatureErrors, Module, Policies, PolicyRef, TranslationKeyRef,
};

fn empty_feature(name: &str) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        knowledge: None,
        defaults: Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
            rate_limit: None,
            audit: None,
        },
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: Policies {
            categories: Vec::new(),
            fields: Vec::new(),
            span_ref: None,
        },
        errors: None,
        commands: Vec::new(),
        apis: Vec::new(),
        records: Vec::new(),
        queries: Vec::new(),
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: None,
        pollers: vec![],
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: vec![],
        mcp_servers: vec![],
        previous_names: Vec::new(),
        span_ref: None,
        synth_origins: std::collections::BTreeMap::new(),
    }
}

fn empty_command(name: &str) -> Command {
    Command {
        name: name.to_owned(),
        public_contract: None,
        kind: CommandKind::Create,
        route: Vec::new(),
        input: CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: CommandEffect::None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: None,
        audit: None,
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        triggers: vec![],
        synthesized_from_cap_file: None,
        owner_scope_sql: None,
        previous_names: Vec::new(),
        span_ref: None,
        derived_from: None,
    }
}

fn module_with(features: Vec<Feature>) -> Module {
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        doctor_allows: Vec::new(),
        features,
    }
}

fn file_contents<'a>(files: &'a [lazuli_codegen_go::GeneratedFile], path: &str) -> Option<&'a str> {
    files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.contents.as_str())
}

/// Sanity baseline — a module with no `errors` blocks and no
/// `policy_when_denied` overrides must NOT emit `errors.gen.go` or
/// `app/error_resolution.gen.go`. Keeps the output listing
/// signal-rich; zero error customisation → zero emitted files.
#[test]
fn no_errors_block_means_no_error_vocab_files() {
    let mut account = empty_feature("account");
    account.commands.push(empty_command("create"));
    let module = module_with(vec![account]);

    let opts = GoEmitOptions {
        module_name: Some("lazuli/test-app".to_owned()),
        ..Default::default()
    };
    let files = generate_v1(&module, &opts);

    assert!(
        file_contents(&files, "account/errors.gen.go").is_none(),
        "feature without `errors` block must not emit errors.gen.go"
    );
    assert!(
        file_contents(&files, "app/error_resolution.gen.go").is_none(),
        "module without any errors block must not emit app/error_resolution.gen.go"
    );
}

/// Per-feature `errors` block → `<feature>/errors.gen.go` with the
/// lowered `lazuli.FeatureErrorContract` value carrying exposure rules
/// + per-code message overrides. App-level `init()` registers it with
/// the runtime resolver via `lazuli.RegisterFeatureErrors`.
#[test]
fn feature_errors_block_lowers_to_errors_gen_go_and_app_registry() {
    let mut account = empty_feature("account");
    account.errors = Some(FeatureErrors {
        default: Some(ErrorExposureDefault::Hide),
        exposure_4xx: vec!["message".to_owned(), "code".to_owned()],
        exposure_5xx: vec!["code".to_owned()],
        messages: vec![
            FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: TranslationKeyRef {
                    key: "account_signin_required".to_owned(),
                    span_ref: None,
                },
                span_ref: None,
            },
            FeatureErrorMessage {
                code: "tenant_mismatch".to_owned(),
                message: TranslationKeyRef {
                    key: "account_wrong_workspace".to_owned(),
                    span_ref: None,
                },
                span_ref: None,
            },
        ],
        field_messages: Vec::new(),
        audience_exposure: Vec::new(),
        redact_patterns: Vec::new(),
        span_ref: None,
    });
    let module = module_with(vec![account]);

    let opts = GoEmitOptions {
        module_name: Some("lazuli/test-app".to_owned()),
        ..Default::default()
    };
    let files = generate_v1(&module, &opts);

    // Per-feature `errors.gen.go` — exists and carries the contract.
    let errors_gen = file_contents(&files, "account/errors.gen.go")
        .expect("expected account/errors.gen.go to be emitted");
    assert!(
        errors_gen.contains("\npackage accountgen\n"),
        "errors.gen.go must declare `package accountgen`:\n{errors_gen}"
    );
    assert!(
        errors_gen.contains("\"lazuli.dev/runtime/lazuli\""),
        "errors.gen.go must import the runtime package:\n{errors_gen}"
    );
    assert!(
        errors_gen.contains("//lazuli:pattern error_resolver v1"),
        "errors.gen.go must carry the pattern header:\n{errors_gen}"
    );
    assert!(
        errors_gen.contains("var FeatureErrors = lazuli.FeatureErrorContract{"),
        "errors.gen.go must declare `var FeatureErrors`:\n{errors_gen}"
    );
    assert!(
        errors_gen.contains("Default:         lazuli.ExposureHide,"),
        "errors.gen.go must lower `default hide`:\n{errors_gen}"
    );
    assert!(
        errors_gen.contains("ExposeClient4xx: []string{\"message\", \"code\"},"),
        "errors.gen.go must lower 4xx exposure list:\n{errors_gen}"
    );
    assert!(
        errors_gen.contains("ExposeClient5xx: []string{\"code\"},"),
        "errors.gen.go must lower 5xx exposure list:\n{errors_gen}"
    );
    // BTreeMap sort: `policy_denied` < `tenant_mismatch`.
    assert!(
        errors_gen.contains(
            "\"policy_denied\": i18n.MessageRef{Feature: \"account\", Key: \"account_signin_required\"},"
        ),
        "errors.gen.go must lower policy_denied override as MessageRef:\n{errors_gen}"
    );
    assert!(
        errors_gen.contains(
            "\"tenant_mismatch\": i18n.MessageRef{Feature: \"account\", Key: \"account_wrong_workspace\"},"
        ),
        "errors.gen.go must lower tenant_mismatch override as MessageRef:\n{errors_gen}"
    );

    // App-level registry — exists and registers the feature.
    let app_resolution = file_contents(&files, "app/error_resolution.gen.go")
        .expect("expected app/error_resolution.gen.go when at least one feature declares errors");
    assert!(
        app_resolution.contains("\npackage app\n"),
        "error_resolution.gen.go must declare `package app`:\n{app_resolution}"
    );
    assert!(
        app_resolution.contains("//lazuli:pattern error_resolver v1"),
        "error_resolution.gen.go must carry the pattern header:\n{app_resolution}"
    );
    assert!(
        app_resolution.contains("func init() {"),
        "error_resolution.gen.go must emit init() block:\n{app_resolution}"
    );
    assert!(
        app_resolution
            .contains("lazuli.RegisterFeatureErrors(\"account\", accountgen.FeatureErrors)"),
        "error_resolution.gen.go must register the account feature:\n{app_resolution}"
    );
    assert!(
        app_resolution.contains("\"lazuli/test-app/account\""),
        "error_resolution.gen.go must import the feature pkg:\n{app_resolution}"
    );
}

/// Per-command `policy_when_denied` lowers to a per-command
/// `<cmd>ErrorKeys` literal in `command.gen.go` plus an `ErrorKeys:`
/// kv row inside the `lazuli.Command[I, O]{...}` value pointing at it.
/// Layer-1 of the runtime resolver chain (proposal §2.E step 1).
#[test]
fn command_policy_when_denied_lowers_to_error_keys_var() {
    let mut feature = empty_feature("account");
    let mut cmd = empty_command("choose_role");
    cmd.policy_when_denied = Some(TranslationKeyRef {
        key: "choose_role_signin_required".to_owned(),
        span_ref: None,
    });
    feature.commands.push(cmd);
    let module = module_with(vec![feature]);

    let opts = GoEmitOptions {
        module_name: Some("lazuli/test-app".to_owned()),
        ..Default::default()
    };
    let files = generate_v1(&module, &opts);

    let command_gen = file_contents(&files, "account/command.gen.go")
        .expect("expected account/command.gen.go to be emitted");

    // Per-command ErrorKeys literal emitted *before* the lazuli.Command
    // value so source order matches resolution order.
    assert!(
        command_gen.contains("//lazuli:pattern error_resolver v1"),
        "command.gen.go must carry the error_resolver pattern header:\n{command_gen}"
    );
    // The var name interleaves the effect-pinned resource pascal
    // (`Result` when `CommandEffect::None`) between the verb and
    // modifier words, mirroring `command_var_name` so the ErrorKeys
    // var name stays in lock-step with the Command var name.
    assert!(
        command_gen.contains("var chooseResultRoleErrorKeys = lazuli.ErrorKeys{"),
        "command.gen.go must declare the per-command ErrorKeys var:\n{command_gen}"
    );
    assert!(
        command_gen.contains(
            "PolicyDenied: i18n.MessageRef{Feature: \"account\", Key: \"choose_role_signin_required\"},"
        ),
        "command.gen.go must wire the policy_denied key reference as MessageRef:\n{command_gen}"
    );

    // The lazuli.Command value must reference the ErrorKeys var via
    // pointer so the runtime can read it at handler-construction time.
    assert!(
        command_gen.contains("ErrorKeys") && command_gen.contains("&chooseResultRoleErrorKeys"),
        "command.gen.go must reference &chooseResultRoleErrorKeys in the Command kv block:\n{command_gen}"
    );
}

/// Commands without `policy_when_denied` must NOT emit an `ErrorKeys`
/// var or `ErrorKeys:` kv row — codegen emits zero LOC for the
/// default-path commands per the LOC budget (proposal §4.4).
#[test]
fn command_without_override_emits_zero_error_keys_loc() {
    let mut feature = empty_feature("account");
    feature.commands.push(empty_command("create"));
    let module = module_with(vec![feature]);

    let opts = GoEmitOptions {
        module_name: Some("lazuli/test-app".to_owned()),
        ..Default::default()
    };
    let files = generate_v1(&module, &opts);

    let command_gen = file_contents(&files, "account/command.gen.go")
        .expect("expected account/command.gen.go to be emitted");

    // No ErrorKeys var, no ErrorKeys kv row, no error_resolver pattern.
    assert!(
        !command_gen.contains("ErrorKeys"),
        "command without policy_when_denied must emit zero ErrorKeys references:\n{command_gen}"
    );
    assert!(
        !command_gen.contains("error_resolver"),
        "command without policy_when_denied must not stamp the error_resolver pattern:\n{command_gen}"
    );
}

/// Mixed module: two features, one with `errors` block + one without;
/// one command with `policy_when_denied` + others without. Verifies
/// the orchestrator wires both surfaces independently and the
/// app-level registry only lists features that opt-in.
#[test]
fn mixed_module_emits_only_features_that_declare_errors() {
    let mut account = empty_feature("account");
    account.errors = Some(FeatureErrors {
        default: Some(ErrorExposureDefault::Expose),
        exposure_4xx: Vec::new(),
        exposure_5xx: Vec::new(),
        messages: Vec::new(),
        field_messages: Vec::new(),
        audience_exposure: Vec::new(),
        redact_patterns: Vec::new(),
        span_ref: None,
    });
    let mut billing = empty_feature("billing");
    billing.commands.push(empty_command("create_invoice"));
    let module = module_with(vec![account, billing]);

    let opts = GoEmitOptions {
        module_name: Some("lazuli/test-app".to_owned()),
        ..Default::default()
    };
    let files = generate_v1(&module, &opts);

    // Only `account` has an errors block → only `account/errors.gen.go`.
    assert!(
        file_contents(&files, "account/errors.gen.go").is_some(),
        "account/errors.gen.go must be emitted"
    );
    assert!(
        file_contents(&files, "billing/errors.gen.go").is_none(),
        "billing has no errors block; billing/errors.gen.go must be skipped"
    );

    // App-level registry registers only the opted-in feature.
    let app_resolution = file_contents(&files, "app/error_resolution.gen.go")
        .expect("app/error_resolution.gen.go must be emitted when any feature has errors");
    assert!(
        app_resolution.contains("lazuli.RegisterFeatureErrors(\"account\","),
        "app/error_resolution.gen.go must register the account feature:\n{app_resolution}"
    );
    assert!(
        !app_resolution.contains("lazuli.RegisterFeatureErrors(\"billing\","),
        "app/error_resolution.gen.go must not register billing (no errors block):\n{app_resolution}"
    );
}
