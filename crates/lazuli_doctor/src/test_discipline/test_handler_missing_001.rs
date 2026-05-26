//! TEST-HANDLER-MISSING-001 — handler exists, but its paired
//! `<name>_test.go` does not.
//!
//! Twin of [`crate::correctness::handler_missing_001`]. The pair-rule
//! discipline (handler `.go` ↔ handler `_test.go` co-located in
//! `<feature>/handlers/`) is what makes Lazuli's TDD/BDD default real:
//! `lazuli generate handler` emits BOTH files, and either file going
//! missing is a doctor finding.
//!
//! Avoids double-fire with HANDLER-MISSING-001 by checking the
//! presence of `<name>.go` first — if the handler itself is missing,
//! demanding a test is incoherent. The author needs to scaffold the
//! handler first; the test rule then re-fires on the next pass once
//! the `.go` arrives.
//!
//! Severity: `info` (prototype) / `warning` (strict) / `error`
//! (production). The dispatcher maps the profile; this module returns
//! plain findings.
//!
//! Reference: docs/proposals/tdd-bdd-first-2026-05-23.md §5.3.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use crate::handler_path;
use crate::handler_walker::{HandlerSite, HandlerSiteKind, iter_handler_sites};

/// One TEST-HANDLER-MISSING-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` source path that references the handler.
    pub path: PathBuf,
    /// Feature owning the handler reference.
    pub feature: String,
    /// Construct name that references the handler (command/validator/etc).
    pub construct_name: String,
    /// Handler site kind, used to render the diagnostic context.
    pub site_kind: HandlerSiteKind,
    /// `fn`, `validator`, `job`, `webhook`, etc.
    pub handler_namespace: String,
    /// Bare handler stem (no `.go` suffix).
    pub handler_name: String,
    /// Existing handler `.go` file path.
    pub handler_path: PathBuf,
    /// Canonical `_test.go` path the rule expects to exist.
    pub expected_test_path: PathBuf,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-HANDLER-MISSING-001";

    /// Render the user-facing diagnostic body — points to the existing
    /// handler and the expected `_test.go` companion file.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::handler_walker::HandlerSiteKind;
    /// use lazuli_doctor::test_discipline::test_handler_missing_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("post.lzi"),
    ///     feature: "post".into(),
    ///     construct_name: "create_post".into(),
    ///     site_kind: HandlerSiteKind::CommandHandler,
    ///     handler_namespace: "fn".into(),
    ///     handler_name: "validate_title".into(),
    ///     handler_path: PathBuf::from("validate_title.go"),
    ///     expected_test_path: PathBuf::from("validate_title_test.go"),
    /// };
    /// assert!(f.message().contains("validate_title"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "@{}.{} handler exists at {} but its paired test is missing at {}. \
             Author a `func Test{}` (table-driven, per the IR contract) or delete \
             the handler.",
            self.handler_namespace,
            self.handler_name,
            self.handler_path.display(),
            self.expected_test_path.display(),
            pascal_case(&self.handler_name),
        )
    }
}

/// Walk `feature` for handlers whose paired test is missing. Symmetric
/// to [`crate::correctness::handler_missing_001::check`] but with the
/// presence ordering inverted: only fires when `<name>.go` exists.
pub fn check(feature: &Feature, lzi_path: &Path, app_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for site in iter_handler_sites(feature) {
        if !is_handler_namespace(&site) {
            continue;
        }
        let go_path = handler_path::resolve(app_root, &feature.name, &site.handler_name);
        let test_path = handler_path::resolve_test(app_root, &feature.name, &site.handler_name);
        // Twin-rule discipline: only fire when the handler itself
        // exists. Otherwise HANDLER-MISSING-001 covers the diagnostic
        // and demanding a test for a missing file would double-fire.
        if go_path.exists() && !test_path.exists() {
            findings.push(Finding {
                path: lzi_path.to_path_buf(),
                feature: feature.name.clone(),
                construct_name: site.construct_name,
                site_kind: site.kind,
                handler_namespace: site.handler_namespace,
                handler_name: site.handler_name,
                handler_path: go_path,
                expected_test_path: test_path,
            });
        }
    }
    findings
}

/// Mirror of [`crate::correctness::handler_missing_001`]'s scope —
/// the twin rule never fires for sites the parent rule wouldn't pair-
/// check. Keeps the twin discipline coherent.
fn is_handler_namespace(site: &HandlerSite) -> bool {
    match site.kind {
        HandlerSiteKind::CommandHandler | HandlerSiteKind::LifecycleInvariantHandler => {
            site.handler_namespace == "fn"
        }
        _ => false,
    }
}

fn pascal_case(snake: &str) -> String {
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
        }
    }

    fn mk_feature(commands: Vec<Command>) -> Feature {
        Feature {
            name: "post".into(),
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

    #[test]
    fn positive_handler_without_test_fires() {
        let tmp = TempDir::new().unwrap();
        let dir = handler_path::handlers_dir(tmp.path(), "post");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("validate_title.go"), "package posthandlers").unwrap();
        // No _test.go file written → rule must fire.

        let feature = mk_feature(vec![mk_cmd_with_handler("create_post", "validate_title")]);
        let findings = check(&feature, Path::new("features/post/post.lzi"), tmp.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].handler_name, "validate_title");
        assert!(findings[0].message().contains("TestValidateTitle"));
        assert_eq!(Finding::CODE, "TEST-HANDLER-MISSING-001");
    }

    #[test]
    fn negative_no_double_fire_when_handler_missing() {
        let tmp = TempDir::new().unwrap();
        // Neither file exists. HANDLER-MISSING-001 fires; this rule
        // stays silent so the author isn't bombarded with two
        // findings about the same gap.
        let feature = mk_feature(vec![mk_cmd_with_handler("create_post", "validate_title")]);
        let findings = check(&feature, Path::new("features/post/post.lzi"), tmp.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn negative_complete_pair_does_not_fire() {
        let tmp = TempDir::new().unwrap();
        let dir = handler_path::handlers_dir(tmp.path(), "post");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("validate_title.go"), "package posthandlers").unwrap();
        fs::write(
            dir.join("validate_title_test.go"),
            "package posthandlers\nfunc TestValidateTitle(t *testing.T) {}",
        )
        .unwrap();

        let feature = mk_feature(vec![mk_cmd_with_handler("create_post", "validate_title")]);
        let findings = check(&feature, Path::new("features/post/post.lzi"), tmp.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn pascal_case_used_in_message() {
        assert_eq!(pascal_case("validate_title_v2"), "ValidateTitleV2");
    }
}
