struct RecoveredFieldType {
    type_text: String,
    required: bool,
    unique: bool,
}

/// FR-PII-STACK — when `@cap.PII(...)` is authored after field modifiers,
/// the syntax parser leaves `required|optional|unique` inside `type_text`
/// because it only peels final bare tokens. Recover those modifiers after
/// removing the field-level decorator so the existing parser remains stable.
fn peel_trailing_field_modifiers(text: &str) -> RecoveredFieldType {
    let mut head = text.trim().to_owned();
    let mut required = false;
    let mut unique = false;
    loop {
        let trimmed = head.trim_end();
        if let Some(rest) = trimmed.strip_suffix(" required") {
            required = true;
            head = rest.to_owned();
        } else if let Some(rest) = trimmed.strip_suffix(" optional") {
            head = rest.to_owned();
        } else if let Some(rest) = trimmed.strip_suffix(" unique") {
            unique = true;
            head = rest.to_owned();
        } else {
            head = trimmed.to_owned();
            break;
        }
    }
    RecoveredFieldType {
        type_text: head,
        required,
        unique,
    }
}

/// FR-PII-STACK — peel a non-leading `@cap.PII(...)` marker out of a
/// resource/record field's type tail and lower it into `Field.pii`. Leading
/// `@cap.PII(...)` remains a normal capability `TypeRef` for fields whose
/// only carrier is PII.
fn extract_field_level_pii_decorator(type_text: &str) -> (String, Option<ir::PiiCapability>) {
    let original = type_text.trim().to_owned();
    let Some((start, end)) = find_field_level_cap_pii_span(type_text) else {
        return (original, None);
    };
    let before = type_text[..start].trim_end();
    if before.is_empty() {
        return (original, None);
    }
    let token = &type_text[start..end];
    let Some(pii) = parse_cap_pii_type(token) else {
        return (original, None);
    };
    let after = type_text[end..].trim_start();
    let mut cleaned = before.to_owned();
    if !cleaned.is_empty() && !after.is_empty() {
        cleaned.push(' ');
    }
    cleaned.push_str(after);
    (cleaned.trim().to_owned(), Some(pii))
}

fn find_field_level_cap_pii_span(text: &str) -> Option<(usize, usize)> {
    const PREFIX: &[u8] = b"@cap.PII(";
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0usize;
    while i + PREFIX.len() <= bytes.len() {
        let ch = bytes[i] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if depth == 0 && &bytes[i..i + PREFIX.len()] == PREFIX {
            let before_ok = i == 0 || (bytes[i - 1] as char).is_whitespace();
            if before_ok
                && let Some(end) = find_balanced_decorator_end(text, i) {
                    return Some((i, end));
                }
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Test-only `validate <profile>` line lowering. Used by `lib.rs::tests`
/// to exercise the lift path without spinning up a full resource AST.
#[cfg(test)]
pub(crate) fn lower_validate_line(line: &str) -> Result<ir::FieldConstraints, AnalyzeError> {
    let trimmed = line.trim();
    let mut decl = syntax::FieldConstraintsDecl::default();
    if let Some(rest) = trimmed.strip_prefix("validate sanitize_html(") {
        let profile = rest.trim_end_matches(')').trim();
        decl.sanitize_html = Some(profile.to_owned());
    } else if trimmed == "validate utf8_safe" {
        decl.utf8_safe = Some(true);
    } else if let Some(rest) = trimmed.strip_prefix("validate max_recursion:") {
        decl.max_recursion = rest.trim().parse::<u32>().ok();
    } else if let Some(rest) = trimmed.strip_prefix("validate max_size:") {
        decl.max_size = rest.trim().parse::<u64>().ok();
    } else if trimmed == "validator covers_pii" {
        decl.covers_pii = Some("covers_pii".to_owned());
    } else {
        return Ok(ir::FieldConstraints::default());
    };
    lift_field_constraints("validate", &decl)
}
