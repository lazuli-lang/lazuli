/// Split `<TypeRef> [decorators...] [required|optional|unique]
/// [<constraint>...] [= <default>] [derived from <expr>]` into
/// structured pieces. L0 #3 §10 adds the constraint axis between
/// modifiers and the default — but we peel from the right, so the
/// order doesn't matter on input. Constraint keywords (`min`, `max`,
/// `pattern`, `between`, `length`, `in`) are scanned via
/// `find_token` at depth 0 so they don't trip on parenthesised
/// decorator args.
pub(super) struct ResourceFieldTail {
    pub type_text: String,
    pub modifiers_text: String,
    pub default: Option<String>,
    pub derived_from: Option<String>,
    pub computed_date: Option<ComputedDateAst>,
    pub constraints: FieldConstraintsDecl,
}

pub(super) fn split_resource_field_after(
    line: &SourceLine<'_>,
    after: &str,
) -> Result<ResourceFieldTail, ParseError> {
    let after = after.trim();

    // W3 GAP-03 / W4 GAP-08 — pull out `computed_date from <base> offset
    // <offset>` (W3) or `schedule_rule from @fn.<name>(<arg>) offset
    // <offset>` (W4), always at the end like `derived from`. Mutually
    // exclusive with `derived from`: a field cannot be both a free-text
    // derived column and a structured computed-date.
    let (head, computed_date) = extract_computed_date(line, after)?;

    // Pull out `derived from <expr>` (always at the end).
    let (head, derived_from) = if let Some(idx) = find_token(&head, " derived from ") {
        let derived = head[idx + " derived from ".len()..].trim().to_owned();
        (head[..idx].trim_end().to_owned(), Some(derived))
    } else {
        (head, None)
    };

    if computed_date.is_some() && derived_from.is_some() {
        return Err(line_error(
            line,
            "a field cannot combine `computed_date` and `derived from` — pick one computed-field kind",
        ));
    }

    // Pull out ` = <default>`.
    let (head, default) = if let Some(idx) = find_default_assignment(&head) {
        let value = head[idx + " = ".len()..].trim().to_owned();
        (head[..idx].trim_end().to_owned(), Some(value))
    } else {
        (head, None)
    };

    // Pull out inline constraints (closed catalog).
    let (head, constraints) = extract_field_constraints(line, &head)?;

    // Now split type (paren-aware) from trailing modifier tokens.
    let (type_text, modifiers_text) = split_type_and_modifiers(&head);
    Ok(ResourceFieldTail {
        type_text,
        modifiers_text,
        default,
        derived_from,
        computed_date,
        constraints,
    })
}

