/// Find the first constraint keyword in `text` (at depth 0). Returns
/// `(before_keyword, after_keyword_including_kw_and_args)`. Returns
/// `None` when no recognized keyword is found.
fn find_constraint_keyword(text: &str) -> Option<(&str, &str)> {
    // Catalog of probes, each `(needle, kw)`. We scan once over the
    // text and pick the earliest occurrence so multiple constraints
    // peel left-to-right deterministically.
    let needles: &[(&str, ConstraintKw)] = &[
        (" min ", ConstraintKw::Min),
        (" max ", ConstraintKw::Max),
        (" pattern ", ConstraintKw::Pattern),
        (" between ", ConstraintKw::Between),
        (" length ", ConstraintKw::Length),
        (" in ", ConstraintKw::In),
        (" validate ", ConstraintKw::Validate),
        (" validator ", ConstraintKw::Validator),
    ];
    let mut best: Option<usize> = None;
    for (needle, _) in needles {
        if let Some(idx) = find_token(text, needle) {
            best = Some(best.map_or(idx, |b| b.min(idx)));
        }
    }
    let idx = best?;
    let before = &text[..idx];
    // Include the leading space so callers can identify which kw was
    // matched without re-scanning.
    let after = &text[idx + 1..];
    Some((before, after))
}

/// Pick the constraint kind from `after_keyword_text` (which starts
/// with the keyword token plus its args).
fn before_keyword_after(_full: &str, after_keyword_text: &str) -> ConstraintKw {
    if after_keyword_text.starts_with("min ") {
        ConstraintKw::Min
    } else if after_keyword_text.starts_with("max ") {
        ConstraintKw::Max
    } else if after_keyword_text.starts_with("pattern ") {
        ConstraintKw::Pattern
    } else if after_keyword_text.starts_with("between ") {
        ConstraintKw::Between
    } else if after_keyword_text.starts_with("length ") {
        ConstraintKw::Length
    } else if after_keyword_text.starts_with("in ") {
        ConstraintKw::In
    } else if after_keyword_text.starts_with("validate ") {
        ConstraintKw::Validate
    } else if after_keyword_text.starts_with("validator ") {
        ConstraintKw::Validator
    } else {
        // Should be unreachable because find_constraint_keyword
        // matched one of these; defensive default.
        ConstraintKw::Min
    }
}

fn split_type_and_modifiers(text: &str) -> (String, String) {
    // Walk from the right, peeling off ` required` / ` optional` / ` unique`
    // trailing modifiers. The type text (which may contain parenthesised
    // decorator args like `@cap.Encrypted(key:@key.tenant)`) stays
    // structurally untouched because the modifier suffixes are bare
    // identifiers that never occur inside the paren-balanced span.
    let mut head = text.to_owned();
    let mut modifiers = Vec::new();
    loop {
        let trimmed = head.trim_end();
        if trimmed.ends_with(" required") {
            modifiers.push("required");
            head = trimmed[..trimmed.len() - " required".len()].to_owned();
        } else if trimmed.ends_with(" optional") {
            modifiers.push("optional");
            head = trimmed[..trimmed.len() - " optional".len()].to_owned();
        } else if trimmed.ends_with(" unique") {
            modifiers.push("unique");
            head = trimmed[..trimmed.len() - " unique".len()].to_owned();
        } else {
            head = trimmed.to_owned();
            break;
        }
    }
    (head, modifiers.join(" "))
}

/// Find ` = ` outside of parens/brackets. The default literal may itself
/// contain `=` (rare), but the fixture's default literals are simple
/// (`= lead`, `= 0`).
fn find_default_assignment(text: &str) -> Option<usize> {
    find_token(text, " = ")
}
