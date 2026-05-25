//! `.lzi` source-text parser — every declarative slot a feature file
//! can carry: resources, commands, queries, jobs, webhooks, agents,
//! notifications, pollers, events, translations, defaults, RBAC
//! catalog, plus the design / plan / package-skeleton top-level parsers
//! that share the same line-stream contract.
//!
//! ## Entry points (public ABI)
//!
//! - `parse_feature_skeletons` — every `feature <name>` block in a
//!   `.lzi` source, expanded to typed children. The workhorse — called
//!   by the analyzer, doctor, LSP, and codegen.
//! - `parse_design_document` — `design.lzi` (color tokens, typography,
//!   shadows, motion, custom tokens).
//! - `parse_plan_blocks` — top-level `plan <name>` blocks.
//! - `parse_feature_gates` — top-level `gate` directives.
//! - `parse_package_skeleton` — top-level RBAC catalog
//!   (`permission` + `role` + `grants`).
//!
//! ## What's NOT here
//!
//! - `.lzx` files are in `lzx.rs` (both app-surface and feature-surface
//!   dialects). The bridge helpers `looks_like_policy_expr` /
//!   `try_parse_policy_expr` / `parse_policy_atom` live in `lzx.rs` as
//!   `pub(super)` items consumed by every `policy <expr>` parser here.
//! - Shared leaf mechanics (`SourceLine`, `source_lines`, error
//!   constructors, ident validators, depth-aware scanners) are in
//!   `common.rs`.
//! - The `ParseError` envelope is in `error.rs`.
//!
//! ## Cross-module bridges
//!
//! - `parse_invalidates_entry` and `parse_translation_key_token` are
//!   `pub(super)` so `lzx.rs`'s `parse_on_success_block` can call them.
//! - `parse_invariant_form` is `pub(crate)` for `lazuli_lsp`.
//!
//! ## Grammar source-of-truth
//!
//! Hand-rolled two-space indentation. No Pest grammar — each parser IS
//! the spec. `docs/canonical-semantics.md` is the prose reference.

use super::common::{SourceLine, is_trivia, line_error, source_lines};
use super::error::ParseError;
// `try_parse_policy_expr` is consumed by sibling parsers (command.rs,
// query.rs, etc.) directly from `super::super::lzx`; mod.rs no longer
// needs the direct import since the agent / auth / job parsers each
// took their own copy when they moved out.

use crate::ast::{
    AggregateDecl, ApiDecl, Auth, CacheProfileDecl, Channel, CommandDecl, EnumDeclAst, EventGroup,
    FeatureDefaults, FeatureErrorsDecl, FeatureSkeleton, Job, Notification, PoliciesDecl,
    PublicContractDeclAst, QueryDecl, RecordDecl, ReportDecl, ResourceDecl, Span, TenantMigration,
    TranslationDecl, UsesClauseAst, Webhook,
};

mod agent;
mod api;
mod auth;
pub mod cache;
mod command;
mod defaults;
pub mod design;
mod enums;
pub mod event;
mod feature_errors;
mod feature_prelude;
mod field_constraints;
mod job;
mod lifecycle;
mod locale;
pub mod mcp;
pub mod notification;
mod numerics;
pub mod package;
pub mod plan;
mod policy;
mod poller;
mod query;
pub mod record;
pub mod report;
mod resource;
pub mod translation;
pub mod types;
mod webhook;

mod helpers;

pub(super) use helpers::{
    is_policy_identifier, parse_named_args, split_call_signature, split_first_token,
    split_top_level_commas, take_identifier, take_quoted_string,
};
#[cfg(test)]
pub(super) use helpers::{InvariantForm, parse_invariant_form};

use agent::parse_agent;
use api::parse_api_decl;
use auth::parse_auth;
pub(super) use command::parse_invalidates_entry;
use defaults::parse_defaults;
pub(super) use defaults::parse_defaults_tenancy;
use enums::parse_enum_decl;
use feature_errors::parse_feature_errors_decl;
use feature_prelude::{
    attach_public_contract_to_query, parse_public_contract_line, parse_uses_line,
    take_matching_public_contract,
};
use job::{parse_job, parse_tenant_migration};
use numerics::{fold_rate_limit_line, parse_rate_limit_line_body};
// `parse_resource_field_decl` is re-imported here (not `pub(super)`)
// because the only outside caller is `lzi/record.rs`, which reaches it
// via `super::parse_resource_field_decl` — i.e., from inside `lzi`,
// where private items are already visible.
use resource::{parse_aggregate_decl, parse_resource_decl, parse_resource_field_decl};
use webhook::parse_webhook;

pub use design::parse_design_document;
pub use package::parse_package_skeleton;
pub use plan::{parse_feature_gates, parse_plan_blocks};
pub(super) use translation::parse_translation_key_token;
pub use types::{
    LifecycleBlockAst, LifecycleInvariantAst, LifecycleStateAst, LifecycleTransitionAst,
    PollerBlockAst, PollerCursorAst, PollerRetryAst, PollerRetryQuirkAst, PollerStateAst,
    PollerTickAst,
};

