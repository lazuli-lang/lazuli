//! Parser-driven semantic-token classifier (Wave H4).
//!
//! There is no lexer / token stream in this crate — "each parser IS the
//! spec" (see `lzi/mod.rs`). The only shared scanning primitive is the
//! indent-aware [`SourceLine`] line-span plus the ad-hoc byte scanners in
//! [`super::common`]. So instead of re-running the recursive-descent
//! parsers (which produce an AST, not positioned tokens), this module
//! builds a small, *correct-where-it-fires* positioned-token classifier
//! directly over the line stream.
//!
//! ## Approach — re-deriving tokens without a lexer
//!
//! [`classify_tokens`] walks one [`SourceLine`] at a time and scans it
//! left-to-right into byte-anchored word tokens, classifying each by a
//! *closed*, conservative set of rules:
//!
//! 1. **Comments** — a `#` outside any quote ends the line; the rest is
//!    one [`SemanticToken::Comment`] span. (`#` inside `"..."`/`'...'`
//!    stays put, mirroring [`super::common::strip_inline_comment`].)
//! 2. **Strings** — `"..."` / `'...'` spans classify as
//!    [`SemanticToken::String`]. **Nothing inside a string is ever
//!    re-scanned**, so a keyword that happens to sit inside a string
//!    literal stays `String` — never mis-classified as a keyword.
//! 3. **`@`-sigils** — `@head` looks up the registry decorator row; the
//!    `@head` namespace span is [`SemanticToken::Decorator`]. A
//!    `.TypeName` suffix (`@semantic.HexColor`, `@cap.File`) classifies
//!    the uppercase-initial suffix as [`SemanticToken::Type`]. A
//!    lowercase suffix (`@policy.create`, `@fn.foo`) is left to the
//!    tmLanguage fallback — under-classify rather than mis-classify.
//! 4. **Registry keyword literals** — a bare word that resolves to a
//!    [`lazuli_keywords::find`] row carrying its `.token` is classified as
//!    that token **only in a position where the classification is
//!    unambiguous**: the head (first non-trivia) word of a line, or a
//!    dotted-kind head (`query.list`). This is the conservative core: a
//!    word like `name` is both a `storage.modifier` connector *and* a
//!    cookie-block statement keyword; classifying it by registry lookup
//!    in arbitrary mid-line position would risk emitting the wrong token,
//!    so mid-line bare words are left to the fallback.
//! 5. **Uppercase-initial type refs** — a word matching `IDENT_UPPER`
//!    (e.g. the `Org` / `Text` in `org: Org required`, or a resource
//!    name `Customer`) classifies as [`SemanticToken::Type`].
//!
//! Everything not covered by these rules (numbers, field names,
//! mid-line modifiers/operators, bound variables, …) is **deliberately
//! left unclassified** and falls through to the static tmLanguage grammar
//! — the design's documented fallback. The invariant this module upholds
//! is: *when it fires, it is correct*; it under-classifies by design.
//!
//! Note on numbers: [`SemanticToken`] (the registry enum that drives the
//! LSP legend) has no numeric variant, so this classifier never emits a
//! number token — numbers stay a tmLanguage `constant.numeric` concern.
//! This keeps the legend's token-type set exactly the registry's variants.

use lazuli_keywords::{SemanticToken, find};

use super::common::{SourceLine, source_lines};

/// One positioned, classified token. Byte-range anchored
/// (`line`/`start_col`/`len` are all 0-based byte offsets within the line)
/// plus the registry's [`SemanticToken`] type. Carries no `tower-lsp`
/// types — the LSP layer maps `(line, start_col, len, token)` to the
/// protocol's relative-delta encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassifiedToken {
    /// 0-based line index.
    pub line: usize,
    /// 0-based byte offset of the token start within the line.
    pub start_col: usize,
    /// Token length in bytes.
    pub len: usize,
    /// The classified semantic-token type.
    pub token: SemanticToken,
}

