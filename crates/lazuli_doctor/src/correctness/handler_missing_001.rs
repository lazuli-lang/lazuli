//! HANDLER-MISSING-001 — handler referenced from `.lzi` has no `.go` file.
//!
//! Fulfills the CLAUDE.md:105 promise ("Doctor enforces" the
//! `<feature>/handlers/<name>.go` convention) that was **dormant** before
//! Wave 5 — sibling to the `VOCAB-TESTS-MISSING-001` dormant-promise case.
//!
//! Walks every site that carries a handler reference
//! (`Command.handler`, `Resource.validate`, `Resource.validates`,
//! `Resource.lifecycle.invariant_handlers`, `Job.body`, `Webhook.handler`)
//! and stats the canonical handler path:
//!
//! ```text
//! <app_dir>/features/<feature>/handlers/<name>.go
//! ```
//!
//! Fires when the file does not exist — emits a finding pointing at the
//! source `.lzi` so the LSP can squiggle the reference. Today `go build`
//! fails late with a cryptic import error; this rule catches it at
//! `lazuli check` / `lazuli doctor` time before any code generation runs.
//!
//! Severity: `warning` (prototype profile) / `error` (strict, production).
//! The dispatcher maps the profile; this module returns plain findings.
//!
//! Reference: docs/proposals/tdd-bdd-first-2026-05-23.md §5.3.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use crate::handler_path;
use crate::handler_walker::{HandlerSite, HandlerSiteKind, iter_handler_sites};

/// One HANDLER-MISSING-001 finding — an `@fn.* / @validator.* / @job.*`
/// reference points at a handler file that doesn't exist on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the reference was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Construct that holds the reference (command name, resource name,
    /// `<resource>.<field>` for field validators, etc.).
    pub construct_name: String,
    /// Kind of site that carried the reference — used by the message
    /// renderer to differentiate `command handler` vs `field validator`.
    pub site_kind: HandlerSiteKind,
    /// `fn`, `validator`, `job`, `webhook`, etc.
    pub handler_namespace: String,
    /// Handler symbol stem — `verify_password` for `@fn.verify_password`.
    pub handler_name: String,
    /// Canonical path the doctor expected to find. Surfaced verbatim so
    /// authors can copy-paste it into a shell.
    pub expected_path: PathBuf,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "HANDLER-MISSING-001";

    /// Render the "handler file missing" message naming the expected
    /// path and the canonical `lazuli generate handler` scaffold cmd.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::handler_missing_001::Finding;
    /// use lazuli_doctor::handler_walker::HandlerSiteKind;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("auth.lzi"),
    ///     feature: "auth".into(),
    ///     construct_name: "register".into(),
    ///     site_kind: HandlerSiteKind::CommandHandler,
    ///     handler_namespace: "fn".into(),
    ///     handler_name: "verify_password".into(),
    ///     expected_path: PathBuf::from("features/auth/handlers/verify_password.go"),
    /// };
    /// assert!(f.message().contains("lazuli generate handler"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "@{}.{} referenced from {} `{}` but handler file is missing at {}. \
             Run `lazuli generate handler {}.{}` to scaffold the pair.",
            self.handler_namespace,
            self.handler_name,
            self.site_kind.describe(),
            self.construct_name,
            self.expected_path.display(),
            self.feature,
            self.handler_name,
        )
    }
}

/// Walk `feature` for missing handler files. `lzi_path` anchors the
/// resulting findings (so the LSP / CLI can format `<file>:<line>:` lines).
/// `app_root` is the directory containing `features/` — resolved via the
/// `Lazurite.toml [lazurite].app_dir` knob on the caller side.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::handler_missing_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with handler refs");
/// let _ = check(&feature, Path::new("auth.lzi"), Path::new("/app"));
/// ```
pub fn check(feature: &Feature, lzi_path: &Path, app_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for site in iter_handler_sites(feature) {
        if !is_handler_namespace(&site) {
            continue;
        }
        let expected = handler_path::resolve(app_root, &feature.name, &site.handler_name);
        if !expected.exists() {
            findings.push(Finding {
                path: lzi_path.to_path_buf(),
                feature: feature.name.clone(),
                construct_name: site.construct_name,
                site_kind: site.kind,
                handler_namespace: site.handler_namespace,
                handler_name: site.handler_name,
                expected_path: expected,
            });
        }
    }
    findings
}