// =============================================================================
// Cut A — feature skeleton + agent slice
//
// Hand-written line-walker mirroring `parse_lzx_document`. Reads `feature
// <name>` headers at column 0 and indented `agent <name>` blocks at the
// feature's two-space child indent. Every other feature child (resources,
// commands, queries, workflows, ...) is silently skipped — the legacy
// pipeline owns them until later cuts migrate.
//
// The slice assumes two-space indentation (the canonical fixture convention
// and what `parse_lzx_document` already enforces). When the broader
// canonical-indent migration ships an INDENT/DEDENT preprocessor, this
// walker collapses into a shared indentation parser.
//
// See docs/proposals/ai-primitives-v0-implementation.md §3.3.
// =============================================================================

pub(super) const AGENT_INDENT_FEATURE_CHILD: usize = 2;
pub(super) const AGENT_INDENT_AGENT_CHILD: usize = 4;
pub(super) const AGENT_INDENT_GRANDCHILD: usize = 6;
pub(super) const AGENT_INDENT_GREAT_GRANDCHILD: usize = 8;

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
// `resource` / `aggregate` / `invariant` parsers moved to `lzi/resource/`.
// `auth` block parser moved to `lzi/auth.rs`.
// -----------------------------------------------------------------------------
// Phase L Tier 3 — job / webhook / notification / event_group parsers.
//
// All four constructs are feature children authored at
// AGENT_INDENT_FEATURE_CHILD (2 spaces); their grandchildren live at
// AGENT_INDENT_AGENT_CHILD (4 spaces). Inner blocks
// (`verify`, `calls`, `payload`) lift their leaves to
// AGENT_INDENT_GRANDCHILD (6 spaces) to match the auth-block pattern.
//
// Route C (`docs/proposals/phase-l-tier-3-job-effect-scope.md`):
// declarative job bodies (`target query.by_id(...) / let / updates /
// creates / deletes / emits`) are captured as raw lines until Tier 4
// lifts the shared declarative spine alongside `parse_command`.
// -----------------------------------------------------------------------------

// `job` / `tenant_migration` parsers moved to `lzi/job.rs`.


#[cfg(test)]
mod tests {
    use super::super::lzx::parse_lzx_document;
    use super::{InvariantForm, SourceLine, parse_invariant_form};
    use crate::{LzxPlatform, LzxViewTestAssertion};

