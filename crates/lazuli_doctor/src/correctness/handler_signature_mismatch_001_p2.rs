/// Extract the input parameter's type from the handler param list.
///
/// Canonical shape: `ctx *lazuli.Ctx, input <Type>` — two parameters.
/// We split on the top-level comma (depth-aware so generic params or
/// anonymous-struct field commas don't fool us), then take the second
/// parameter's type (everything after the name token).
fn extract_input_ident(params: &str) -> Option<String> {
    let parts = split_top_level_commas(params);
    if parts.len() != 2 {
        return None;
    }
    // The first parameter must be `<name> *<...>Ctx` or `<name> *Ctx` —
    // we don't enforce the name, just verify the second param's shape.
    let second = parts[1].trim();
    // `name TYPE` — split on first ASCII whitespace.
    let (name, ty) = split_param_name_and_type(second)?;
    if name.is_empty() || ty.is_empty() {
        return None;
    }
    Some(canonical_ident(ty))
}

/// Extract the first return-type ident. Canonical shape:
/// `<Output>, error`. We split on the top-level comma and require the
/// second to be `error`. Returns the trimmed first part.
fn extract_output_ident(returns: &str) -> Option<String> {
    let parts = split_top_level_commas(returns);
    if parts.len() != 2 {
        return None;
    }
    if parts[1].trim() != "error" {
        return None;
    }
    let first = parts[0].trim();
    if first.is_empty() {
        return None;
    }
    Some(canonical_ident(first))
}

/// Split a Go parameter `name TYPE` into the two halves. Handles
/// `input accountgen.Foo` and `input  struct{}` (extra whitespace).
fn split_param_name_and_type(param: &str) -> Option<(&str, &str)> {
    let trimmed = param.trim();
    let mut split_at = None;
    for (i, ch) in trimmed.char_indices() {
        if ch.is_ascii_whitespace() {
            split_at = Some(i);
            break;
        }
    }
    let split_at = split_at?;
    let name = trimmed[..split_at].trim();
    let ty = trimmed[split_at..].trim();
    Some((name, ty))
}

// ── codegen extraction ──────────────────────────────────────────────────────

/// Walk codegen Go source looking for a `lazuli.Command[I, O]{ ... Name: "<qualified>" ... }`
/// block. Returns the two type-parameter idents.
///
/// Strategy: find every `lazuli.Command[` occurrence; for each, parse
/// the bracketed type-parameter list, then scan forward until the
/// matching `{` block and look for `Name: "<qualified>"`. Stop on
/// first match.
fn extract_command_signature(source: &str, qualified_name: &str) -> Option<GenSig> {
    let bytes = source.as_bytes();
    let needle = "lazuli.Command[";
    let mut search_from = 0usize;
    let target_name_literal = format!("\"{}\"", qualified_name);

    while let Some(rel) = source[search_from..].find(needle) {
        let start = search_from + rel;
        let bracket_open = start + needle.len();
        // Walk to matching ']' at depth 0.
        let bracket_end = match scan_to_matching(bytes, bracket_open, b'[', b']') {
            Some(p) => p,
            None => {
                search_from = bracket_open;
                continue;
            }
        };
        let type_params = &source[bracket_open..bracket_end];
        let parts = split_top_level_commas(type_params);
        if parts.len() != 2 {
            search_from = bracket_end + 1;
            continue;
        }
        let input = canonical_ident(parts[0].trim());
        let output = canonical_ident(parts[1].trim());

        // Now find the `{` immediately after the `]` (Go allows
        // whitespace between them, e.g. `Command[I, O] {`).
        let mut idx = bracket_end + 1;
        skip_whitespace(bytes, &mut idx);
        if idx >= bytes.len() || bytes[idx] != b'{' {
            search_from = bracket_end + 1;
            continue;
        }
        let brace_open = idx + 1;
        let brace_end = match scan_to_matching(bytes, brace_open, b'{', b'}') {
            Some(p) => p,
            None => {
                search_from = brace_open;
                continue;
            }
        };
        let body = &source[brace_open..brace_end];

        // Look for Name: "<qualified>" anywhere in the body. Conservative
        // match — the codegen always emits `Name:` with this exact shape.
        if body.contains(&target_name_literal) {
            return Some(GenSig { input, output });
        }
        search_from = brace_end + 1;
    }
    None
}

// ── ident normalisation ─────────────────────────────────────────────────────

/// Compute the diff between handler and gen signatures, normalising
/// idents against the `<feature>gen.` package prefix.
fn diff_signatures(handler: &HandlerSig, gen_sig: &GenSig, feature: &str) -> Option<Diff> {
    let prefix = format!("{}gen.", feature);
    let h_in = strip_prefix(&handler.input, &prefix);
    let h_out = strip_prefix(&handler.output, &prefix);
    let g_in = strip_prefix(&gen_sig.input, &prefix);
    let g_out = strip_prefix(&gen_sig.output, &prefix);

    let input_match = idents_match(h_in, g_in);
    let output_match = idents_match(h_out, g_out);

    match (input_match, output_match) {
        (true, true) => None,
        (false, true) => Some(Diff::InputMismatch {
            expected: g_in.to_owned(),
            found: h_in.to_owned(),
        }),
        (true, false) => Some(Diff::OutputMismatch {
            expected: g_out.to_owned(),
            found: h_out.to_owned(),
        }),
        (false, false) => Some(Diff::Both {
            input_expected: g_in.to_owned(),
            input_found: h_in.to_owned(),
            output_expected: g_out.to_owned(),
            output_found: h_out.to_owned(),
        }),
    }
}

