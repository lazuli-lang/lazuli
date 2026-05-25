//! IR Error-Vocab — 7 typed analyzer cross-checks (Cell ANALYZE-1).
//!
//! Each rule is a self-contained `check_*` function in its own
//! `rule_<code>.rs` module file. The doctor pipeline
//! (`lazuli_cli::doctor::aggregators::error_vocab`) adapts findings to
//! `DoctorDiagnostic` and routes the per-rule severity. The LSP can
//! mount the same checks for live diagnostics in a later cell.
//!
//! ## Module layout
//!
//! - `catalogs.rs` — closed catalogs (`FRAMEWORK_ERROR_CODES`,
//!   `EXPOSE_4XX_FIELDS`, `EXPOSE_5XX_FIELDS`) and the shared
//!   `has_policy_denied_catchall` helper.
//! - `rule_policies_no_when_denied.rs` — ERR-VOCAB-001
//! - `rule_translation_key_unknown.rs` — ERR-VOCAB-002
//! - `rule_builtin_fallback.rs` — ERR-VOCAB-003
//! - `rule_code_unknown.rs` — ERR-VOCAB-CODE-UNKNOWN
//! - `rule_expose_unknown.rs` — ERR-VOCAB-EXPOSE-UNKNOWN
//! - `rule_when_denied_no_policy.rs` — ERR-VOCAB-WHEN-DENIED-NO-POLICY
//! - `rule_expose_5xx_message.rs` — ERR-VOCAB-EXPOSE-5XX-MESSAGE
//!
//! External callers continue to write
//! `lazuli_doctor::error_vocab::error_vocab::check_*` — every `pub` item
//! from the rule files is re-exported here so the surface stays stable.
//!
//! Closed catalogs (proposal §2.B / §2.C):
//! * Error codes (12): `policy_denied`, `validation_failed`,
//!   `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`,
//!   `method_not_allowed`, `integration_error`, `unique_violation`,
//!   `foreign_key_violation`, `not_null_violation`, `check_violation`.
//! * 4xx exposure fields: `message`, `code`, `data`, `message_key`.
//! * 5xx exposure fields: `code`, `data`. (`message` is rejected — see
//!   ERR-VOCAB-EXPOSE-5XX-MESSAGE).
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6 §11
//! Cell ANALYZE-1.

mod catalogs;
mod rule_builtin_fallback;
mod rule_code_unknown;
mod rule_expose_5xx_message;
mod rule_expose_unknown;
mod rule_policies_no_when_denied;
mod rule_translation_key_unknown;
mod rule_when_denied_no_policy;

pub use catalogs::{EXPOSE_4XX_FIELDS, EXPOSE_5XX_FIELDS, FRAMEWORK_ERROR_CODES};
pub use rule_builtin_fallback::{BuiltinFallbackFinding, check_builtin_fallback};
pub use rule_code_unknown::{CodeUnknownFinding, check_code_unknown};
pub use rule_expose_5xx_message::{Expose5xxMessageFinding, check_expose_5xx_message};
pub use rule_expose_unknown::{ExposeUnknownFinding, check_expose_unknown};
pub use rule_policies_no_when_denied::{
    PoliciesNoWhenDeniedFinding, check_policies_no_when_denied,
};
pub use rule_translation_key_unknown::{KeyUnknownFinding, check_translation_key_unknown};
pub use rule_when_denied_no_policy::{
    WhenDeniedNoPolicyFinding, WhenDeniedSite, check_when_denied_no_policy,
};

