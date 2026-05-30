fn parse_tab_group_case(line: &SourceLine<'_>, value: &str) -> Result<TabGroupCaseAst, ParseError> {
    let rest = value.strip_prefix("case ").ok_or_else(|| {
        line_error(
            line,
            "`tab_group` arms are `case <VARIANTS> -> tab \"<label>\"`",
        )
    })?;
    let (variants_raw, tab_raw) = split_lzx_arrow(rest)
        .ok_or_else(|| line_error(line, "`tab_group` arm requires `-> tab \"<label>\"`"))?;
    let variants = split_lzx_list(variants_raw);
    if variants.is_empty() {
        return Err(line_error(
            line,
            "`tab_group case` requires at least one variant",
        ));
    }
    let tab_clause = tab_raw
        .trim()
        .strip_prefix("tab ")
        .ok_or_else(|| line_error(line, "`tab_group` arm right side must be `tab \"<label>\"`"))?;
    let label = unquote_lzx_value(tab_clause.trim()).to_owned();
    if label.is_empty() {
        return Err(line_error(
            line,
            "`tab_group` tab label must be a quoted string",
        ));
    }
    Ok(TabGroupCaseAst {
        variants,
        label,
        span: Span::new(line.start, line.end),
    })
}

/// `view.board [<name>]` block — a single child line `lanes derived_from
/// <field>`. GAP-UX-05. Returns the first unconsumed index + the block end
/// offset.
pub(in crate::parser::lzx) fn parse_board_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
    header_rest: &str,
    ux: &mut ViewUxAst,
) -> Result<(usize, usize), ParseError> {
    let header = &lines[start];
    if ux.board.is_some() {
        return Err(line_error(
            header,
            "view declares `view.board` at most once",
        ));
    }
    // Optional `<name>` on the header line. Must be a bare identifier.
    let name = header_rest.trim();
    if !name.is_empty() && !is_kebab_or_snake_ident(name) {
        return Err(line_error_owned(
            header,
            format!(
                "`view.board` name `{}` must be a kebab/snake identifier",
                name
            ),
        ));
    }

    let child_indent = body_indent + 2;
    let mut lanes_source: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`view.board` body uses one indentation level deeper than the header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim();
        let field = trimmed.strip_prefix("lanes derived_from ").ok_or_else(|| {
            line_error(
                line,
                "`view.board` body line is `lanes derived_from <field>`",
            )
        })?;
        let field = field.trim();
        if !is_kebab_or_snake_ident(field) {
            return Err(line_error_owned(
                line,
                format!(
                    "`view.board lanes derived_from` field `{}` must be a kebab/snake identifier",
                    field
                ),
            ));
        }
        if lanes_source.is_some() {
            return Err(line_error(
                line,
                "`view.board` declares `lanes derived_from` exactly once",
            ));
        }
        lanes_source = Some(field.to_owned());
        last_end = line.end;
        i += 1;
    }

    let lanes_source = lanes_source.ok_or_else(|| {
        line_error(
            header,
            "`view.board` requires a `lanes derived_from <field>` line",
        )
    })?;
    ux.board = Some(BoardAst {
        name: name.to_owned(),
        lanes_source,
        span: Span::new(header.start, last_end),
    });
    Ok((i, last_end))
}