    fn make_invariant_line() -> SourceLine<'static> {
        SourceLine {
            text: "  invariant test",
            indent: 2,
            start: 0,
            end: 16,
        }
    }

    #[test]
    fn parses_terminal_immutable() {
        let line = make_invariant_line();
        assert_eq!(
            parse_invariant_form(&line, "terminal_immutable").unwrap(),
            InvariantForm::TerminalImmutable
        );
    }

    #[test]
    fn parses_no_jump_more_than_one() {
        let line = make_invariant_line();
        assert_eq!(
            parse_invariant_form(&line, "no_jump_more_than_one").unwrap(),
            InvariantForm::NoJumpMoreThanOne
        );
    }

    #[test]
    fn parses_single_state_per_scope() {
        let line = make_invariant_line();
        let form = parse_invariant_form(&line, "single gold per item_id").unwrap();

        match form {
            InvariantForm::SingleStatePerScope { state, scope_field } => {
                assert_eq!(state, "gold");
                assert_eq!(scope_field, "item_id");
            }
            _ => panic!("expected SingleStatePerScope"),
        }
    }

    #[test]
    fn rejects_unknown_form() {
        let line = make_invariant_line();
        let err = parse_invariant_form(&line, "my_custom_thing").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("closed catalog"));
    }

    #[test]
    fn rejects_single_with_missing_tokens() {
        let line = make_invariant_line();
        let err = parse_invariant_form(&line, "single gold per").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("closed catalog") || msg.contains("requires"));
    }

    #[test]
    fn rejects_single_with_wrong_separator() {
        let line = make_invariant_line();
        let err = parse_invariant_form(&line, "single gold by item_id").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("closed catalog"));
    }

    #[test]
    fn rejects_predicate_style() {
        let line = make_invariant_line();
        let err = parse_invariant_form(&line, "single gold where item_id = parent.id").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("closed catalog"));
    }


    // -------------------------------------------------------------------------
    // Cut A — feature skeleton + agent slice (§3.5 snapshot tests)
    // -------------------------------------------------------------------------

    use super::parse_feature_skeletons;
    use crate::{
        AgentEvalKind, AgentEvalPredicate, AgentOutput, ContainsRhs, DefaultsTenancy, ToolsCallsOp,
    };


    // -------------------------------------------------------------------------
    // Phase L Tier 4a — `defaults` block parser slice
    // -------------------------------------------------------------------------

    #[test]
    fn defaults_full_block_parses() {
        let source = r#"
feature customer
  defaults
    tenancy org
    timestamps
    policy_for jobs, webhooks: @actor.system
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let defaults = features[0].defaults.as_ref().expect("defaults block");
        assert!(matches!(defaults.tenancy, Some(DefaultsTenancy::Org)));
        assert!(defaults.timestamps);
        assert_eq!(defaults.policy_for.len(), 1);
        assert_eq!(defaults.policy_for[0].kinds, vec!["jobs", "webhooks"]);
        assert_eq!(defaults.policy_for[0].atom, "@actor.system");
    }

    #[test]
    fn defaults_tenancy_only_parses() {
        let source = r#"
feature customer_auth
  defaults
    tenancy team
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let defaults = features[0].defaults.as_ref().expect("defaults block");
        assert!(matches!(defaults.tenancy, Some(DefaultsTenancy::Team)));
        assert!(!defaults.timestamps);
        assert!(defaults.policy_for.is_empty());
    }

    #[test]
    fn defaults_custom_tenancy_parses() {
        let source = r#"
feature workspace_pinned
  defaults
    tenancy workspace
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let defaults = features[0].defaults.as_ref().expect("defaults block");
        match defaults.tenancy.as_ref().expect("axis") {
            DefaultsTenancy::Custom(axis) => assert_eq!(axis, "workspace"),
            other => panic!("expected custom axis, got {other:?}"),
        }
    }

    #[test]
    fn defaults_duplicate_block_errors() {
        let source = r#"
feature customer
  defaults
    tenancy org

  defaults
    tenancy team
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("at most one"),
            "error should mention duplicate defaults: {message}"
        );
    }

    #[test]
    fn defaults_unknown_child_errors() {
        let source = r#"
feature customer
  defaults
    timestaps
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("tenancy"),
            "error should list valid children: {message}"
        );
    }

    #[test]
    fn defaults_policy_for_without_colon_errors() {
        let source = r#"
feature customer
  defaults
    policy_for jobs @actor.system
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("<kinds>: <atom>"),
            "error should require explicit `:` (got {message:?})"
        );
    }

    // -------------------------------------------------------------------------
    // Migrations bucket cycle — `tenant_migration` block parser slice
    // -------------------------------------------------------------------------

    #[test]
    fn tenant_migration_full_block_parses() {
        let source = r#"
feature customer
  tenant_migration backfill_lifecycle_stage
    target query.by_id
    axis org_id
    idempotency envelope.tenant_id
    timeout 10m
    retry 3 backoff exponential
    handler "./migrations/backfill_lifecycle_stage.go"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let tm = &features[0].tenant_migrations[0];
        assert_eq!(tm.name, "backfill_lifecycle_stage");
        assert_eq!(tm.target_ref.as_deref(), Some("query.by_id"));
        assert_eq!(tm.target_axis, "org_id");
        assert_eq!(tm.idempotency_by.as_deref(), Some("envelope.tenant_id"));
        assert_eq!(tm.timeout.as_deref(), Some("10m"));
        assert_eq!(tm.retry.as_ref().expect("retry").count, 3);
        assert_eq!(tm.handler, "./migrations/backfill_lifecycle_stage.go");
    }

    #[test]
    fn tenant_migration_legacy_target_and_idempotency_parses() {
        let source = r#"
feature customer
  tenant_migration backfill
    target tenants org
    idempotency by tenant.org_id
    handler "./migrations/backfill.go"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let tm = &features[0].tenant_migrations[0];
        assert!(tm.target_ref.is_none());
        assert_eq!(tm.target_axis, "org");
        assert_eq!(tm.idempotency_by.as_deref(), Some("tenant.org_id"));
    }

    #[test]
    fn tenant_migration_missing_axis_errors() {
        let source = r#"
feature customer
  tenant_migration backfill
    target query.by_id
    idempotency envelope.tenant_id
    handler "./migrations/backfill.go"
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("axis <name>"), "got {message}");
    }

    #[test]
    fn tenant_migration_unknown_child_errors() {
        let source = r#"
feature customer
  tenant_migration backfill
    target query.by_id
    axis org
    emits changed
    handler "./migrations/backfill.go"
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("tenant_migration children"),
            "got {message}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4c — `resource` block parser slice
    // -------------------------------------------------------------------------

    #[test]
    fn resource_full_block_parses() {
        let source = r#"
feature customer
  domain
    resource Customer
      previously migrated Account
      owner: User optional
      name: Text required
      email: @semantic.Email @pii.contact required
      lifecycle_stage: CustomerStatus = lead
        previously migrated status
      score: Integer @pii.derived = 0
      external_id: @cap.Encrypted(key:@key.tenant) @pii.external optional
      is_high_value: Boolean derived from score > 80
      has_many notes: CustomerNote inverse customer

      soft_delete
      retention 7y then anonymize

      validates @validator.tier_check
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let resources = &features[0].resources;
        assert_eq!(resources.len(), 1);
        let r = &resources[0];
        assert_eq!(r.name, "Customer");
        assert_eq!(r.previously, vec!["migrated Account"]);
        assert!(r.soft_delete);
        let ret = r.retention.as_ref().expect("retention");
        assert_eq!(ret.duration, "7y");
        assert!(matches!(
            ret.action,
            crate::ResourceRetentionAction::Anonymize
        ));
        assert_eq!(r.validates, vec!["@validator.tier_check"]);
        assert_eq!(r.has_many.len(), 1);
        assert_eq!(r.has_many[0].name, "notes");
        assert_eq!(r.has_many[0].type_text, "CustomerNote");
        assert_eq!(r.has_many[0].inverse.as_deref(), Some("customer"));
        // 7 fields (owner, name, email, lifecycle_stage, score, external_id,
        // is_high_value).
        assert_eq!(r.fields.len(), 7);
        let lifecycle = r
            .fields
            .iter()
            .find(|f| f.name == "lifecycle_stage")
            .expect("lifecycle_stage");
        assert_eq!(lifecycle.type_text, "CustomerStatus");
        assert_eq!(lifecycle.default.as_deref(), Some("lead"));
        assert_eq!(lifecycle.previously, vec!["migrated status"]);
        let derived = r
            .fields
            .iter()
            .find(|f| f.name == "is_high_value")
            .expect("is_high_value");
        assert_eq!(derived.derived_from.as_deref(), Some("score > 80"));
        let external = r
            .fields
            .iter()
            .find(|f| f.name == "external_id")
            .expect("external_id");
        assert!(external.optional);
        assert!(
            external
                .type_text
                .starts_with("@cap.Encrypted(key:@key.tenant)")
        );
    }

    #[test]
    fn resource_retention_invalid_action_errors() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
      retention 7y then incinerate
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("anonymize"),
            "error should list valid actions: {message}"
        );
    }

    #[test]
    fn parses_minimal_lifecycle_block() {
        let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let lifecycle = features[0].resources[0]
            .lifecycle
            .as_ref()
            .expect("lifecycle");

        assert_eq!(lifecycle.discriminator_field, "status");
        assert_eq!(lifecycle.states.len(), 2);
        assert_eq!(lifecycle.states[0].name, "scheduled");
        assert_eq!(lifecycle.states[1].name, "published");
        assert_eq!(lifecycle.transitions.len(), 1);
        assert_eq!(lifecycle.transitions[0].name, "publish");
        assert_eq!(lifecycle.transitions[0].from, vec!["scheduled"]);
        assert_eq!(lifecycle.transitions[0].to, "published");
    }

    // -----------------------------------------------------------------
    // CL.C.4 — `aggregate` + `invariant` + `@slug` parser tests.
    //
    // Coverage targets per spec: 4 aggregate, 3 invariant, 2 slug.
    // -----------------------------------------------------------------

    #[test]
    fn parses_aggregate_minimal_root_only() {
        let source = "
feature billing
  aggregate Order
    root Order
";
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features[0].aggregates.len(), 1);
        assert_eq!(features[0].aggregates[0].name, "Order");
        assert_eq!(features[0].aggregates[0].root, "Order");
        assert!(features[0].aggregates[0].contains.is_empty());
        assert!(features[0].aggregates[0].invariants.is_empty());
    }

    #[test]
    fn parses_aggregate_with_contains_list() {
        let source = "
feature billing
  aggregate Order
    root Order
    contains OrderLine, Payment
";
        let features = parse_feature_skeletons(source).unwrap();
        let agg = &features[0].aggregates[0];
        assert_eq!(agg.contains, vec!["OrderLine", "Payment"]);
    }

    #[test]
    fn parses_aggregate_with_invariants_block() {
        let source = "
feature billing
  aggregate Order
    root Order
    contains OrderLine
    invariants
      invariant total_consistent
        when total = total
        message \"line totals must match order total\"
";
        let features = parse_feature_skeletons(source).unwrap();
        let agg = &features[0].aggregates[0];
        assert_eq!(agg.invariants.len(), 1);
        assert_eq!(agg.invariants[0].name, "total_consistent");
        assert_eq!(agg.invariants[0].when, "total = total");
        assert_eq!(
            agg.invariants[0].message,
            "line totals must match order total"
        );
    }

    #[test]
    fn aggregate_rejects_missing_root() {
        let source = "
feature billing
  aggregate Order
    contains OrderLine
";
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("requires a `root <Resource>` declaration"),
            "got: {message}"
        );
    }

    #[test]
    fn parses_resource_level_invariant() {
        let source = "
feature billing
  resource Order
    total: Integer required
    invariant total_non_negative
      when total >= 0
      message \"order total cannot be negative\"
";
        let features = parse_feature_skeletons(source).unwrap();
        let r = &features[0].resources[0];
        assert_eq!(r.invariants.len(), 1);
        assert_eq!(r.invariants[0].name, "total_non_negative");
        assert_eq!(r.invariants[0].when, "total >= 0");
    }

    #[test]
    fn invariant_rejects_missing_when() {
        let source = "
feature billing
  resource Order
    total: Integer required
    invariant bad
      message \"oops\"
";
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("requires a `when <predicate>` clause"),
            "got: {message}"
        );
    }

    #[test]
    fn invariant_rejects_unknown_child() {
        let source = "
feature billing
  resource Order
    invariant bad
      when total = 0
      bogus thing
";
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("`invariant` children are"),
            "got: {message}"
        );
    }

    #[test]
    fn parses_slug_field_decorator() {
        let source = "
feature blog
  resource Post
    slug: Text @slug required
    title: Text required
";
        let features = parse_feature_skeletons(source).unwrap();
        let r = &features[0].resources[0];
        assert_eq!(r.fields.len(), 2);
        // First field is the slug field; `@slug` peeled, type clean.
        assert_eq!(r.fields[0].name, "slug");
        assert!(r.fields[0].slug, "`@slug` should peel into Field.slug");
        assert!(r.fields[0].required);
        assert!(
            !r.fields[0].type_text.contains("@slug"),
            "@slug should be stripped from type_text; got: {}",
            r.fields[0].type_text
        );
        // Second field has no `@slug`.
        assert!(!r.fields[1].slug);
    }

    #[test]
    fn slug_decorator_coexists_with_unique_modifier() {
        let source = "
feature blog
  resource Post
    slug: Text @slug required unique
";
        let features = parse_feature_skeletons(source).unwrap();
        let f = &features[0].resources[0].fields[0];
        assert!(f.slug);
        assert!(f.unique);
        assert!(f.required);
    }

    // -------------------------------------------------------------------
    // `ir-resource-conventions-owner-scope` Cell O1 — `@owner_axis(through: <ident>)`
    // -------------------------------------------------------------------

    #[test]
    fn parses_owner_axis_decorator_with_through_ident() {
        let source = "
feature catalog
  resource Property
    org: Org required
    host: Host required @owner_axis(through: user)
    name: Text required
";
        let features = parse_feature_skeletons(source).unwrap();
        let property = &features[0].resources[0];
        let host_field = &property.fields[1];
        assert_eq!(host_field.name, "host");
        let axis = host_field
            .owner_axis
            .as_ref()
            .expect("`@owner_axis(through: user)` should peel into ResourceFieldDecl.owner_axis");
        assert_eq!(axis.through_column, "user");
        assert!(
            !host_field.type_text.contains("@owner_axis"),
            "@owner_axis should be stripped from type_text; got: {}",
            host_field.type_text,
        );
        // The neighbouring fields stay axis-free.
        assert!(property.fields[0].owner_axis.is_none());
        assert!(property.fields[2].owner_axis.is_none());
    }

    #[test]
    fn owner_axis_rejects_string_literal_argument() {
        let source = "
feature catalog
  resource Property
    host: Host required @owner_axis(through: \"user\")
";
        let err = parse_feature_skeletons(source).expect_err(
            "string literal in @owner_axis(through: ...) must be a parse error per §7.1",
        );
        let message = format!("{err}");
        assert!(
            message.contains("requires a bare identifier"),
            "got: {message}",
        );
    }

    #[test]
    fn owner_axis_without_arguments_is_a_parse_error() {
        let source = "
feature catalog
  resource Property
    host: Host required @owner_axis
";
        let err = parse_feature_skeletons(source)
            .expect_err("bare @owner_axis must be rejected — annotation requires (through: ...)");
        let message = format!("{err}");
        assert!(
            message.contains("`@owner_axis` requires `(through: <ident>)`"),
            "got: {message}",
        );
    }

    #[test]
    fn parses_lifecycle_with_terminal_states_and_invariants() {
        let source = r#"
feature publication
  domain
    resource Publication
      workspace: Workspace required
      scheduled_at: DateTime required
      publishing_at: DateTime
      published_at: DateTime
      failed_at: DateTime
      cancelled_at: DateTime
      error_reason: Text

      lifecycle status
        state scheduled initial
        state publishing
        state published terminal
        state failed terminal
        state cancelled terminal

        transition begin_publishing
          from scheduled
          to publishing
          policy @policy.publisher_or_admin
          audit default
          timestamps publishing_at

        transition mark_published
          from publishing
          to published
          audit default
          timestamps published_at
          emits publication_published

        transition mark_failed
          from publishing
          to failed
          audit error_reason
          timestamps failed_at
          emits publication_failed payload error_reason

        transition cancel
          from scheduled, publishing
          to cancelled
          audit default
          timestamps cancelled_at
          emits publication_cancelled

        invariant terminal_immutable
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let lifecycle = features[0].resources[0]
            .lifecycle
            .as_ref()
            .expect("lifecycle");

        assert_eq!(lifecycle.states[0].kind_keyword.as_deref(), Some("initial"));
        assert_eq!(
            lifecycle.states[2].kind_keyword.as_deref(),
            Some("terminal")
        );
        assert_eq!(lifecycle.invariants.len(), 1);
        assert_eq!(lifecycle.invariants[0].raw, "terminal_immutable");
    }

    #[test]
    fn lifecycle_rejects_fewer_than_two_states() {
        let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        transition publish
          from scheduled
          to published
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");

        assert!(
            message.contains("at least 2"),
            "error should require at least 2 states: {message}"
        );
    }

    #[test]
    fn lifecycle_rejects_unknown_state_modifier() {
        let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled foo
        state published
        transition publish
          from scheduled
          to published
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");

        assert!(
            message.contains("initial") && message.contains("terminal"),
            "error should list valid state modifiers: {message}"
        );
    }

    #[test]
    fn lifecycle_double_block_rejects() {
        let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
      lifecycle other_status
        state draft
        state archived
        transition archive
          from draft
          to archived
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");

        assert!(
            message.contains("at most one"),
            "error should reject duplicate lifecycle blocks: {message}"
        );
    }

    #[test]
    fn transition_multi_from_parsed() {
        let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        state publishing
        state cancelled
        transition cancel
          from scheduled, publishing
          to cancelled
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let lifecycle = features[0].resources[0]
            .lifecycle
            .as_ref()
            .expect("lifecycle");

        assert_eq!(
            lifecycle.transitions[0].from,
            vec!["scheduled", "publishing"]
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4d — `query` and `record` block parser slice
    // -------------------------------------------------------------------------

    #[test]
    fn query_list_full_block_parses() {
        let source = r#"
feature customer
  domain
    query.list list
      modifier @query_modifier.query_scope_modifier

      params
        lifecycle_stage: CustomerStatus optional
        search: Text optional

      filters
        lifecycle_stage when params.lifecycle_stage

      search params.search over name, email
        mode contains

      cache
        key customer.list(params)
        ttl "5 minutes"

      paginate 50
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features[0].queries.len(), 1);
        match &features[0].queries[0] {
            crate::QueryDecl::List(q) => {
                assert_eq!(q.name, "list");
                assert_eq!(
                    q.modifier.as_deref(),
                    Some("@query_modifier.query_scope_modifier")
                );
                assert_eq!(q.params.len(), 2);
                assert_eq!(q.filters.len(), 1);
                let search = q.search.as_ref().expect("search");
                assert_eq!(search.fields, vec!["name", "email"]);
                assert_eq!(search.mode.as_deref(), Some("contains"));
                assert_eq!(q.paginate, Some(50));
            }
            other => panic!("expected query.list, got {other:?}"),
        }
    }

    #[test]
    fn query_lookup_inline_parses() {
        let source = r#"
feature customer
  domain
    query.lookup by_id by id: ID
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::Lookup(l) => {
                assert_eq!(l.name, "by_id");
                assert_eq!(l.keys.len(), 1);
                assert_eq!(l.keys[0].name, "id");
                assert_eq!(l.keys[0].type_text, "ID");
            }
            other => panic!("expected query.lookup, got {other:?}"),
        }
    }

    #[test]
    fn query_sql_parses() {
        let source = r#"
feature customer
  domain
    query.sql lifetime_value
      returns CustomerLtv[]
      scope
        org = ctx.user.org
      sql "./queries/customer_lifetime_value.sql"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::Sql(s) => {
                assert_eq!(s.kind, crate::SqlQueryKind::Sql);
                assert_eq!(s.name, "lifetime_value");
                assert_eq!(s.returns, "CustomerLtv[]");
                assert_eq!(s.sql_path, "./queries/customer_lifetime_value.sql");
                assert_eq!(s.scope_lines.len(), 1);
            }
            other => panic!("expected query.sql, got {other:?}"),
        }
    }

    #[test]
    fn query_view_parses_file_source_and_list_returns() {
        let source = r#"
feature host
  domain
    query.view host_home_view
      policy @policy.host_only
      returns list of HostHomeRow
      source @file.host_home_view.sql
      params
        user_id: ID required
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::Sql(s) => {
                assert_eq!(s.kind, crate::SqlQueryKind::View);
                assert_eq!(s.name, "host_home_view");
                assert_eq!(s.policy.as_deref(), Some("@policy.host_only"));
                assert_eq!(s.returns, "list of HostHomeRow");
                assert_eq!(s.sql_path, "@file.host_home_view.sql");
                assert_eq!(s.params.len(), 1);
                assert_eq!(s.params[0].name, "user_id");
            }
            other => panic!("expected query.view, got {other:?}"),
        }
    }

    #[test]
    fn query_view_parses_scalar_returns_and_scope() {
        let source = r#"
feature host
  domain
    query.view property_detail_view
      returns PropertyDetailRow
      source @file.property_detail_view.sql
      scope
        org = ctx.actor.org_id
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::Sql(s) => {
                assert_eq!(s.kind, crate::SqlQueryKind::View);
                assert_eq!(s.name, "property_detail_view");
                assert_eq!(s.returns, "PropertyDetailRow");
                assert_eq!(s.sql_path, "@file.property_detail_view.sql");
                assert_eq!(s.scope_lines, vec!["org = ctx.actor.org_id"]);
                assert!(s.params.is_empty());
            }
            other => panic!("expected query.view, got {other:?}"),
        }
    }

    #[test]
    fn record_block_parses() {
        let source = r#"
feature customer
  domain
    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money
      currency: Text
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features[0].records.len(), 1);
        let r = &features[0].records[0];
        assert_eq!(r.name, "CustomerLtv");
        assert_eq!(r.fields.len(), 3);
        assert_eq!(r.fields[1].name, "amount");
        assert_eq!(r.fields[1].type_text, "@semantic.Money");
    }

    // -------------------------------------------------------------------------
    // IR Error-Vocab (Cell PARSE-1) — parser slice tests for
    //   * `when_denied @translation.<key>` under a command's `policy`
    //   * `when_denied @translation.<key>` under a `policies` category
    //   * `errors` block with `default`, `expose client <4xx|5xx>`, and
    //     `<code> message @translation.<key>` lines.
    //
    // Each test exercises both the happy-path lift and at least one
    // structural rejection. Closed-catalog enforcement (allowed codes,
    // allowed exposure fields) lives analyzer/doctor side — the parser
    // keeps verbatim tokens so doctor can quote the offending text.
    // -------------------------------------------------------------------------

    #[test]
    fn command_policy_when_denied_lifts_translation_key_ref() {
        let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied @translation.choose_role_signin_required
    input
      role_id: ID required
    returns User
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let command = features
            .iter()
            .find(|f| f.name == "account")
            .and_then(|f| f.commands.iter().find(|c| c.name == "choose_role"))
            .expect("choose_role command");
        let key = command
            .policy_when_denied
            .as_ref()
            .expect("policy_when_denied lifted");
        assert_eq!(key.key, "choose_role_signin_required");
    }

    #[test]
    fn command_policy_when_denied_rejects_duplicate() {
        let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied @translation.first
      when_denied @translation.second
    input
      role_id: ID required
    returns User
"#;
        let err = parse_feature_skeletons(source).expect_err("duplicate when_denied must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("at most one `when_denied`"),
            "expected duplicate-detection error, got {msg}"
        );
    }

    #[test]
    fn command_policy_rejects_non_translation_when_denied() {
        let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied "plain string"
    input
      role_id: ID required
    returns User
"#;
        let err = parse_feature_skeletons(source).expect_err("non-@translation.<key> must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("@translation.") || msg.contains("expected `@translation.<key>`"),
            "expected `@translation.<key>` form error, got {msg}"
        );
    }

    #[test]
    fn policy_category_when_denied_lifts_translation_key_ref() {
        let source = r#"
feature account
  policies
    authenticated: @scope.authenticated
      when_denied @translation.must_be_signed_in
    admin_only: @role.admin
      when_denied @translation.admin_only_action
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let policies = features
            .iter()
            .find(|f| f.name == "account")
            .and_then(|f| f.policies.as_ref())
            .expect("policies block lifted");
        assert_eq!(policies.categories.len(), 2);
        let authenticated = policies
            .categories
            .iter()
            .find(|c| c.name == "authenticated")
            .expect("authenticated category");
        assert_eq!(
            authenticated.when_denied.as_ref().map(|k| k.key.as_str()),
            Some("must_be_signed_in")
        );
        let admin = policies
            .categories
            .iter()
            .find(|c| c.name == "admin_only")
            .expect("admin_only category");
        assert_eq!(
            admin.when_denied.as_ref().map(|k| k.key.as_str()),
            Some("admin_only_action")
        );
    }

    #[test]
    fn policy_category_when_denied_route_lifts_route_targets() {
        let source = r#"
feature account
  policies
    host_only: @scope.authenticated, @role.host
      when_denied @translation.host_only_required
      when_denied_route
        unauthenticated -> view sign_in
        role_mismatch traveler -> view explore
        role_mismatch operator -> view dashboard
        default -> path "/welcome"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let policies = features[0].policies.as_ref().expect("policies block");
        let category = policies
            .categories
            .iter()
            .find(|category| category.name == "host_only")
            .expect("host_only category");
        let route = category
            .when_denied_route
            .as_ref()
            .expect("when_denied_route lifted");

        assert_eq!(
            route.unauthenticated,
            Some(crate::ast::RouteRedirectTargetAst::View(
                "sign_in".to_string()
            ))
        );
        assert_eq!(route.role_mismatch.len(), 2);
        assert_eq!(route.role_mismatch[0].role, "traveler");
        assert_eq!(
            route.role_mismatch[0].target,
            crate::ast::RouteRedirectTargetAst::View("explore".to_string())
        );
        assert_eq!(
            route.default,
            Some(crate::ast::RouteRedirectTargetAst::Path(
                "/welcome".to_string()
            ))
        );
    }

    #[test]
    fn policy_category_when_denied_route_rejects_duplicate_default() {
        let source = r#"
feature account
  policies
    host_only: @scope.authenticated
      when_denied_route
        default -> view welcome
        default -> path "/"
"#;
        let err = parse_feature_skeletons(source).expect_err("duplicate default must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("declares `default` at most once"),
            "expected duplicate default error, got {msg}"
        );
    }

    #[test]
    fn feature_errors_block_lifts_default_exposure_and_messages() {
        let source = r#"
feature account
  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

    policy_denied message @translation.account_signin_required
    validation_failed message @translation.account_invalid_input
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let errors = features
            .iter()
            .find(|f| f.name == "account")
            .and_then(|f| f.errors.as_ref())
            .expect("errors block lifted");
        assert_eq!(
            errors.default,
            Some(crate::ast::ErrorExposureDefaultAst::Hide)
        );
        assert_eq!(errors.exposure_4xx, vec!["message", "code"]);
        assert_eq!(errors.exposure_5xx, vec!["code"]);
        assert_eq!(errors.messages.len(), 2);
        assert_eq!(errors.messages[0].code, "policy_denied");
        assert_eq!(errors.messages[0].message.key, "account_signin_required");
        assert_eq!(errors.messages[1].code, "validation_failed");
        assert_eq!(errors.messages[1].message.key, "account_invalid_input");
    }

    #[test]
    fn feature_errors_block_rejects_duplicate_block() {
        let source = r#"
feature account
  errors
    default hide
  errors
    default expose
"#;
        let err = parse_feature_skeletons(source).expect_err("duplicate errors block must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("at most one `errors` block"),
            "expected duplicate-block error, got {msg}"
        );
    }

    #[test]
    fn feature_errors_block_rejects_invalid_default() {
        let source = r#"
feature account
  errors
    default sometimes
"#;
        let err = parse_feature_skeletons(source).expect_err("invalid default must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("`default hide` or `default expose`"),
            "expected canonical default error, got {msg}"
        );
    }

    #[test]
    fn feature_errors_block_rejects_unknown_child() {
        let source = r#"
feature account
  errors
    splat ok
"#;
        let err = parse_feature_skeletons(source).expect_err("unknown errors child must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("`errors` children are"),
            "expected children-enumeration error, got {msg}"
        );
    }

    #[test]
    fn feature_errors_round_trip_via_full_capsule_fixture() {
        // Smoke check that the canonical fixture (extended in Cell
        // PARSE-1) still parses end-to-end and the new IR slots are
        // populated for the `customer` feature.
        let source = include_str!("../../../../../examples/full-capsule/full-capsule.lzi");
        let features = parse_feature_skeletons(source).expect("parses");
        let customer = features
            .iter()
            .find(|f| f.name == "customer")
            .expect("customer feature");

        // Per-policy when_denied: `update` category gained one in the
        // PARSE-1 fixture extension.
        let policies = customer.policies.as_ref().expect("policies block present");
        let update = policies
            .categories
            .iter()
            .find(|c| c.name == "update")
            .expect("update category");
        assert_eq!(
            update.when_denied.as_ref().map(|k| k.key.as_str()),
            Some("customer_update_admin_only")
        );

        // Per-command when_denied: `capture_lead` gained one.
        let capture_lead = customer
            .commands
            .iter()
            .find(|c| c.name == "capture_lead")
            .expect("capture_lead command");
        assert_eq!(
            capture_lead
                .policy_when_denied
                .as_ref()
                .map(|k| k.key.as_str()),
            Some("capture_lead_signin_required")
        );

        // Feature-level errors block: two `<code> message
        // @translation.<key>` rows + the pre-existing exposure rules.
        let errors = customer.errors.as_ref().expect("errors block present");
        assert_eq!(
            errors.default,
            Some(crate::ast::ErrorExposureDefaultAst::Hide)
        );
        assert!(errors.exposure_4xx.contains(&"message".to_owned()));
        assert!(errors.exposure_4xx.contains(&"code".to_owned()));
        assert!(errors.exposure_5xx.contains(&"code".to_owned()));
        assert_eq!(errors.messages.len(), 2);
        let codes: Vec<&str> = errors.messages.iter().map(|m| m.code.as_str()).collect();
        assert!(codes.contains(&"policy_denied"));
        assert!(codes.contains(&"validation_failed"));
    }
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