/// Strip the `<feature>gen.` package prefix when present so the rule
/// can compare `accountgen.LoginInput` vs `LoginInput` cleanly.
fn strip_prefix<'a>(ident: &'a str, prefix: &str) -> &'a str {
    ident.strip_prefix(prefix).unwrap_or(ident)
}

/// Compare two idents after collapsing internal whitespace. Mainly
/// handles `struct {}` vs `struct{}` and similar trivial whitespace
/// drift; non-trivial divergence (extra fields inside an anonymous
/// struct) still differs, which is what we want.
fn idents_match(a: &str, b: &str) -> bool {
    collapse_ws(a) == collapse_ws(b)
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if !ch.is_whitespace() {
            out.push(ch);
        }
    }
    out
}

/// Convert a snake_case handler name to PascalCase (mirrors codegen).
/// `verify_password_v2` → `VerifyPasswordV2`.
fn pascal_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper_next = true;
    for ch in snake.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Trim and lightly normalise an ident. Whitespace collapsing happens
/// at compare-time via [`collapse_ws`].
fn canonical_ident(raw: &str) -> String {
    raw.trim().to_owned()
}

// ── byte-walker helpers ─────────────────────────────────────────────────────

/// Walk `bytes` from `start` forward, tracking `open`/`close` bracket
/// depth (starting at 1 because the caller has already consumed the
/// initial open). Returns the index of the matching close-bracket when
/// the depth reaches zero, or `None` when the input runs out.
///
/// Aware of Go string literals (double-quoted, backtick raw) and
/// line comments (`//`) and block comments (`/* */`) so we don't
/// trip on brackets inside them.
fn scan_to_matching(bytes: &[u8], start: usize, open: u8, close: u8) -> Option<usize> {
    let mut idx = start;
    let mut depth: i64 = 1;
    while idx < bytes.len() {
        let b = bytes[idx];
        // Skip Go string literals.
        if b == b'"' {
            idx += 1;
            while idx < bytes.len() && bytes[idx] != b'"' {
                if bytes[idx] == b'\\' && idx + 1 < bytes.len() {
                    idx += 2;
                    continue;
                }
                idx += 1;
            }
            idx += 1;
            continue;
        }
        if b == b'`' {
            idx += 1;
            while idx < bytes.len() && bytes[idx] != b'`' {
                idx += 1;
            }
            idx += 1;
            continue;
        }
        if b == b'/' && idx + 1 < bytes.len() {
            if bytes[idx + 1] == b'/' {
                // Line comment — skip to newline.
                idx += 2;
                while idx < bytes.len() && bytes[idx] != b'\n' {
                    idx += 1;
                }
                continue;
            }
            if bytes[idx + 1] == b'*' {
                // Block comment — skip to `*/`.
                idx += 2;
                while idx + 1 < bytes.len() && !(bytes[idx] == b'*' && bytes[idx + 1] == b'/') {
                    idx += 1;
                }
                idx += 2;
                continue;
            }
        }
        if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
        idx += 1;
    }
    None
}

/// Advance `*idx` past ASCII whitespace and Go line/block comments.
fn skip_whitespace(bytes: &[u8], idx: &mut usize) {
    while *idx < bytes.len() {
        let b = bytes[*idx];
        if b.is_ascii_whitespace() {
            *idx += 1;
            continue;
        }
        if b == b'/' && *idx + 1 < bytes.len() {
            if bytes[*idx + 1] == b'/' {
                *idx += 2;
                while *idx < bytes.len() && bytes[*idx] != b'\n' {
                    *idx += 1;
                }
                continue;
            }
            if bytes[*idx + 1] == b'*' {
                *idx += 2;
                while *idx + 1 < bytes.len() && !(bytes[*idx] == b'*' && bytes[*idx + 1] == b'/') {
                    *idx += 1;
                }
                *idx += 2;
                continue;
            }
        }
        break;
    }
}

/// Split `s` on commas at bracket-depth 0. Bracket-aware so generic
/// type parameters like `Foo[A, B]` are not torn apart at the inner
/// comma. Tracks `()`, `[]`, `{}`.
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut depth_brace = 0i32;
    let mut start = 0usize;
    let mut idx = 0usize;
    while idx < bytes.len() {
        let b = bytes[idx];
        // Skip string literals — inline so we don't return a tuple.
        if b == b'"' {
            idx += 1;
            while idx < bytes.len() && bytes[idx] != b'"' {
                if bytes[idx] == b'\\' && idx + 1 < bytes.len() {
                    idx += 2;
                    continue;
                }
                idx += 1;
            }
            idx += 1;
            continue;
        }
        if b == b'`' {
            idx += 1;
            while idx < bytes.len() && bytes[idx] != b'`' {
                idx += 1;
            }
            idx += 1;
            continue;
        }
        match b {
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'[' => depth_brack += 1,
            b']' => depth_brack -= 1,
            b'{' => depth_brace += 1,
            b'}' => depth_brace -= 1,
            b',' if depth_paren == 0 && depth_brack == 0 && depth_brace == 0 => {
                out.push(&s[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }
    if start <= s.len() {
        out.push(&s[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    include!("handler_signature_mismatch_001_tests_p1.rs");
    include!("handler_signature_mismatch_001_tests_p2.rs");
}