/// W3 GAP-03 / W4 GAP-08 — peel `computed_date from <base> offset
/// <offset>` (W3) or `schedule_rule from @fn.<name>(<arg>) offset
/// <offset>` (W4) off the field tail. Returns the head text with the
/// clause removed plus the optional typed payload. The clause is always
/// trailing (after type + modifiers + constraints), mirroring `derived
/// from`.
///
/// Grammar:
/// - W3: `computed_date from <ident> offset <ident-or-int>`.
///   `<base>` is a bare identifier (a `Date` field name).
/// - W4: `schedule_rule from @fn.<name>(<rule_arg>) offset <ident-or-int>`.
///   The `@fn` returns the base `Date` selected by `<rule_arg>`.
/// - `<offset>` is a bare identifier (an `Integer` field name) OR an
///   integer literal (number of days) in both forms.
///
/// Errors: missing `from`, missing base, malformed `@fn(...)`, missing
/// `offset`, missing offset operand. Type-checking of the references is
/// deferred to the analyzer + doctor (`COMPUTED-DATE-EXPR-001`,
/// `SCHEDULE-RULE-001`).
fn extract_computed_date(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(String, Option<ComputedDateAst>), ParseError> {
    // Detect either keyword at a word boundary; `schedule_rule` is the
    // W4 GAP-08 sibling of `computed_date` and shares the trailing shape.
    let detect = |kw: &str| -> Option<(usize, String)> {
        let leading = format!("{kw} ");
        if let Some(rest) = text.strip_prefix(&leading) {
            return Some((0usize, rest.to_owned()));
        }
        let mid = format!(" {kw} ");
        find_token(text, &mid).map(|idx| (idx, text[idx + mid.len()..].to_owned()))
    };

    let (start, rest, is_rule) = if let Some((start, rest)) = detect("schedule_rule") {
        (start, rest, true)
    } else if let Some((start, rest)) = detect("computed_date") {
        (start, rest, false)
    } else {
        return Ok((text.to_owned(), None));
    };

    let kw = if is_rule { "schedule_rule" } else { "computed_date" };
    // `rest` begins right after the keyword.
    let rest = rest.trim_start();
    let body = rest.strip_prefix("from ").ok_or_else(|| {
        line_error_owned(
            line,
            format!("`{kw}` requires `from <base> offset <offset>`"),
        )
    })?;
    let body = body.trim_start();

    // Split the base operand from the trailing ` offset <offset>` tail.
    // For W4 the base is `@fn.<name>(<arg>)` — which may itself contain
    // spaces inside the parens — so we locate ` offset ` at paren depth 0.
    let offset_kw = " offset ";
    let offset_idx = find_token(body, offset_kw).ok_or_else(|| {
        line_error_owned(
            line,
            format!("`{kw} from <base>` must be followed by `offset <offset>`"),
        )
    })?;
    let base_text = body[..offset_idx].trim();
    let offset_text = body[offset_idx + offset_kw.len()..].trim();
    if base_text.is_empty() {
        return Err(line_error_owned(
            line,
            format!("`{kw} from` is missing the base operand"),
        ));
    }
    if offset_text.is_empty() {
        return Err(line_error_owned(
            line,
            format!("`{kw} ... offset` is missing the offset field or integer literal"),
        ));
    }
    if offset_text.split_whitespace().count() != 1 {
        return Err(line_error_owned(
            line,
            format!("`{kw} from <base> offset <offset>` accepts exactly one offset operand"),
        ));
    }

    let base = if is_rule {
        parse_schedule_rule_base(line, base_text)?
    } else {
        // W3 form: the base is a bare identifier (no `@fn`, no parens).
        if base_text.split_whitespace().count() != 1 || base_text.contains('@') {
            return Err(line_error(
                line,
                "`computed_date from <base>` expects a bare field name (use `schedule_rule` for an `@fn` base)",
            ));
        }
        ComputedDateBaseAst::Field(base_text.to_owned())
    };

    let offset = if let Ok(n) = offset_text.parse::<i64>() {
        ComputedDateOffsetAst::Literal(n)
    } else {
        ComputedDateOffsetAst::Field(offset_text.to_owned())
    };
    let head = text[..start].trim_end().to_owned();
    Ok((head, Some(ComputedDateAst { base, offset })))
}

/// W4 GAP-08 — parse the `schedule_rule` base operand `@fn.<name>(<arg>)`.
/// Returns a `Rule` base carrying the bare binding-fn name and the
/// verbatim rule argument. The `@fn` reference and the argument are
/// validated by doctor `SCHEDULE-RULE-001`.
fn parse_schedule_rule_base(
    line: &SourceLine<'_>,
    base_text: &str,
) -> Result<ComputedDateBaseAst, ParseError> {
    let after = base_text.strip_prefix("@fn.").ok_or_else(|| {
        line_error(
            line,
            "`schedule_rule from` requires an `@fn.<name>(<rule_arg>)` base",
        )
    })?;
    let open = after.find('(').ok_or_else(|| {
        line_error(
            line,
            "`schedule_rule from @fn.<name>` requires a `(<rule_arg>)` argument",
        )
    })?;
    if !after.ends_with(')') {
        return Err(line_error(
            line,
            "`schedule_rule from @fn.<name>(<rule_arg>)` is missing the closing `)`",
        ));
    }
    let fn_ref = after[..open].trim();
    let rule = after[open + 1..after.len() - 1].trim();
    if fn_ref.is_empty() {
        return Err(line_error(
            line,
            "`schedule_rule from @fn.` is missing the binding-fn name",
        ));
    }
    if rule.is_empty() {
        return Err(line_error(
            line,
            "`schedule_rule from @fn.<name>()` is missing the rule argument",
        ));
    }
    Ok(ComputedDateBaseAst::Rule {
        rule: rule.to_owned(),
        fn_ref: fn_ref.to_owned(),
    })
}

/// L0 #3 §10 — scan the field tail for inline constraint keywords.
/// Returns the head text with constraint segments removed plus a
/// populated `FieldConstraintsDecl`. Each keyword is recognised at
/// depth 0 (outside parens/brackets) and stripped from the head so
/// the remaining text walks cleanly through `split_type_and_modifiers`.
///
/// Catalog: `min N`, `max N`, `pattern "STRING"`, `between A and B`,
/// `length N`, `in [a, b, c]`, `validate sanitize_html(profile)`.
/// Combination rule enforcement happens
/// in the analyzer; the parser only captures presence + values and
/// reports basic shape errors (unparsable integer, missing bracket).
pub(super) fn extract_field_constraints(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(String, FieldConstraintsDecl), ParseError> {
    let mut head = text.to_owned();
    let mut constraints = FieldConstraintsDecl::default();
    // Loop until no more constraint keywords appear. Each iteration
    // peels at most one keyword off the right; ordering of multiple
    // constraints in the source is preserved by the iterative scan.
    loop {
        let scan = head.clone();
        if let Some((before, rest)) = find_constraint_keyword(&scan) {
            let rest = rest.trim_start();
            match before_keyword_after(&scan, rest) {
                // `in [...]` — bracketed list.
                ConstraintKw::In => {
                    let (values, tail) = parse_constraint_in_list(line, rest)?;
                    if constraints.r#in.is_some() {
                        return Err(line_error(line, "duplicate `in` constraint on field"));
                    }
                    constraints.r#in = Some(values);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Min => {
                    let (n, tail) = parse_constraint_int(line, rest, "min")?;
                    if constraints.min.is_some() {
                        return Err(line_error(line, "duplicate `min` constraint on field"));
                    }
                    constraints.min = Some(n);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Max => {
                    let (n, tail) = parse_constraint_int(line, rest, "max")?;
                    if constraints.max.is_some() {
                        return Err(line_error(line, "duplicate `max` constraint on field"));
                    }
                    constraints.max = Some(n);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Length => {
                    let (n, tail) = parse_constraint_int(line, rest, "length")?;
                    if n < 0 {
                        return Err(line_error(
                            line,
                            "`length` constraint must be a non-negative integer",
                        ));
                    }
                    if constraints.length.is_some() {
                        return Err(line_error(line, "duplicate `length` constraint on field"));
                    }
                    constraints.length = Some(n as usize);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Pattern => {
                    let (pat, tail) = parse_constraint_string(line, rest, "pattern")?;
                    if constraints.pattern.is_some() {
                        return Err(line_error(line, "duplicate `pattern` constraint on field"));
                    }
                    constraints.pattern = Some(pat);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Between => {
                    let (lo, hi, tail) = parse_constraint_between(line, rest)?;
                    if constraints.between.is_some() {
                        return Err(line_error(line, "duplicate `between` constraint on field"));
                    }
                    constraints.between = Some((lo, hi));
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Validate => {
                    let (validated, tail) = parse_constraint_validate(line, rest)?;
                    match validated {
                        ParsedValidateConstraint::SanitizeHtml(profile) => {
                            if constraints.sanitize_html.is_some() {
                                return Err(line_error(
                                    line,
                                    "duplicate `validate sanitize_html` constraint on field",
                                ));
                            }
                            constraints.sanitize_html = Some(profile);
                        }
                        ParsedValidateConstraint::Utf8Safe => {
                            if constraints.utf8_safe.is_some() {
                                return Err(line_error(
                                    line,
                                    "duplicate `validate utf8_safe` constraint on field",
                                ));
                            }
                            constraints.utf8_safe = Some(true);
                        }
                        ParsedValidateConstraint::MaxRecursion(n) => {
                            if constraints.max_recursion.is_some() {
                                return Err(line_error(
                                    line,
                                    "duplicate `validate max_recursion` constraint on field",
                                ));
                            }
                            constraints.max_recursion = Some(n);
                        }
                        ParsedValidateConstraint::MaxSize(n) => {
                            if constraints.max_size.is_some() {
                                return Err(line_error(
                                    line,
                                    "duplicate `validate max_size` constraint on field",
                                ));
                            }
                            constraints.max_size = Some(n);
                        }
                    }
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
                ConstraintKw::Validator => {
                    let (validator, tail) = parse_constraint_validator(line, rest)?;
                    if constraints.covers_pii.is_some() {
                        return Err(line_error(
                            line,
                            "duplicate `validator covers_pii` constraint on field",
                        ));
                    }
                    constraints.covers_pii = Some(validator);
                    head = format!("{}{}", before, tail);
                    head = head.trim_end().to_owned();
                }
            }
        } else {
            break;
        }
    }
    Ok((head, constraints))
}

#[derive(Debug, Clone, Copy)]
enum ConstraintKw {
    Min,
    Max,
    Pattern,
    Between,
    Length,
    In,
    Validate,
    Validator,
}
