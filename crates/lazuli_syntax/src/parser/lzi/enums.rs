//! `enum <Name>` declaration parser — closed-catalog variant lists
//! with optional storage values + colon metadata (label/hint/icon).
//!
//! The fixture authors enums inside `domain` at indent 4 (header) with
//! variants at indent 6. A variant is either `name` or `name = <value>`
//! where the value is a bare integer or a quoted string. Wave A.5 adds
//! optional colon metadata:
//!
//!   name: label @translation.key, hint @translation.hint, icon "user"

use super::super::common::{SourceLine, is_trivia, line_error, strip_inline_comment};
use super::super::error::ParseError;
use crate::ast::{EnumDeclAst, EnumStorageValueDecl, EnumVariantDecl, Span};

pub(super) fn parse_enum_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(EnumDeclAst, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let name = trimmed
        .strip_prefix("enum ")
        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
        .ok_or_else(|| line_error(header, "enum header must be `enum <Name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "enum header requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;

    let mut variants: Vec<EnumVariantDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw_body = line.text.trim_start();
        if is_trivia(raw_body) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`enum` variants use one indentation level deeper than the header",
            ));
        }
        let body = strip_inline_comment(raw_body).trim_end();
        let (main_body, metadata_body) = match find_enum_metadata_separator(body) {
            Some(idx) => (body[..idx].trim_end(), Some(body[idx + 1..].trim())),
            None => (body, None),
        };
        let (label_key, hint_key, icon_key) = match metadata_body {
            Some(metadata) => parse_enum_variant_metadata(line, metadata)?,
            None => (None, None, None),
        };
        let (variant_name, storage) = match main_body.split_once('=') {
            Some((lhs, rhs)) => {
                let var_name = lhs.trim().to_owned();
                let raw = rhs.trim();
                let storage = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
                    Some(EnumStorageValueDecl::String(
                        raw[1..raw.len() - 1].to_owned(),
                    ))
                } else if let Ok(n) = raw.parse::<i64>() {
                    Some(EnumStorageValueDecl::Integer(n))
                } else {
                    return Err(line_error(
                        line,
                        "enum variant value must be an integer or a quoted string",
                    ));
                };
                (var_name, storage)
            }
            None => (main_body.to_owned(), None),
        };
        if variant_name.is_empty() {
            return Err(line_error(line, "enum variant requires a name"));
        }
        variants.push(EnumVariantDecl {
            name: variant_name,
            storage,
            label_key,
            hint_key,
            icon_key,
            span: Span::new(line.start, line.end),
        });
        last_end = line.end;
        i += 1;
    }

    Ok((
        EnumDeclAst {
            name,
            public_contract: None,
            variants,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn find_enum_metadata_separator(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut in_quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match in_quote {
            Some(q) if b == q => in_quote = None,
            Some(_) if b == b'\\' && i + 1 < bytes.len() => i += 1,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => in_quote = Some(b),
            None if b == b':' => return Some(i),
            None => {}
        }
        i += 1;
    }
    None
}

fn parse_enum_variant_metadata(
    line: &SourceLine<'_>,
    metadata: &str,
) -> Result<(Option<String>, Option<String>, Option<String>), ParseError> {
    if metadata.trim().is_empty() {
        return Err(line_error(
            line,
            "enum variant metadata requires `label <key>`",
        ));
    }

    let mut label_key: Option<String> = None;
    let mut hint_key: Option<String> = None;
    let mut icon_key: Option<String> = None;

    for part in split_enum_metadata_commas(metadata) {
        let item = part.trim();
        if item.is_empty() {
            return Err(line_error(
                line,
                "enum variant metadata contains an empty item",
            ));
        }
        if let Some(rest) = item.strip_prefix("label ") {
            if label_key.is_some() {
                return Err(line_error(line, "duplicate enum variant `label` metadata"));
            }
            label_key = Some(parse_enum_metadata_key(line, rest, "label")?);
        } else if let Some(rest) = item.strip_prefix("hint ") {
            if hint_key.is_some() {
                return Err(line_error(line, "duplicate enum variant `hint` metadata"));
            }
            hint_key = Some(parse_enum_metadata_key(line, rest, "hint")?);
        } else if let Some(rest) = item.strip_prefix("icon ") {
            if icon_key.is_some() {
                return Err(line_error(line, "duplicate enum variant `icon` metadata"));
            }
            icon_key = Some(parse_enum_icon_key(line, rest)?);
        } else {
            return Err(line_error(
                line,
                "enum variant metadata items are `label <key>`, `hint <key>`, or `icon \"<name>\"`",
            ));
        }
    }

    if label_key.is_none() {
        return Err(line_error(
            line,
            "enum variant metadata requires `label <key>` before `hint` or `icon`",
        ));
    }

    Ok((label_key, hint_key, icon_key))
}

fn split_enum_metadata_commas(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut in_quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match in_quote {
            Some(q) if b == q => in_quote = None,
            Some(_) if b == b'\\' && i + 1 < bytes.len() => i += 1,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => in_quote = Some(b),
            None if b == b',' => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            None => {}
        }
        i += 1;
    }
    out.push(&text[start..]);
    out
}

fn parse_enum_metadata_key(
    line: &SourceLine<'_>,
    raw: &str,
    kind: &'static str,
) -> Result<String, ParseError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(line_error(
            line,
            "enum variant metadata key cannot be empty",
        ));
    }
    let mut parts = trimmed.split_whitespace();
    let token = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "enum variant metadata keys must be single tokens",
        ));
    }
    if let Some(key) = token.strip_prefix("@translation.") {
        if key.is_empty() {
            return Err(line_error(
                line,
                "`@translation.` enum metadata reference requires a key",
            ));
        }
        return Ok(key.to_owned());
    }
    if token.starts_with('@') {
        return Err(line_error(
            line,
            "enum variant metadata keys must be bare keys or `@translation.<key>` references",
        ));
    }
    if token.is_empty() {
        return Err(line_error(
            line,
            "enum variant metadata key cannot be empty",
        ));
    }
    if kind == "label" || kind == "hint" {
        Ok(token.to_owned())
    } else {
        // Callers gate `kind` to {"label", "hint"}. Surface as a parse
        // error rather than a panic if a future regression breaks that.
        Err(line_error(line, "unsupported enum metadata kind"))
    }
}