/// `repeatable input <name> group <f>: <T>, … validates sum(<f>) = <n>`
/// — single line, brace-free (SPEC-02). GAP-UX-05.
pub(in crate::parser::lzx) fn parse_repeatable_group_line(
    line: &SourceLine<'_>,
    rest: &str,
    ux: &mut ViewUxAst,
) -> Result<(), ParseError> {
    // `<name> group <f>: <T>, … validates sum(<f>) = <n>`
    let (name_raw, after_name) = rest.split_once(" group ").ok_or_else(|| {
        line_error(
            line,
            "`repeatable input` is `repeatable input <name> group <f>: <T>, … validates sum(<f>) = <n>`",
        )
    })?;
    let name = name_raw.trim().to_owned();
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            line,
            format!(
                "`repeatable input` name `{}` must be a kebab/snake identifier",
                name
            ),
        ));
    }
    if ux.repeatable_groups.iter().any(|g| g.name == name) {
        return Err(line_error_owned(
            line,
            format!("duplicate `repeatable input` group `{}`", name),
        ));
    }

    // SPEC-02 — brace-free: the comma-separated field list is delimited by the
    // ` validates ` clause (no `{ }` / `;` — the only braces the language had).
    let after_name = after_name.trim_start();
    let (body, validates_body) = after_name.split_once(" validates ").ok_or_else(|| {
        line_error(
            line,
            "`repeatable input <name> group <f>: <T>, … validates sum(<f>) = <n>`",
        )
    })?;

    let mut fields: Vec<RepeatableFieldAst> = Vec::new();
    for entry in body.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (fname, ftype) = entry.split_once(':').ok_or_else(|| {
            line_error(line, "`repeatable input` group fields are `<name>: <Type>`")
        })?;
        let fname = fname.trim().to_owned();
        let ftype = ftype.trim().to_owned();
        if !is_kebab_or_snake_ident(&fname) {
            return Err(line_error_owned(
                line,
                format!(
                    "`repeatable input` field `{}` must be a kebab/snake identifier",
                    fname
                ),
            ));
        }
        if ftype.is_empty() {
            return Err(line_error_owned(
                line,
                format!("`repeatable input` field `{}` is missing a type", fname),
            ));
        }
        fields.push(RepeatableFieldAst {
            name: fname,
            type_name: ftype,
        });
    }
    if fields.is_empty() {
        return Err(line_error(
            line,
            "`repeatable input` group requires at least one `<name>: <Type>` field",
        ));
    }

    // `sum(<f>) = <n>` (the `validates` keyword was the split delimiter above).
    let validates = validates_body.trim();
    let sum_inner = validates
        .strip_prefix("sum(")
        .and_then(|s| {
            let end = s.find(')')?;
            Some((s[..end].trim().to_owned(), s[end + 1..].trim()))
        })
        .ok_or_else(|| {
            line_error(
                line,
                "`repeatable input validates` must be `sum(<field>) = <n>`",
            )
        })?;
    let (sum_field, after_paren) = sum_inner;
    if !is_kebab_or_snake_ident(&sum_field) {
        return Err(line_error_owned(
            line,
            format!(
                "`repeatable input` sum field `{}` must be a kebab/snake identifier",
                sum_field
            ),
        ));
    }
    let target_raw = after_paren
        .strip_prefix('=')
        .map(str::trim)
        .ok_or_else(|| {
            line_error(
                line,
                "`repeatable input validates sum(<f>)` must be followed by `= <n>`",
            )
        })?;
    if target_raw.is_empty() || target_raw.parse::<f64>().is_err() {
        return Err(line_error_owned(
            line,
            format!(
                "`repeatable input` sum target `{}` must be a number literal",
                target_raw
            ),
        ));
    }

    ux.repeatable_groups.push(RepeatableGroupAst {
        name,
        fields,
        sum_field,
        sum_target: target_raw.to_owned(),
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

// ===========================================================================
// Audience-level containers (tabs / wizard)
// ===========================================================================

/// `tabs` block — child lines are `tab "<label>" -> view <name> [audience <a>]`.
/// GAP-UX-03. Returns the parsed AST + the first unconsumed line index.
pub(in crate::parser::lzx) fn parse_tabs_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(TabsAst, usize), ParseError> {
    let header = &lines[start];
    let body_indent = parent_indent + 2;
    let mut entries: Vec<TabEntryAst> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != body_indent {
            return Err(line_error(
                line,
                "`tabs` entries use one indentation level deeper than the `tabs` header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim();
        entries.push(parse_tab_entry(line, trimmed)?);
        last_end = line.end;
        i += 1;
    }

    if entries.is_empty() {
        return Err(line_error(
            header,
            "`tabs` requires at least one `tab` entry",
        ));
    }
    Ok((
        TabsAst {
            entries,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse one `tab "<label>" -> view <name> [audience <a>]` row.
fn parse_tab_entry(line: &SourceLine<'_>, value: &str) -> Result<TabEntryAst, ParseError> {
    let rest = value
        .strip_prefix("tab ")
        .ok_or_else(|| line_error(line, "`tabs` rows are `tab \"<label>\" -> view <name>`"))?;
    let (label_raw, target_raw) = split_lzx_arrow(rest)
        .ok_or_else(|| line_error(line, "`tab` row requires `-> view <name>`"))?;
    let label = unquote_lzx_value(label_raw.trim()).to_owned();
    if label.is_empty() {
        return Err(line_error(line, "`tab` label must be a quoted string"));
    }
    let target = target_raw.trim();
    let view_clause = target.strip_prefix("view ").ok_or_else(|| {
        line_error(
            line,
            "`tab` right side must be `view <name> [audience <a>]`",
        )
    })?;
    let mut parts = view_clause.split_whitespace();
    let view = parts
        .next()
        .ok_or_else(|| line_error(line, "`tab -> view` requires a view name"))?
        .to_owned();
    if !is_kebab_or_snake_ident(&view) {
        return Err(line_error_owned(
            line,
            format!("`tab` view `{}` must be a kebab/snake identifier", view),
        ));
    }
    let mut audience = None;
    if let Some(keyword) = parts.next() {
        if keyword != "audience" {
            return Err(line_error(
                line,
                "`tab -> view <name>` trailing clause must be `audience <a>`",
            ));
        }
        let aud = parts
            .next()
            .ok_or_else(|| line_error(line, "`tab ... audience` requires an audience name"))?
            .to_owned();
        if parts.next().is_some() {
            return Err(line_error(
                line,
                "`tab` row has trailing tokens after `audience <a>`",
            ));
        }
        if !is_kebab_or_snake_ident(&aud) {
            return Err(line_error_owned(
                line,
                format!("`tab` audience `{}` must be a kebab/snake identifier", aud),
            ));
        }
        audience = Some(aud);
    }
    Ok(TabEntryAst {
        label,
        view,
        audience,
        span: Span::new(line.start, line.end),
    })
}

/// `wizard <name> steps` block — child lines are `step <N>: <ref>`.
/// GAP-UX-03. Returns the parsed AST + the first unconsumed line index.
pub(in crate::parser::lzx) fn parse_wizard_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
    header_rest: &str,
) -> Result<(WizardAst, usize), ParseError> {
    let header = &lines[start];
    let name = header_rest
        .strip_suffix(" steps")
        .map(str::trim)
        .ok_or_else(|| line_error(header, "`wizard` header is `wizard <name> steps`"))?;
    if !is_kebab_or_snake_ident(name) {
        return Err(line_error_owned(
            header,
            format!("`wizard` name `{}` must be a kebab/snake identifier", name),
        ));
    }
    let body_indent = parent_indent + 2;
    let mut steps: Vec<WizardStepAst> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != body_indent {
            return Err(line_error(
                line,
                "`wizard` steps use one indentation level deeper than the `wizard` header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim();
        steps.push(parse_wizard_step(line, trimmed)?);
        last_end = line.end;
        i += 1;
    }

    if steps.is_empty() {
        return Err(line_error(header, "`wizard` requires at least one `step`"));
    }
    Ok((
        WizardAst {
            name: name.to_owned(),
            steps,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
