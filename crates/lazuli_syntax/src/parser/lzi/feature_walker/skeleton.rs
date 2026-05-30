//! Inner walker — the giant per-feature dispatcher that builds a
//! `FeatureSkeleton` from one `feature <name>` block body. Sibling of
//! `feature_walker/mod.rs`, which keeps the public entry
//! `parse_feature_skeletons` plus the indent constants Rails-thin.

use super::super::super::common::{SourceLine, is_trivia, line_error, line_error_owned};
use super::super::super::error::{
    E_ATTACH_CTX_RETIRED, E_CONTEXT_RETIRED, E_WORKFLOW_RETIRED, ParseError,
};

use super::super::types::PollerBlockAst;
use crate::ast::{
    AggregateDecl, ApiDecl, Auth, CacheProfileDecl, Channel, CommandDecl, EnumDeclAst, EventGroup,
    FeatureDefaults, FeatureErrorsDecl, FeatureSkeleton, Job, Notification, PoliciesDecl,
    PublicContractDeclAst, QueryDecl, RecordDecl, ReportDecl, ResourceDecl, Span, TenantMigration,
    TranslationDecl, UsesClauseAst, Webhook,
};

use super::super::agent::parse_agent;
use super::super::api::parse_api_decl;
use super::super::auth::parse_auth;
use super::super::cache;
use super::super::command;
use super::super::defaults::parse_defaults;
use super::super::enums::parse_enum_decl;
use super::super::event;
use super::super::feature_errors::parse_feature_errors_decl;
use super::super::feature_prelude::{
    attach_public_contract_to_query, parse_public_contract_line, parse_uses_line,
    take_matching_public_contract,
};
use super::super::iron_hand_context::{
    parse_feature_knowledge_line, parse_feature_non_goals_block, parse_feature_purpose_line,
};
use super::super::job::{parse_job, parse_tenant_migration};
use super::super::mcp;
use super::super::notification;
use super::super::policy;
use super::super::poller;
use super::super::query;
use super::super::record;
use super::super::report;
use super::super::resource::{parse_aggregate_decl, parse_resource_decl};
use super::super::translation;
use super::super::webhook::parse_webhook;

use super::AGENT_INDENT_FEATURE_CHILD;

pub(super) fn parse_feature_skeleton(
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
    // Iron-hand context vocabulary — purpose / non_goals / knowledge.
    // Each at most once per feature; duplicates are parse errors.
    // (Feature context is resolved by the co-located `<feature>.ctx.md`
    // CONVENTION in the analyzer — the retired `attach_ctx` keyword
    // hard-errors below as `E-ATTACH-CTX-RETIRED`.)
    let mut purpose: Option<crate::ast::LziFeaturePurpose> = None;
    let mut non_goals: Option<crate::ast::LziFeatureNonGoals> = None;
    let mut knowledge: Option<crate::ast::LziFeatureKnowledge> = None;
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
            purpose = Some(parse_feature_purpose_line(line, rest)?);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(next) = parse_feature_async_context_line(
            lines,
            &name,
            i,
            &mut last_end,
            &mut non_goals,
            &mut knowledge,
            &mut jobs,
            &mut webhooks,
            &mut notifications,
            &mut pollers,
            &mut channels,
            &mut caches,
            &mut aggregates,
        )? {
            i = next;
            continue;
        }

        // event_group accepts any indent > feature-child (keyword
        // unambiguous; fixture authors it under `domain` at indent 4).
        if trimmed.starts_with("event_group ") {
            let (parsed, next) = event::parse_event_group(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            event_groups.push(parsed);
            i = next;
            continue;
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("mcp_server ") {
            let (parsed, next) = mcp::parse_mcp_server(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            mcp_servers.push(parsed);
            i = next;
            continue;
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("tenant_migration ") {
            let (parsed, next) = parse_tenant_migration(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            tenant_migrations.push(parsed);
            i = next;
            continue;
        }

        // `uses <feature>[, <feature>]* [version v<N>]` — cross-feature
        // contracts §5.4. Multiple comma-separated entries fan out.
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

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("api ") {
            let (parsed, next) = parse_api_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            apis.push(parsed);
            i = next;
            continue;
        }

        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("report ") {
            let (parsed, next) = report::parse_report_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            reports.push(parsed);
            i = next;
            continue;
        }

        // `resource` accepts any indent > FEATURE_CHILD (fixture nests
        // under `domain` at indent 4); the decl walker enforces children.
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

        if trimmed.starts_with("record ") {
            let (mut parsed, next) = record::parse_record_decl(lines, i)?;
            parsed.public_contract =
                take_matching_public_contract(line, &mut pending_contract, "record", &parsed.name)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            records.push(parsed);
            i = next;
            continue;
        }

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

        if trimmed.starts_with("enum ") && line.indent > AGENT_INDENT_FEATURE_CHILD {
            let (mut parsed, next) = parse_enum_decl(lines, i)?;
            parsed.public_contract =
                take_matching_public_contract(line, &mut pending_contract, "enum", &parsed.name)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            enums.push(parsed);
            i = next;
            continue;
        }

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
        //
        // Coded as `E-WORKFLOW-RETIRED` per audit Cell I + §2.1.4 +
        // §5 last row of the lifecycle-vocab proposal. The leading
        // `[E-WORKFLOW-RETIRED]` tag on the message is the stable
        // marker downstream tooling reads to populate the diagnostic
        // `code` field.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("workflow ") {
            return Err(line_error_owned(
                line,
                format!(
                    "[{E_WORKFLOW_RETIRED}] the `workflow` keyword was retired in favor of \
                     `lifecycle` (proposal: docs/proposals/lifecycle-vocab.md). Refactor to a \
                     `lifecycle <field>` block inside the targeted `resource`. Each transition \
                     lifts 1:1: `name: from -> to emits X` becomes \
                     `transition name\\n  from <state>\\n  to <state>\\n  emits X`.",
                ),
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
            knowledge,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

include!("skeleton_p1.rs");
