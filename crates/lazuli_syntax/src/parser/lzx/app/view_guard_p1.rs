pub(crate) fn parse_lzx_route_guard_defaults(
    lines: &[SourceLine<'_>],
    start: usize,
    guard_indent: usize,
) -> Result<(LzxRouteGuardDefaults, usize), ParseError> {
    let header = &lines[start];
    if header.text.trim_start() != "route_guard" {
        return Err(line_error(header, "route guard blocks use `route_guard`"));
    }

    let child_indent = guard_indent + 2;
    let mut default_policy = None;
    let mut on_unauthenticated = None;
    let mut on_unauthorized = None;
    let mut skeleton = None;
    let mut index = start + 1;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            index += 1;
            continue;
        }
        if line.indent <= guard_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`route_guard` children use one indentation level deeper than `route_guard`",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("default_policy ") {
            if default_policy.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `default_policy` at most once",
                ));
            }
            default_policy = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthenticated ") {
            if on_unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `on_unauthenticated` at most once",
                ));
            }
            on_unauthenticated = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthorized ") {
            if on_unauthorized.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `on_unauthorized` at most once",
                ));
            }
            on_unauthorized = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("skeleton ") {
            if skeleton.is_some() {
                return Err(line_error(
                    line,
                    "`route_guard` declares `skeleton` at most once",
                ));
            }
            skeleton = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`route_guard` children are `default_policy`, `on_unauthenticated redirect`, `on_unauthorized redirect`, or `skeleton @client.<name>` declarations",
            ));
        }

        last_end = line.end;
        index += 1;
    }

    Ok((
        LzxRouteGuardDefaults {
            default_policy,
            on_unauthenticated,
            on_unauthorized,
            skeleton,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

pub(crate) fn parse_lzx_view_guard(
    lines: &[SourceLine<'_>],
    start: usize,
    policy_indent: usize,
) -> Result<(LzxViewGuard, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let Some(rest) = trimmed.strip_prefix("policy ") else {
        return Err(line_error(
            header,
            "view guard blocks use `policy <policy>`",
        ));
    };
    let policy = parse_lzx_view_guard_policy(header, rest.trim())?;

    let child_indent = policy_indent + 2;
    let grandchild_indent = child_indent + 2;
    let mut on_unauthenticated = None;
    let mut on_unauthorized = None;
    let mut requires_lifecycle = None;
    let mut on_lifecycle_pending = None;
    let mut forbid_when: Vec<LzxForbidWhen> = Vec::new();
    let mut requires_lifecycle_in: Option<LzxRequiresLifecycleIn> = None;
    let mut requires_field: Vec<LzxRequiresField> = Vec::new();
    let mut index = start + 1;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            index += 1;
            continue;
        }
        if line.indent <= policy_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`policy` redirect children use one indentation level deeper than the `policy` line",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("on_unauthenticated ") {
            if on_unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_unauthenticated` at most once",
                ));
            }
            on_unauthenticated = Some(parse_lzx_redirect_clause(line, rest.trim())?);
            last_end = line.end;
            index += 1;
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthorized ") {
            if on_unauthorized.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_unauthorized` at most once",
                ));
            }
            on_unauthorized = Some(parse_lzx_redirect_clause(line, rest.trim())?);
            last_end = line.end;
            index += 1;
        } else if let Some(rest) = trimmed.strip_prefix("requires_lifecycle_in ") {
            // ir-route-guard-escape-hatch-2026-05-28 §4.1 — allow-list
            // form. Format:
            //   requires_lifecycle_in <Resource> [<state>, ...]
            if requires_lifecycle_in.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `requires_lifecycle_in` at most once",
                ));
            }
            requires_lifecycle_in = Some(parse_lzx_requires_lifecycle_in(line, rest.trim())?);
            last_end = line.end;
            index += 1;
        } else if let Some(rest) = trimmed.strip_prefix("requires_lifecycle ") {
            // router-w3 Tier 2 — lifecycle gate. Format:
            //   requires_lifecycle <Resource> = <state>[ on substep <tag>]
            if requires_lifecycle.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `requires_lifecycle` at most once",
                ));
            }
            requires_lifecycle = Some(parse_lzx_requires_lifecycle(line, rest.trim())?);
            last_end = line.end;
            index += 1;
        } else if let Some(rest) = trimmed.strip_prefix("on_lifecycle_pending ") {
            // router-w3 Tier 2 — optional resume router reference. v1
            // codegen prefers the W4 lifecycle_routes table on the
            // resource; this slot is kept for back-compat with apps
            // that authored resume routers.
            if on_lifecycle_pending.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_lifecycle_pending` at most once",
                ));
            }
            on_lifecycle_pending = Some(parse_lzx_on_lifecycle_pending(line, rest.trim())?);
            last_end = line.end;
            index += 1;
        } else if let Some(rest) = trimmed.strip_prefix("forbid_when ") {
            // router-w3 Tier 3 — positive-state redirect. Format:
            //   forbid_when <atom> dispatch_to "<url>"
            // where <atom> is `@<ns>.<name>` (e.g. `@role.host`).
            //
            // ir-route-guard-escape-hatch-2026-05-28 §4.1 — optional
            // child line: `only_when lifecycle <R> = <state>`.
            let mut fw = parse_lzx_forbid_when(line, rest.trim())?;
            let mut span_end = line.end;
            let mut next = index + 1;
            while next < lines.len() {
                let candidate = &lines[next];
                let candidate_trimmed = candidate.text.trim_start();
                if is_trivia(candidate_trimmed) {
                    next += 1;
                    continue;
                }
                if candidate.indent != grandchild_indent {
                    break;
                }
                if let Some(after) = candidate_trimmed.strip_prefix("only_when ") {
                    if fw.only_when_lifecycle.is_some() {
                        return Err(line_error(
                            candidate,
                            "`forbid_when` declares `only_when` at most once",
                        ));
                    }
                    fw.only_when_lifecycle =
                        Some(parse_lzx_only_when_lifecycle(candidate, after.trim())?);
                    span_end = candidate.end;
                    next += 1;
                } else {
                    break;
                }
            }
            fw.span.end = span_end;
            forbid_when.push(fw);
            last_end = span_end;
            index = next;
        } else if let Some(rest) = trimmed.strip_prefix("requires ") {
            // ir-route-guard-escape-hatch-2026-05-28 §4.1 — row-field
            // predicate. Single-line form:
            //   requires <feature>.lookup_my.<field> = <literal>
            //     on_unmet redirect "<path>"
            // The trailing `on_unmet redirect "..."` MAY appear on a
            // child line (2 spaces deeper) or inline after the `=
            // <literal>` clause.
            let (predicate_text, next_after, span_end) =
                consume_requires_continuation(lines, index, grandchild_indent)?;
            let parsed = parse_lzx_requires_field(line, rest.trim(), &predicate_text, span_end)?;
            requires_field.push(parsed);
            last_end = span_end;
            index = next_after;
        } else {
            return Err(line_error(
                line,
                "`policy` children are `on_unauthenticated redirect \"<path>\"`, `on_unauthorized redirect \"<path>\"`, `requires_lifecycle <Resource> = <state>`, `requires_lifecycle_in <Resource> [<state>, ...]`, `on_lifecycle_pending @resume <name>`, `forbid_when <atom> dispatch_to \"<path>\" [only_when lifecycle <R> = <state>]`, or `requires <feature>.lookup_my.<field> = <literal> on_unmet redirect \"<path>\"`",
            ));
        }
    }

    Ok((
        LzxViewGuard {
            policy,
            on_unauthenticated,
            on_unauthorized,
            requires_lifecycle,
            on_lifecycle_pending,
            forbid_when,
            requires_lifecycle_in,
            requires_field,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

/// Collect the `requires` declaration's continuation lines (the trailing
/// `on_unmet redirect "..."` clause may live on a child line two
/// indentation levels deeper than the `policy` block). Returns the
/// concatenated tail text (joined with a single space), the next
/// line-index past the consumed block, and the span end of the last
/// consumed line.
fn consume_requires_continuation(
    lines: &[SourceLine<'_>],
    index: usize,
    grandchild_indent: usize,
) -> Result<(String, usize, usize), ParseError> {
    let head = &lines[index];
    let mut span_end = head.end;
    let mut tail = String::new();
    let mut next = index + 1;
    while next < lines.len() {
        let candidate = &lines[next];
        let candidate_trimmed = candidate.text.trim_start();
        if is_trivia(candidate_trimmed) {
            next += 1;
            continue;
        }
        if candidate.indent != grandchild_indent {
            break;
        }
        // Only the `on_unmet` clause is a recognised continuation for
        // `requires`. Anything else at the same depth belongs to the
        // surrounding block (or is a typo); break and let the outer
        // parser surface the diagnostic.
        if !candidate_trimmed.starts_with("on_unmet") {
            break;
        }
        if !tail.is_empty() {
            tail.push(' ');
        }
        tail.push_str(candidate_trimmed);
        span_end = candidate.end;
        next += 1;
    }
    Ok((tail, next, span_end))
}

/// router-w3 Tier 3 — parse `forbid_when <atom> dispatch_to "<url>"`.
fn parse_lzx_forbid_when(line: &SourceLine<'_>, text: &str) -> Result<LzxForbidWhen, ParseError> {
    let (atom_part, url_part) = text.split_once("dispatch_to").ok_or_else(|| {
        line_error(
            line,
            "`forbid_when` uses `forbid_when <atom> dispatch_to \"<url>\"`",
        )
    })?;
    let atom_ref = atom_part.trim().to_owned();
    if !atom_ref.starts_with('@') {
        return Err(line_error(
            line,
            "`forbid_when` atom must be a policy atom like `@role.host` or `@scope.X`",
        ));
    }
    let url_text = url_part.trim();
    if !url_text.starts_with('"') || !url_text.ends_with('"') || url_text.len() < 2 {
        return Err(line_error(
            line,
            "`forbid_when` URL must be a double-quoted string",
        ));
    }
    let dispatch_to = url_text[1..url_text.len() - 1].to_owned();
    Ok(LzxForbidWhen {
        atom_ref,
        dispatch_to,
        only_when_lifecycle: None,
        span: Span::new(line.start, line.end),
    })
}

/// `ir-route-guard-escape-hatch-2026-05-28` §4.1 — parse the
/// `only_when lifecycle <Resource> = <state>` sub-clause that may
/// appear on a child line under `forbid_when`.
fn parse_lzx_only_when_lifecycle(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<LzxRequiresLifecycle, ParseError> {
    let after = text.strip_prefix("lifecycle ").ok_or_else(|| {
        line_error(
            line,
            "`only_when` uses `only_when lifecycle <Resource> = <state>`",
        )
    })?;
    parse_lzx_requires_lifecycle(line, after.trim())
}

/// `ir-route-guard-escape-hatch-2026-05-28` §4.1 — parse
/// `requires_lifecycle_in <Resource> [<state>, ...]`.
pub(crate) fn parse_lzx_requires_lifecycle_in(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LzxRequiresLifecycleIn, ParseError> {
    let (resource, list_text) = rest.split_once('[').ok_or_else(|| {
        line_error(
            line,
            "`requires_lifecycle_in` uses `requires_lifecycle_in <Resource> [<state>, ...]`",
        )
    })?;
    let resource = resource.trim();
    if resource.is_empty()
        || !resource
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        || !is_lzx_bare_ident(resource)
    {
        return Err(line_error(
            line,
            "`requires_lifecycle_in` resource must be an upper-case resource identifier",
        ));
    }
    let list_text = list_text.trim();
    let inner = list_text.strip_suffix(']').ok_or_else(|| {
        line_error(
            line,
            "`requires_lifecycle_in` allow-list must be `[state1, state2, ...]`",
        )
    })?;
    let mut allowed_states = Vec::new();
    for raw in inner.split(',') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            // Empty entry — could be a fully-empty list (`[]`) or a
            // trailing comma. Skip silently here; doctor rule
            // ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002 surfaces the empty
            // list case at lint time so it shows up in the diagnostic
            // catalog rather than as a parser failure.
            continue;
        }
        if !is_lzx_bare_ident(trimmed) {
            return Err(line_error(
                line,
                "`requires_lifecycle_in` allow-list entries must be bare lifecycle state identifiers",
            ));
        }
        allowed_states.push(trimmed.to_owned());
    }
    Ok(LzxRequiresLifecycleIn {
        resource: resource.to_owned(),
        allowed_states,
        span: Span::new(line.start, line.end),
    })
}
