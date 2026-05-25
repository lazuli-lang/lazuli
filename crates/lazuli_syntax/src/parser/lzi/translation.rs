//! `translation` block parser + the `@translation.<key>` reference token.
//!
//! Translation blocks live as feature children (indent 2 under a feature
//! header) and carry a single `catalog "<path>"` line plus any number of
//! `key <name>` sub-blocks. Each key declares one BCP-47 variant per
//! locale, optionally with `plural <arm>` sub-blocks for ordinal /
//! cardinal arms (`one`, `few`, `many`, ...).
//!
//! ## Two contracts
//!
//! - `parse_translation_decl` — the block parser invoked from the legacy
//!   feature walker. Returns a fully-resolved `TranslationDecl`.
//! - `parse_translation_key_token` — the IR Error-Vocab single-token
//!   reader (Cell PARSE-1). Extracts the bare key from a literal
//!   `@translation.<key>` token used inside rule messages, when-denied
//!   policies, and other surface forms that quote a translation key
//!   inline. Re-exported as `pub use` so `lzx/mod.rs` can call it.
//!
//! ## Indentation contract
//!
//! Translation blocks honour the shared agent-indent constants from the
//! parent module:
//!
//! - feature child header (indent 2): `translation`
//! - block children (indent 4): `catalog`, `key`
//! - key body (indent 6): BCP-47 variants, `plural <arm>`
//! - plural body (indent 8): BCP-47 variants
//!
//! ## See also
//!
//! - `docs/proposals/i18n-translation-vocab.md`
//! - `lazuli_ir::nodes::translation`

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD,
    AGENT_INDENT_GREAT_GRANDCHILD,
};

use crate::ast::{
    Span, TranslationDecl, TranslationKeyDecl, TranslationKeyRefAst, TranslationPluralArmDecl,
    TranslationVariantDecl,
};

/// i18n bucket cycle — parse a `translation` block. Header is the
/// bare keyword (no name). Children at indent 4: `catalog "<path>"`
/// (required, exactly one) and `key <name>` (repeatable). Inside a
/// `key <name>`, indent 6 carries BCP-47 variants (`pt-BR "..."`) and
/// optional `plural <arm>` blocks; inside `plural <arm>`, indent 8
/// carries another set of BCP-47 variants.
pub(super) fn parse_translation_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(TranslationDecl, usize), ParseError> {
    let header = &lines[start];
    let mut catalog: Option<String> = None;
    let mut keys: Vec<TranslationKeyDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "translation body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("catalog ") {
            if catalog.is_some() {
                return Err(line_error(
                    line,
                    "`translation` may declare at most one `catalog` line",
                ));
            }
            catalog = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("key ") {
            let name = rest.trim().to_owned();
            if name.is_empty() {
                return Err(line_error(line, "`key` requires a name"));
            }
            let (key, next) = parse_translation_key(lines, i, name)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            keys.push(key);
            i = next;
        } else {
            return Err(line_error(
                line,
                "translation children are `catalog \"<path>\"` and `key <name>`",
            ));
        }
    }

    let catalog = catalog.ok_or_else(|| {
        line_error(
            header,
            "`translation` requires a `catalog \"<path>\"` declaration",
        )
    })?;

    Ok((
        TranslationDecl {
            catalog,
            keys,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_translation_key(
    lines: &[SourceLine<'_>],
    start: usize,
    name: String,
) -> Result<(TranslationKeyDecl, usize), ParseError> {
    let header = &lines[start];
    let mut variants: Vec<TranslationVariantDecl> = Vec::new();
    let mut plurals: Vec<TranslationPluralArmDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "translation key children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("plural ") {
            let arm = rest.trim().to_owned();
            if arm.is_empty() {
                return Err(line_error(line, "`plural` requires an arm name"));
            }
            let (plural, next) = parse_translation_plural(lines, i, arm)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            plurals.push(plural);
            i = next;
        } else if let Some((locale, rest)) = trimmed.split_once(' ') {
            let text = unquote_lzx_value(rest.trim()).to_owned();
            variants.push(TranslationVariantDecl {
                locale: locale.to_owned(),
                text,
            });
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "translation key body lines are `<bcp47-tag> \"<text>\"` or `plural <arm>`",
            ));
        }
    }

    Ok((
        TranslationKeyDecl {
            name,
            variants,
            plurals,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_translation_plural(
    lines: &[SourceLine<'_>],
    start: usize,
    arm: String,
) -> Result<(TranslationPluralArmDecl, usize), ParseError> {
    let header = &lines[start];
    let mut variants: Vec<TranslationVariantDecl> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_GRANDCHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GREAT_GRANDCHILD {
            return Err(line_error(
                line,
                "plural arm body uses eight-space indentation",
            ));
        }
        if let Some((locale, rest)) = trimmed.split_once(' ') {
            let text = unquote_lzx_value(rest.trim()).to_owned();
            variants.push(TranslationVariantDecl {
                locale: locale.to_owned(),
                text,
            });
            i += 1;
        } else {
            return Err(line_error(
                line,
                "plural arm body lines are `<bcp47-tag> \"<text>\"`",
            ));
        }
    }
    // `header` is read above for span anchoring; this fn returns the
    // plural arm tied to the source range tracked by the caller.
    let _ = header;

    Ok((TranslationPluralArmDecl { arm, variants }, i))
}

/// IR Error-Vocab (Cell PARSE-1) — extract the bare key from a literal
/// `@translation.<key>` token. Trailing whitespace and trailing tokens
/// past the first whitespace are rejected; the surface intentionally
/// keeps the form single-token to stay inside the existing
/// `Rule.message @translation.<key>` precedent.
pub(crate) fn parse_translation_key_token(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<TranslationKeyRefAst, ParseError> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return Err(line_error(
            line,
            "`@translation.<key>` reference requires a key",
        ));
    }
    let mut parts = trimmed.split_whitespace();
    let first = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`@translation.<key>` reference must be a single token",
        ));
    }
    let key = first.strip_prefix("@translation.").ok_or_else(|| {
        line_error(
            line,
            "expected `@translation.<key>` reference (must start with `@translation.`)",
        )
    })?;
    if key.is_empty() {
        return Err(line_error(
            line,
            "`@translation.` reference requires a non-empty key after the dot",
        ));
    }
    Ok(TranslationKeyRefAst {
        key: key.to_owned(),
        span: Span::new(line.start, line.end),
    })
}