fn parse_enum_icon_key(line: &SourceLine<'_>, raw: &str) -> Result<String, ParseError> {
    let trimmed = raw.trim();
    if !(trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2) {
        return Err(line_error(
            line,
            "enum variant `icon` metadata expects a quoted string",
        ));
    }
    Ok(trimmed[1..trimmed.len() - 1].to_owned())
}

#[cfg(test)]
mod enum_metadata_parser_tests {
    use super::super::parse_feature_skeletons;
    use crate::ast::EnumStorageValueDecl;

    #[test]
    fn enum_metadata_parses_label_hint_icon_combinations() {
        let source = r#"
feature account
  domain
    enum Gender
      male: label @translation.gender_male
      female: label @translation.gender_female, icon "user"
      non_binary: label gender_non_binary, hint @translation.gender_non_binary_hint
      prefer_not: label @translation.gender_prefer_not, hint @translation.gender_prefer_not_hint, icon "eye-off"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let variants = &features[0].enums[0].variants;

        assert_eq!(variants[0].name, "male");
        assert_eq!(variants[0].label_key.as_deref(), Some("gender_male"));
        assert_eq!(variants[0].hint_key, None);
        assert_eq!(variants[0].icon_key, None);

        assert_eq!(variants[1].label_key.as_deref(), Some("gender_female"));
        assert_eq!(variants[1].icon_key.as_deref(), Some("user"));
        assert_eq!(variants[1].hint_key, None);

        assert_eq!(variants[2].label_key.as_deref(), Some("gender_non_binary"));
        assert_eq!(
            variants[2].hint_key.as_deref(),
            Some("gender_non_binary_hint")
        );
        assert_eq!(variants[2].icon_key, None);

        assert_eq!(variants[3].label_key.as_deref(), Some("gender_prefer_not"));
        assert_eq!(
            variants[3].hint_key.as_deref(),
            Some("gender_prefer_not_hint")
        );
        assert_eq!(variants[3].icon_key.as_deref(), Some("eye-off"));
    }

    #[test]
    fn enum_metadata_preserves_bare_variants_and_storage_values() {
        let source = r#"
feature account
  domain
    enum Status
      draft
      active = "live": label @translation.status_active, icon "check"
      archived = 9: label status_archived
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let variants = &features[0].enums[0].variants;

        assert_eq!(variants[0].name, "draft");
        assert_eq!(variants[0].storage, None);
        assert_eq!(variants[0].label_key, None);

        assert_eq!(variants[1].name, "active");
        assert_eq!(
            variants[1].storage,
            Some(EnumStorageValueDecl::String("live".to_owned()))
        );
        assert_eq!(variants[1].label_key.as_deref(), Some("status_active"));
        assert_eq!(variants[1].icon_key.as_deref(), Some("check"));

        assert_eq!(variants[2].name, "archived");
        assert_eq!(variants[2].storage, Some(EnumStorageValueDecl::Integer(9)));
        assert_eq!(variants[2].label_key.as_deref(), Some("status_archived"));
    }

    #[test]
    fn enum_metadata_rejects_hint_or_icon_without_label() {
        let source = r#"
feature account
  domain
    enum Status
      active: icon "check"
"#;
        let err = parse_feature_skeletons(source).expect_err("rejects missing label");
        assert!(err.to_string().contains("requires `label <key>`"), "{err}");
    }
}