/// Classify `source` into a flat, line-ordered, non-overlapping list of
/// positioned semantic tokens.
///
/// Pure: same input → same output, no I/O, no allocation beyond the
/// result `Vec`. The result is ordered by `(line, start_col)` and every
/// token is non-overlapping (within a line each token's
/// `[start_col, start_col+len)` is disjoint from its neighbours), which is
/// exactly the precondition the LSP delta encoder needs.
///
/// It does **not** classify everything — see the module docs. It is
/// correct where it fires; anything it leaves out is covered by the
/// static tmLanguage grammar.
///
/// ## Examples
///
/// ```
/// use lazuli_syntax::{classify_tokens, ClassifiedToken};
/// use lazuli_keywords::SemanticToken;
///
/// let toks = classify_tokens("feature billing\n");
/// // `feature` is a top-level declaration keyword.
/// assert_eq!(toks[0].token, SemanticToken::Keyword);
/// assert_eq!((toks[0].line, toks[0].start_col, toks[0].len), (0, 0, 7));
/// // `Billing`-style uppercase name would be a Type; `billing` (lower)
/// // is a feature name left to the fallback.
/// ```
pub fn classify_tokens(source: &str) -> Vec<ClassifiedToken> {
    let mut out = Vec::new();
    for (line_idx, line) in source_lines(source).iter().enumerate() {
        classify_line(line_idx, line, &mut out);
    }
    out
}

/// Classify a single source line, pushing tokens onto `out`.
fn classify_line(line_idx: usize, line: &SourceLine<'_>, out: &mut Vec<ClassifiedToken>) {
    let bytes = line.text.as_bytes();
    let mut i = 0usize;
    // Index, within the line, of the first non-whitespace token start —
    // used to decide whether a bare-keyword lookup is in "head" position.
    let mut first_word_start: Option<usize> = None;

    while i < bytes.len() {
        let b = bytes[i];

        // Whitespace — skip.
        if b == b' ' || b == b'\t' {
            i += 1;
            continue;
        }

        // Comment — `#` outside a quote ends the line.
        if b == b'#' {
            let len = bytes.len() - i;
            if len > 0 {
                out.push(ClassifiedToken {
                    line: line_idx,
                    start_col: i,
                    len,
                    token: SemanticToken::Comment,
                });
            }
            return;
        }

        // String literal — `"..."` or `'...'`. Scan to the matching
        // (unescaped) close quote, or to end-of-line if unterminated.
        if b == b'"' || b == b'\'' {
            let start = i;
            let quote = b;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(ClassifiedToken {
                line: line_idx,
                start_col: start,
                len: i - start,
                token: SemanticToken::String,
            });
            continue;
        }

        // A non-whitespace, non-comment, non-string word starts here.
        let word_start = i;
        let is_head = first_word_start.is_none();
        if first_word_start.is_none() {
            first_word_start = Some(word_start);
        }

        // `@`-sigil decorator.
        if b == b'@' {
            i = classify_at_token(line_idx, line.text, word_start, out);
            continue;
        }

        // Bare word — scan to the next word boundary (whitespace, quote,
        // `#`, or an opening bracket / colon that ends the head token).
        let word_end = scan_word_end(bytes, word_start);
        if word_end == word_start {
            // `word_start` sits on a boundary punctuation char that
            // `scan_word_end` won't consume (e.g. the `(` / `,` / `)` of a
            // decorator arg list like `@doctor.allow(CODE, ...)`). It is not
            // part of a word and carries no token — skip exactly one byte so
            // the scanner always makes progress (else: infinite loop on the
            // unconsumable char). Punctuation is left unclassified by design.
            i += 1;
            continue;
        }
        let word = &line.text[word_start..word_end];
        i = word_end;

        classify_bare_word(line_idx, word, word_start, is_head, out);
    }
}

/// Scan to the end of a bare word starting at `start`. A word runs until
/// whitespace, a quote, a `#`, or a bracket — but a `.` is included so
/// dotted kinds (`query.list`, `event.trace`) stay one token.
fn scan_word_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'"' | b'\'' | b'#' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' => {
                break;
            }
            _ => i += 1,
        }
    }
    i
}

