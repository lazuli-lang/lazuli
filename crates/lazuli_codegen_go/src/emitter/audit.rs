//! Audit metadata emission for the Lazuli Go runtime.
//!
//! This module is intentionally standalone so the top-level orchestrator can
//! wire it into `emit_module` after merge without touching command emission in
//! this branch.

use lazuli_ir::{AuditSpec, Command, CommandEffect, Feature, Module};

use crate::GeneratedFile;

use super::casing::lower_camel;
use super::imports::ImportSet;
use super::printer::GoPrinter;

const AUDIT_LOG_PATH: &str = "migrations/audit_log.sql";

const AUDIT_LOG_DDL: &str = "\
CREATE TABLE IF NOT EXISTS audit_log (
    id BIGSERIAL PRIMARY KEY,
    org_id BIGINT,                          -- nullable for system actors
    actor_id BIGINT,                        -- nullable for anonymous
    actor_kind TEXT NOT NULL,               -- 'user' | 'system' | 'service'
    command_name TEXT NOT NULL,             -- \"customer.create\"
    target_resource TEXT,                   -- \"Customer\"
    target_id BIGINT,                       -- nullable
    payload JSONB,                          -- request input
    result_status TEXT NOT NULL,            -- 'ok' | 'error'
    error_code TEXT,                        -- when result_status='error'
    happened_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    correlation_id TEXT
);

CREATE INDEX IF NOT EXISTS audit_log_command_idx ON audit_log (command_name);
CREATE INDEX IF NOT EXISTS audit_log_target_idx ON audit_log (target_resource, target_id);
CREATE INDEX IF NOT EXISTS audit_log_actor_idx ON audit_log (actor_id);
CREATE INDEX IF NOT EXISTS audit_log_org_time_idx ON audit_log (org_id, happened_at DESC);
";

/// Emit a single `dist/go/migrations/audit_log.sql` table DDL referenced by
/// all audit-emitting commands across features.
///
/// The returned path is relative to the generated Go output root, matching the
/// migration emitter convention: `migrations/audit_log.sql`.
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_go::emitter::audit::emit_audit_log_ddl;
/// let file = emit_audit_log_ddl();
/// assert!(file.path.ends_with("audit_log.sql"));
/// ```
pub fn emit_audit_log_ddl() -> GeneratedFile {
    GeneratedFile {
        path: AUDIT_LOG_PATH.to_owned(),
        contents: AUDIT_LOG_DDL.to_owned(),
    }
}

/// Walk module.features for commands with `audit default` and emit audit
/// binding metadata (currently optional; could be lifted into the existing
/// command.rs emit_command later).
///
/// ## Examples
///
/// ```ignore
/// let files = emit_audit_metadata(&module);
/// // Empty when no command opts into audit; otherwise one file per
/// // emitting feature.
/// ```
pub fn emit_audit_metadata(module: &Module) -> Vec<GeneratedFile> {
    let mut features: Vec<(&Feature, Vec<&Command>)> = module
        .features
        .iter()
        .filter_map(|feature| {
            let mut commands: Vec<&Command> = feature
                .commands
                .iter()
                .filter(|command| command.audit.as_ref().is_some_and(is_default_audit))
                .collect();

            if commands.is_empty() {
                return None;
            }

            commands.sort_by(|a, b| a.name.cmp(&b.name));
            Some((feature, commands))
        })
        .collect();

    features.sort_by(|(a, _), (b, _)| a.name.cmp(&b.name));

    features
        .into_iter()
        .map(|(feature, commands)| GeneratedFile {
            path: format!("{}/audit.gen.go", feature.name),
            contents: emit_feature_audit_metadata(feature, &commands),
        })
        .collect()
}

fn emit_feature_audit_metadata(feature: &Feature, commands: &[&Command]) -> String {
    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli/auth");

    p.banner(
        "lazuli module",
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    let mut first = true;
    for command in commands {
        if !first {
            p.blank();
        }
        first = false;
        emit_command_audit_entry(&mut p, feature, command);
    }

    p.finish()
}

fn emit_command_audit_entry(p: &mut GoPrinter, feature: &Feature, command: &Command) {
    let qualified_name = format!("{}.{}", feature.name, command.name);

    write_section_banner(
        p,
        &[
            format!("Audit: {qualified_name}"),
            format!("  command {}", command.name),
        ],
    );

    p.line(&format!(
        "var {} = auth.AuditEntry{{",
        audit_entry_var_name(&command.name)
    ));
    p.indent();

    let mut rows = vec![(
        "CommandName:".to_owned(),
        format!("\"{}\",", escape_string(&qualified_name)),
    )];
    if let Some(resource) = command_target_resource(command) {
        rows.push((
            "TargetResource:".to_owned(),
            format!("\"{}\",", escape_string(resource)),
        ));
    }

    let key_width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }

    p.dedent();
    p.line("}");
}

fn is_default_audit(audit: &AuditSpec) -> bool {
    let subjects: Vec<&str> = audit
        .subjects
        .iter()
        .map(|subject| subject.trim())
        .filter(|subject| !subject.is_empty())
        .collect();

    subjects.len() == 1 && subjects[0].eq_ignore_ascii_case("default")
}

