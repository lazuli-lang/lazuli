//! RETURNS-LIST-001 - `returns list <T>` handlers must return a typed Go slice.
//!
//! The rule is intentionally CLI-local because it has to read user-authored Go
//! handlers from `app/features/<feature>/handlers/<name>.go`.
//!
//! Split into sub-files: the small Go signature parser
//! (`go_signature`) and the TypeRef-to-Go-source emitter
//! (`type_emit`); the orchestration (`diagnostics` →
//! `check_command_signature` → `Finding::into_doctor`) and tests stay in
//! this file.

use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::{Command, CommandEffect, TypeRef};

use super::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

mod go_signature;
mod type_emit;

use go_signature::{
    find_handler_return, is_opaque_json_return, returns_list_location, strip_named_return,
};
use type_emit::{
    exported_func_name, gen_package_name, go_type_for_stub, path_name_for,
    qualify_generated_stub_type, type_ref_label,
};

pub(super) const CODE: &str = "RETURNS-LIST-001";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    path: PathBuf,
    line: usize,
    column: usize,
    command: String,
    actual_return: String,
    declared_return: String,
    ir_path: PathBuf,
    ir_line: usize,
    suggestion: String,
}

impl Finding {
    fn message(&self) -> String {
        format!(
            "handler signature mismatch: command `{}` declares `{}` ({}:{}), but the handler returns `{}`. Change the handler signature to `({}, error)`.",
            self.command,
            self.declared_return,
            self.ir_path.display(),
            self.ir_line,
            self.actual_return,
            self.suggestion
        )
    }