// =============================================================================
// Tests — exercise each rule positive + negative against synthetic IR.
//
// Fixture-driven coverage lives in `crates/lazuli_cli/tests/doctor_error_vocab.rs`
// (the dispatcher feeds each fixture through `DoctorPackage::diagnostics()`
// and asserts the expected code fires exactly once). These unit tests
// keep the rule shape pinned without depending on the doctor scaffolding.
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Command, CommandEffect, CommandInput, CommandKind, Defaults, Feature, FeatureErrorMessage,
        FeatureErrors, Policies, PolicyCategory, PolicyExpr, PolicyRef, ReturnsEffect, SpanRef,
        Translation, TranslationKey, TranslationKeyRef, TranslationVariant, TypeRef,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    fn span() -> SpanRef {
        SpanRef { start: 0, end: 1 }
    }

    fn key_ref(name: &str) -> TranslationKeyRef {
        TranslationKeyRef {
            key: name.to_owned(),
            span_ref: Some(span()),
        }
    }

    fn mk_translation(keys: &[&str]) -> Translation {
        Translation {
            catalog: "./i18n/test.<locale>.json".to_owned(),
            keys: keys
                .iter()
                .map(|name| TranslationKey {
                    name: (*name).to_owned(),
                    variants: vec![TranslationVariant {
                        locale: "pt-BR".to_owned(),
                        text: "x".to_owned(),
                    }],
                    plurals: vec![],
                })
                .collect(),
        }
    }

    fn mk_command(name: &str, policy: PolicyRef) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(lazuli_ir::BuiltinType::Boolean),
            }),
            policy,
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
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: Some(span()),
        }
    }

    fn mk_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
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
            span_ref: Some(span()),
        }
    }

    fn declared_policies(categories: Vec<PolicyCategory>) -> Policies {
        Policies {
            categories,
            fields: vec![],
            span_ref: Some(span()),
        }
    }

    fn p_cat(name: &str, atoms: &[&str], when_denied: Option<&str>) -> PolicyCategory {
        PolicyCategory {
            name: name.to_owned(),
            atoms: atoms.iter().map(|a| (*a).to_owned()).collect(),
            previous_names: vec![],
            when_denied: when_denied.map(key_ref),
            when_denied_route: None,
        }
    }

    fn path() -> &'static Path {
        Path::new("features/test.lzi")
    }

    fn empty_keys_index() -> BTreeMap<String, BTreeSet<String>> {
        BTreeMap::new()
    }

    // ── ERR-VOCAB-001 ─────────────────────────────────────────────────────

    #[test]
    fn err_vocab_001_fires_when_policies_block_lacks_when_denied() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        let findings = check_policies_no_when_denied(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(PoliciesNoWhenDeniedFinding::CODE, "ERR-VOCAB-001");
    }

    #[test]
    fn err_vocab_001_silent_when_when_denied_present() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], Some("admin_only"))]);
        f.translation = Some(mk_translation(&["admin_only"]));
        assert!(check_policies_no_when_denied(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_001_silent_when_errors_catchall_present() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        f.errors = Some(FeatureErrors {
            messages: vec![FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: key_ref("signin_required"),
                span_ref: Some(span()),
            }],
            ..Default::default()
        });
        f.translation = Some(mk_translation(&["signin_required"]));
        assert!(check_policies_no_when_denied(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_001_silent_when_policies_block_absent() {
        // `Policies::default()` carries `span_ref: None` — feature has no
        // `policies` block at all, so the rule cannot fire.
        let f = mk_feature("customer");
        assert!(check_policies_no_when_denied(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-002 ─────────────────────────────────────────────────────

    #[test]
    fn err_vocab_002_fires_when_command_when_denied_unknown() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("update", PolicyRef::Local("update".to_owned()));
        cmd.policy_when_denied = Some(key_ref("ghost_key"));
        f.commands = vec![cmd];
        f.translation = Some(mk_translation(&["other_key"]));
        let findings = check_translation_key_unknown(&f, path(), &empty_keys_index());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].key, "ghost_key");
        assert_eq!(KeyUnknownFinding::CODE, "ERR-VOCAB-002");
    }

    #[test]
    fn err_vocab_002_silent_when_key_resolves_locally() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("update", PolicyRef::Local("update".to_owned()));
        cmd.policy_when_denied = Some(key_ref("admin_only"));
        f.commands = vec![cmd];
        f.translation = Some(mk_translation(&["admin_only"]));
        assert!(check_translation_key_unknown(&f, path(), &empty_keys_index()).is_empty());
    }

    #[test]
    fn err_vocab_002_silent_when_key_resolves_through_uses() {
        let mut f = mk_feature("sales");
        f.uses = vec!["crm".to_owned()];
        f.policies = declared_policies(vec![p_cat("create", &["@role.sales"], Some("shared_key"))]);
        let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut crm_keys = BTreeSet::new();
        crm_keys.insert("shared_key".to_owned());
        index.insert("crm".to_owned(), crm_keys);
        assert!(check_translation_key_unknown(&f, path(), &index).is_empty());
    }

    #[test]
    fn err_vocab_002_fires_for_unknown_errors_block_key() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            messages: vec![FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: key_ref("missing_key"),
                span_ref: Some(span()),
            }],
            ..Default::default()
        });
        let findings = check_translation_key_unknown(&f, path(), &empty_keys_index());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].key, "missing_key");
    }

    // ── ERR-VOCAB-003 ─────────────────────────────────────────────────────

    #[test]
    fn err_vocab_003_fires_when_policy_has_no_when_denied_and_no_catchall() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        f.commands = vec![mk_command("update", PolicyRef::Local("update".to_owned()))];
        let findings = check_builtin_fallback(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].command, "update");
        assert_eq!(BuiltinFallbackFinding::CODE, "ERR-VOCAB-003");
    }

    #[test]
    fn err_vocab_003_silent_when_command_overrides_when_denied() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        let mut cmd = mk_command("update", PolicyRef::Local("update".to_owned()));
        cmd.policy_when_denied = Some(key_ref("admin_only"));
        f.commands = vec![cmd];
        f.translation = Some(mk_translation(&["admin_only"]));
        assert!(check_builtin_fallback(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_003_silent_when_feature_catchall_present() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        f.commands = vec![mk_command("update", PolicyRef::Local("update".to_owned()))];
        f.errors = Some(FeatureErrors {
            messages: vec![FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: key_ref("signin_required"),
                span_ref: Some(span()),
            }],
            ..Default::default()
        });
        f.translation = Some(mk_translation(&["signin_required"]));
        assert!(check_builtin_fallback(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_003_silent_when_command_has_no_policy() {
        // A command without `policy` cannot trigger `policy_denied`,
        // so the built-in fallback warning does not apply.
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        f.commands = vec![mk_command("update", PolicyRef::None)];
        assert!(check_builtin_fallback(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_003_silent_when_policy_authors_when_denied() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], Some("admin_only"))]);
        f.commands = vec![mk_command("update", PolicyRef::Local("update".to_owned()))];
        f.translation = Some(mk_translation(&["admin_only"]));
        assert!(check_builtin_fallback(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-CODE-UNKNOWN ────────────────────────────────────────────

    #[test]
    fn err_vocab_code_unknown_fires() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            messages: vec![FeatureErrorMessage {
                code: "blah_error".to_owned(),
                message: key_ref("k"),
                span_ref: Some(span()),
            }],
            ..Default::default()
        });
        let findings = check_code_unknown(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "blah_error");
        assert_eq!(CodeUnknownFinding::CODE, "ERR-VOCAB-CODE-UNKNOWN");
    }

    #[test]
    fn err_vocab_code_unknown_silent_for_all_known_codes() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            messages: FRAMEWORK_ERROR_CODES
                .iter()
                .map(|code| FeatureErrorMessage {
                    code: (*code).to_owned(),
                    message: key_ref("k"),
                    span_ref: Some(span()),
                })
                .collect(),
            ..Default::default()
        });
        assert!(check_code_unknown(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_code_unknown_silent_for_db_integrity_codes() {
        // DB-INTEGRITY-CATALOG-EXT regression guard: the 4 new
        // db-integrity codes must parse cleanly in `errors` blocks so
        // hostpoint can author `errors unique_violation message
        // @translation.account_email_already_registered`.
        let mut f = mk_feature("account");
        f.errors = Some(FeatureErrors {
            messages: [
                "unique_violation",
                "foreign_key_violation",
                "not_null_violation",
                "check_violation",
            ]
            .iter()
            .map(|code| FeatureErrorMessage {
                code: (*code).to_owned(),
                message: key_ref("k"),
                span_ref: Some(span()),
            })
            .collect(),
            ..Default::default()
        });
        assert!(check_code_unknown(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-EXPOSE-UNKNOWN ──────────────────────────────────────────

    #[test]
    fn err_vocab_expose_unknown_fires_for_4xx_unknown_field() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_4xx: vec!["message".to_owned(), "schmessage".to_owned()],
            ..Default::default()
        });
        let findings = check_expose_unknown(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "schmessage");
        assert_eq!(findings[0].axis, "4xx");
        assert_eq!(ExposeUnknownFinding::CODE, "ERR-VOCAB-EXPOSE-UNKNOWN");
    }

    #[test]
    fn err_vocab_expose_unknown_does_not_double_count_5xx_message() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_5xx: vec!["message".to_owned()],
            ..Default::default()
        });
        // `message` is delegated to ERR-VOCAB-EXPOSE-5XX-MESSAGE — this
        // rule must stay silent so the same authoring mistake produces
        // exactly one diagnostic.
        assert!(check_expose_unknown(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_expose_unknown_silent_for_all_known_fields() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_4xx: EXPOSE_4XX_FIELDS.iter().map(|s| (*s).to_owned()).collect(),
            exposure_5xx: EXPOSE_5XX_FIELDS.iter().map(|s| (*s).to_owned()).collect(),
            ..Default::default()
        });
        assert!(check_expose_unknown(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-WHEN-DENIED-NO-POLICY ───────────────────────────────────

    #[test]
    fn err_vocab_when_denied_no_policy_fires_for_command_site() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("ping", PolicyRef::None);
        cmd.policy_when_denied = Some(key_ref("blocked"));
        f.commands = vec![cmd];
        let findings = check_when_denied_no_policy(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].site, WhenDeniedSite::Command("ping".to_owned()));
        assert_eq!(
            WhenDeniedNoPolicyFinding::CODE,
            "ERR-VOCAB-WHEN-DENIED-NO-POLICY"
        );
    }

    #[test]
    fn err_vocab_when_denied_no_policy_silent_when_policy_present() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("ping", PolicyRef::Local("update".to_owned()));
        cmd.policy_when_denied = Some(key_ref("blocked"));
        f.commands = vec![cmd];
        assert!(check_when_denied_no_policy(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_when_denied_no_policy_silent_when_policy_expr_present() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("ping", PolicyRef::None);
        cmd.policy_expr = Some(PolicyExpr::Authenticated);
        cmd.policy_when_denied = Some(key_ref("blocked"));
        f.commands = vec![cmd];
        assert!(check_when_denied_no_policy(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_when_denied_no_policy_fires_for_empty_policy_category() {
        // `policies.update:` with no atoms but a `when_denied` child —
        // the override is dead code because the category gates nothing.
        let mut f = mk_feature("customer");
        let mut cat = p_cat("update", &[], Some("blocked"));
        // Sanity: helper builds atoms from the slice; empty here.
        cat.atoms.clear();
        f.policies = declared_policies(vec![cat]);
        let findings = check_when_denied_no_policy(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].site,
            WhenDeniedSite::Policy("update".to_owned())
        );
    }

    #[test]
    fn err_vocab_when_denied_no_policy_silent_when_policy_category_has_atoms() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], Some("blocked"))]);
        assert!(check_when_denied_no_policy(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-EXPOSE-5XX-MESSAGE ──────────────────────────────────────

    #[test]
    fn err_vocab_expose_5xx_message_fires() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_5xx: vec!["code".to_owned(), "message".to_owned()],
            ..Default::default()
        });
        let findings = check_expose_5xx_message(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            Expose5xxMessageFinding::CODE,
            "ERR-VOCAB-EXPOSE-5XX-MESSAGE"
        );
    }

    #[test]
    fn err_vocab_expose_5xx_message_silent_when_message_absent() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_5xx: vec!["code".to_owned(), "data".to_owned()],
            ..Default::default()
        });
        assert!(check_expose_5xx_message(&f, path()).is_empty());
    }
}
