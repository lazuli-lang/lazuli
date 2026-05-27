//! Signature-aware Go test stub renderer.
//!
//! Reads the IR contract for the construct that references a
//! `@fn`/`@validator`/`@hook` handler and emits a table-driven Go test
//! stub seeded with enumerated cases drawn from the handler's
//! signature:
//!
//! - **Command-driven `@fn` referenced from `requires`**: boolean return
//!   type; cases = `golden path` (returns true) + `policy boundary`
//!   (returns false).
//! - **Command-driven `@fn` referenced from `effect`**: error return
//!   type; cases = `golden path` (nil error) + each authored emit
//!   predicate boundary.
//! - **Validator** (`Resource.validate` / `Resource.validates`): returns
//!   `*lazuli.ValidationError`; cases = `valid input` (nil) + one per
//!   field constraint boundary.
//! - **Lifecycle invariant handler**: returns boolean (invariant
//!   holds?); cases = `invariant holds` + `invariant violated`.
//! - **Job handler**: returns whatever the IR `returns` clause says;
//!   cases = `success path` + `error path`.
//! - **Anything else (unrecognized site)**: opaque single case.
//!
//! Every emitted case carries an `@TODO authored:` marker so a
//! follow-up rule (`TEST-HANDLER-STUB-001`) can later detect
//! abandoned scaffolds. v0.1 of Wave 5 emits the markers; the rule
//! itself is a follow-up cell.
//!
//! The stub uses **only Go standard library** (`testing`) — no
//! `testify`/`gomock`/etc. Authors extend the stub with whatever
//! library they prefer; the seed must compile on a fresh clone with
//! zero extra deps.
//!
//! Reference: docs/proposals/tdd-bdd-first-2026-05-23.md §5.4.

use lazuli_doctor::handler_walker::{HandlerSite, HandlerSiteKind};
use lazuli_ir::{Command, CommandEffect, Feature};

/// Aggregate of every input the renderer needs. Owned by the caller so
/// the function stays pure.
pub struct StubContext<'a> {
    pub feature: &'a Feature,
    pub site: &'a HandlerSite,
}

/// Render a table-driven Go test file body for the handler `site`.
/// Returns valid Go source ready to write to disk.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::signature_aware_stub::{render_test_stub, StubContext};
///
/// // let go_src = render_test_stub(&ctx);
/// ```
pub fn render_test_stub(ctx: &StubContext) -> String {
    let pkg = format!("{}handlers", ctx.feature.name);
    let fn_pascal = pascal_case(&ctx.site.handler_name);
    let cases = enumerate_cases(ctx);
    let signature_summary = describe_signature(ctx);
    let case_block = render_case_block(&cases);

    format!(
        r#"package {pkg}

// Auto-generated test stub for @{namespace}.{handler}.
// {signature}
//
// Replace each `@TODO authored:` marker with a real assertion. Each
// case is independent — table-driven so adding a boundary is one row.
//
// The stub depends only on the Go standard library so it compiles on a
// fresh clone. Per-pilot test libraries and `testsupport.Setup` are
// additive — drop them in after the first author pass if the case
// warrants it.

import (
	"testing"
)

func Test{fn_pascal}(t *testing.T) {{
{case_block}
}}
"#,
        pkg = pkg,
        namespace = ctx.site.handler_namespace,
        handler = ctx.site.handler_name,
        signature = signature_summary,
        fn_pascal = fn_pascal,
        case_block = case_block,
    )
}

/// One row in the table-driven test. `name` is the subtest label; the
/// `@TODO authored:` marker carries the boundary hint the author needs
/// to fill in.
struct TestCase {
    name: String,
    todo_hint: String,
}

