//! Feature-skeleton walker: the hand-rolled line-stream parser that
//! walks every `feature <name>` block in a `.lzi` source and dispatches
//! to sibling parsers for each feature child (agent, auth, job, command,
//! resource, ...). Re-exported from `parser/lzi/mod.rs`.
//!
//! See the parent module docs for ABI notes; the walker stays here so
//! `lzi/mod.rs` can be the thin module root the rails-style split asks
//! for.

use super::super::common::{SourceLine, is_trivia, line_error, source_lines};
use super::super::error::ParseError;

use crate::ast::{
    AggregateDecl, ApiDecl, Auth, CacheProfileDecl, Channel, CommandDecl, EnumDeclAst, EventGroup,
    FeatureDefaults, FeatureErrorsDecl, FeatureSkeleton, Job, Notification, PoliciesDecl,
    PublicContractDeclAst, QueryDecl, RecordDecl, ReportDecl, ResourceDecl, Span, TenantMigration,
    TranslationDecl, UsesClauseAst, Webhook,
};
use super::types::PollerBlockAst;

use super::agent::parse_agent;
use super::api::parse_api_decl;
use super::auth::parse_auth;
use super::cache;
use super::command;
use super::defaults::parse_defaults;
use super::enums::parse_enum_decl;
use super::event;
use super::feature_errors::parse_feature_errors_decl;
use super::feature_prelude::{
    attach_public_contract_to_query, parse_public_contract_line, parse_uses_line,
    take_matching_public_contract,
};
use super::helpers::take_quoted_string;
use super::job::{parse_job, parse_tenant_migration};
use super::mcp;
use super::notification;
use super::poller;
use super::policy;
use super::query;
use super::record;
use super::report;
use super::resource::{parse_aggregate_decl, parse_resource_decl};
use super::translation;
use super::webhook::parse_webhook;

pub(crate) const AGENT_INDENT_FEATURE_CHILD: usize = 2;
pub(crate) const AGENT_INDENT_AGENT_CHILD: usize = 4;
pub(crate) const AGENT_INDENT_GRANDCHILD: usize = 6;
pub(crate) const AGENT_INDENT_GREAT_GRANDCHILD: usize = 8;

/// Parse every `feature <name>` block in a `.lzi` source, returning a
/// skeleton that lists only the agents inside each feature. Other feature
/// children are not surfaced — Cut A intentionally narrows.
pub fn parse_feature_skeletons(source: &str) -> Result<Vec<FeatureSkeleton>, ParseError> {
    let lines = source_lines(source);
    let mut features = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent == 0 {
            if let Some(rest) = trimmed.strip_prefix("feature ") {
                let name = rest.trim().to_owned();
                if name.is_empty() {
                    return Err(line_error(
                        line,
                        "feature header requires a name: `feature <name>`",
                    ));
                }
                let (feature, next) = parse_feature_skeleton(&lines, i, name)?;
                features.push(feature);
                i = next;
                continue;
            }
            // Other top-level constructs (`app`, `workspace`, `contract`, ...)
            // are not parsed by this slice. Skip the line.
            i += 1;
            continue;
        }

        // Stray indented top-level content — outside any feature. Skip.
        i += 1;
    }

    Ok(features)
}