/// Only namespaces this rule cares about. v0.1 conservatively limits
/// the scope to:
///
/// - `CommandHandler` with `@fn.<name>` — the canonical
///   `features/<f>/handlers/<name>.go` convention that CLAUDE.md:105
///   promises Doctor would enforce.
/// - `LifecycleInvariantHandler` with `@fn.<name>` — same shape, same
///   directory.
///
/// Excluded for v0.1 (follow-up cells):
/// - `ResourceValidate` / `ResourceFieldValidate` — analyzer carries
///   these as symbolic `@validator.<name>` references resolved through
///   `extensions`, not concrete file paths. Path-convention rule needs
///   to traverse the extensions index first.
/// - `JobHandler` — jobs may live under `features/<f>/jobs/` rather
///   than `handlers/`. Per-job convention rule is a follow-up.
/// - `WebhookHandler` — webhooks live under `features/<f>/webhooks/`.
fn is_handler_namespace(site: &HandlerSite) -> bool {
    match site.kind {
        // `CommandBindingFnHook` is the secondary `@fn` hook on a
        // declarative-body command (e.g. `updates X { ... } handler
        // @fn.hook`). It self-registers a Go file under the same
        // `handlers/<name>.go` convention, so HANDLER-MISSING-001 still
        // gates on its presence — only HANDLER-SIGNATURE-MISMATCH-001 skips
        // asserting it against the command's `Command[I, O]`.
        HandlerSiteKind::CommandHandler
        | HandlerSiteKind::CommandBindingFnHook
        | HandlerSiteKind::LifecycleInvariantHandler => site.handler_namespace == "fn",
        _ => false,
    }
}

#[cfg(test)]
mod tests {
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

    fn mk_feature(commands: Vec<Command>) -> Feature {
        Feature {
            name: "post".into(),
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

    #[test]
    fn positive_missing_handler_fires() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path();
        // No handler file written → rule must fire.
        let feature = mk_feature(vec![mk_cmd_with_handler("create_post", "validate_title")]);
        let findings = check(&feature, Path::new("features/post/post.lzi"), app_root);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].handler_name, "validate_title");
        assert_eq!(findings[0].construct_name, "create_post");
        assert!(findings[0].message().contains("@fn.validate_title"));
        assert!(findings[0].message().contains("lazuli generate handler"));
        assert_eq!(Finding::CODE, "HANDLER-MISSING-001");
    }

    #[test]
    fn negative_handler_present_does_not_fire() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path();
        // Write the handler file at the canonical path.
        let dir = handler_path::handlers_dir(app_root, "post");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("validate_title.go"), "package posthandlers").unwrap();

        let feature = mk_feature(vec![mk_cmd_with_handler("create_post", "validate_title")]);
        let findings = check(&feature, Path::new("features/post/post.lzi"), app_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn negative_command_without_handler_skipped() {
        let tmp = TempDir::new().unwrap();
        let mut cmd = mk_cmd_with_handler("create_post", "validate_title");
        cmd.handler = None;
        let feature = mk_feature(vec![cmd]);
        let findings = check(&feature, Path::new("features/post/post.lzi"), tmp.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn webhook_handlers_skipped() {
        // Webhook handlers live in `webhooks/` not `handlers/`. v0.1
        // rule scope explicitly excludes them so we don't double-fire
        // on webhook-shaped paths (covered by a sibling follow-up
        // cell).
        let mut feature = mk_feature(vec![]);
        feature.webhooks.push(lazuli_ir::Webhook {
            name: "stripe_invoice_paid".into(),
            route: "/webhooks/stripe/invoice-paid".into(),
            verify: lazuli_ir::PathRef::authored("./webhooks/verify_stripe.go".to_owned()),
            structured_verify: None,
            tenant_from: None,
            scope_global: None,
            idempotency: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            handler: lazuli_ir::PathRef::authored("./webhooks/handle_invoice_paid.go".to_owned()),
            returns: None,
            emits: vec![],
            emit_predicates: vec![],
            payload_from: None,
            replay: None,
            dlq: None,
            retry: None,
            previous_names: vec![],
            span_ref: None,
        });

        let tmp = TempDir::new().unwrap();
        let findings = check(&feature, Path::new("features/post/post.lzi"), tmp.path());
        assert!(findings.is_empty());
    }
}