fn enumerate_cases(ctx: &StubContext) -> Vec<TestCase> {
    match ctx.site.kind {
        HandlerSiteKind::CommandHandler => enumerate_command_cases(ctx),
        HandlerSiteKind::ResourceValidate | HandlerSiteKind::ResourceFieldValidate => {
            enumerate_validator_cases(ctx)
        }
        HandlerSiteKind::LifecycleInvariantHandler => vec![
            TestCase {
                name: "invariant holds".into(),
                todo_hint: "supply a row whose state satisfies the invariant; expect true".into(),
            },
            TestCase {
                name: "invariant violated".into(),
                todo_hint: "supply a row that breaks the invariant; expect false".into(),
            },
        ],
        HandlerSiteKind::JobHandler => vec![
            TestCase {
                name: "happy path".into(),
                todo_hint: "supply a typical payload + assert nil error".into(),
            },
            TestCase {
                name: "downstream failure".into(),
                todo_hint: "stub the failing dependency; assert wrapped error".into(),
            },
        ],
        HandlerSiteKind::WebhookHandler => vec![TestCase {
            name: "happy path".into(),
            todo_hint: "supply a verified payload; assert handler succeeds".into(),
        }],
    }
}

fn enumerate_command_cases(ctx: &StubContext) -> Vec<TestCase> {
    let cmd = find_command(ctx.feature, &ctx.site.construct_name);
    match cmd.map(|c| &c.effect) {
        Some(CommandEffect::Returns(_)) => vec![
            TestCase {
                name: "golden path".into(),
                todo_hint: "supply valid input; assert expected return value".into(),
            },
            TestCase {
                name: "policy boundary".into(),
                todo_hint: "supply input that should fail the requires-predicate; assert false / \
                            ErrPolicyDenied"
                    .into(),
            },
        ],
        Some(CommandEffect::Creates(_))
        | Some(CommandEffect::Updates(_))
        | Some(CommandEffect::Deletes(_)) => vec![
            TestCase {
                name: "golden path".into(),
                todo_hint: "supply valid input; assert handler returns nil + DB row matches"
                    .into(),
            },
            TestCase {
                name: "validation rejection".into(),
                todo_hint:
                    "supply input that should fail at least one validator; assert ValidationError"
                        .into(),
            },
            TestCase {
                name: "tenancy boundary".into(),
                todo_hint:
                    "supply a ctx whose tenant differs from the row's tenant; assert ErrTenancyMismatch"
                        .into(),
            },
        ],
        _ => vec![
            TestCase {
                name: "golden path".into(),
                todo_hint: "exercise the handler's success branch".into(),
            },
            TestCase {
                name: "error path".into(),
                todo_hint: "exercise at least one error branch".into(),
            },
        ],
    }
}

fn enumerate_validator_cases(ctx: &StubContext) -> Vec<TestCase> {
    // We don't yet introspect the per-field constraints (boundary
    // enumeration is a Wave 5.4 follow-up; v0.1 ships the
    // structurally-correct skeleton). Cases enumerate "valid" +
    // "invalid" + one targeted hint per field name that appears in
    // the construct path.
    let mut cases = vec![
        TestCase {
            name: "valid input".into(),
            todo_hint: "supply input that satisfies every constraint; expect nil".into(),
        },
        TestCase {
            name: "missing required field".into(),
            todo_hint: "supply input with the first required field zeroed; expect ValidationError"
                .into(),
        },
    ];
    if let Some((_, field)) = ctx.site.construct_name.split_once('.') {
        cases.push(TestCase {
            name: format!("field `{}` boundary", field),
            todo_hint: format!(
                "supply input that crosses the `{}` field's documented bound (length, range, \
                 regex); assert the validator pinpoints the field",
                field
            ),
        });
    }
    cases
}

fn find_command<'a>(feature: &'a Feature, name: &str) -> Option<&'a Command> {
    feature.commands.iter().find(|c| c.name == name)
}