fn command_target_resource(command: &Command) -> Option<&str> {
    match &command.effect {
        CommandEffect::Creates(effect) => Some(effect.resource.name.as_str()),
        CommandEffect::Updates(effect) => Some(effect.resource.name.as_str()),
        CommandEffect::Deletes(effect) => Some(effect.resource.name.as_str()),
        CommandEffect::Reorders(effect) => Some(effect.resource.name.as_str()),
        CommandEffect::Returns(_) | CommandEffect::None => None,
    }
}

fn audit_entry_var_name(command_name: &str) -> String {
    format!("{}AuditEntry", lower_camel(command_name))
}

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    p.line("// ------------------------------------------------------------");
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line("// ------------------------------------------------------------");
}

fn escape_string(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AuditSpec, CommandEffect, CommandInput, CommandKind, CreateEffect, Defaults, Feature,
        Module, Policies, PolicyRef, QualifiedName, ReturnsEffect, TypeRef,
    };

    fn base_module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    fn base_feature(name: &str) -> Feature {
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

    fn base_command(name: &str) -> Command {
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

    fn audit_default() -> AuditSpec {
        AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
            materialize: None,
        }
    }

    fn local_qname(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    #[test]
    fn emits_audit_log_ddl_shape() {
        let file = emit_audit_log_ddl();

        assert_eq!(file.path, "migrations/audit_log.sql");
        assert_eq!(file.contents, AUDIT_LOG_DDL);
        assert!(
            file.contents
                .contains("CREATE TABLE IF NOT EXISTS audit_log (")
        );
        assert!(file.contents.contains("id BIGSERIAL PRIMARY KEY,"));
        assert!(file.contents.contains("org_id BIGINT,"));
        assert!(file.contents.contains("actor_id BIGINT,"));
        assert!(file.contents.contains("actor_kind TEXT NOT NULL,"));
        assert!(file.contents.contains("command_name TEXT NOT NULL,"));
        assert!(file.contents.contains("target_resource TEXT,"));
        assert!(file.contents.contains("target_id BIGINT,"));
        assert!(file.contents.contains("payload JSONB,"));
        assert!(file.contents.contains("result_status TEXT NOT NULL,"));
        assert!(file.contents.contains("error_code TEXT,"));
        assert!(
            file.contents
                .contains("happened_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),")
        );
        assert!(file.contents.contains("correlation_id TEXT"));
        assert!(file.contents.contains(
            "CREATE INDEX IF NOT EXISTS audit_log_command_idx ON audit_log (command_name);"
        ));
        assert!(file.contents.contains(
            "CREATE INDEX IF NOT EXISTS audit_log_target_idx ON audit_log (target_resource, target_id);"
        ));
        assert!(
            file.contents.contains(
                "CREATE INDEX IF NOT EXISTS audit_log_actor_idx ON audit_log (actor_id);"
            )
        );
        assert!(file.contents.contains(
            "CREATE INDEX IF NOT EXISTS audit_log_org_time_idx ON audit_log (org_id, happened_at DESC);"
        ));
    }

    #[test]
    fn emits_metadata_for_default_audit_commands_only() {
        let mut feature = base_feature("customer");

        let mut create = base_command("create");
        create.audit = Some(audit_default());
        create.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: false,
            assignments: Vec::new(),
        });
        feature.commands.push(create);

        let mut reassign = base_command("reassign");
        reassign.audit = Some(AuditSpec {
            subjects: vec!["actor".to_owned(), "target.id".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
            materialize: None,
        });
        feature.commands.push(reassign);

        let files = emit_audit_metadata(&base_module(vec![feature]));

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "customer/audit.gen.go");
        assert!(files[0].contents.contains("package customergen"));
        assert!(
            files[0]
                .contents
                .contains("\"lazuli.dev/runtime/lazuli/auth\"")
        );
        assert!(
            files[0]
                .contents
                .contains("var createAuditEntry = auth.AuditEntry{")
        );
        assert!(
            files[0]
                .contents
                .contains("CommandName:    \"customer.create\",")
        );
        assert!(files[0].contents.contains("TargetResource: \"Customer\","));
        assert!(!files[0].contents.contains("reassignAuditEntry"));
    }

    #[test]
    fn metadata_omits_target_resource_for_pure_commands() {
        let mut feature = base_feature("auth");
        let mut login = base_command("login");
        login.kind = CommandKind::Returns;
        login.audit = Some(audit_default());
        login.effect = CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::Unresolved("Session".to_owned()),
        });
        feature.commands.push(login);

        let files = emit_audit_metadata(&base_module(vec![feature]));

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "auth/audit.gen.go");
        assert!(files[0].contents.contains("CommandName: \"auth.login\","));
        assert!(!files[0].contents.contains("TargetResource:"));
    }

    #[test]
    fn metadata_is_empty_without_default_audit_commands() {
        let mut feature = base_feature("customer");
        let mut command = base_command("create");
        command.audit = Some(AuditSpec {
            subjects: vec!["actor".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
            materialize: None,
        });
        feature.commands.push(command);

        let files = emit_audit_metadata(&base_module(vec![feature]));

        assert!(files.is_empty());
    }
}
