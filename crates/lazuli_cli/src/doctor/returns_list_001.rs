//! RETURNS-LIST-001 - `returns list <T>` handlers must return a typed Go slice.
//!
//! The rule is intentionally CLI-local because it has to read user-authored Go
//! handlers from `app/features/<feature>/handlers/<name>.go`.

use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, CapabilityRef, Command, CommandEffect, TypeRef};

use super::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

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
struct GoReturnType {
    raw: String,
    line: usize,
    column: usize,
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

fn find_handler_return(source: &str, handler_name: &str) -> Option<GoReturnType> {
    let fn_name = exported_func_name(handler_name);
    let lines: Vec<&str> = source.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("func ") else {
            continue;
        };
        if !starts_with_func_name(rest.trim_start(), &fn_name) {
            continue;
        }

        let signature = collect_signature(&lines, idx);
        return parse_return_type(&signature, idx + 1);
    }

    None
}

fn starts_with_func_name(rest: &str, fn_name: &str) -> bool {
    rest.strip_prefix(fn_name)
        .is_some_and(|after| after.trim_start().starts_with('('))
}

fn collect_signature(lines: &[&str], start: usize) -> String {
    let mut out = String::new();
    for line in lines.iter().skip(start).take(12) {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
        if line.trim_end().ends_with('{') {
            break;
        }
    }
    out
}

fn parse_return_type(signature: &str, start_line: usize) -> Option<GoReturnType> {
    let params_start = signature.find('(')?;
    let params_end = matching_delimiter(signature, params_start, '(', ')')?;
    let after_params_start = params_end + 1;
    let after_params = &signature[after_params_start..];
    let skipped = after_params.len() - after_params.trim_start().len();
    let return_start = after_params_start + skipped;
    let after = &signature[return_start..];

    if after.starts_with('(') {
        let group_end = matching_delimiter(signature, return_start, '(', ')')?;
        let returns = &signature[(return_start + 1)..group_end];
        let first = first_top_level_comma_part(returns)?;
        let local_start = returns.find(first).unwrap_or(0);
        let absolute = return_start + 1 + local_start;
        let (line, column) = line_col_for_offset(signature, start_line, absolute);
        return Some(GoReturnType {
            raw: first.trim().to_owned(),
            line,
            column,
        });
    }

    let end = after
        .find(|ch: char| ch == '{' || ch.is_whitespace())
        .unwrap_or(after.len());
    let raw = after[..end].trim();
    if raw.is_empty() {
        return None;
    }
    let (line, column) = line_col_for_offset(signature, start_line, return_start);
    Some(GoReturnType {
        raw: raw.to_owned(),
        line,
        column,
    })
}

fn matching_delimiter(source: &str, open_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in source.char_indices().skip_while(|(idx, _)| *idx < open_at) {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn first_top_level_comma_part(source: &str) -> Option<&str> {
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;

    for (idx, ch) in source.char_indices() {
        match ch {
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            ',' if paren == 0 && bracket == 0 && brace == 0 => {
                let first = source[..idx].trim();
                return (!first.is_empty()).then_some(first);
            }
            _ => {}
        }
    }

    let trimmed = source.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn line_col_for_offset(source: &str, start_line: usize, offset: usize) -> (usize, usize) {
    let mut line = start_line;
    let mut column = 1;
    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn is_opaque_json_return(raw: &str) -> bool {
    let compact: String = raw.chars().filter(|ch| !ch.is_whitespace()).collect();
    matches!(
        compact.as_str(),
        "interface{}" | "any" | "lazuli.JSON" | "[]byte"
    )
}

fn strip_named_return(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut parts = trimmed.split_whitespace();
    let Some(first) = parts.next() else {
        return String::new();
    };
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        return trimmed.to_owned();
    }
    if is_go_identifier(first) && !is_known_go_type_start(first) {
        return rest.join(" ");
    }
    trimmed.to_owned()
}

fn is_go_identifier(raw: &str) -> bool {
    let mut chars = raw.chars();
    matches!(chars.next(), Some(ch) if ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_known_go_type_start(raw: &str) -> bool {
    matches!(
        raw,
        "interface"
            | "any"
            | "map"
            | "chan"
            | "func"
            | "error"
            | "string"
            | "bool"
            | "int"
            | "int64"
            | "float64"
            | "struct"
    ) || raw.starts_with("[]")
        || raw.contains('.')
}

fn returns_list_location(source: &str, header_line: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let start = header_line.saturating_sub(1);
    let mut end = start + 1;
    while end < lines.len() && super::leading_spaces(lines[end]) > 2 {
        end += 1;
    }
    for (idx, line) in lines.iter().enumerate().take(end).skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("returns list ") || trimmed.starts_with("returns list\t") {
            let column = line.find("returns").map(|col| col + 1).unwrap_or(1);
            return Some((idx + 1, column));
        }
    }
    None
}

fn type_ref_label(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => builtin_label(builtin).to_owned(),
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => qn.name.clone(),
        TypeRef::Many(inner) => format!("list {}", type_ref_label(inner)),
        TypeRef::Capability(_) => "@cap.*".to_owned(),
        TypeRef::Unresolved(raw) => raw.clone(),
    }
}

fn builtin_label(builtin: &BuiltinType) -> &'static str {
    match builtin {
        BuiltinType::Id => "ID",
        BuiltinType::Text => "Text",
        BuiltinType::Boolean => "Boolean",
        BuiltinType::Integer => "Integer",
        BuiltinType::Decimal => "Decimal",
        BuiltinType::Date => "Date",
        BuiltinType::DateTime => "DateTime",
        BuiltinType::Json => "JSON",
        BuiltinType::SemanticEmail => "Email",
        BuiltinType::SemanticMoney { .. } => "Money",
        BuiltinType::SemanticPhone => "Phone",
        BuiltinType::SemanticUrl => "Url",
        BuiltinType::SemanticUuid => "Uuid",
        BuiltinType::SemanticCurrency => "Currency",
        BuiltinType::SemanticGeoPoint => "GeoPoint",
        BuiltinType::SemanticPluginType { .. } => "SemanticPluginType",
        BuiltinType::CapSecret => "@cap.Secret",
        BuiltinType::CapFile => "@cap.File",
    }
}

fn go_type_for_stub(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => go_type_for_builtin(builtin),
        TypeRef::Capability(capability) => go_type_for_capability(capability).to_owned(),
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) if qn.name.trim() == "Empty" => {
            "struct{}".to_owned()
        }
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => pascal_case(&qn.name),
        TypeRef::Many(inner) => format!("[]{}", go_type_for_stub(inner)),
        TypeRef::Unresolved(raw) if raw.trim() == "Empty" => "struct{}".to_owned(),
        TypeRef::Unresolved(raw) => raw.trim().to_owned(),
    }
}