fn parse_feature_skeleton(
    lines: &[SourceLine<'_>],
    start: usize,
    name: String,
) -> Result<(FeatureSkeleton, usize), ParseError> {
    let header = &lines[start];
    let mut agents = Vec::new();
    let mut auth: Option<Auth> = None;
    let mut jobs: Vec<Job> = Vec::new();
    let mut webhooks: Vec<Webhook> = Vec::new();
    let mut notifications: Vec<Notification> = Vec::new();
    let mut event_groups: Vec<EventGroup> = Vec::new();
    let mut tenant_migrations: Vec<TenantMigration> = Vec::new();
    let mut defaults: Option<FeatureDefaults> = None;
    let mut commands: Vec<CommandDecl> = Vec::new();
    let mut apis: Vec<ApiDecl> = Vec::new();
    let mut resources: Vec<ResourceDecl> = Vec::new();
    let mut queries: Vec<QueryDecl> = Vec::new();
    let mut records: Vec<RecordDecl> = Vec::new();
    let mut policies_block: Option<PoliciesDecl> = None;
    // IR Error-Vocab (Cell PARSE-1) — at most one `errors` block per
    // feature; duplicate is a parse error.
    let mut errors_block: Option<FeatureErrorsDecl> = None;
    let mut enums: Vec<EnumDeclAst> = Vec::new();
    let mut translation: Option<TranslationDecl> = None;
    let mut pollers: Vec<PollerBlockAst> = Vec::new();
    let mut reports: Vec<ReportDecl> = Vec::new();
    let mut channels: Vec<Channel> = Vec::new();
    // CL.C.3 — `cache <name>` feature-level profiles.
    let mut caches: Vec<CacheProfileDecl> = Vec::new();
    // CL.C.4 — `aggregate <Name>` blocks (DDD consistency boundaries).
    let mut aggregates: Vec<AggregateDecl> = Vec::new();
    // Cross-feature contracts — `uses <feature>[, ...] [version v<N>]` lines.
    let mut uses_clauses: Vec<UsesClauseAst> = Vec::new();
    // MCP bucket cycle — `mcp_server <name>` feature-scoped blocks.
    let mut mcp_servers: Vec<crate::ast::McpServer> = Vec::new();
    // Iron-hand context vocabulary — purpose / non_goals / attach_ctx.
    // Each at most once per feature; duplicates are parse errors.
    let mut purpose: Option<crate::ast::LziFeaturePurpose> = None;
    let mut non_goals: Option<crate::ast::LziFeatureNonGoals> = None;
    let mut attach_ctx: Option<crate::ast::LziFeatureAttachCtx> = None;
    let mut pending_contract: Option<(String, PublicContractDeclAst)> = None;
    let mut i = start + 1;
    let mut last_end = header.end;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        // A new top-level construct ends this feature body.
        if line.indent == 0 {
            break;
        }

        if let Some((symbol, contract)) = parse_public_contract_line(line)? {
            if pending_contract.is_some() {
                return Err(line_error(
                    line,
                    "two `public contract` declarations may not appear in a row without a matching symbol declaration",
                ));
            }
            // TODO(events): wire up `public contract event.<name>` when
            // event AST gains a public_contract field.
            pending_contract = Some((symbol, contract));
            last_end = line.end;
            i += 1;
            continue;
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("agent ") {
            let (agent, next) = parse_agent(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            agents.push(agent);
            i = next;
            continue;
        }

        // Phase L — `auth` block. One per feature; duplicate is a parse error.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed == "auth" {
            if auth.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `auth` block",
                ));
            }
            let (parsed, next) = parse_auth(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            auth = Some(parsed);
            i = next;
            continue;
        }

        // Iron-hand context vocabulary — `purpose "<sentence>"`.
        // Single quoted-string line at indent 2. At most one per feature.
        if line.indent == AGENT_INDENT_FEATURE_CHILD
            && let Some(rest) = trimmed.strip_prefix("purpose ")
        {
            if purpose.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `purpose` line",
                ));
            }
            let (text, tail) = take_quoted_string(rest.trim_start(), line).map_err(|_| {
                line_error(
                    line,
                    "`purpose` requires a quoted string literal — e.g. \
                     `purpose \"Discover and book lodging\"`",
                )
            })?;
            if !tail.trim().is_empty() {
                return Err(line_error(
                    line,
                    "`purpose` accepts exactly one quoted string and no trailing tokens",
                ));
            }
            purpose = Some(crate::ast::LziFeaturePurpose {
                text,
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
            continue;
        }

        // Iron-hand context vocabulary — `non_goals` block. Two
        // surface shapes are accepted (the lowered IR is identical):
        //
        //   non_goals
        //     "Full marketplace listing optimization"
        //     "Real-time chat (use messaging feature)"
        //
        //   non_goals
        //     delegated_to
        //       customer_auth: "customer login and MFA"
        //       customer_tags: "tag management"
        //     out_of_scope
        //       "Invoicing"
        //
        // The flat form is preferred for new features; the partitioned
        // form is retained because the canonical full-capsule fixture
        // already authors it and removing it would invalidate the
        // cold-read bar. Both lower to a flat list of descriptions for
        // `VOCAB-CONTEXT-NONGOALS-001`; the analyzer keeps `key` for
        // the optional partition tag (empty for flat entries).
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed == "non_goals" {
            if non_goals.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `non_goals` block",
                ));
            }
            let header_indent = line.indent;
            let child_indent = header_indent + 2;
            let grandchild_indent = child_indent + 2;
            let block_start = line.start;
            let mut block_end = line.end;
            let mut entries: Vec<String> = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let child = &lines[j];
                let child_trim = child.text.trim_start();
                if is_trivia(child_trim) {
                    j += 1;
                    continue;
                }
                if child.indent <= header_indent {
                    break;
                }
                if child.indent != child_indent {
                    return Err(line_error(
                        child,
                        "`non_goals` entries must be indented by exactly two spaces \
                         beyond the `non_goals` header",
                    ));
                }
                // Partitioned form: `delegated_to` / `out_of_scope`
                // group header followed by `key: \"text\"` lines.
                if child_trim == "delegated_to" || child_trim == "out_of_scope" {
                    block_end = child.end;
                    let mut k = j + 1;
                    while k < lines.len() {
                        let grand = &lines[k];
                        let grand_trim = grand.text.trim_start();
                        if is_trivia(grand_trim) {
                            k += 1;
                            continue;
                        }
                        if grand.indent <= child_indent {
                            break;
                        }
                        if grand.indent != grandchild_indent {
                            return Err(line_error(
                                grand,
                                "`non_goals` partition entries must be indented by exactly \
                                 two spaces beyond their group header",
                            ));
                        }
                        // Accept either `key: "text"` (the canonical
                        // partitioned shape) or a bare `"text"` line.
                        if let Some(colon_pos) = grand_trim.find(':') {
                            let value_part = grand_trim[colon_pos + 1..].trim_start();
                            let (text, tail) =
                                take_quoted_string(value_part, grand).map_err(|_| {
                                    line_error(
                                        grand,
                                        "`non_goals` partition entry value must be a quoted \
                                         string — e.g. `customer_auth: \"customer login and MFA\"`",
                                    )
                                })?;
                            if !tail.trim().is_empty() {
                                return Err(line_error(
                                    grand,
                                    "`non_goals` partition entry accepts exactly one quoted \
                                     string after `:`",
                                ));
                            }
                            entries.push(text);
                        } else {
                            let (text, tail) =
                                take_quoted_string(grand_trim, grand).map_err(|_| {
                                    line_error(
                                        grand,
                                        "`non_goals` entries must be quoted strings or \
                                         `<key>: \"<text>\"` pairs",
                                    )
                                })?;
                            if !tail.trim().is_empty() {
                                return Err(line_error(
                                    grand,
                                    "`non_goals` entries accept exactly one quoted string \
                                     per line",
                                ));
                            }
                            entries.push(text);
                        }
                        block_end = grand.end;
                        k += 1;
                    }
                    j = k;
                    continue;
                }
                // Flat form: bare quoted string at child indent.
                let (text, tail) = take_quoted_string(child_trim, child).map_err(|_| {
                    line_error(
                        child,
                        "`non_goals` entries must be quoted strings — e.g. \
                         `  \"Full marketplace listing optimization\"`",
                    )
                })?;
                if !tail.trim().is_empty() {
                    return Err(line_error(
                        child,
                        "`non_goals` entries accept exactly one quoted string per line",
                    ));
                }
                entries.push(text);
                block_end = child.end;
                j += 1;
            }
            non_goals = Some(crate::ast::LziFeatureNonGoals {
                entries,
                span: Span::new(block_start, block_end),
            });
            last_end = block_end;
            i = j;
            continue;
        }

        // Iron-hand context vocabulary — `attach_ctx "<relative-path>"`.
        // Single quoted-string line at indent 2. At most one per feature.
        if line.indent == AGENT_INDENT_FEATURE_CHILD
            && let Some(rest) = trimmed.strip_prefix("attach_ctx ")
        {
            if attach_ctx.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `attach_ctx` line",
                ));
            }
            let (path, tail) = take_quoted_string(rest.trim_start(), line).map_err(|_| {
                line_error(
                    line,
                    "`attach_ctx` requires a quoted relative path — e.g. \
                     `attach_ctx \"./ctx.md\"`",
                )
            })?;
            if !tail.trim().is_empty() {
                return Err(line_error(
                    line,
                    "`attach_ctx` accepts exactly one quoted path and no trailing tokens",
                ));
            }
            attach_ctx = Some(crate::ast::LziFeatureAttachCtx {
                path,
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
            continue;
        }

        // Phase L Tier 3 — `job <name>` block.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("job ") {
            let (parsed, next) = parse_job(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            jobs.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 3 — `webhook <name>` block.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("webhook ") {
            let (parsed, next) = parse_webhook(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            webhooks.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 3 — `notification <name>` block.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("notification ") {
            let (parsed, next) = notification::parse_notification(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            notifications.push(parsed);
            i = next;
            continue;
        }

        // L0 #8 — `poller <name>` block (docs/proposals/poller-vocab.md).
        // Closed-catalog feature kind, parallel sibling of `job` /
        // `webhook` / `notification`.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("poller ") {
            let (parsed, next) = poller::parse_poller_block(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            pollers.push(parsed);
            i = next;
            continue;
        }

        // Realtime bucket cycle MVP — `channel <name>` block. Closed
        // three-child body (`tenant_from`, `policy`, `payload`). See
        // `docs/proposals/bucket-realtime-cycle.md`.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("channel ") {
            let (parsed, next) = notification::parse_channel(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            channels.push(parsed);
            i = next;
            continue;
        }

        // Cache bucket cycle (CL.C.3) — feature-level `cache <name>`
        // profile block. Sibling of `notification`/`channel`/`poller`.
        // Queries reference profiles by name via `cache <profile>` in
        // their body; the inline `cache { key, ttl }` shape on a query
        // stays for one-off ttl/key pairs.
        // See `docs/proposals/bucket-cache-cycle.md` + roadmap §1.15.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("cache ") {
            let (parsed, next) = cache::parse_cache_profile(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            caches.push(parsed);
            i = next;
            continue;
        }

        // CL.C.4 — `aggregate <Name>` block (DDD consistency boundary).
        // Closed body: `root <Resource>` (required), `contains
        // <Resource>, ...` (optional, repeatable), `invariants` block
        // (optional). Sibling of `resource`/`command` at feature-child
        // indent. See roadmap §1.7.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("aggregate ") {
            let (parsed, next) = parse_aggregate_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            aggregates.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 3 — `event_group <pattern> on <Resource>` block.
        // The fixture authors the group inside `domain` at indent 4, so
        // we accept any indent > feature-child (the construct keyword is
        // unambiguous).
        if trimmed.starts_with("event_group ") {
            let (parsed, next) = event::parse_event_group(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            event_groups.push(parsed);
            i = next;
            continue;
        }

        // MCP bucket cycle — `mcp_server <name>` block. Closed-catalog
        // children: transport / scope / auth / metadata / tool /
        // resource / prompt. See `docs/proposals/bucket-mcp-cycle.md`.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("mcp_server ") {
            let (parsed, next) = mcp::parse_mcp_server(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            mcp_servers.push(parsed);
            i = next;
            continue;
        }

        // Migrations bucket cycle Route C — `tenant_migration <name>`
        // block. Sibling of `job`/`webhook`/`notification`; closed body.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("tenant_migration ") {
            let (parsed, next) = parse_tenant_migration(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            tenant_migrations.push(parsed);
            i = next;
            continue;
        }

        // Cross-feature contracts §5.4 — feature-level
        // `uses <feature>[, <feature>]* [version v<N>]` line. Multiple
        // comma-separated entries on one line yield multiple UsesClauseAst
        // entries; the trailing `version v<N>` (when present) pins every
        // entry on the line to that consumer-side version.
        if line.indent == AGENT_INDENT_FEATURE_CHILD
            && let Some(rest) = trimmed.strip_prefix("uses ")
        {
            let line_span = Span::new(line.start, line.end);
            for clause in parse_uses_line(rest, line, line_span)? {
                uses_clauses.push(clause);
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        // Phase L Tier 4a — `defaults` block.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed == "defaults" {
            if defaults.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `defaults` block",
                ));
            }
            let (parsed, next) = parse_defaults(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            defaults = Some(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 4b — `command <name>` block.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("command ") {
            let (mut parsed, next) = command::parse_command_decl(lines, i)?;
            parsed.public_contract = take_matching_public_contract(
                line,
                &mut pending_contract,
                "command",
                &parsed.name,
            )?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            commands.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 4b — `api <name>` block.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("api ") {
            let (parsed, next) = parse_api_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            apis.push(parsed);
            i = next;
            continue;
        }

        // Report vocab — `report <name>` block. Tabular export contract
        // (CSV / XLSX). Sibling of `api`/`command`/`query`.
        // See `docs/proposals/report-vocab.md` v0.2.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("report ") {
            let (parsed, next) = report::parse_report_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            reports.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 4c — `resource <Name>` block. The fixture authors
        // resources inside `domain` at indent 4 (and historically also
        // at indent 2 directly under `feature`), so we accept any
        // indent > FEATURE_CHILD as long as the keyword and children
        // shape are unambiguous. `parse_resource_decl` enforces the
        // child indent contract relative to its own header.
        if trimmed.starts_with("resource ") {
            let (mut parsed, next) = parse_resource_decl(lines, i)?;
            parsed.public_contract = take_matching_public_contract(
                line,
                &mut pending_contract,
                "resource",
                &parsed.name,
            )?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            resources.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 4d — `query.list` / `query.lookup` / `query.sql`
        // / `query.view`
        // blocks. Authored inside `domain` at indent 4. Header is
        // recognised unambiguously by the keyword prefix.
        if trimmed.starts_with("query.list ")
            || trimmed.starts_with("query.lookup ")
            || trimmed.starts_with("query.sql ")
            || trimmed.starts_with("query.view ")
        {
            let (mut parsed, next) = query::parse_query_decl(lines, i)?;
            attach_public_contract_to_query(line, &mut pending_contract, &mut parsed)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            queries.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 4d — `record <Name>` block.
        if trimmed.starts_with("record ") {
            let (mut parsed, next) = record::parse_record_decl(lines, i)?;
            parsed.public_contract =
                take_matching_public_contract(line, &mut pending_contract, "record", &parsed.name)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            records.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 4 follow-up — `policies` block. At most one per
        // feature; duplicate is a parse error.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed == "policies" {
            if policies_block.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `policies` block",
                ));
            }
            let (parsed, next) = policy::parse_policies_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            policies_block = Some(parsed);
            i = next;
            continue;
        }

        // IR Error-Vocab (Cell PARSE-1) — `errors` block at indent 2.
        // At most one per feature; duplicate is a parse error.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed == "errors" {
            if errors_block.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `errors` block",
                ));
            }
            let (parsed, next) = parse_feature_errors_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            errors_block = Some(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 4 follow-up — `enum <Name>` declaration. The
        // fixture authors enums inside `domain` at indent 4. Header is
        // recognised unambiguously by the keyword prefix at indent > 2.
        if trimmed.starts_with("enum ") && line.indent > AGENT_INDENT_FEATURE_CHILD {
            let (mut parsed, next) = parse_enum_decl(lines, i)?;
            parsed.public_contract =
                take_matching_public_contract(line, &mut pending_contract, "enum", &parsed.name)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            enums.push(parsed);
            i = next;
            continue;
        }

        // i18n bucket cycle — `translation` block. At most one per
        // feature; duplicate is a parse error.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed == "translation" {
            if translation.is_some() {
                return Err(line_error(
                    line,
                    "feature may declare at most one `translation` block",
                ));
            }
            let (parsed, next) = translation::parse_translation_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            translation = Some(parsed);
            i = next;
            continue;
        }

        // Retired by docs/proposals/lifecycle-vocab.md v0.3 §2.1.
        // Authors used to write `workflow <name> on <Resource>.<field>`
        // at feature level; the new canonical form is a `lifecycle <field>`
        // block child of the resource itself. Detect the legacy keyword
        // explicitly so cold-readers see one form, not two.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("workflow ") {
            return Err(line_error(
                line,
                "the `workflow` keyword was retired in favor of `lifecycle` \
                 (proposal: docs/proposals/lifecycle-vocab.md). Refactor to a \
                 `lifecycle <field>` block inside the targeted `resource`. \
                 Each transition lifts 1:1: `name: from -> to emits X` becomes \
                 `transition name\\n  from <state>\\n  to <state>\\n  emits X`.",
            ));
        }

        // Any other feature child is skipped silently — surfaces remain
        // in the legacy text-pattern doctor pipeline.
        last_end = line.end;
        i += 1;
    }

    if pending_contract.is_some() {
        return Err(line_error(
            header,
            "trailing `public contract` declaration with no following matching symbol",
        ));
    }

    Ok((
        FeatureSkeleton {
            name,
            agents,
            auth,
            jobs,
            webhooks,
            notifications,
            event_groups,
            tenant_migrations,
            defaults,
            commands,
            apis,
            resources,
            queries,
            records,
            policies: policies_block,
            errors: errors_block,
            enums,
            translation,
            pollers,
            reports,
            channels,
            caches,
            aggregates,
            uses_clauses,
            mcp_servers,
            purpose,
            non_goals,
            attach_ctx,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
// =============================================================================
// Iron-hand context vocabulary — parser tests for `purpose`, `non_goals`,
// `attach_ctx`. Driven by docs/canonical-semantics.md#feature-context-
// vocabulary and the meta-bundle proposal: the `tdd-iron-hand` preset
// escalates the three VOCAB-CONTEXT-* rules from warn to error, so the
// parser MUST anchor each field with its own span for precise
// diagnostics.
// =============================================================================
#[cfg(test)]
mod iron_hand_context_tests {
    use super::parse_feature_skeletons;

    #[test]
    fn purpose_line_lowers_into_skeleton() {
        let source = "\nfeature catalog\n  purpose \"Discover and book lodging\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let purpose = features[0].purpose.as_ref().expect("purpose present");
        assert_eq!(purpose.text, "Discover and book lodging");
    }

    #[test]
    fn empty_purpose_string_parses_but_keeps_empty_text() {
        // The lint, not the parser, decides whether empty is allowed.
        let source = "\nfeature catalog\n  purpose \"\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(features[0].purpose.as_ref().unwrap().text, "");
    }

    #[test]
    fn purpose_requires_quoted_string() {
        let source = "\nfeature catalog\n  purpose Discover and book lodging\n";
        let err = parse_feature_skeletons(source).expect_err("rejects bareword");
        let msg = format!("{err}");
        assert!(
            msg.contains("quoted string"),
            "expected quoted-string diagnostic, got: {msg}",
        );
    }

    #[test]
    fn duplicate_purpose_is_rejected() {
        let source = "\nfeature catalog\n  purpose \"A\"\n  purpose \"B\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dup");
        let msg = format!("{err}");
        assert!(msg.contains("at most one `purpose`"), "got: {msg}");
    }

    #[test]
    fn non_goals_flat_form_collects_entries() {
        let source = "\nfeature catalog\n  non_goals\n    \"Full marketplace listing optimization\"\n    \"Real-time chat (use messaging feature)\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let block = features[0].non_goals.as_ref().expect("non_goals present");
        assert_eq!(block.entries.len(), 2);
        assert_eq!(block.entries[0], "Full marketplace listing optimization");
        assert_eq!(block.entries[1], "Real-time chat (use messaging feature)");
    }

    #[test]
    fn non_goals_partitioned_form_flattens_into_entries() {
        let source = "\nfeature customer\n  non_goals\n    delegated_to\n      user: \"staff authentication\"\n      customer_auth: \"customer login and MFA\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let block = features[0].non_goals.as_ref().expect("non_goals present");
        assert_eq!(block.entries.len(), 2);
        assert_eq!(block.entries[0], "staff authentication");
        assert_eq!(block.entries[1], "customer login and MFA");
    }

    #[test]
    fn non_goals_empty_block_is_legal_at_parse_time() {
        // Lint VOCAB-CONTEXT-NONGOALS-001 owns the empty-block rule.
        let source = "\nfeature catalog\n  non_goals\n  defaults\n    timestamps\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let block = features[0].non_goals.as_ref().expect("non_goals present");
        assert!(block.entries.is_empty());
    }

    #[test]
    fn duplicate_non_goals_is_rejected() {
        let source = "\nfeature catalog\n  non_goals\n    \"A\"\n  non_goals\n    \"B\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dup");
        let msg = format!("{err}");
        assert!(msg.contains("at most one `non_goals`"), "got: {msg}");
    }

    #[test]
    fn attach_ctx_line_lowers_into_skeleton() {
        let source = "\nfeature catalog\n  attach_ctx \"./ctx.md\"\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let ctx = features[0].attach_ctx.as_ref().expect("attach_ctx present");
        assert_eq!(ctx.path, "./ctx.md");
    }

    #[test]
    fn attach_ctx_requires_quoted_path() {
        let source = "\nfeature catalog\n  attach_ctx ./ctx.md\n";
        let err = parse_feature_skeletons(source).expect_err("rejects bareword");
        let msg = format!("{err}");
        assert!(msg.contains("quoted relative path"), "got: {msg}");
    }

    #[test]
    fn duplicate_attach_ctx_is_rejected() {
        let source = "\nfeature catalog\n  attach_ctx \"./a.md\"\n  attach_ctx \"./b.md\"\n";
        let err = parse_feature_skeletons(source).expect_err("rejects dup");
        let msg = format!("{err}");
        assert!(msg.contains("at most one `attach_ctx`"), "got: {msg}");
    }

    #[test]
    fn iron_hand_block_combines_with_existing_children() {
        // Smoke-check the three fields parse alongside resources /
        // commands / defaults — the canonical iron-hand-clean layout.
        let source = r#"
feature catalog
  purpose "Discover and book lodging via host properties + services"
  non_goals
    "Full marketplace listing optimization"
    "Real-time chat (use messaging feature)"
  attach_ctx "./ctx.md"
  defaults
    timestamps
  resource Property
    name: Text required
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let f = &features[0];
        assert_eq!(
            f.purpose.as_ref().unwrap().text,
            "Discover and book lodging via host properties + services"
        );
        assert_eq!(f.non_goals.as_ref().unwrap().entries.len(), 2);
        assert_eq!(f.attach_ctx.as_ref().unwrap().path, "./ctx.md");
        assert_eq!(f.resources.len(), 1);
    }
}
