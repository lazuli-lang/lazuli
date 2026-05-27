//! RETURNS-LIST-002 - pure read commands should not hide row shapes behind JSON.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Command, CommandEffect, TypeRef};

use super::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

pub(super) const CODE: &str = "RETURNS-LIST-002";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    path: PathBuf,
    line: usize,
    column: usize,
    command: String,
}

impl Finding {
    fn message(&self) -> String {
        format!(
            "side-effect-free command `{}` returns JSON. Command has zero declared side-effects (pure read). Pilots that ship as `returns JSON` lose typed client SDK (`defineCommand<I, unknown>` instead of `defineQuery<I, X[]>`). Declare `returns list <Record>` where `<Record>` matches the row type your handler marshals; TS codegen will emit `defineQuery` automatically.",
            self.command
        )
    }

    fn into_doctor(self) -> DoctorDiagnostic {
        let message = self.message();
        DoctorDiagnostic {
            path: self.path,
            line: self.line,
            column: self.column,
            severity: DoctorSeverity::Warning,
            code: CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        }
    }
}

pub(super) fn diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    let mut out = Vec::new();

    for fact in facts {
        let source = fs::read_to_string(&fact.path).ok();
        out.extend(diagnostics_for_commands(
            &fact.path,
            fact.feature_line,
            &fact.commands,
            &fact.command_lines,
            source.as_deref(),
        ));
    }

    out
}

fn diagnostics_for_commands(
    path: &Path,
    feature_line: usize,
    commands: &[Command],
    command_lines: &BTreeMap<String, usize>,
    source: Option<&str>,
) -> Vec<DoctorDiagnostic> {
    let mut out = Vec::new();

    for command in commands {
        let header_line = command_lines
            .get(&command.name)
            .copied()
            .unwrap_or(feature_line);
        let (line, column) = source
            .and_then(|source| returns_json_location(source, header_line))
            .unwrap_or((header_line, 1));

        if let Some(finding) = check_command(command, path, line, column) {
            out.push(finding.into_doctor());
        }
    }

    out
}

fn check_command(command: &Command, path: &Path, line: usize, column: usize) -> Option<Finding> {
    if !returns_json(command) || !is_side_effect_free(command) {
        return None;
    }

    Some(Finding {
        path: path.to_path_buf(),
        line,
        column,
        command: command.name.clone(),
    })
}

fn returns_json(command: &Command) -> bool {
    matches!(
        &command.effect,
        CommandEffect::Returns(ret)
            if matches!(ret.return_type, TypeRef::Builtin(BuiltinType::Json))
    )
}

fn is_side_effect_free(command: &Command) -> bool {
    command.emits.is_empty()
        && command.triggers.is_empty()
        && command.invalidates.is_empty()
        && command.external_calls.is_empty()
}

fn returns_json_location(source: &str, header_line: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let start = header_line.saturating_sub(1);
    let mut end = start + 1;
    while end < lines.len() && super::leading_spaces(lines[end]) > 2 {
        end += 1;
    }

    for (idx, line) in lines.iter().enumerate().take(end).skip(start + 1) {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("returns ") else {
            continue;
        };
        let token = rest.split_whitespace().next().unwrap_or("");
        if token.eq_ignore_ascii_case("JSON") {
            let column = line.find("returns").map(|col| col + 1).unwrap_or(1);
            return Some((idx + 1, column));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use lazuli_ir::{
        CommandInput, CommandKind, CreateEffect, HandlerRef, InvalidatesSpec, PolicyRef,
        QualifiedName, ReturnsEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn command(effect: CommandEffect) -> Command {
        let kind = match &effect {
            CommandEffect::Creates(_) => CommandKind::Create,
            CommandEffect::Updates(_) => CommandKind::Update,
            CommandEffect::Deletes(_) => CommandKind::Delete,
            CommandEffect::Returns(_) => CommandKind::Returns,
            CommandEffect::None => CommandKind::Returns,
        };

        Command {
            name: "list_chat_inbox".to_owned(),
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
                namespace: "fn".to_owned(),
                name: "list_chat_inbox".to_owned(),
                span_ref: None,
            }),
            tests: None,
            previous_names: vec![],
            span_ref: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn returns_json_command() -> Command {
        command(CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::Builtin(BuiltinType::Json),
        }))
    }

    fn returns_list_command() -> Command {
        command(CommandEffect::Returns(ReturnsEffect {
            return_type: TypeRef::Many(Box::new(TypeRef::UserDefined(qn("ChatListEntry")))),
        }))
    }

    fn diagnostics_for_source(commands: Vec<Command>, source: &str) -> Vec<DoctorDiagnostic> {
        let mut lines = BTreeMap::new();
        lines.insert("list_chat_inbox".to_owned(), 2);
        diagnostics_for_commands(
            Path::new("features/messaging/messaging.lzi"),
            1,
            &commands,
            &lines,
            Some(source),
        )
    }

    #[test]
    fn returns_list_002_positive_pure_read_returns_json_warns() {
        let source = r#"feature messaging
  command list_chat_inbox
    returns JSON
    handler @fn.list_chat_inbox
"#;

        let diagnostics = diagnostics_for_source(vec![returns_json_command()], source);

        assert_eq!(CODE, "RETURNS-LIST-002");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Warning);
        assert_eq!(diagnostics[0].line, 3);
        assert!(diagnostics[0].message.contains("returns list <Record>"));
    }

    #[test]
    fn returns_list_002_negative_command_with_emits_silent() {
        let mut command = returns_json_command();
        command.emits.push("chat.thread_listed".to_owned());

        let finding = check_command(
            &command,
            Path::new("features/messaging/messaging.lzi"),
            3,
            5,
        );

        assert!(finding.is_none());
    }

    #[test]
    fn returns_list_002_negative_command_with_invalidates_silent() {
        let mut command = returns_json_command();
        command.invalidates.push(InvalidatesSpec {
            query: qn("chat_inbox"),
            args: vec![],
        });

        let finding = check_command(
            &command,
            Path::new("features/messaging/messaging.lzi"),
            3,
            5,
        );

        assert!(finding.is_none());
    }

    #[test]
    fn returns_list_002_negative_command_with_triggers_silent() {
        let mut command = returns_json_command();
        command.triggers.push("send_digest".to_owned());

        let finding = check_command(
            &command,
            Path::new("features/messaging/messaging.lzi"),
            3,
            5,
        );

        assert!(finding.is_none());
    }

    #[test]
    fn returns_list_002_negative_effect_creates_silent() {
        let command = command(CommandEffect::Creates(CreateEffect {
            resource: qn("Message"),
            from_input: true,
            assignments: vec![],
        }));

        let finding = check_command(
            &command,
            Path::new("features/messaging/messaging.lzi"),
            3,
            5,
        );

        assert!(finding.is_none());
    }

    #[test]
    fn returns_list_002_negative_returns_list_silent() {
        let finding = check_command(
            &returns_list_command(),
            Path::new("features/messaging/messaging.lzi"),
            3,
            5,
        );

        assert!(finding.is_none());
    }

    #[test]
    fn returns_list_002_negative_query_block_silent() {
        let source = r#"feature messaging
  query.list chat_inbox
    returns JSON
"#;

        let diagnostics = diagnostics_for_source(Vec::new(), source);

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }
}