fn go_type_for_builtin(builtin: &BuiltinType) -> String {
    match builtin {
        BuiltinType::Id => "lazuli.ID".to_owned(),
        BuiltinType::Text => "string".to_owned(),
        BuiltinType::Boolean => "bool".to_owned(),
        BuiltinType::Integer => "int64".to_owned(),
        BuiltinType::Decimal => "float64".to_owned(),
        BuiltinType::Date => "lazuli.Date".to_owned(),
        BuiltinType::DateTime => "lazuli.Time".to_owned(),
        BuiltinType::Json => "lazuli.JSON".to_owned(),
        BuiltinType::SemanticEmail => "lazuli.Email".to_owned(),
        BuiltinType::SemanticMoney { .. } => "lazuli.Money".to_owned(),
        BuiltinType::SemanticPhone => "lazuli.Phone".to_owned(),
        BuiltinType::SemanticUrl => "lazuli.URL".to_owned(),
        BuiltinType::SemanticUuid => "lazuli.UUID".to_owned(),
        BuiltinType::SemanticCurrency => "lazuli.Currency".to_owned(),
        BuiltinType::SemanticGeoPoint => "any".to_owned(),
        BuiltinType::SemanticPluginType { carrier, .. } => go_type_for_builtin(carrier),
        BuiltinType::CapSecret => "lazuli.Secret".to_owned(),
        BuiltinType::CapFile => "any".to_owned(),
    }
}

fn go_type_for_capability(capability: &CapabilityRef) -> &'static str {
    match capability {
        CapabilityRef::Hashed(_) => "lazuli.HashedRef",
        CapabilityRef::Encrypted(_) | CapabilityRef::E2ee(_) => "lazuli.EncryptedRef",
        CapabilityRef::Token(_) => "lazuli.TokenRef",
        CapabilityRef::PII(_) => "string",
        CapabilityRef::File(_) => "any",
    }
}

fn qualify_generated_stub_type(raw: &str, gen_alias: &str) -> (String, bool) {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix("[]") {
        let (qualified, used) = qualify_generated_stub_type(inner, gen_alias);
        return (format!("[]{qualified}"), used);
    }
    if is_builtin_stub_type(trimmed) || trimmed.contains('.') || trimmed.starts_with("map[") {
        return (trimmed.to_owned(), false);
    }
    if trimmed
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        return (format!("{gen_alias}.{trimmed}"), true);
    }
    (trimmed.to_owned(), false)
}

fn is_builtin_stub_type(raw: &str) -> bool {
    matches!(
        raw,
        "any" | "string" | "bool" | "int" | "int64" | "float64" | "struct{}" | "error"
    )
}

fn gen_package_name(feature: &str) -> String {
    format!("{feature}gen")
}

fn path_name_for(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

fn exported_func_name(name: &str) -> String {
    let pascal = pascal_case(name);
    if pascal.is_empty() {
        return "Handler".to_owned();
    }
    if pascal
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        return format!("Handler{pascal}");
    }
    pascal
}

fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in split_words(s) {
        if word.is_empty() {
            continue;
        }
        if is_acronym(&word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for upper in first.to_uppercase() {
                out.push(upper);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl" | "uuid"
    )
}

fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (idx, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if ch.is_ascii_uppercase() {
            let prev_lower =
                idx > 0 && (chars[idx - 1].is_ascii_lowercase() || chars[idx - 1].is_ascii_digit());
            let next_lower = idx + 1 < chars.len() && chars[idx + 1].is_ascii_lowercase();
            if !current.is_empty() && (prev_lower || next_lower) {
                if !next_lower {
                    current.push(ch);
                    continue;
                }
                words.push(std::mem::take(&mut current));
            }
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        CommandInput, CommandKind, HandlerRef, PolicyRef, QualifiedName, ReturnsEffect,
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