    fn into_doctor(self) -> DoctorDiagnostic {
        let message = self.message();
        DoctorDiagnostic {
            path: self.path,
            line: self.line,
            column: self.column,
            severity: DoctorSeverity::Error,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GoReturnType {
    pub raw: String,
    pub line: usize,
    pub column: usize,
}

pub(super) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let mut out = Vec::new();

    for fact in facts {
        let ir_source = fs::read_to_string(&fact.path).ok();
        for command in &fact.commands {
            let Some(handler) = command.handler.as_ref() else {
                continue;
            };
            if handler.namespace != "fn" {
                continue;
            }

            let Some(handler_path) =
                resolve_handler_path(project_root, &fact.feature, &handler.name)
            else {
                continue;
            };
            let Ok(handler_source) = fs::read_to_string(&handler_path) else {
                continue;
            };
            let ir_line = ir_source
                .as_deref()
                .and_then(|source| {
                    fact.command_lines
                        .get(&command.name)
                        .copied()
                        .and_then(|line| returns_list_location(source, line).map(|(line, _)| line))
                })
                .or_else(|| fact.command_lines.get(&command.name).copied())
                .unwrap_or(fact.feature_line);

            if let Some(finding) = check_command_signature(
                command,
                &fact.feature,
                &fact.path,
                ir_line,
                &handler_path,
                &handler_source,
            ) {
                out.push(finding.into_doctor());
            }
        }
    }

    out
}

fn check_command_signature(
    command: &Command,
    feature: &str,
    ir_path: &Path,
    ir_line: usize,
    handler_path: &Path,
    handler_source: &str,
) -> Option<Finding> {
    let inner = returns_list_inner(command)?;
    let handler = command.handler.as_ref()?;
    let actual = find_handler_return(handler_source, &handler.name)?;
    let actual_type = strip_named_return(&actual.raw);
    if !is_opaque_json_return(&actual_type) {
        return None;
    }

    let declared_return = format!("returns list {}", type_ref_label(inner));
    let suggestion = qualify_generated_stub_type(
        &go_type_for_stub(&TypeRef::Many(Box::new(inner.clone()))),
        &gen_package_name(feature),
    )
    .0;

    Some(Finding {
        path: handler_path.to_path_buf(),
        line: actual.line,
        column: actual.column,
        command: command.name.clone(),
        actual_return: actual_type,
        declared_return,
        ir_path: ir_path.to_path_buf(),
        ir_line,
        suggestion,
    })
}

fn returns_list_inner(command: &Command) -> Option<&TypeRef> {
    match &command.effect {
        CommandEffect::Returns(ret) => match &ret.return_type {
            TypeRef::Many(inner) => Some(inner.as_ref()),
            _ => None,
        },
        _ => None,
    }
}

fn resolve_handler_path(project_root: &Path, feature: &str, handler: &str) -> Option<PathBuf> {
    let path_name = path_name_for(handler);
    if path_name.is_empty() {
        return None;
    }
    let file_name = format!("{path_name}.go");
    let candidates = [
        project_root
            .join("app")
            .join("features")
            .join(feature)
            .join("handlers")
            .join(&file_name),
        project_root
            .join("features")
            .join(feature)
            .join("handlers")
            .join(&file_name),
        project_root
            .join("app")
            .join("features")
            .join(feature)
            .join(&file_name),
        project_root.join("features").join(feature).join(&file_name),
        project_root
            .join("dist")
            .join("go")
            .join(feature)
            .join(&file_name),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, CommandInput, CommandKind, HandlerRef, PolicyRef, QualifiedName,
        ReturnsEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn returns(return_type: TypeRef) -> Command {
        Command {
            name: "list_chat_inbox".to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect { return_type }),
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
        }
    }

    fn list_record() -> TypeRef {
        TypeRef::Many(Box::new(TypeRef::UserDefined(qn("ChatListEntry"))))
    }

    fn check(source: &str, return_type: TypeRef) -> Option<Finding> {
        let command = returns(return_type);
        check_command_signature(
            &command,
            "messaging",
            Path::new("features/messaging/messaging.lzi"),
            127,
            Path::new("app/features/messaging/handlers/list_chat_inbox.go"),
            source,
        )
    }

    #[test]
    fn returns_list_001_positive_interface_handler_fires() {
        let source = r#"package messaginghandlers

func ListChatInbox(ctx *lazuli.Ctx, input messaginggen.ListChatInboxInput) (interface{}, error) {
    return nil, nil
}
"#;

        let finding = check(source, list_record()).expect("expected RETURNS-LIST-001");

        assert_eq!(CODE, "RETURNS-LIST-001");
        assert_eq!(finding.line, 3);
        assert_eq!(finding.actual_return, "interface{}");
        assert_eq!(finding.declared_return, "returns list ChatListEntry");
        assert_eq!(finding.suggestion, "[]messaginggen.ChatListEntry");
        assert!(finding.message().contains("handler signature mismatch"));
    }

    #[test]
    fn returns_list_001_negative_typed_slice_silent() {
        let source = r#"package messaginghandlers

func ListChatInbox(ctx *lazuli.Ctx, input messaginggen.ListChatInboxInput) ([]messaginggen.ChatListEntry, error) {
    return nil, nil
}
"#;

        assert!(check(source, list_record()).is_none());
    }

    #[test]
    fn returns_list_001_edge_returns_json_silent() {
        let source = r#"package messaginghandlers

func ListChatInbox(ctx *lazuli.Ctx, input messaginggen.ListChatInboxInput) (interface{}, error) {
    return nil, nil
}
"#;
        let json = TypeRef::Builtin(BuiltinType::Json);

        assert!(check(source, json).is_none());
    }

    #[test]
    fn returns_list_001_positive_any_and_lazuli_json_and_bytes_are_opaque() {
        for raw in ["any", "lazuli.JSON", "[]byte"] {
            let source = format!(
                "package messaginghandlers\n\nfunc ListChatInbox(ctx *lazuli.Ctx, input messaginggen.ListChatInboxInput) ({raw}, error) {{\n    return nil, nil\n}}\n"
            );
            assert!(
                check(&source, list_record()).is_some(),
                "{raw} should be flagged"
            );
        }
    }

    #[test]
    fn returns_list_001_parses_named_return() {
        let source = r#"package messaginghandlers

func ListChatInbox(ctx *lazuli.Ctx, input messaginggen.ListChatInboxInput) (out interface{}, err error) {
    return nil, nil
}
"#;

        let finding = check(source, list_record()).expect("expected named opaque return");

        assert_eq!(finding.actual_return, "interface{}");
    }

    #[test]
    fn returns_list_001_parses_multiline_signature() {
        let source = r#"package messaginghandlers

func ListChatInbox(
    ctx *lazuli.Ctx,
    input messaginggen.ListChatInboxInput,
) (
    interface{},
    error,
) {
    return nil, nil
}
"#;

        let finding = check(source, list_record()).expect("expected multiline opaque return");

        assert_eq!(finding.line, 7);
        assert_eq!(finding.actual_return, "interface{}");
    }

    #[test]
    fn returns_list_001_keeps_builtin_slice_suggestion_unqualified() {
        let source = r#"package messaginghandlers

func ListChatInbox(ctx *lazuli.Ctx, input struct{}) (any, error) {
    return nil, nil
}
"#;
        let list_text = TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Text)));

        let finding = check(source, list_text).expect("expected opaque builtin list");

        assert_eq!(finding.suggestion, "[]string");
    }
}