fn render_case_block(cases: &[TestCase]) -> String {
    let mut s = String::new();
    s.push_str("\ttests := []struct {\n");
    s.push_str("\t\tname string\n");
    s.push_str("\t\t// @TODO authored: extend with handler-specific fields (input, want, etc.)\n");
    s.push_str("\t}{\n");
    for case in cases {
        s.push_str(&format!(
            "\t\t{{name: {name:?}}}, // @TODO authored: {hint}\n",
            name = case.name,
            hint = case.todo_hint,
        ));
    }
    s.push_str("\t}\n");
    s.push_str("\tfor _, tt := range tests {\n");
    s.push_str("\t\tt.Run(tt.name, func(t *testing.T) {\n");
    s.push_str("\t\t\t// @TODO authored: invoke the handler with tt's fields and assert.\n");
    s.push_str("\t\t\t_ = tt\n");
    s.push_str("\t\t})\n");
    s.push_str("\t}\n");
    s
}

fn describe_signature(ctx: &StubContext) -> String {
    let kind = match ctx.site.kind {
        HandlerSiteKind::CommandHandler => "command handler",
        HandlerSiteKind::ResourceValidate => "resource validator",
        HandlerSiteKind::ResourceFieldValidate => "field validator",
        HandlerSiteKind::LifecycleInvariantHandler => "lifecycle invariant handler",
        HandlerSiteKind::JobHandler => "job handler",
        HandlerSiteKind::WebhookHandler => "webhook handler",
    };
    format!(
        "Driven by {kind} on `{construct}` in feature `{feature}`.",
        kind = kind,
        construct = ctx.site.construct_name,
        feature = ctx.feature.name,
    )
}

/// Convert a `snake_case` identifier to `PascalCase`. Lazuli Go
/// codegen emits Pascal-cased handler/struct names, so this helper is
/// shared across the stub renderer and the public surface.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::signature_aware_stub::pascal_case;
/// assert_eq!(pascal_case("hello_world"), "HelloWorld");
/// ```
pub fn pascal_case(snake: &str) -> String {
    let mut out = String::new();
    for part in snake.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_doctor::handler_walker::{HandlerSite, HandlerSiteKind};
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, Policies,
        PolicyRef, ReturnsEffect, TypeRef,
    };

    fn mk_cmd(name: &str, effect: CommandEffect) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect,
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
            handler: None,
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

    fn site(kind: HandlerSiteKind, construct: &str, handler: &str) -> HandlerSite {
        HandlerSite {
            kind,
            feature_name: "post".into(),
            construct_name: construct.into(),
            handler_namespace: "fn".into(),
            handler_name: handler.into(),
        }
    }

    #[test]
    fn renders_table_driven_skeleton() {
        let feature = mk_feature(
            "post",
            vec![mk_cmd(
                "create_post",
                CommandEffect::Returns(ReturnsEffect {
                    return_type: TypeRef::Builtin(BuiltinType::Boolean),
                }),
            )],
        );
        let s = site(
            HandlerSiteKind::CommandHandler,
            "create_post",
            "validate_title",
        );
        let out = render_test_stub(&StubContext {
            feature: &feature,
            site: &s,
        });
        assert!(out.contains("package posthandlers"));
        assert!(out.contains("func TestValidateTitle("));
        assert!(out.contains("\"testing\""));
        assert!(out.contains("tests := []struct {"));
        assert!(out.contains("@TODO authored"));
        assert!(out.contains("golden path"));
        assert!(out.contains("policy boundary"));
        // No external library imports — fresh-clone safe.
        assert!(!out.contains("testify"));
        assert!(!out.contains("github.com/"));
    }

    #[test]
    fn validator_field_boundary_case_emitted() {
        let feature = mk_feature("post", vec![]);
        let s = site(
            HandlerSiteKind::ResourceFieldValidate,
            "Post.tier",
            "validate_tier",
        );
        let out = render_test_stub(&StubContext {
            feature: &feature,
            site: &s,
        });
        assert!(out.contains("field `tier` boundary"));
    }

    #[test]
    fn pascal_case_handles_multi_underscore() {
        assert_eq!(pascal_case("validate_post_title"), "ValidatePostTitle");
        assert_eq!(pascal_case("foo"), "Foo");
        assert_eq!(pascal_case("a_b_c"), "ABC");
    }
}
