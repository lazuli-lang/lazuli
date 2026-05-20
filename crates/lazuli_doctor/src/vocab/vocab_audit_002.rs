//! VOCAB-AUDIT-002 — handler-only command on capability-tagged fields lacks audit.
//!
//! Companion to VOCAB-AUDIT-001. This rule covers the conservative IR-visible
//! case where a handler-only command (`returns` / no effect) invalidates a
//! resource carrying sensitive `@cap.*` fields but declares no `audit` child.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{CapabilityRef, Command, CommandEffect, Feature, Field, Resource, TypeRef};

const SENSITIVE_TIERS: &[&str] = &["Encrypted", "Token", "Hashed", "PII"];

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-AUDIT-002 finding: a handler-only command can mutate sensitive
/// capability-tagged fields without an explicit audit contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the offending command.
    pub command: String,
    /// Invalidated resource carrying sensitive capability-tagged fields.
    pub resource: String,
    /// Sensitive field names on the invalidated resource.
    pub sensitive_fields: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-AUDIT-002";

    pub fn message(&self) -> String {
        format!(
            "handler-only command `{}` invalidates `{}` which has \
             {} field(s) with sensitive @cap.* tier ({}) but declares no \
             `audit` child — handler-side mutation of capability-tagged \
             fields requires an explicit audit contract. Add \
             `audit default` or `audit <fields>` with a documented reason.",
            self.command,
            self.resource,
            self.sensitive_fields.len(),
            self.sensitive_fields.join(", ")
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-AUDIT-002 for all commands in one feature.
///
/// `Command.invalidates` currently models `invalidates query.<name>(...)`.
/// This v1 rule intentionally stays conservative: it only fires when that
/// invalidation target name also resolves to a resource in the same feature.
/// Handler/policy body analysis remains outside doctor vocabulary lints.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let resources_by_name: HashMap<&str, &Resource> = feature
        .resources
        .iter()
        .map(|resource| (resource.name.as_str(), resource))
        .collect();

    feature
        .commands
        .iter()
        .filter(|cmd| cmd.audit.is_none())
        .filter(|cmd| is_handler_like(cmd))
        .flat_map(|cmd| {
            sensitive_invalidates(cmd, &resources_by_name)
                .into_iter()
                .map(move |(resource, sensitive_fields)| Finding {
                    path: path.to_path_buf(),
                    command: cmd.name.clone(),
                    resource,
                    sensitive_fields,
                })
        })
        .collect()
}

// ── internals ─────────────────────────────────────────────────────────────────

fn is_handler_like(cmd: &Command) -> bool {
    matches!(cmd.effect, CommandEffect::Returns(_) | CommandEffect::None)
}

fn sensitive_invalidates(
    cmd: &Command,
    by_name: &HashMap<&str, &Resource>,
) -> Vec<(String, Vec<String>)> {
    cmd.invalidates
        .iter()
        .filter_map(|invalidate| {
            let resource_name = invalidate.query.name.as_str();
            by_name
                .get(resource_name)
                .map(|resource| (invalidate.query.name.clone(), sensitive_fields(resource)))
        })
        .filter(|(_, fields)| !fields.is_empty())
        .collect()
}

fn sensitive_fields(resource: &Resource) -> Vec<String> {
    resource
        .fields
        .iter()
        .filter(|field| has_sensitive_capability(field))
        .map(|field| field.name.clone())
        .collect()
}

fn has_sensitive_capability(field: &Field) -> bool {
    match &field.type_ref {
        TypeRef::Capability(capability) => is_sensitive_tier(capability),
        _ => false,
    }
}

fn is_sensitive_tier(capability: &CapabilityRef) -> bool {
    let tier = match capability {
        CapabilityRef::Encrypted(_) => "Encrypted",
        // `@cap.E2ee` is server-blind ciphertext — strictly more
        // sensitive than `@cap.Encrypted` (the server cannot decrypt
        // at all). Treated the same audit-tier-wise.
        CapabilityRef::E2ee(_) => "E2ee",
        CapabilityRef::Token(_) => "Token",
        CapabilityRef::Hashed(_) => "Hashed",
        CapabilityRef::File(_) => "File",
    };

    SENSITIVE_TIERS.contains(&tier)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AuditSpec, BuiltinType, CommandInput, CommandKind, Defaults, EncryptedCapability,
        FieldConstraints, HashAlgorithm, HashedCapability, InvalidatesSpec, NamedArg, Policies,
        PolicyRef, QualifiedName, ReturnsEffect, TokenCapability, TokenStore, UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_cmd(
        name: &str,
        effect: CommandEffect,
        audit: Option<AuditSpec>,
        invalidates: Vec<&str>,
    ) -> Command {
        let kind = match &effect {
            CommandEffect::Creates(_) => CommandKind::Create,
            CommandEffect::Updates(_) => CommandKind::Update,
            CommandEffect::Deletes(_) => CommandKind::Delete,
            _ => CommandKind::Returns,
        };

        Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
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
            audit,
            approval: None,
            invalidates: invalidates
                .into_iter()
                .map(|name| InvalidatesSpec {
                    query: qn(name),
                    args: vec![],
                })
                .collect(),
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(resources: Vec<Resource>, commands: Vec<Command>) -> Feature {
        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
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
            span_ref: None,
        }
    }

    fn resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
        }
    }

    fn field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn encrypted_field(name: &str) -> Field {
        field(
            name,
            TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
                key: "@key.default".to_owned(),
            })),
        )
    }

    fn hashed_field(name: &str) -> Field {
        field(
            name,
            TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                algorithm: HashAlgorithm::Argon2id,
            })),
        )
    }

    fn token_field(name: &str) -> Field {
        field(
            name,
            TypeRef::Capability(CapabilityRef::Token(TokenCapability {
                ttl: "15m".to_owned(),
                single_use: false,
                store: TokenStore::Hashed,
            })),
        )
    }

    fn text_field(name: &str) -> Field {
        field(name, TypeRef::Builtin(BuiltinType::Text))
    }

    fn returns_effect() -> CommandEffect {
        CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::Builtin(BuiltinType::Boolean),
        })
    }

    fn updates_effect() -> CommandEffect {
        CommandEffect::Updates(UpdateEffect {
            resource: qn("Connection"),
            assignments: vec![],
        })
    }

    fn audit_default() -> Option<AuditSpec> {
        Some(AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
        })
    }

    // ── positive ──────────────────────────────────────────────────────────────

    #[test]
    fn positive_handler_invalidates_encrypted_resource_no_audit_fires() {
        let feature = mk_feature(
            vec![resource(
                "Connection",
                vec![encrypted_field("access_token")],
            )],
            vec![mk_cmd(
                "refresh_tokens",
                returns_effect(),
                None,
                vec!["Connection"],
            )],
        );

        let findings = check(
            &feature,
            Path::new("features/integrations/integrations.lzi"),
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].command, "refresh_tokens");
        assert_eq!(findings[0].resource, "Connection");
        assert_eq!(findings[0].sensitive_fields, vec!["access_token"]);
        assert_eq!(Finding::CODE, "VOCAB-AUDIT-002");
        assert!(
            findings[0].message().contains("refresh_tokens"),
            "message should name the command"
        );
    }

    // ── negatives ─────────────────────────────────────────────────────────────

    #[test]
    fn negative_command_with_audit_default_does_not_fire() {
        let feature = mk_feature(
            vec![resource(
                "Connection",
                vec![encrypted_field("access_token")],
            )],
            vec![mk_cmd(
                "refresh_tokens",
                returns_effect(),
                audit_default(),
                vec!["Connection"],
            )],
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_updates_effect_handled_by_001() {
        let feature = mk_feature(
            vec![resource(
                "Connection",
                vec![encrypted_field("access_token")],
            )],
            vec![mk_cmd(
                "refresh_tokens",
                updates_effect(),
                None,
                vec!["Connection"],
            )],
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "structured write effects are VOCAB-AUDIT-001 territory"
        );
    }

    #[test]
    fn negative_invalidates_resource_without_sensitive_caps_does_not_fire() {
        let feature = mk_feature(
            vec![resource("Post", vec![text_field("title")])],
            vec![mk_cmd(
                "refresh_posts",
                returns_effect(),
                None,
                vec!["Post"],
            )],
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_no_invalidates_does_not_fire() {
        let feature = mk_feature(
            vec![resource(
                "Connection",
                vec![encrypted_field("access_token")],
            )],
            vec![mk_cmd("refresh_tokens", returns_effect(), None, vec![])],
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn positive_multiple_sensitive_capabilities_are_reported() {
        let feature = mk_feature(
            vec![resource(
                "Connection",
                vec![hashed_field("password_hash"), token_field("reset_token")],
            )],
            vec![mk_cmd(
                "rotate_secret",
                CommandEffect::None,
                None,
                vec!["Connection"],
            )],
        );

        let findings = check(&feature, Path::new("f.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].sensitive_fields,
            vec!["password_hash", "reset_token"]
        );
    }

    #[test]
    fn negative_invalidates_unmatched_query_name_does_not_fire() {
        let feature = mk_feature(
            vec![resource(
                "Connection",
                vec![encrypted_field("access_token")],
            )],
            vec![mk_cmd(
                "refresh_tokens",
                returns_effect(),
                None,
                vec!["connection_by_publisher"],
            )],
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "v1 only fires when invalidates.query.name also resolves to a resource"
        );
    }

    #[test]
    fn invalidates_args_are_ignored_for_resource_matching() {
        let mut cmd = mk_cmd("refresh_tokens", returns_effect(), None, vec!["Connection"]);
        cmd.invalidates[0].args = vec![NamedArg {
            name: "id".to_owned(),
            value: lazuli_ir::Expr::Path(lazuli_ir::Path::from_segments(["route", "id"])),
        }];

        let feature = mk_feature(
            vec![resource(
                "Connection",
                vec![encrypted_field("access_token")],
            )],
            vec![cmd],
        );

        assert_eq!(check(&feature, Path::new("f.lzi")).len(), 1);
    }
}