/// Classify an `@head[.suffix]` sigil token. Returns the byte index just
/// past the consumed token. `at` is the byte offset of the `@`.
fn classify_at_token(
    line_idx: usize,
    text: &str,
    at: usize,
    out: &mut Vec<ClassifiedToken>,
) -> usize {
    let bytes = text.as_bytes();
    let token_end = scan_word_end(bytes, at);
    let token = &text[at..token_end];

    // Spec 0028 — `@doctor.allow(...)` is a DOTTED decorator head (the registry
    // literal is `@doctor.allow`, not `@doctor`). The generic single-dot split
    // below would yield `@doctor`, which is not a registry row, so recognize the
    // dotted head explicitly and classify it as the annotation/decorator token.
    if token.starts_with("@doctor.allow") {
        out.push(ClassifiedToken {
            line: line_idx,
            start_col: at,
            len: "@doctor.allow".len(),
            token: SemanticToken::Decorator,
        });
        return token_end;
    }

    // Split `@head` from an optional `.suffix` (`@semantic.HexColor`).
    let head_end = token.find('.').map(|d| at + d).unwrap_or(token_end);
    let head = &text[at..head_end]; // includes the leading `@`

    // The `@head` must resolve to a registry decorator row to fire; an
    // unknown `@foo` is left to the fallback (under-classify).
    let head_is_known = find(head)
        .map(|spec| spec.token == SemanticToken::Decorator)
        .unwrap_or(false);

    if head_is_known {
        out.push(ClassifiedToken {
            line: line_idx,
            start_col: at,
            len: head_end - at,
            token: SemanticToken::Decorator,
        });

        // A dotted `.TypeName` suffix with an uppercase initial is a type
        // reference (`@semantic.HexColor`, `@cap.File`). A lowercase
        // suffix (`@policy.create`, `@fn.foo`) is a reference name we
        // leave to the fallback.
        if head_end < token_end {
            let suffix_start = head_end + 1; // skip the `.`
            if suffix_start < token_end {
                let suffix = &text[suffix_start..token_end];
                if is_ident_upper(suffix) {
                    out.push(ClassifiedToken {
                        line: line_idx,
                        start_col: suffix_start,
                        len: token_end - suffix_start,
                        token: SemanticToken::Type,
                    });
                }
            }
        }
    }

    token_end
}

/// Classify a bare (non-sigil) word.
///
/// * An uppercase-initial identifier (`Org`, `Text`, `Customer`) is a
///   [`SemanticToken::Type`] reference — safe anywhere because the grammar
///   only uses `IDENT_UPPER` for type / resource / entity names.
/// * Otherwise, **only in head position**, a registry lookup classifies
///   the word as its `.token`. Mid-line bare words are left to the
///   fallback to avoid mis-classifying context-sensitive connectors.
fn classify_bare_word(
    line_idx: usize,
    word: &str,
    start: usize,
    is_head: bool,
    out: &mut Vec<ClassifiedToken>,
) {
    if word.is_empty() {
        return;
    }

    // Uppercase-initial → type reference. Correct anywhere in the grammar.
    if is_ident_upper(word) {
        out.push(ClassifiedToken {
            line: line_idx,
            start_col: start,
            len: word.len(),
            token: SemanticToken::Type,
        });
        return;
    }

    if !is_head {
        // Mid-line bare word: context-sensitive — leave to the fallback.
        return;
    }

    // Head word: classify via the registry if it resolves to a row whose
    // token is one we can emit unambiguously in declaration/statement
    // head position. We restrict to Keyword (and its decorator/dotted
    // siblings already handled): a head word that the registry knows as a
    // Keyword is a declaration/section/statement opener, which is exactly
    // what head position means.
    if let Some(spec) = find(word) {
        // Other token kinds (Modifier / Operator / EnumMember) are
        // context-sensitive even in head position (e.g. a policy expression can
        // begin with the `not` operator); under-classify and leave them to the
        // tmLanguage fallback.
        if spec.token == SemanticToken::Keyword {
            out.push(ClassifiedToken {
                line: line_idx,
                start_col: start,
                len: word.len(),
                token: SemanticToken::Keyword,
            });
        }
    }
}

/// `IDENT_UPPER`: an ASCII uppercase initial followed by ASCII
/// alphanumerics / underscores. Matches the grammar's type/entity-name
/// shape (`Org`, `Text`, `Customer`, `HexColor`).
fn is_ident_upper(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    include!("highlight_tests.rs");
}
