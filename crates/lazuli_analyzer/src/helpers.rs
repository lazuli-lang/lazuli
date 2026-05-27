//! Pure utility helpers shared across the lowering submodules.
//!
//! ## Why this slot exists
//!
//! The analyzer's lowering pipeline (feature, lzx, command, workflow)
//! reuses a small set of generic string-shape predicates and span
//! conversions. Pulling them into a single sibling module keeps the
//! domain-heavy slices focused on what makes them distinct: the AST
//! → IR projection rules. Helpers here carry no domain semantics —
//! they are mechanical text munging or IR-construction primitives
//! that any slot can call.
//!
//! ## What lives here
//!
//! * Span conversion (`span_of`) — bridge from `syntax::Span` to
//!   `ir::SpanRef`. Used by every lowering function that records
//!   provenance.
//! * Case conversion (`pascal_to_snake`, `snake_to_pascal`) — used by
//!   conventions synth (resource name → table column) and SQL builders.
//! * Edit-distance (`levenshtein`, `conventions_levenshtein`) — drives
//!   the nearest-name suggestion in `crud_synth_*` and conventions
//!   diagnostics.
//! * Text shape predicates (`strip_quotes`, `find_word`,
//!   `find_top_level_operator`, `has_top_level_comma`,
//!   `first_paren_balanced_token`, `find_balanced_decorator_end`,
//!   `find_keyword_line_offset`, `is_valid_design_hex`).
//! * SQL quoting (`quoted_table`, `quoted_ident`) — minimal Postgres
//!   identifier quoting for codegen pass-through strings.
//!
//! ## What does NOT live here
//!
//! Anything that touches `ir::Feature`, `ir::Module`, or a syntax AST
//! node larger than `syntax::Span` belongs in the per-domain slice.
//! Helpers must be small, easy to test in isolation, and reusable.

use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Bridge `syntax::Span` (parser-local offsets) onto `ir::SpanRef`
/// (IR-local offsets). Mechanical; carries no semantics.
pub(crate) fn span_of(span: syntax::Span) -> ir::SpanRef {
    ir::SpanRef {
        start: span.start,
        end: span.end,
    }
}

/// Minimal Levenshtein for the nearest-name suggestion. Small (~12
/// LOC) so we don't pull a dependency for a one-off use. Copying the
/// pattern from elsewhere in the analyzer keeps the suggestion
/// quality consistent.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        dp[i][0] = i;
    }
    for j in 0..=b.len() {
        dp[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

/// Edit-distance variant used by the conventions catalog suggestion.
/// Two-row rolling implementation; identical semantics to
/// `levenshtein` above but lower memory cost on short inputs.
pub(crate) fn conventions_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Find the byte offset of the first line in `body` that starts (after
/// trimming leading whitespace) with `keyword`. Used by doctor
/// surfaces that need a span into a verbatim multi-line block body.
pub(crate) fn find_keyword_line_offset(body: &str, keyword: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(keyword) {
            return Some(offset);
        }
        offset += line.len() + 1;
    }
    None
}

/// Return the prefix of `text` up to the first top-level whitespace,
/// where "top-level" means outside any `(...)` or `[...]` group. Used
/// to pluck the head identifier off a type literal like
/// `Money(currency:BRL) required` → `Money(currency:BRL)`.
pub(crate) fn first_paren_balanced_token(text: &str) -> &str {
    let text = text.trim();
    let mut depth = 0i32;
    for (idx, ch) in text.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => return text[..idx].trim_end(),
            _ => {}
        }
    }
    text
}

/// Given a starting byte offset that points at an `(` (or `[`), walk
/// forward and return the byte offset just past the matching close.
/// Honours nested groups and double-quoted strings (escapes too).
/// Returns `None` if the group is unterminated.
pub(crate) fn find_balanced_decorator_end(text: &str, start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

/// Strip a single or double-quoted wrapper from `raw`. Returns `None`
/// if the input isn't quoted. Whitespace is trimmed before the check.
pub(crate) fn strip_quotes(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        Some(&trimmed[1..trimmed.len() - 1])
    } else {
        None
    }
}

/// Find a whole-word token (`word`) in `text`, returning its byte
/// offset. Returns `None` when the substring only appears as part of
/// a longer identifier (e.g. `withdrawn`).
pub(crate) fn find_word(text: &str, word: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(word) {
        let abs = from + rel;
        let before_ok = abs == 0 || bytes[abs - 1].is_ascii_whitespace();
        let after_pos = abs + word.len();
        let after_ok = after_pos >= bytes.len() || bytes[after_pos].is_ascii_whitespace();
        if before_ok && after_ok {
            return Some(abs);
        }
        from = abs + word.len();
    }
    None
}

/// Find the byte offset of the first occurrence of `op` in `text` that
/// is outside a double-quoted string region. Used by eval-predicate
/// lowering to split closed comparison operators (`==`, `>=`, etc.)
/// without false-firing inside quoted literals.
pub(crate) fn find_top_level_operator(text: &str, op: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }
        if !in_quote && text[i..].starts_with(op) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Detect a top-level comma in a CSS box-shadow value, signaling
/// multi-layer composition. Commas inside `(...)` or `[...]` (e.g.
/// `rgb(0, 0, 0)`) do NOT count. Strings inside quoted regions also
/// don't count (a hypothetical `content: ","` would not trigger).
pub(crate) fn has_top_level_comma(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' | b'\'' => in_quote = !in_quote,
            b'(' | b'[' if !in_quote => depth += 1,
            b')' | b']' if !in_quote => depth -= 1,
            b',' if !in_quote && depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Validate a CSS hex-color literal in the closed shape `#RGB`,
/// `#RGBA`, `#RRGGBB`, or `#RRGGBBAA`. Returns `false` on any other
/// length or non-hex char.
pub(crate) fn is_valid_design_hex(text: &str) -> bool {
    let trimmed = text.trim();
    if !trimmed.starts_with('#') {
        return false;
    }
    let rest = &trimmed[1..];
    let len = rest.len();
    if !(len == 3 || len == 4 || len == 6 || len == 8) {
        return false;
    }
    rest.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Convert `PascalCase` to `snake_case`. Used by the conventions synth
/// to derive default table / column names from resource names.
pub(crate) fn pascal_to_snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    let mut iter = s.chars().peekable();

    while let Some(ch) = iter.next() {
        if ch.is_ascii_uppercase() {
            let next_is_lower = iter
                .peek()
                .copied()
                .is_some_and(|next| next.is_ascii_lowercase());
            let prev_needs_sep = prev.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next_is_lower)
            });
            if !out.is_empty() && prev_needs_sep {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
        prev = Some(ch);
    }

    out
}

/// Convert `snake_case` to `PascalCase`. Inverse of `pascal_to_snake`
/// for the common subset (no acronym preservation).
pub(crate) fn snake_to_pascal(s: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;

    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
            continue;
        }
        if capitalize_next {
            out.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }

    out
}

/// §7.3 — wrap a resource name in Postgres double-quotes after
/// snake-casing. Codegen pastes the result inside an SQL fragment.
pub(crate) fn quoted_table(resource_name: &str) -> String {
    format!("\"{}\"", pascal_to_snake(resource_name))
}

/// §7.3 — quote an identifier when codegen will paste it inside an
/// SQL fragment. Postgres treats `"user"` and `user` differently
/// (the latter is a reserved keyword in many positions); the
/// hand-rolled handlers in the trigger pilot (`the canonical pilot/.../delete_service.go`
/// §1.1) quote both sides. We match that shape.
pub(crate) fn quoted_ident(ident: &str) -> String {
    format!("\"{}\"", ident)
}
