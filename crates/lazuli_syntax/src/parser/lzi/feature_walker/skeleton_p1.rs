// SPEC-19: extracted async/context dispatch branches of
// `parse_feature_skeleton` (kept the per-feature dispatcher under the
// 500-LOC ceiling). include!'d into `skeleton.rs` — same module.

#[allow(clippy::too_many_arguments)]
fn parse_feature_async_context_line(
    lines: &[SourceLine<'_>],
    name: &str,
    i: usize,
    last_end: &mut usize,
    non_goals: &mut Option<crate::ast::LziFeatureNonGoals>,
    knowledge: &mut Option<crate::ast::LziFeatureKnowledge>,
    jobs: &mut Vec<Job>,
    webhooks: &mut Vec<Webhook>,
    notifications: &mut Vec<Notification>,
    pollers: &mut Vec<PollerBlockAst>,
    channels: &mut Vec<Channel>,
    caches: &mut Vec<CacheProfileDecl>,
    aggregates: &mut Vec<AggregateDecl>,
) -> Result<Option<usize>, ParseError> {
    let line = &lines[i];
    let trimmed = line.text.trim_start();

        // Iron-hand context vocabulary — `non_goals` block (see
        // `lzi::iron_hand_context::parse_feature_non_goals_block` for
        // the closed two-shape grammar).
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed == "non_goals" {
            if non_goals.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `non_goals` block",
                ));
            }
            let (block, next) = parse_feature_non_goals_block(lines, i)?;
            *last_end = block.span.end;
            *non_goals = Some(block);
            return Ok(Some(next));
        }

        // Retired keyword — feature-header-level `attach_ctx "<path>"`.
        // The `attach_ctx` keyword was retired in favour of a co-located
        // `<feature>.ctx.md` CONVENTION: the analyzer probes
        // `<dir-of-the-.lzi>/<feature>.ctx.md` and auto-attaches it when
        // present (no keyword, no path argument, a single resolution
        // base). We special-case the known dead form — an `attach_ctx `
        // line whose argument is a quoted string literal — so this stays
        // scoped and does not over-reject other unknown children (which
        // remain in the legacy text-pattern doctor pipeline).
        //
        // Coded as `E-ATTACH-CTX-RETIRED`; mirrors `E-CONTEXT-RETIRED` /
        // `E-WORKFLOW-RETIRED` exactly. The leading `[E-ATTACH-CTX-RETIRED]`
        // tag on the message is the stable marker downstream tooling reads
        // to populate the diagnostic `code` field.
        if line.indent == AGENT_INDENT_FEATURE_CHILD
            && let Some(rest) = trimmed.strip_prefix("attach_ctx ")
            && rest.trim_start().starts_with('"')
        {
            return Err(line_error_owned(
                line,
                format!(
                    "[{E_ATTACH_CTX_RETIRED}] the `attach_ctx \"...\"` keyword was retired; \
                     feature context is now resolved by the co-located `{name}.ctx.md` \
                     convention. Delete this line and place the prose in `{name}.ctx.md` next \
                     to this `.lzi` file — the analyzer auto-attaches `<feature>.ctx.md` when \
                     present (a single resolution base: no path argument, no project-root fallback).",
                ),
            ));
        }

        // Iron-hand context vocabulary — `knowledge <sector>`.
        // Single bareword sector-slug line at indent 2 naming the
        // `knowledge/<sector>/` vault. At most one per feature.
        // See `docs/proposals/knowledge-sector-field.md`.
        if line.indent == AGENT_INDENT_FEATURE_CHILD
            && let Some(rest) = trimmed.strip_prefix("knowledge ")
        {
            if knowledge.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `knowledge` line",
                ));
            }
            *knowledge = Some(parse_feature_knowledge_line(line, rest)?);
            *last_end = line.end;
            return Ok(Some(i + 1));
        }

        // Retired dead form — a feature-header-level `context "<path>"`
        // line never had a parser branch, so it used to be silently
        // dropped (zero `context_path` in the IR), violating inviolable
        // rule #7 (no silent runtime behaviour). Feature context is now
        // resolved by CONVENTION: a co-located `<feature>.ctx.md` sidecar
        // next to this `.lzi` file (no keyword, no path argument). We
        // special-case the known dead form ONLY — a `context ` line whose
        // argument is a quoted string literal — so this stays scoped and
        // does not over-reject other unknown children (which remain in
        // the legacy text-pattern doctor pipeline). The live agent-body
        // `context <expr>` keyword lives inside `parse_agent` at indent 4
        // and is untouched.
        //
        // Coded as `E-CONTEXT-RETIRED`; mirrors `E-WORKFLOW-RETIRED`
        // exactly. The leading `[E-CONTEXT-RETIRED]` tag on the message
        // is the stable marker downstream tooling reads to populate the
        // diagnostic `code` field.
        if line.indent == AGENT_INDENT_FEATURE_CHILD
            && let Some(rest) = trimmed.strip_prefix("context ")
            && rest.trim_start().starts_with('"')
        {
            return Err(line_error_owned(
                line,
                format!(
                    "[{E_CONTEXT_RETIRED}] feature-level `context \"...\"` is not \
                     recognized. Feature context is resolved by CONVENTION: \
                     place the prose in a co-located `{name}.ctx.md` sidecar \
                     next to this `.lzi` file (no keyword, no path argument). \
                     The bare `context` form was never wired into the parser \
                     and was silently dropped (no context_path in the IR).",
                ),
            ));
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("job ") {
            let (parsed, next) = parse_job(lines, i)?;
            *last_end = lines[next.saturating_sub(1).max(i)].end;
            jobs.push(parsed);
            return Ok(Some(next));
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("webhook ") {
            let (parsed, next) = parse_webhook(lines, i)?;
            *last_end = lines[next.saturating_sub(1).max(i)].end;
            webhooks.push(parsed);
            return Ok(Some(next));
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("notification ") {
            let (parsed, next) = notification::parse_notification(lines, i)?;
            *last_end = lines[next.saturating_sub(1).max(i)].end;
            notifications.push(parsed);
            return Ok(Some(next));
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("poller ") {
            let (parsed, next) = poller::parse_poller_block(lines, i)?;
            *last_end = lines[next.saturating_sub(1).max(i)].end;
            pollers.push(parsed);
            return Ok(Some(next));
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("channel ") {
            let (parsed, next) = notification::parse_channel(lines, i)?;
            *last_end = lines[next.saturating_sub(1).max(i)].end;
            channels.push(parsed);
            return Ok(Some(next));
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("cache ") {
            let (parsed, next) = cache::parse_cache_profile(lines, i)?;
            *last_end = lines[next.saturating_sub(1).max(i)].end;
            caches.push(parsed);
            return Ok(Some(next));
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("aggregate ") {
            let (parsed, next) = parse_aggregate_decl(lines, i)?;
            *last_end = lines[next.saturating_sub(1).max(i)].end;
            aggregates.push(parsed);
            return Ok(Some(next));
        }

    Ok(None)
}
