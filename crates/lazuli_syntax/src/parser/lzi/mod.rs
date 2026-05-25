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

use super::common::{
    SourceLine, find_token, find_top_level_token, is_kebab_or_snake_ident, is_lzx_bare_ident,
    is_lzx_resume_ref, is_trivia, line_error, line_error_owned, parse_lzx_bool, source_lines,
    split_lzx_arrow, split_lzx_list, strip_inline_comment, unquote_lzx_value,
};
use super::error::ParseError;
use super::lzx::try_parse_policy_expr;

use crate::ast::OwnerAxisAst;
use crate::ast::{
    Agent, AgentEvalAssertion, AgentEvalCase, AgentEvalGolden, AgentEvalKind, AgentEvalPredicate,
    AgentExpose, AgentExposeRouteSlot, AgentInputSlot, AgentOutput, AgentTool, AggregateDecl,
    ApiDecl, ApprovalThenDecl, AssignmentDecl, Auth, AuthDurationClause, AuthIdentity, AuthMfa,
    AuthOAuthProvider, AuthPassword, AuthSessionRotation, AuthSessions, AuthTheftDetectionAction,
    AuthTheftDetectionActionClause, CacheProfileDecl, Channel, ColorStateAst, ColorTokenAst,
    CommandApproval, CommandAudit, CommandDecl, CommandDeprecatedDecl, CommandEffectDecl,
    CommandEffectKindDecl, CommandEmit, CommandInputDecl, CommandInputSlot, CommandRouteSlot,
    CommandRouteSlotKind, CommandWriteWindow, ContainsRhs, CustomTokenAst, DefaultsPolicyFor,
    DefaultsTenancy, DesignDeclAst, EasingTokenAst, EnumDeclAst, EnumStorageValueDecl,
    EnumVariantDecl, ErrorExposureDefaultAst, EventGroup, EventVariantFieldDecl,
    EventVariantKindAst, FamilyTokenAst, FeatureDefaults, FeatureErrorExposeRuleDecl,
    FeatureErrorMessageDecl, FeatureErrorsDecl, FeatureGatesAst, FeatureSkeleton,
    FieldConstraintsDecl, FieldPoliciesDecl, FieldPolicyDecl, GateDirectiveAst, HttpMethod,
    InvalidatesDecl, InvariantDecl, Job, JobBody, JobDeclarativeTyped, JobExternalCall,
    JobExternalCallArg, JobFanout, JobHandler, JobRetry, JobTrigger, LetBindingDecl, ListQueryDecl,
    LocaleNegotiateDecl, LookupKey, LookupQueryDecl, MotionAst, Notification, NotificationDigest,
    NotificationThrottle, PackageSkeleton, PermissionDeclAst, PlanBlockAst, PlanFeatureRefAst,
    PlanLimitRefAst, PlanTrialAst, PoliciesDecl, PolicyCategoryDecl, PolicyExprAst,
    PublicContractDeclAst, QueryDecl, QuerySearch, RateLimitByEnvAst, RateLimitSpecAst, RecordDecl,
    ReportColumnAst, ReportColumnSourceAst, ReportDecl, ResourceCompositeKey,
    ResourceConstraintAst, ResourceConventionAst, ResourceDecl, ResourceFieldDecl, ResourceHasMany,
    ResourceIndexAst, ResourceIndexMethodAst, ResourceLock, ResourceRetention,
    ResourceRetentionAction, ResourceUniqueAst, RoleDeclAst, RoleGrantsAst, RoleMismatchArmAst,
    RouteRedirectTargetAst, ScaleTokenAst, ShadowTokenAst, Span, SqlQueryDecl, SqlQueryKind,
    TargetArgDecl, TargetExprDecl, TenantMigration, TextScaleTokenAst, ToolsCallsOp,
    TrackingTokenAst, TranslationDecl, TranslationKeyDecl, TranslationKeyRefAst,
    TranslationPluralArmDecl, TranslationVariantDecl, TypographyAst, UsesClauseAst, Webhook,
    WebhookDlq, WebhookHandler, WebhookReplay, WebhookVerify, WeightTokenAst, WhenDeniedRouteAst,
    ZTokenAst,
};

pub mod design;
pub mod event;
pub mod mcp;
pub mod notification;
pub mod package;
pub mod plan;
pub mod translation;
pub mod types;

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
            let (parsed, next) = parse_poller_block(lines, i)?;
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
            let (parsed, next) = parse_cache_profile(lines, i)?;
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
            let (mut parsed, next) = parse_command_decl(lines, i)?;
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
            let (parsed, next) = parse_report_decl(lines, i)?;
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
            let (mut parsed, next) = parse_query_decl(lines, i)?;
            attach_public_contract_to_query(line, &mut pending_contract, &mut parsed)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            queries.push(parsed);
            i = next;
            continue;
        }

        // Phase L Tier 4d — `record <Name>` block.
        if trimmed.starts_with("record ") {
            let (mut parsed, next) = parse_record_decl(lines, i)?;
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
            let (parsed, next) = parse_policies_decl(lines, i)?;
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

/// Try to parse a feature-body line as `public contract <Symbol> as v<N>`.
/// Returns `Some((symbol, decl))` when the line matches; `None` otherwise.
/// Returns `Err` for malformed `public contract` lines.
fn parse_public_contract_line(
    line: &SourceLine<'_>,
) -> Result<Option<(String, PublicContractDeclAst)>, ParseError> {
    let trimmed = line.text.trim_start();
    let Some(rest) = trimmed.strip_prefix("public contract ") else {
        return Ok(None);
    };

    let mut parts = rest.split_whitespace();
    let symbol = parts
        .next()
        .ok_or_else(|| line_error(line, "`public contract` requires a symbol name"))?
        .to_owned();
    if !is_public_contract_symbol(&symbol) {
        return Err(line_error(line, "`public contract` requires a symbol name"));
    }

    let as_kw = parts
        .next()
        .ok_or_else(|| line_error(line, "`public contract <X>` requires `as v<N>` suffix"))?;
    if as_kw != "as" {
        return Err(line_error(
            line,
            "`public contract <X>` requires `as v<N>` suffix",
        ));
    }

    let version_token = parts
        .next()
        .ok_or_else(|| line_error(line, "`public contract <X> as` requires a version `v<N>`"))?;
    let Some(version_digits) = version_token.strip_prefix('v') else {
        return Err(line_error(line, "version must start with `v`, e.g. `v1`"));
    };
    let version: u16 = version_digits
        .parse()
        .map_err(|_| line_error(line, "version must be a positive integer (u16)"))?;
    if version == 0 {
        return Err(line_error(line, "version must be a positive integer (u16)"));
    }
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`public contract <X> as v<N>` admits no trailing tokens",
        ));
    }

    Ok(Some((
        symbol,
        PublicContractDeclAst {
            version,
            span: Span::new(line.start, line.end),
        },
    )))
}

/// Parse the body of a `uses` line — comma-separated feature names with
/// an optional trailing `version v<N>` clause that applies to ALL entries
/// on the line. Returns one `UsesClauseAst` per imported feature.
///
/// Examples:
/// - `account` → `[{feature: "account", version: None}]`
/// - `org, user, billing` → 3 clauses, all `version: None`
/// - `account version v1` → `[{feature: "account", version: Some(1)}]`
/// - `account, billing version v2` → 2 clauses, BOTH at v2 (line-level pin)
fn parse_uses_line(
    rest: &str,
    line: &SourceLine<'_>,
    line_span: Span,
) -> Result<Vec<UsesClauseAst>, ParseError> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return Err(line_error(
            line,
            "`uses` requires at least one feature name",
        ));
    }

    // Split into the feature-list portion and the optional `version v<N>`
    // suffix. Single-pass: find " version v" (whitespace-bounded keyword)
    // OR fall through with no pin.
    let (list_part, version) = match trimmed.find(" version ") {
        Some(idx) => {
            let list_part = &trimmed[..idx];
            let version_part = trimmed[idx + " version ".len()..].trim();
            let Some(digits) = version_part.strip_prefix('v') else {
                return Err(line_error(
                    line,
                    "`uses ... version v<N>` requires `v` prefix on version",
                ));
            };
            let version: u16 = digits
                .parse()
                .map_err(|_| line_error(line, "`uses ... version v<N>` requires a positive u16"))?;
            if version == 0 {
                return Err(line_error(
                    line,
                    "`uses ... version v<N>` requires a positive u16",
                ));
            }
            (list_part, Some(version))
        }
        None => (trimmed, None),
    };

    let mut clauses = Vec::new();
    for name in list_part.split(',') {
        let name = name.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`uses` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        // Feature names follow IDENT_LOWER convention; let the analyzer
        // enforce the lexical rule (it has the canonical regex). Here we
        // just confirm non-empty + no obvious whitespace inside.
        if name.chars().any(char::is_whitespace) {
            return Err(line_error(
                line,
                "feature names in `uses` list cannot contain whitespace; separate with commas",
            ));
        }
        clauses.push(UsesClauseAst {
            feature: name.to_owned(),
            version,
            span: line_span,
        });
    }
    Ok(clauses)
}

/// Parse the special-form `public contract identity as v<N>` line per
/// `docs/proposals/cross-feature-contracts.md` §5.3 row 7. "identity" is a
/// literal keyword here (not a symbol name) — the singleton identity
/// surface of an `auth` block has no per-name binding. Returns
/// `Some(decl)` when the line matches; `None` when the line is something
/// else; `Err` when the line begins with `public contract identity` but is
/// malformed (e.g. missing `as v<N>`).
fn parse_auth_identity_contract_line(
    line: &SourceLine<'_>,
) -> Result<Option<PublicContractDeclAst>, ParseError> {
    let trimmed = line.text.trim_start();
    let Some(rest) = trimmed.strip_prefix("public contract identity ") else {
        // Not the identity-contract form. Bare `public contract identity`
        // with no trailing tokens is also rejected (caught by the
        // general parse_public_contract_line which would error on the
        // missing `as` suffix). Return None to let the regular flow run.
        return Ok(None);
    };

    let mut parts = rest.split_whitespace();
    let as_kw = parts
        .next()
        .ok_or_else(|| line_error(line, "`public contract identity` requires `as v<N>` suffix"))?;
    if as_kw != "as" {
        return Err(line_error(
            line,
            "`public contract identity` requires `as v<N>` suffix",
        ));
    }
    let version_token = parts.next().ok_or_else(|| {
        line_error(
            line,
            "`public contract identity as` requires a version `v<N>`",
        )
    })?;
    let Some(version_digits) = version_token.strip_prefix('v') else {
        return Err(line_error(line, "version must start with `v`, e.g. `v1`"));
    };
    let version: u16 = version_digits
        .parse()
        .map_err(|_| line_error(line, "version must be a positive integer (u16)"))?;
    if version == 0 {
        return Err(line_error(line, "version must be a positive integer (u16)"));
    }
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`public contract identity as v<N>` admits no trailing tokens",
        ));
    }
    Ok(Some(PublicContractDeclAst {
        version,
        span: Span::new(line.start, line.end),
    }))
}

fn is_public_contract_symbol(symbol: &str) -> bool {
    !symbol.is_empty()
        && symbol.split('.').all(|part| {
            let mut chars = part.chars();
            matches!(chars.next(), Some(first) if first.is_ascii_alphabetic())
                && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

fn take_matching_public_contract(
    line: &SourceLine<'_>,
    pending_contract: &mut Option<(String, PublicContractDeclAst)>,
    kind: &str,
    name: &str,
) -> Result<Option<PublicContractDeclAst>, ParseError> {
    let Some((symbol, contract)) = pending_contract.take() else {
        return Ok(None);
    };
    if symbol == name || symbol == format!("{kind}.{name}") {
        return Ok(Some(contract));
    }
    Err(line_error_owned(
        line,
        format!(
            "public contract `{symbol}` precedes a `{kind} {name}` declaration; the name must match the next symbol's name."
        ),
    ))
}

fn attach_public_contract_to_query(
    line: &SourceLine<'_>,
    pending_contract: &mut Option<(String, PublicContractDeclAst)>,
    query: &mut QueryDecl,
) -> Result<(), ParseError> {
    match query {
        QueryDecl::List(q) => {
            q.public_contract =
                take_matching_public_contract(line, pending_contract, "query.list", &q.name)?;
        }
        QueryDecl::Lookup(q) => {
            q.public_contract =
                take_matching_public_contract(line, pending_contract, "query.lookup", &q.name)?;
        }
        QueryDecl::Sql(q) => {
            let kind = match q.kind {
                SqlQueryKind::Sql => "query.sql",
                SqlQueryKind::View => "query.view",
            };
            q.public_contract =
                take_matching_public_contract(line, pending_contract, kind, &q.name)?;
        }
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Phase L Tier 4 follow-up — `policies` block parser.
//
// The `policies` header sits at AGENT_INDENT_FEATURE_CHILD (2 spaces). Two
// kinds of children appear at indent 4:
//
//   * `<name>: <atom>, <atom>, ...` — named category (`create: @role.admin`).
//   * `fields <Resource>` — field-override subblock with grandchild field
//     names at indent 6 and `read:` / `write:` at indent 8.
//
// Non-`@`-prefixed atoms are silently dropped (matches the retired
// `collect_policy_atoms` walker contract). Unknown indent levels produce
// a parse error so authors get an early diagnostic instead of policies
// vanishing silently.
// -----------------------------------------------------------------------------

fn parse_policies_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(PoliciesDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;
    let greatgrand_indent = header_indent + 6;

    let mut categories: Vec<PolicyCategoryDecl> = Vec::new();
    let mut field_blocks: Vec<FieldPoliciesDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`policies` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("fields ") {
            let resource = rest.trim().to_owned();
            if resource.is_empty() {
                return Err(line_error(
                    line,
                    "`fields` requires a resource name (`fields <Resource>`)",
                ));
            }
            let (block, next) = parse_field_policies_block(
                lines,
                i,
                resource,
                grandchild_indent,
                greatgrand_indent,
            )?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            field_blocks.push(block);
            i = next;
            continue;
        }

        if let Some((name, atoms_text)) = trimmed.split_once(':') {
            let name = name.trim();
            if !is_policy_identifier(name) {
                i += 1;
                continue;
            }
            let atoms = atoms_text
                .split(',')
                .map(str::trim)
                .filter(|atom| atom.starts_with('@'))
                .map(str::to_owned)
                .collect();
            let category_header_line = line;
            let category_header_end = line.end;
            let mut category_last_end = category_header_end;
            // IR Error-Vocab (Cell PARSE-1) — consume the optional
            // `when_denied @translation.<key>` child(ren) at
            // grandchild_indent (6 spaces under a feature). Zero-or-one
            // per category; duplicate is a parse error.
            let mut when_denied: Option<TranslationKeyRefAst> = None;
            let mut when_denied_route: Option<WhenDeniedRouteAst> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let inner = &lines[j];
                let inner_trim = inner.text.trim_start();
                if is_trivia(inner_trim) {
                    j += 1;
                    continue;
                }
                if inner.indent <= child_indent {
                    break;
                }
                if inner.indent != grandchild_indent {
                    return Err(line_error(
                        inner,
                        "policy category children use one indentation level deeper than the category line",
                    ));
                }
                if let Some(rest) = inner_trim.strip_prefix("when_denied ") {
                    if when_denied.is_some() {
                        return Err(line_error(
                            inner,
                            "policy category may declare at most one `when_denied` child (ERR-VOCAB-MULTIPLE-WHEN-DENIED)",
                        ));
                    }
                    when_denied = Some(parse_translation_key_token(inner, rest)?);
                    category_last_end = inner.end;
                    j += 1;
                    continue;
                }
                if inner_trim == "when_denied_route" {
                    if when_denied_route.is_some() {
                        return Err(line_error(
                            inner,
                            "policy category may declare at most one `when_denied_route` child",
                        ));
                    }
                    let (parsed, next) = parse_when_denied_route_block(
                        lines,
                        j,
                        grandchild_indent,
                        greatgrand_indent,
                    )?;
                    category_last_end = lines[next.saturating_sub(1).max(j)].end;
                    when_denied_route = Some(parsed);
                    j = next;
                    continue;
                }
                return Err(line_error(
                    inner,
                    "policy category children are `when_denied @translation.<key>` or `when_denied_route` only",
                ));
            }
            categories.push(PolicyCategoryDecl {
                name: name.to_owned(),
                atoms,
                when_denied,
                when_denied_route,
                span: Span::new(category_header_line.start, category_last_end),
            });
            last_end = category_last_end;
            i = j;
            continue;
        }

        return Err(line_error(
            line,
            "`policies` children are `<name>: <atom>, ...` or `fields <Resource>` headers",
        ));
    }

    Ok((
        PoliciesDecl {
            categories,
            fields: field_blocks,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_when_denied_route_block(
    lines: &[SourceLine<'_>],
    start: usize,
    header_indent: usize,
    arm_indent: usize,
) -> Result<(WhenDeniedRouteAst, usize), ParseError> {
    let header = &lines[start];
    if header.indent != header_indent || header.text.trim_start() != "when_denied_route" {
        return Err(line_error(
            header,
            "policy route denial blocks use `when_denied_route`",
        ));
    }

    let mut unauthenticated: Option<RouteRedirectTargetAst> = None;
    let mut role_mismatch: Vec<RoleMismatchArmAst> = Vec::new();
    let mut default: Option<RouteRedirectTargetAst> = None;
    let mut seen_roles = std::collections::BTreeSet::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != arm_indent {
            return Err(line_error(
                line,
                "`when_denied_route` arms use one indentation level deeper than the block",
            ));
        }
        let Some((left, right)) = split_lzx_arrow(trimmed) else {
            return Err(line_error(
                line,
                "`when_denied_route` arms use `<case> -> view <name>` or `<case> -> path \"...\"`",
            ));
        };
        let left = left.trim();
        let target = parse_route_redirect_target(line, right.trim())?;
        if left == "unauthenticated" {
            if unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares `unauthenticated` at most once",
                ));
            }
            unauthenticated = Some(target);
        } else if left == "default" {
            if default.is_some() {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares `default` at most once",
                ));
            }
            default = Some(target);
        } else if let Some(role) = left.strip_prefix("role_mismatch ") {
            let role = role.trim();
            if !is_lzx_bare_ident(role) {
                return Err(line_error(
                    line,
                    "`role_mismatch` requires a bare role identifier",
                ));
            }
            if !seen_roles.insert(role.to_owned()) {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares each `role_mismatch <role>` at most once",
                ));
            }
            role_mismatch.push(RoleMismatchArmAst {
                role: role.to_owned(),
                target,
                span: Span::new(line.start, line.end),
            });
        } else {
            return Err(line_error(
                line,
                "`when_denied_route` arms are `unauthenticated`, `role_mismatch <role>`, or `default`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    if unauthenticated.is_none() && role_mismatch.is_empty() && default.is_none() {
        return Err(line_error(
            header,
            "`when_denied_route` requires at least one arm",
        ));
    }

    Ok((
        WhenDeniedRouteAst {
            unauthenticated,
            role_mismatch,
            default,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_route_redirect_target(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<RouteRedirectTargetAst, ParseError> {
    if let Some(view) = value.strip_prefix("view ") {
        let view = view.trim();
        if !is_lzx_resume_ref(view) {
            return Err(line_error(
                line,
                "`view` redirect targets use `<view>` or `<feature>.<view>`",
            ));
        }
        return Ok(RouteRedirectTargetAst::View(view.to_owned()));
    }
    if let Some(path) = value.strip_prefix("path ") {
        let path = path.trim();
        if !(path.starts_with('"') && path.ends_with('"')) {
            return Err(line_error(
                line,
                "`path` redirect targets must be quoted string literals",
            ));
        }
        return Ok(RouteRedirectTargetAst::Path(
            unquote_lzx_value(path).to_owned(),
        ));
    }
    Err(line_error(
        line,
        "`when_denied_route` targets use `view <name>` or `path \"...\"`",
    ))
}

fn parse_field_policies_block(
    lines: &[SourceLine<'_>],
    start: usize,
    resource: String,
    field_indent: usize,
    clause_indent: usize,
) -> Result<(FieldPoliciesDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let mut fields: Vec<FieldPolicyDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != field_indent {
            return Err(line_error(
                line,
                "`fields` children use one indentation level deeper than the header",
            ));
        }

        // Bare field name at field_indent (`email`); read/write at
        // clause_indent below.
        let field_name = trimmed.to_owned();
        if field_name.is_empty() || !is_policy_identifier(&field_name) {
            return Err(line_error(
                line,
                "field policy entry must be a bare identifier",
            ));
        }
        let field_header_end = line.end;
        let mut read: Option<Vec<String>> = None;
        let mut write: Option<Vec<String>> = None;
        let mut last_field_end = field_header_end;
        let mut j = i + 1;
        while j < lines.len() {
            let inner = &lines[j];
            let inner_trim = inner.text.trim_start();
            if is_trivia(inner_trim) {
                j += 1;
                continue;
            }
            if inner.indent <= field_indent {
                break;
            }
            if inner.indent != clause_indent {
                return Err(line_error(
                    inner,
                    "field policy clauses use one indentation level deeper than the field name",
                ));
            }
            let parsed_atoms = |rest: &str| -> Vec<String> {
                rest.split(',')
                    .map(str::trim)
                    .filter(|atom| atom.starts_with('@'))
                    .map(str::to_owned)
                    .collect()
            };
            if let Some(rest) = inner_trim.strip_prefix("read:") {
                read = Some(parsed_atoms(rest));
                last_field_end = inner.end;
                j += 1;
                continue;
            }
            if let Some(rest) = inner_trim.strip_prefix("write:") {
                write = Some(parsed_atoms(rest));
                last_field_end = inner.end;
                j += 1;
                continue;
            }
            return Err(line_error(
                inner,
                "field policy clauses are `read:` or `write:` followed by atoms",
            ));
        }
        fields.push(FieldPolicyDecl {
            field: field_name,
            read,
            write,
            span: Span::new(line.start, last_field_end),
        });
        last_end = last_field_end;
        i = j;
    }

    Ok((
        FieldPoliciesDecl {
            resource,
            fields,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn is_policy_identifier(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let mut chars = text.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// -----------------------------------------------------------------------------
// IR Error-Vocab (Cell PARSE-1) — `errors` block parser.
//
// Promotes the pre-existing LSP-only shape validator (legacy site at
// `crates/lazuli_lsp/src/lib.rs:6933` and around) into the canonical-indent
// parser so the surface earns a real IR slot (`ir::FeatureErrors`).
//
// Header at indent 2 (FEATURE_CHILD); children at indent 4 (AGENT_CHILD).
// Closed-catalog grammar (verbatim from
// `docs/proposals/ir-error-messages-vocab.md` §2.C):
//
//   errors
//     default hide
//     expose client 4xx <comma-list>
//     expose client 5xx <comma-list>
//     <code> message @translation.<key>          (zero or more)
//
// Closed-catalog enforcement (allowed codes, allowed field-name lists)
// lives analyzer-side / doctor-side (see ERR-VOCAB-CODE-UNKNOWN /
// ERR-VOCAB-EXPOSE-UNKNOWN); the parser keeps verbatim tokens so doctor
// diagnostics can surface canonical messages with the offending text.
// -----------------------------------------------------------------------------

fn parse_feature_errors_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(FeatureErrorsDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let mut default: Option<ErrorExposureDefaultAst> = None;
    let mut exposure_4xx: Option<Vec<String>> = None;
    let mut exposure_5xx: Option<Vec<String>> = None;
    let mut audience_exposure: Vec<FeatureErrorExposeRuleDecl> = Vec::new();
    let mut redact_patterns: Vec<String> = Vec::new();
    let mut messages: Vec<FeatureErrorMessageDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= header_indent {
            break;
        }

        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`errors` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("default ") {
            if default.is_some() {
                return Err(line_error(
                    line,
                    "`errors` may declare at most one `default <hide|expose>` line",
                ));
            }
            match rest.trim() {
                "hide" => default = Some(ErrorExposureDefaultAst::Hide),
                "expose" => default = Some(ErrorExposureDefaultAst::Expose),
                _ => {
                    return Err(line_error(
                        line,
                        "`default` must be `default hide` or `default expose`",
                    ));
                }
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("expose client ") {
            let rest = rest.trim();
            let (kind, fields_text) = rest.split_once(' ').ok_or_else(|| {
                line_error(
                    line,
                    "`expose client <4xx|5xx> <comma-list>` requires both a status family and a field list",
                )
            })?;
            let fields: Vec<String> = fields_text
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            if fields.is_empty() {
                return Err(line_error(
                    line,
                    "`expose client <4xx|5xx>` requires at least one field",
                ));
            }
            match kind {
                "4xx" => {
                    if exposure_4xx.is_some() {
                        return Err(line_error(
                            line,
                            "`errors` may declare at most one `expose client 4xx` line",
                        ));
                    }
                    exposure_4xx = Some(fields);
                }
                "5xx" => {
                    if exposure_5xx.is_some() {
                        return Err(line_error(
                            line,
                            "`errors` may declare at most one `expose client 5xx` line",
                        ));
                    }
                    exposure_5xx = Some(fields);
                }
                _ => {
                    return Err(line_error(
                        line,
                        "`expose client` status family must be `4xx` or `5xx`",
                    ));
                }
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("error_redact ") {
            let pattern = unquote_lzx_value(rest.trim()).trim().to_owned();
            if pattern.is_empty() {
                return Err(line_error(line, "`error_redact` requires a pattern"));
            }
            redact_patterns.push(pattern);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("expose to @audience ") {
            let (audience, fields_text) = rest.trim().split_once(' ').ok_or_else(|| {
                line_error(
                    line,
                    "`expose to @audience <name> <comma-list>` requires an audience and field list",
                )
            })?;
            if !is_kebab_or_snake_ident(audience) {
                return Err(line_error_owned(
                    line,
                    format!("audience `{}` must be kebab/snake case", audience),
                ));
            }
            let fields: Vec<String> = fields_text
                .split(',')
                .map(str::trim)
                .filter(|field| !field.is_empty())
                .map(str::to_owned)
                .collect();
            if fields.is_empty() {
                return Err(line_error(
                    line,
                    "`expose to @audience` requires at least one field",
                ));
            }
            audience_exposure.push(FeatureErrorExposeRuleDecl {
                audience: Some(audience.to_owned()),
                fields,
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
            continue;
        }

        // `<code> message @translation.<key>` — closed-catalog enforced
        // analyzer-side. The parser only checks structural shape (split
        // on `message ` keyword).
        if let Some((code_part, message_part)) = trimmed.split_once(" message ") {
            let code = code_part.trim().to_owned();
            if code.is_empty() {
                return Err(line_error(
                    line,
                    "`<code> message @translation.<key>` requires a code identifier",
                ));
            }
            let key = parse_translation_key_token(line, message_part)?;
            messages.push(FeatureErrorMessageDecl {
                code,
                message: key,
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`errors` children are `default <hide|expose>`, `expose client <4xx|5xx> <fields>`, or `<code> message @translation.<key>`",
        ));
    }

    Ok((
        FeatureErrorsDecl {
            default,
            exposure_4xx: exposure_4xx.unwrap_or_default(),
            exposure_5xx: exposure_5xx.unwrap_or_default(),
            audience_exposure,
            redact_patterns,
            messages,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

// -----------------------------------------------------------------------------
// Phase L Tier 4 follow-up — `enum <Name>` declaration parser.
//
// The fixture authors enums inside `domain` at indent 4 (header) with
// variants at indent 6. A variant is either `name` or `name = <value>`
// where the value is a bare integer or a quoted string. Wave A.5 adds
// optional colon metadata:
//
//   name: label @translation.key, hint @translation.hint, icon "user"
// -----------------------------------------------------------------------------

fn parse_enum_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(EnumDeclAst, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let name = trimmed
        .strip_prefix("enum ")
        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
        .ok_or_else(|| line_error(header, "enum header must be `enum <Name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "enum header requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;

    let mut variants: Vec<EnumVariantDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw_body = line.text.trim_start();
        if is_trivia(raw_body) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`enum` variants use one indentation level deeper than the header",
            ));
        }
        let body = strip_inline_comment(raw_body).trim_end();
        let (main_body, metadata_body) = match find_enum_metadata_separator(body) {
            Some(idx) => (body[..idx].trim_end(), Some(body[idx + 1..].trim())),
            None => (body, None),
        };
        let (label_key, hint_key, icon_key) = match metadata_body {
            Some(metadata) => parse_enum_variant_metadata(line, metadata)?,
            None => (None, None, None),
        };
        let (variant_name, storage) = match main_body.split_once('=') {
            Some((lhs, rhs)) => {
                let var_name = lhs.trim().to_owned();
                let raw = rhs.trim();
                let storage = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
                    Some(EnumStorageValueDecl::String(
                        raw[1..raw.len() - 1].to_owned(),
                    ))
                } else if let Ok(n) = raw.parse::<i64>() {
                    Some(EnumStorageValueDecl::Integer(n))
                } else {
                    return Err(line_error(
                        line,
                        "enum variant value must be an integer or a quoted string",
                    ));
                };
                (var_name, storage)
            }
            None => (main_body.to_owned(), None),
        };
        if variant_name.is_empty() {
            return Err(line_error(line, "enum variant requires a name"));
        }
        variants.push(EnumVariantDecl {
            name: variant_name,
            storage,
            label_key,
            hint_key,
            icon_key,
            span: Span::new(line.start, line.end),
        });
        last_end = line.end;
        i += 1;
    }

    Ok((
        EnumDeclAst {
            name,
            public_contract: None,
            variants,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn find_enum_metadata_separator(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut in_quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match in_quote {
            Some(q) if b == q => in_quote = None,
            Some(_) if b == b'\\' && i + 1 < bytes.len() => i += 1,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => in_quote = Some(b),
            None if b == b':' => return Some(i),
            None => {}
        }
        i += 1;
    }
    None
}

fn parse_enum_variant_metadata(
    line: &SourceLine<'_>,
    metadata: &str,
) -> Result<(Option<String>, Option<String>, Option<String>), ParseError> {
    if metadata.trim().is_empty() {
        return Err(line_error(
            line,
            "enum variant metadata requires `label <key>`",
        ));
    }

    let mut label_key: Option<String> = None;
    let mut hint_key: Option<String> = None;
    let mut icon_key: Option<String> = None;

    for part in split_enum_metadata_commas(metadata) {
        let item = part.trim();
        if item.is_empty() {
            return Err(line_error(
                line,
                "enum variant metadata contains an empty item",
            ));
        }
        if let Some(rest) = item.strip_prefix("label ") {
            if label_key.is_some() {
                return Err(line_error(line, "duplicate enum variant `label` metadata"));
            }
            label_key = Some(parse_enum_metadata_key(line, rest, "label")?);
        } else if let Some(rest) = item.strip_prefix("hint ") {
            if hint_key.is_some() {
                return Err(line_error(line, "duplicate enum variant `hint` metadata"));
            }
            hint_key = Some(parse_enum_metadata_key(line, rest, "hint")?);
        } else if let Some(rest) = item.strip_prefix("icon ") {
            if icon_key.is_some() {
                return Err(line_error(line, "duplicate enum variant `icon` metadata"));
            }
            icon_key = Some(parse_enum_icon_key(line, rest)?);
        } else {
            return Err(line_error(
                line,
                "enum variant metadata items are `label <key>`, `hint <key>`, or `icon \"<name>\"`",
            ));
        }
    }

    if label_key.is_none() {
        return Err(line_error(
            line,
            "enum variant metadata requires `label <key>` before `hint` or `icon`",
        ));
    }

    Ok((label_key, hint_key, icon_key))
}

fn split_enum_metadata_commas(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut in_quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match in_quote {
            Some(q) if b == q => in_quote = None,
            Some(_) if b == b'\\' && i + 1 < bytes.len() => i += 1,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => in_quote = Some(b),
            None if b == b',' => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            None => {}
        }
        i += 1;
    }
    out.push(&text[start..]);
    out
}

fn parse_enum_metadata_key(
    line: &SourceLine<'_>,
    raw: &str,
    kind: &'static str,
) -> Result<String, ParseError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(line_error(
            line,
            "enum variant metadata key cannot be empty",
        ));
    }
    let mut parts = trimmed.split_whitespace();
    let token = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "enum variant metadata keys must be single tokens",
        ));
    }
    if let Some(key) = token.strip_prefix("@translation.") {
        if key.is_empty() {
            return Err(line_error(
                line,
                "`@translation.` enum metadata reference requires a key",
            ));
        }
        return Ok(key.to_owned());
    }
    if token.starts_with('@') {
        return Err(line_error(
            line,
            "enum variant metadata keys must be bare keys or `@translation.<key>` references",
        ));
    }
    if token.is_empty() {
        return Err(line_error(
            line,
            "enum variant metadata key cannot be empty",
        ));
    }
    if kind == "label" || kind == "hint" {
        Ok(token.to_owned())
    } else {
        unreachable!("metadata key kind is restricted by parser callers")
    }
}

fn parse_enum_icon_key(line: &SourceLine<'_>, raw: &str) -> Result<String, ParseError> {
    let trimmed = raw.trim();
    if !(trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2) {
        return Err(line_error(
            line,
            "enum variant `icon` metadata expects a quoted string",
        ));
    }
    Ok(trimmed[1..trimmed.len() - 1].to_owned())
}

fn parse_agent(lines: &[SourceLine<'_>], start: usize) -> Result<(Agent, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("agent ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "agent header must be `agent <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "agent header requires a name"));
    }

    let mut input = Vec::new();
    let mut context: Option<String> = None;
    let mut policy: Option<Vec<String>> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut output: Option<AgentOutput> = None;
    let mut model: Option<String> = None;
    let mut temperature: Option<f64> = None;
    let mut max_tokens: Option<u32> = None;
    let mut top_p: Option<f64> = None;
    let mut seed: Option<i64> = None;
    let mut prompt: Option<String> = None;
    let mut safety = Vec::new();
    let mut tools = Vec::new();
    let mut evals = Vec::new();
    let mut expose: Option<AgentExpose> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "agent body children use four-space indentation",
            ));
        }

        if trimmed == "input" {
            let (slots, next) = parse_agent_input(lines, i)?;
            input = slots;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("context ") {
            context = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(split_policy_atoms(rest));
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("output ") {
            output = Some(parse_agent_output_value(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("model ") {
            model = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("temperature ") {
            temperature = Some(parse_float(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("max_tokens ") {
            max_tokens = Some(parse_uint32(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("top_p ") {
            top_p = Some(parse_float(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("seed ") {
            seed = Some(parse_int64(line, rest)?);
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("prompt ") {
            prompt = Some(unquote_lzx_value(rest.trim()).to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("safety ") {
            safety = split_policy_atoms(rest);
            i += 1;
        } else if trimmed == "tools" {
            let (parsed, next) = parse_agent_tools(lines, i)?;
            tools = parsed;
            i = next;
        } else if trimmed == "evals" {
            let (parsed, next) = parse_agent_evals(lines, i)?;
            evals = parsed;
            i = next;
        } else if trimmed == "expose http" {
            if expose.is_some() {
                return Err(line_error(
                    line,
                    "agent may declare at most one `expose http` block",
                ));
            }
            let (parsed, next) = parse_agent_expose(lines, i)?;
            expose = Some(parsed);
            i = next;
        } else {
            return Err(line_error(
                line,
                "agent children are `input`, `context`, `policy`, `rate_limit`, `output`, `model`, `temperature`, `max_tokens`, `top_p`, `seed`, `prompt`, `safety`, `tools`, `evals`, or `expose http`",
            ));
        }

        last_end = lines[i.saturating_sub(1).max(start)].end;
    }

    Ok((
        Agent {
            name,
            input,
            context,
            policy,
            rate_limit,
            output,
            model,
            temperature,
            max_tokens,
            top_p,
            seed,
            prompt,
            safety,
            tools,
            evals,
            expose,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

// -----------------------------------------------------------------------------
// Phase L Tier 4a — `defaults` block parser.
//
// The `defaults` header sits at AGENT_INDENT_FEATURE_CHILD (2 spaces).
// Children live at AGENT_INDENT_AGENT_CHILD (4 spaces):
//
//   defaults
//     tenancy org
//     timestamps
//     policy_for jobs, webhooks: @actor.system
//
// Unknown children are a parse error so an LLM cannot author silent
// typos like `timestapms` or `policy-for`.
// -----------------------------------------------------------------------------

fn parse_defaults(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(FeatureDefaults, usize), ParseError> {
    let header = &lines[start];
    let mut tenancy: Option<DefaultsTenancy> = None;
    let mut timestamps = false;
    let mut policy_for: Vec<DefaultsPolicyFor> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`defaults` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("tenancy ") {
            if tenancy.is_some() {
                return Err(line_error(
                    line,
                    "`defaults tenancy` may be declared at most once",
                ));
            }
            let axis = rest.trim();
            if axis.is_empty() {
                return Err(line_error(
                    line,
                    "`defaults tenancy` requires an axis (`org`, `team`, `none`, or a custom name)",
                ));
            }
            tenancy = Some(parse_defaults_tenancy(axis));
            last_end = line.end;
            i += 1;
        } else if trimmed == "timestamps" {
            if timestamps {
                return Err(line_error(
                    line,
                    "`defaults timestamps` may be declared at most once",
                ));
            }
            timestamps = true;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy_for ") {
            policy_for.push(parse_defaults_policy_for(line, rest)?);
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`defaults` children are `tenancy`, `timestamps`, or `policy_for <kinds>: <atom>`",
            ));
        }
    }

    Ok((
        FeatureDefaults {
            tenancy,
            timestamps,
            policy_for,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_defaults_tenancy(axis: &str) -> DefaultsTenancy {
    match axis.trim() {
        "org" => DefaultsTenancy::Org,
        "team" => DefaultsTenancy::Team,
        "none" => DefaultsTenancy::None,
        other => DefaultsTenancy::Custom(other.to_owned()),
    }
}

fn parse_defaults_policy_for(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<DefaultsPolicyFor, ParseError> {
    let (kinds_part, atom_part) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`policy_for` requires `<kinds>: <atom>` (e.g. `policy_for jobs, webhooks: @actor.system`)",
        )
    })?;
    let kinds: Vec<String> = kinds_part
        .split(',')
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
        .collect();
    if kinds.is_empty() {
        return Err(line_error(
            line,
            "`policy_for` requires at least one construct kind before the `:`",
        ));
    }
    let atom = atom_part.trim().to_owned();
    if atom.is_empty() {
        return Err(line_error(
            line,
            "`policy_for` requires a policy atom after the `:` (e.g. `@actor.system`)",
        ));
    }
    Ok(DefaultsPolicyFor {
        kinds,
        atom,
        span: Span::new(line.start, line.end),
    })
}

// -----------------------------------------------------------------------------
// Phase L Tier 4b — `command` / `api` block parsers + shared declarative
// spine helpers (`target`, `let`, `creates`/`updates`/`deletes` body).
//
// The `command` and `api` headers sit at AGENT_INDENT_FEATURE_CHILD (2
// spaces). Their children live at AGENT_INDENT_AGENT_CHILD (4 spaces);
// grandchildren (input slots, audit `emit_to`, approval modifiers,
// effect assignments) live at AGENT_INDENT_GRANDCHILD (6 spaces).
//
// `parse_target_expr`, `parse_let_binding`, and the assignment helpers
// are factored so `parse_job` and `parse_command_decl` share the same
// declarative-spine recogniser — closes the Tier 3 `JobDeclarative.raw_*`
// carve-out.
// -----------------------------------------------------------------------------

fn parse_command_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("command ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "command header must be `command <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "command header requires a name"));
    }

    let mut previously: Vec<String> = Vec::new();
    let mut route: Vec<CommandRouteSlot> = Vec::new();
    let mut input = CommandInputDecl::Empty;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    // IR Error-Vocab (Cell PARSE-1) — `when_denied @translation.<key>`
    // child under `policy` at indent 6 (GRANDCHILD).
    let mut policy_when_denied: Option<TranslationKeyRefAst> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut audit: Option<CommandAudit> = None;
    let mut approval: Option<CommandApproval> = None;
    let mut target: Option<TargetExprDecl> = None;
    let mut lets: Vec<LetBindingDecl> = Vec::new();
    let mut validate: Vec<String> = Vec::new();
    let mut effect: Option<CommandEffectDecl> = None;
    let mut returns: Option<String> = None;
    let mut handler: Option<JobHandler> = None;
    let mut emits: Vec<CommandEmit> = Vec::new();
    let mut triggers: Vec<String> = Vec::new();
    let mut invalidates: Vec<InvalidatesDecl> = Vec::new();
    let mut external_calls: Vec<JobExternalCall> = Vec::new();
    let mut tests: Vec<String> = Vec::new();
    let mut deprecated: Option<CommandDeprecatedDecl> = None;
    let mut timeout: Option<String> = None;
    let mut retry: Option<JobRetry> = None;
    let mut idempotency_by: Option<String> = None;
    let mut write_window: Option<CommandWriteWindow> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`command` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("previously ") {
            previously.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            route.push(parse_command_route_slot(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed == "input" {
            let (parsed, next) = parse_command_input_block(lines, i)?;
            input = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("input ") {
            // Short form: `input <field>` — single inline name.
            let value = rest.trim();
            if value.is_empty() {
                return Err(line_error(
                    line,
                    "`input <name>` short form requires a name",
                ));
            }
            input = CommandInputDecl::Short(value.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            // IR Error-Vocab (Cell PARSE-1) — consume optional
            // `when_denied @translation.<key>` child at indent 6
            // under the `policy` line.
            let mut j = i + 1;
            while j < lines.len() {
                let inner = &lines[j];
                let inner_trim = inner.text.trim_start();
                if is_trivia(inner_trim) {
                    j += 1;
                    continue;
                }
                if inner.indent <= AGENT_INDENT_AGENT_CHILD {
                    break;
                }
                if inner.indent != AGENT_INDENT_GRANDCHILD {
                    return Err(line_error(
                        inner,
                        "`policy` children use six-space indentation",
                    ));
                }
                if let Some(rest) = inner_trim.strip_prefix("when_denied ") {
                    if policy_when_denied.is_some() {
                        return Err(line_error(
                            inner,
                            "`policy` may declare at most one `when_denied` child (ERR-VOCAB-MULTIPLE-WHEN-DENIED)",
                        ));
                    }
                    policy_when_denied = Some(parse_translation_key_token(inner, rest)?);
                    last_end = inner.end;
                    j += 1;
                    continue;
                }
                return Err(line_error(
                    inner,
                    "`policy` children are `when_denied @translation.<key>` only",
                ));
            }
            i = j;
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("audit data_subject ") {
            let subject_field = rest.trim();
            if subject_field.is_empty() || !is_kebab_or_snake_ident(subject_field) {
                return Err(line_error(
                    line,
                    "`audit data_subject` requires a field identifier",
                ));
            }
            let Some(audit_spec) = audit.as_mut() else {
                return Err(line_error(
                    line,
                    "`audit data_subject <field>` must follow an `audit <subjects>` line",
                ));
            };
            if audit_spec.data_subject.is_some() {
                return Err(line_error(
                    line,
                    "`audit data_subject` may be declared at most once",
                ));
            }
            audit_spec.data_subject = Some(subject_field.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("audit ") {
            let (parsed, next) = parse_command_audit(lines, i, rest)?;
            audit = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "approval" {
            let (parsed, next) = parse_command_approval(lines, i)?;
            approval = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("target ") {
            let parsed = parse_target_expr(line, rest)?;
            target = Some(parsed);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("let ") {
            lets.push(parse_let_binding(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("validate ") {
            validate.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("creates ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Creates, rest)?;
            effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("updates ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Updates, rest)?;
            effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("deletes ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Deletes, rest)?;
            effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("returns ") {
            returns = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(parse_handler_line(rest));
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            let (parsed, next) = parse_command_emit(lines, i, rest)?;
            emits.push(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "triggers" {
            if !triggers.is_empty() {
                return Err(line_error(
                    line,
                    "`triggers transition` may be declared at most once",
                ));
            }
            let (parsed, next) = parse_command_triggers_block(lines, i)?;
            triggers = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("triggers ") {
            if !triggers.is_empty() {
                return Err(line_error(
                    line,
                    "`triggers transition` may be declared at most once",
                ));
            }
            triggers = parse_command_triggers(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if trimmed == "invalidates" {
            let (parsed, next) = parse_invalidates_block(lines, i)?;
            invalidates.extend(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("invalidates ") {
            // Single-line form: `invalidates query.list`.
            invalidates.push(parse_invalidates_entry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("calls ") {
            let (call, next) = parse_external_call(lines, i, rest)?;
            external_calls.push(call);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            // Phase L Tier 4 follow-up — mirror `parse_job` timeout.
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            // Phase L Tier 4 follow-up — mirror `parse_job` retry.
            retry = Some(parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            // Phase L Tier 4 follow-up — mirror `parse_job` idempotency.
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("write_window ") {
            write_window = Some(parse_command_write_window(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — `gate behind plan.feature: ...` / `gate quota plan.limit: ...`.
            // These directives are lifted via the side-channel
            // `parse_feature_gates` pass. Accept and discard here so the
            // canonical-indent parser does not reject the body.
            last_end = line.end;
            i += 1;
        } else if trimmed == "tests" {
            let (parsed, next) = parse_command_tests_block(lines, i)?;
            tests.extend(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "deprecated" {
            let (parsed, next) = parse_deprecated_block(lines, i)?;
            deprecated = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("deprecated ") {
            deprecated = Some(parse_command_deprecated(line, rest)?);
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`command` children are `previously`, `route`, `input`, `policy`, `rate_limit`, `audit`, `approval`, `deprecated`, `target`, `let`, `validate`, `creates`/`updates`/`deletes`, `returns`, `handler`, `emits`, `triggers transition`, `invalidates`, `calls`, `timeout`, `retry`, `idempotency by`, `write_window`, or `tests`",
            ));
        }
    }

    Ok((
        CommandDecl {
            name,
            public_contract: None,
            previously,
            route,
            input,
            policy,
            policy_expr,
            policy_when_denied,
            rate_limit,
            audit,
            approval,
            target,
            lets,
            validate,
            effect,
            returns,
            handler,
            emits,
            triggers,
            invalidates,
            external_calls,
            timeout,
            retry,
            idempotency_by,
            write_window,
            tests,
            deprecated,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_command_triggers(line: &SourceLine<'_>, rest: &str) -> Result<Vec<String>, ParseError> {
    let rest = rest.trim();
    let names = if rest == "transition" {
        ""
    } else if let Some(names) = rest.strip_prefix("transition ") {
        names.trim()
    } else {
        // Legacy pilot files used `triggers <transition>` before the surface
        // grew the explicit `transition` discriminator. Keep accepting it,
        // but normalize to the same `CommandDecl.triggers` vector.
        rest
    };
    if names.is_empty() {
        return Err(line_error(
            line,
            "`triggers transition` requires at least one transition name",
        ));
    }

    parse_command_trigger_names(line, names)
}

fn parse_command_trigger_names(
    line: &SourceLine<'_>,
    names: &str,
) -> Result<Vec<String>, ParseError> {
    let mut triggers = Vec::new();
    for name in names.split(',') {
        let name = name.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`triggers transition` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        if name.chars().any(char::is_whitespace) {
            return Err(line_error(
                line,
                "transition names in `triggers transition` cannot contain whitespace; separate with commas",
            ));
        }
        triggers.push(name.to_owned());
    }
    Ok(triggers)
}

fn parse_command_triggers_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<String>, usize), ParseError> {
    let mut triggers = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`triggers` children use six-space indentation",
            ));
        }

        let Some(rest) = trimmed.strip_prefix("transition ") else {
            return Err(line_error(
                line,
                "`triggers` children use `transition <name>[, <name>]`",
            ));
        };

        let parsed = parse_command_trigger_names(line, rest.trim())?;
        triggers.extend(parsed);
        i += 1;
    }

    if triggers.is_empty() {
        return Err(line_error(
            &lines[start],
            "`triggers` requires at least one `transition <name>` child",
        ));
    }

    Ok((triggers, i))
}

fn parse_command_write_window(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandWriteWindow, ParseError> {
    let Some(rest) = rest.trim().strip_prefix("by ") else {
        return Err(line_error(
            line,
            "`write_window` must be `write_window by <path> within <duration_or_ref>`",
        ));
    };
    let Some((by, within)) = rest.trim().split_once(" within ") else {
        return Err(line_error(
            line,
            "`write_window by <path>` requires `within <duration_or_ref>`",
        ));
    };
    let by = by.trim();
    let within = within.trim();
    if by.is_empty() {
        return Err(line_error(line, "`write_window by` requires a path"));
    }
    if within.is_empty() {
        return Err(line_error(
            line,
            "`write_window within` requires a duration or reference",
        ));
    }
    Ok(CommandWriteWindow {
        by: by.to_owned(),
        within: within.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

/// Parse `deprecated [since "<X>"] [replacement <ref>] [sunset "<Y>"]` —
/// inline single-line shape. Keys may appear in any order; each at most
/// once.
fn parse_command_deprecated(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandDeprecatedDecl, ParseError> {
    let mut since: Option<String> = None;
    let mut replacement: Option<String> = None;
    let mut sunset: Option<String> = None;
    let mut cursor = rest.trim();
    while !cursor.is_empty() {
        if let Some(after) = cursor.strip_prefix("since ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated since` requires a value"))?;
            since = Some(val);
            cursor = next.trim_start();
        } else if let Some(after) = cursor.strip_prefix("replacement ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated replacement` requires a value"))?;
            replacement = Some(val);
            cursor = next.trim_start();
        } else if let Some(after) = cursor.strip_prefix("sunset ") {
            let (val, next) = take_quoted_or_word(after)
                .ok_or_else(|| line_error(line, "`deprecated sunset` requires a value"))?;
            sunset = Some(val);
            cursor = next.trim_start();
        } else {
            return Err(line_error(
                line,
                "`deprecated` children are `since`, `replacement`, `sunset`",
            ));
        }
    }
    Ok(CommandDeprecatedDecl {
        since,
        replacement,
        sunset,
        span: Span::new(line.start, line.end),
    })
}

fn parse_deprecated_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandDeprecatedDecl, usize), ParseError> {
    let header = &lines[start];
    let mut since: Option<String> = None;
    let mut replacement: Option<String> = None;
    let mut sunset: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`deprecated` block children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("since ") {
            since = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("replacement ") {
            let (value, _) = take_quoted_or_word(rest)
                .ok_or_else(|| line_error(line, "`deprecated replacement` requires a value"))?;
            replacement = Some(value);
        } else if let Some(rest) = trimmed.strip_prefix("sunset ") {
            sunset = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`deprecated` children are `since`, `replacement`, `sunset`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    Ok((
        CommandDeprecatedDecl {
            since,
            replacement,
            sunset,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Take a quoted string or a single bare word (dotted refs allowed),
/// returning the unquoted value and the remainder of the input.
fn take_quoted_or_word(s: &str) -> Option<(String, &str)> {
    let trimmed = s.trim_start();
    if let Some(after_quote) = trimmed.strip_prefix('"') {
        let end = after_quote.find('"')?;
        let value = after_quote[..end].to_owned();
        let next = &after_quote[end + 1..];
        Some((value, next))
    } else {
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let value = trimmed[..end].to_owned();
        if value.is_empty() {
            return None;
        }
        let next = &trimmed[end..];
        Some((value, next))
    }
}

fn parse_command_route_slot(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<CommandRouteSlot, ParseError> {
    let rest = rest.trim();
    if rest == "signed_token" {
        return Ok(CommandRouteSlot {
            name: "signed_token".to_owned(),
            type_text: "Text".to_owned(),
            from: None,
            kind: CommandRouteSlotKind::SignedToken,
            span: Span::new(line.start, line.end),
        });
    }
    let signed_token_rest;
    let (kind, rest) = if let Some(after) = rest.strip_prefix("opaque ") {
        (CommandRouteSlotKind::OpaqueToken, after.trim())
    } else if let Some(after) = rest.strip_prefix("signed_token:") {
        signed_token_rest = format!("signed_token:{}", after);
        (
            CommandRouteSlotKind::SignedToken,
            signed_token_rest.as_str(),
        )
    } else {
        (CommandRouteSlotKind::Plain, rest)
    };
    let (name, after) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`route` requires `<name>: <Type>` (e.g. `route id: ID`)",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(line, "`route` requires a slot name before `:`"));
    }
    let after = after.trim();
    let (type_text, from) = if let Some(idx) = after.find(" from ") {
        let from_expr = after[idx + " from ".len()..].trim().to_owned();
        (after[..idx].trim().to_owned(), Some(from_expr))
    } else {
        (after.to_owned(), None)
    };
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "`route` requires a type after `:` (e.g. `ID`)",
        ));
    }
    Ok(CommandRouteSlot {
        name: name.to_owned(),
        type_text,
        from,
        kind,
        span: Span::new(line.start, line.end),
    })
}

fn parse_command_input_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandInputDecl, usize), ParseError> {
    let mut slots: Vec<CommandInputSlot> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`command input` children use six-space indentation",
            ));
        }

        let (name_part, type_part) = trimmed.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "`command input` slots use `<name>: <Type> [required|optional]`",
            )
        })?;
        let name = name_part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`command input` slot requires a name before `:`",
            ));
        }
        let rest = type_part.trim();
        // L0 #3 §10 — peel inline constraints first so the residual
        // string is just `<Type> [required|optional]`. Constraint
        // combination rules are enforced in the analyzer.
        let (rest_after, constraints) = extract_field_constraints(line, rest)?;
        // Walk to find the `required` or `optional` token at the end,
        // honouring parenthesised type-arg lists.
        let (type_text, required, optional) = split_command_input_modifiers(&rest_after);
        if type_text.is_empty() {
            return Err(line_error(
                line,
                "`command input` slot requires a type after `:`",
            ));
        }
        slots.push(CommandInputSlot {
            name: name.to_owned(),
            type_text,
            required,
            optional,
            constraints,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    Ok((CommandInputDecl::Typed(slots), i))
}

fn split_command_input_modifiers(rest: &str) -> (String, bool, bool) {
    // Find the last whitespace-separated tokens. Walk from the right and
    // peel `required` / `optional` modifiers; whatever remains is the
    // type text.
    let mut type_text = rest.to_owned();
    let mut required = false;
    let mut optional = false;
    loop {
        let trimmed = type_text.trim_end();
        if trimmed.ends_with(" required") {
            required = true;
            type_text = trimmed[..trimmed.len() - " required".len()].to_owned();
        } else if trimmed.ends_with(" optional") {
            optional = true;
            type_text = trimmed[..trimmed.len() - " optional".len()].to_owned();
        } else {
            type_text = trimmed.to_owned();
            break;
        }
    }
    (type_text, required, optional)
}

fn parse_command_audit(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(CommandAudit, usize), ParseError> {
    let header = &lines[start];
    let mut subjects: Vec<String> = Vec::new();
    let mut record_before = false;
    let mut record_after = false;
    let mut retain_for: Option<String> = None;
    for part in rest.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if part == "before" {
            record_before = true;
        } else if part == "after" {
            record_after = true;
        } else if let Some(duration) = part.strip_prefix("retain ") {
            let duration = duration.trim();
            if duration.is_empty() {
                return Err(line_error(header, "`audit retain` requires a duration"));
            }
            retain_for = Some(duration.to_owned());
        } else {
            subjects.push(part.to_owned());
        }
    }
    if subjects.is_empty() && !record_before && !record_after && retain_for.is_none() {
        return Err(line_error(
            header,
            "`audit` requires at least one subject (e.g. `audit actor, target.id`)",
        ));
    }
    let mut emit_to: Option<String> = None;
    let mut data_subject: Option<String> = None;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`audit` children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("emit_to ") {
            if emit_to.is_some() {
                return Err(line_error(
                    line,
                    "`audit emit_to` may be declared at most once",
                ));
            }
            emit_to = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("data_subject ") {
            let subject_field = rest.trim();
            if subject_field.is_empty() || !is_kebab_or_snake_ident(subject_field) {
                return Err(line_error(
                    line,
                    "`audit data_subject` requires a field identifier",
                ));
            }
            if data_subject.is_some() {
                return Err(line_error(
                    line,
                    "`audit data_subject` may be declared at most once",
                ));
            }
            data_subject = Some(subject_field.to_owned());
            i += 1;
        } else if trimmed == "before" {
            record_before = true;
            i += 1;
        } else if trimmed == "after" {
            record_after = true;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retain ") {
            if retain_for.is_some() {
                return Err(line_error(
                    line,
                    "`audit retain` may be declared at most once",
                ));
            }
            let duration = rest.trim();
            if duration.is_empty() {
                return Err(line_error(line, "`audit retain` requires a duration"));
            }
            retain_for = Some(duration.to_owned());
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`audit` children are `emit_to <event_group>`, `data_subject <field>`, `before`, `after`, or `retain <duration>` only",
            ));
        }
    }
    Ok((
        CommandAudit {
            subjects,
            emit_to,
            data_subject,
            record_before,
            record_after,
            retain_for,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

fn parse_command_approval(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CommandApproval, usize), ParseError> {
    let header = &lines[start];
    let mut required_when: Option<String> = None;
    let mut by: Option<String> = None;
    let mut timeout: Option<String> = None;
    let mut then: Option<ApprovalThenDecl> = None;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`approval` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("required_when ") {
            required_when = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("by ") {
            by = Some(rest.trim().to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("then ") {
            then = Some(match rest.trim() {
                "deny" => ApprovalThenDecl::Deny,
                "allow" => ApprovalThenDecl::Allow,
                "escalate" => ApprovalThenDecl::Escalate,
                other => {
                    return Err(line_error_owned(
                        line,
                        format!(
                            "`approval then` requires `deny`, `allow`, or `escalate` (got `{other}`)"
                        ),
                    ));
                }
            });
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`approval` children are `required_when`, `by`, `timeout`, or `then`",
            ));
        }
    }
    let by = by.ok_or_else(|| {
        line_error(
            header,
            "`approval` requires a `by @role.<name>` or `by @actor.<name>` declaration",
        )
    })?;
    let then = then.ok_or_else(|| {
        line_error(
            header,
            "`approval` requires a `then deny | allow | escalate` declaration",
        )
    })?;
    Ok((
        CommandApproval {
            required_when,
            by,
            timeout,
            then,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

/// `target query.<name>(args)` — single-line; args are name=expr pairs
/// inside the parens. The parser keeps the dotted query reference
/// verbatim so the analyzer's namespace resolver decides between
/// local/cross-feature.
fn parse_target_expr(line: &SourceLine<'_>, rest: &str) -> Result<TargetExprDecl, ParseError> {
    let rest = rest.trim();
    let (query_part, args_part) = split_call_signature(line, rest)?;
    let args = parse_named_args(line, args_part)?;
    Ok(TargetExprDecl {
        query: query_part.to_owned(),
        args,
        span: Span::new(line.start, line.end),
    })
}

fn parse_let_binding(line: &SourceLine<'_>, rest: &str) -> Result<LetBindingDecl, ParseError> {
    let rest = rest.trim();
    let (name, value) = rest.split_once('=').ok_or_else(|| {
        line_error(
            line,
            "`let` requires `<name> = <expr>` (e.g. `let resolved = user.query.by_id(id: input.id)`)",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(line, "`let` requires a binding name before `=`"));
    }
    Ok(LetBindingDecl {
        name: name.to_owned(),
        value: value.trim().to_owned(),
        span: Span::new(line.start, line.end),
    })
}

/// Parse the `creates X`, `updates X`, `deletes X` family. Children at
/// AGENT_INDENT_GRANDCHILD (6) are `<field> = <expr>` assignments. The
/// `from input` shorthand collapses into `from_input: true` with no
/// assignment block.
fn parse_command_effect(
    lines: &[SourceLine<'_>],
    start: usize,
    kind: CommandEffectKindDecl,
    rest: &str,
) -> Result<(CommandEffectDecl, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    let (resource, from_input) = if let Some(res) = rest.strip_suffix(" from input") {
        (res.trim().to_owned(), true)
    } else {
        (rest.to_owned(), false)
    };
    if resource.is_empty() {
        return Err(line_error(
            header,
            "`creates`/`updates`/`deletes` requires a resource name",
        ));
    }
    let mut assignments: Vec<AssignmentDecl> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "command effect children use six-space indentation",
            ));
        }
        let (field, value) = trimmed
            .split_once('=')
            .ok_or_else(|| line_error(line, "command effect assignments use `<field> = <expr>`"))?;
        let field = field.trim();
        if field.is_empty() {
            return Err(line_error(
                line,
                "command effect assignment requires a field name before `=`",
            ));
        }
        assignments.push(AssignmentDecl {
            field: field.to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((
        CommandEffectDecl {
            kind,
            resource,
            from_input,
            assignments,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

/// `emits <event>` line. Recognises trailing ` from creates` /
/// ` from updates` / ` from deletes`. Optional child block uses six-
/// space indent with `<key> = <expr>` lines.
fn parse_command_emit(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(CommandEmit, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    let (name, from) = if let Some(n) = rest.strip_suffix(" from creates") {
        (n.trim().to_owned(), Some(CommandEffectKindDecl::Creates))
    } else if let Some(n) = rest.strip_suffix(" from updates") {
        (n.trim().to_owned(), Some(CommandEffectKindDecl::Updates))
    } else if let Some(n) = rest.strip_suffix(" from deletes") {
        (n.trim().to_owned(), Some(CommandEffectKindDecl::Deletes))
    } else {
        (rest.to_owned(), None)
    };
    if name.is_empty() {
        return Err(line_error(header, "`emits` requires an event name"));
    }
    let mut fields: Vec<AssignmentDecl> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`emits` children use six-space indentation",
            ));
        }
        let (field, value) = trimmed
            .split_once('=')
            .ok_or_else(|| line_error(line, "`emits` field children use `<field> = <expr>`"))?;
        let field = field.trim();
        if field.is_empty() {
            return Err(line_error(
                line,
                "`emits` field child requires a field name before `=`",
            ));
        }
        fields.push(AssignmentDecl {
            field: field.to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((
        CommandEmit {
            name,
            from,
            fields,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

fn parse_invalidates_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<InvalidatesDecl>, usize), ParseError> {
    let mut out: Vec<InvalidatesDecl> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`invalidates` children use six-space indentation",
            ));
        }
        out.push(parse_invalidates_entry(line, trimmed)?);
        i += 1;
    }
    Ok((out, i))
}

pub(super) fn parse_invalidates_entry(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<InvalidatesDecl, ParseError> {
    let rest = rest.trim();
    // `query.list` or `query.by_id(id: route.id)`.
    if rest.contains('(') {
        let (query, args_part) = split_call_signature(line, rest)?;
        let args = parse_named_args(line, args_part)?;
        Ok(InvalidatesDecl {
            query: query.to_owned(),
            args,
            span: Span::new(line.start, line.end),
        })
    } else {
        if rest.is_empty() {
            return Err(line_error(
                line,
                "`invalidates` entry requires a query reference",
            ));
        }
        Ok(InvalidatesDecl {
            query: rest.to_owned(),
            args: Vec::new(),
            span: Span::new(line.start, line.end),
        })
    }
}

fn parse_command_tests_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<String>, usize), ParseError> {
    let mut out: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        out.push(trimmed.to_owned());
        i += 1;
    }
    Ok((out, i))
}

/// Split `foo.bar(arg: expr, ...)` into `("foo.bar", "arg: expr, ...")`.
/// Returns the query reference and the **content** between the parens
/// (or an empty string when no parens are present).
fn split_call_signature<'a>(
    line: &SourceLine<'_>,
    rest: &'a str,
) -> Result<(&'a str, &'a str), ParseError> {
    let rest = rest.trim_end();
    if let Some(open) = rest.find('(') {
        if !rest.ends_with(')') {
            return Err(line_error(
                line,
                "call expression must end with `)` (e.g. `query.by_id(id: route.id)`)",
            ));
        }
        let query = rest[..open].trim();
        let args = rest[open + 1..rest.len() - 1].trim();
        Ok((query, args))
    } else {
        Ok((rest.trim(), ""))
    }
}

/// Parse `name: expr, name: expr, ...` arg lists. Splits on the
/// top-level comma (no nested parens in v0 — `derived from` inside
/// queries is the only nesting today and doesn't appear in call args).
fn parse_named_args(line: &SourceLine<'_>, text: &str) -> Result<Vec<TargetArgDecl>, ParseError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<TargetArgDecl> = Vec::new();
    for piece in split_top_level_commas(text) {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let (name, value) = piece.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "call arguments use `<name>: <expr>` (e.g. `id: route.id`)",
            )
        })?;
        let name = name.trim();
        if name.is_empty() {
            return Err(line_error(line, "call argument requires a name before `:`"));
        }
        out.push(TargetArgDecl {
            name: name.to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
    }
    Ok(out)
}

fn split_top_level_commas(text: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (idx, ch) in text.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&text[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

// -----------------------------------------------------------------------------
// `api <name>` block parser.
// -----------------------------------------------------------------------------

fn parse_api_decl(lines: &[SourceLine<'_>], start: usize) -> Result<(ApiDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("api ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "api header must be `api <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "api header requires a name"));
    }
    let mut method: Option<HttpMethod> = None;
    let mut path: Option<String> = None;
    let mut output: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut handler: Option<String> = None;
    let mut locale_negotiate: Option<LocaleNegotiateDecl> = None;
    let mut route: Vec<CommandRouteSlot> = Vec::new();
    let mut input: Option<CommandInputDecl> = None;
    let mut deprecated: Option<CommandDeprecatedDecl> = None;
    let mut last_end = header.end;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`api` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("method ") {
            let token = rest.trim();
            method = Some(HttpMethod::from_token(token).ok_or_else(|| {
                line_error(
                    line,
                    "`api method` requires GET, POST, PUT, PATCH, or DELETE",
                )
            })?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("path ") {
            path = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("output ") {
            output = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            route.push(parse_command_route_slot(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed == "input" {
            let (parsed, next) = parse_command_input_block(lines, i)?;
            input = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "locale_negotiate" {
            if locale_negotiate.is_some() {
                return Err(line_error(
                    line,
                    "`api` may declare at most one `locale_negotiate` block",
                ));
            }
            let (parsed, next) = parse_locale_negotiate_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            locale_negotiate = Some(parsed);
            i = next;
        } else if trimmed == "deprecated" {
            let (parsed, next) = parse_deprecated_block(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            deprecated = Some(parsed);
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("deprecated ") {
            deprecated = Some(parse_command_deprecated(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass; tolerate here.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`api` children are `method`, `path`, `route`, `input`, `output`, `policy`, `rate_limit`, `handler`, `locale_negotiate`, `deprecated`, or `gate behind/quota plan.*`",
            ));
        }
    }
    let method =
        method.ok_or_else(|| line_error(header, "`api` requires a `method <VERB>` declaration"))?;
    let path =
        path.ok_or_else(|| line_error(header, "`api` requires a `path \"<route>\"` declaration"))?;
    let output = output.ok_or_else(|| {
        line_error(
            header,
            "`api` requires an `output <Type>` declaration (e.g. `output @cap.File(...)`)",
        )
    })?;
    Ok((
        ApiDecl {
            name,
            method,
            path,
            output,
            policy,
            policy_expr,
            rate_limit,
            handler,
            locale_negotiate,
            route,
            input,
            deprecated,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

// -----------------------------------------------------------------------------
// Report vocab — `report <name>` block parser.
//
// Header at AGENT_INDENT_FEATURE_CHILD (indent 2). Children at
// AGENT_INDENT_AGENT_CHILD (indent 4). The `columns` and `audit` blocks
// have grandchildren at AGENT_INDENT_GRANDCHILD (indent 6).
//
// See `docs/proposals/report-vocab.md` v0.2 §Linguagem.
// -----------------------------------------------------------------------------

fn parse_report_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(ReportDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("report ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "report header must be `report <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "report header requires a name"));
    }

    let mut source: Option<String> = None;
    let mut columns: Vec<ReportColumnAst> = Vec::new();
    let mut formats: Vec<String> = Vec::new();
    let mut storage: Option<String> = None;
    let mut visibility: Option<String> = None;
    let mut signed_ttl: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut audit: Option<CommandAudit> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`report` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            source = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "columns" {
            let (parsed, next) = parse_report_columns_block(lines, i)?;
            columns = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("formats ") {
            formats = rest
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("storage ") {
            storage = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("visibility ") {
            visibility = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("signed_ttl ") {
            signed_ttl = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("filename ") {
            filename = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("audit ") {
            let (parsed, next) = parse_command_audit(lines, i, rest)?;
            audit = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "audit" {
            return Err(line_error(
                line,
                "`report audit` requires at least one subject (e.g. `audit actor, ctx.now`)",
            ));
        } else {
            return Err(line_error(
                line,
                "`report` children are `source`, `columns`, `formats`, `storage`, `visibility`, `signed_ttl`, `filename`, `policy`, `rate_limit`, or `audit`",
            ));
        }
    }

    let source = source.ok_or_else(|| {
        line_error(
            header,
            "`report` requires a `source <query_ref>` declaration",
        )
    })?;
    if formats.is_empty() {
        return Err(line_error(
            header,
            "`report` requires a `formats <id>, ...` declaration (e.g. `formats csv, xlsx`)",
        ));
    }

    Ok((
        ReportDecl {
            name,
            source,
            columns,
            formats,
            storage,
            visibility,
            signed_ttl,
            filename,
            policy,
            policy_expr,
            rate_limit,
            audit,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse the `columns` block of a `report`. Header at indent 4, column
/// rows at indent 6. Each row: `<name> from <column_source> [label "..."] [format "..."]`.
fn parse_report_columns_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<ReportColumnAst>, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let mut columns: Vec<ReportColumnAst> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`report columns` rows use six-space indentation",
            ));
        }

        columns.push(parse_report_column(line)?);
        i += 1;
    }

    Ok((columns, i))
}

/// Parse one column row inside `report.columns`.
/// Grammar: `<name> from <column_source> [label "..."] [format "..."]`.
/// `column_source` is `row.<field>` or `@fn.<name>(arg, arg)`.
fn parse_report_column(line: &SourceLine<'_>) -> Result<ReportColumnAst, ParseError> {
    let trimmed = strip_inline_comment(line.text.trim_start()).trim_end();
    // Split off the column name first.
    let (name, rest) = split_first_token(trimmed).ok_or_else(|| {
        line_error(
            line,
            "report column row must be `<name> from <source> [label \"...\"] [format \"...\"]`",
        )
    })?;
    if name.is_empty() {
        return Err(line_error(line, "report column requires a name"));
    }

    let rest = rest.trim_start();
    let after_from = rest.strip_prefix("from ").ok_or_else(|| {
        line_error(
            line,
            "report column requires `from row.<field>` or `from @fn.<name>(args)`",
        )
    })?;

    // The column source extends through `row.<field>` or
    // `@fn.<name>(args...)`. Find the end of the source token, then
    // scan the tail for optional `label "..."` / `format "..."`.
    let (source, tail) = parse_report_column_source(after_from.trim_start(), line)?;

    let mut label: Option<String> = None;
    let mut format: Option<String> = None;
    let mut tail = tail.trim_start();
    while !tail.is_empty() {
        if let Some(rest) = tail.strip_prefix("label ") {
            let (value, next) = take_quoted_string(rest.trim_start(), line)?;
            label = Some(value);
            tail = next.trim_start();
        } else if let Some(rest) = tail.strip_prefix("format ") {
            let (value, next) = take_quoted_string(rest.trim_start(), line)?;
            format = Some(value);
            tail = next.trim_start();
        } else {
            return Err(line_error(
                line,
                "report column trailing modifiers are `label \"...\"` and `format \"...\"`",
            ));
        }
    }

    Ok(ReportColumnAst {
        name: name.to_owned(),
        source,
        label,
        format,
        span: Span::new(line.start, line.end),
    })
}

/// Parse a column source token: `row.<field>` or `@fn.<name>(arg, arg)`.
/// Returns the parsed source plus the remaining tail of the line.
fn parse_report_column_source<'a>(
    input: &'a str,
    line: &SourceLine<'_>,
) -> Result<(ReportColumnSourceAst, &'a str), ParseError> {
    if let Some(rest) = input.strip_prefix("row.") {
        let (field, tail) = take_identifier(rest);
        if field.is_empty() {
            return Err(line_error(
                line,
                "report column source `row.` requires a field identifier",
            ));
        }
        return Ok((ReportColumnSourceAst::RowField(field.to_owned()), tail));
    }

    if let Some(rest) = input.strip_prefix("@fn.") {
        let (name, after_name) = take_identifier(rest);
        if name.is_empty() {
            return Err(line_error(
                line,
                "report column source `@fn.` requires a function name",
            ));
        }
        let after_name = after_name.trim_start();
        let after_paren = after_name.strip_prefix('(').ok_or_else(|| {
            line_error(
                line,
                "report column source `@fn.<name>(args)` requires parentheses",
            )
        })?;
        let close_idx = after_paren.find(')').ok_or_else(|| {
            line_error(
                line,
                "report column source `@fn.<name>(...)` is missing the closing parenthesis",
            )
        })?;
        let args_text = &after_paren[..close_idx];
        let tail = &after_paren[close_idx + 1..];
        let args: Vec<String> = args_text
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok((
            ReportColumnSourceAst::FnCall {
                name: name.to_owned(),
                args,
            },
            tail,
        ));
    }

    Err(line_error(
        line,
        "report column source must be `row.<field>` or `@fn.<name>(args)`",
    ))
}

/// Split off the first whitespace-delimited token from `input`. Returns
/// `(token, rest_after_token)`. Returns `None` for an empty input.
fn split_first_token(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    let end = input
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(input.len());
    Some((&input[..end], &input[end..]))
}

/// Take an identifier prefix (`[A-Za-z_][A-Za-z0-9_]*`). Returns
/// `(ident, remainder)`.
fn take_identifier(input: &str) -> (&str, &str) {
    let mut end = 0;
    for (i, c) in input.char_indices() {
        let ok = if i == 0 {
            c.is_ascii_alphabetic() || c == '_'
        } else {
            c.is_ascii_alphanumeric() || c == '_'
        };
        if !ok {
            break;
        }
        end = i + c.len_utf8();
    }
    (&input[..end], &input[end..])
}

/// Take a `"..."` quoted string from the head of `input`. Returns
/// `(content_without_quotes, remainder_after_close_quote)`.
fn take_quoted_string<'a>(
    input: &'a str,
    line: &SourceLine<'_>,
) -> Result<(String, &'a str), ParseError> {
    let rest = input.strip_prefix('"').ok_or_else(|| {
        line_error(
            line,
            "report column modifier value must be a `\"...\"` literal",
        )
    })?;
    let close_idx = rest
        .find('"')
        .ok_or_else(|| line_error(line, "report column modifier missing closing quote"))?;
    let value = rest[..close_idx].to_owned();
    let tail = &rest[close_idx + 1..];
    Ok((value, tail))
}

/// i18n bucket cycle — parse a `locale_negotiate` block. Header at
/// indent 4 (inside `api`) or higher (inside `app.runtime unit` is
/// parsed by `app_manifest.rs` separately). Children at indent 6
/// (six-space): `source <axis>`, `strategy <name>`, `fallback <tag>`.
/// All slots optional.
fn parse_locale_negotiate_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LocaleNegotiateDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let mut source: Option<String> = None;
    let mut strategy: Option<String> = None;
    let mut fallback: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`locale_negotiate` body children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            source = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("strategy ") {
            strategy = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("fallback ") {
            fallback = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`locale_negotiate` children are `source`, `strategy`, or `fallback`",
            ));
        }
    }

    Ok((
        LocaleNegotiateDecl {
            source,
            strategy,
            fallback,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

// -----------------------------------------------------------------------------
// Phase L Tier 4c — `resource <Name>` block parser.
//
// The resource header lives at indent `H` (typically 4, inside
// `domain`; the slice supports either 2 or 4). Children live at `H+2`
// and grandchildren (`previously` under fields) at `H+4`. The parser
// computes the child indent dynamically so an LLM that authors
// resources at either feature-direct (`indent 2`) or `domain`-nested
// (`indent 4`) indentation gets the same recogniser.
// -----------------------------------------------------------------------------

fn parse_resource_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(ResourceDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("resource ")
        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
        .ok_or_else(|| line_error(header, "resource header must be `resource <Name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "resource header requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut state = ResourceBodyState::default();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "resource body children use one indentation level deeper than the `resource` header",
            ));
        }

        if trimmed == "soft_delete" {
            state.soft_delete = true;
            last_end = line.end;
            i += 1;
            continue;
        }
        if trimmed == "timestamps" {
            state.timestamps = true;
            last_end = line.end;
            i += 1;
            continue;
        }
        if trimmed == "lifecycle" {
            return Err(line_error(
                line,
                "`lifecycle` requires a discriminator field name: `lifecycle <field>`",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("lifecycle ") {
            if state.lifecycle.is_some() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `lifecycle` block",
                ));
            }
            if rest.trim().is_empty() {
                return Err(line_error(
                    line,
                    "`lifecycle` requires a discriminator field name: `lifecycle <field>`",
                ));
            }
            let (block, next) = parse_lifecycle_block(lines, i)?;
            state.lifecycle = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }
        // router-w4 — `lifecycle_routes` block.
        if trimmed == "lifecycle_routes" {
            if state.lifecycle_routes.is_some() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `lifecycle_routes` block",
                ));
            }
            let (block, next) = parse_resource_lifecycle_routes(lines, i)?;
            state.lifecycle_routes = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        // CL.C.4 — resource-scoped `invariant <name>` block. Shares
        // parser with the aggregate-scoped form; closed body is
        // `when <predicate>` plus optional `message "<text>"`.
        if let Some(rest) = trimmed.strip_prefix("invariant ") {
            let (inv, next) = parse_invariant_decl(lines, i, rest)?;
            state.invariants.push(inv);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        // Roadmap §1.5 (CL.C.2) — `lock optimistic version_field: <name>`,
        // `lock pessimistic`, `lock row_level`. Single-line decorator;
        // at most one per resource.
        if trimmed == "lock" {
            return Err(line_error(
                line,
                "`lock` requires a strategy: `lock optimistic version_field: <field>`, `lock pessimistic`, or `lock row_level`",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("lock ") {
            if state.lock.is_some() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `lock` decorator",
                ));
            }
            state.lock = Some(parse_resource_lock(line, rest)?);
            last_end = line.end;
            i += 1;
            continue;
        }

        // Roadmap §1.5 (CL.C.2) — `composite_key` block. Children at
        // grandchild indent: `fields <a>, <b>, ...` and `primary true|false`.
        if trimmed == "composite_key" {
            if state.composite_key.is_some() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `composite_key` block",
                ));
            }
            let (ck, next) = parse_resource_composite_key(lines, i, grandchild_indent)?;
            state.composite_key = Some(ck);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("composite_key ") {
            // Reject inline arguments — composite_key uses a block form
            // for child fields/primary lines.
            let _ = rest;
            return Err(line_error(
                line,
                "`composite_key` does not accept inline arguments — list fields under the block",
            ));
        }

        // `conventions [<name>, ...]` resource-level slot. Closed catalog
        // (today: `crud`). Empty list is a parse error — author writes no
        // slot at all rather than an empty one. See
        // `docs/proposals/ir-resource-conventions-crud.md` §4.1.
        if trimmed == "conventions" {
            return Err(line_error(
                line,
                "`conventions` requires a bracketed identifier list: `conventions [crud]`",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("conventions ") {
            if !state.conventions.is_empty() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `conventions` slot",
                ));
            }
            let entries = parse_resource_conventions_list(line, rest)?;
            state.conventions = entries;
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed.contains(':')
            && !resource_body_handlers()
                .iter()
                .any(|(prefix, _)| trimmed.starts_with(prefix))
        {
            // `<name>: <Type> [modifiers...]` field declaration. Consume
            // optional `previously` grandchild block.
            let (field, next) = parse_resource_field_decl(lines, i, grandchild_indent)?;
            state.fields.push(field);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        let mut matched = false;
        for (prefix, handler) in resource_body_handlers() {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                handler(line, rest, &mut state)?;
                last_end = line.end;
                i += 1;
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(line_error(
                line,
                "`resource` children are `previously`, `tenancy`, `soft_delete`, `timestamps`, `retention`, `validates`, `has_many`, `lifecycle`, `conventions`, `index on`, `unique (...)`, `fts on (...)`, or `<field>: <Type>`",
            ));
        }
    }

    Ok((
        ResourceDecl {
            name,
            public_contract: None,
            previously: state.previously,
            tenancy: state.tenancy,
            fields: state.fields,
            has_many: state.has_many,
            soft_delete: state.soft_delete,
            timestamps: state.timestamps,
            retention: state.retention,
            validates: state.validates,
            lifecycle: state.lifecycle,
            invariants: state.invariants,
            lock: state.lock,
            composite_key: state.composite_key,
            conventions: state.conventions,
            constraints: state.constraints,
            lifecycle_routes: state.lifecycle_routes,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

#[derive(Default)]
struct ResourceBodyState {
    previously: Vec<String>,
    tenancy: Option<DefaultsTenancy>,
    fields: Vec<ResourceFieldDecl>,
    has_many: Vec<ResourceHasMany>,
    soft_delete: bool,
    timestamps: bool,
    retention: Option<ResourceRetention>,
    validates: Vec<String>,
    lifecycle: Option<LifecycleBlockAst>,
    /// CL.C.4 — resource-scoped `invariant <name>` blocks.
    invariants: Vec<InvariantDecl>,
    /// Roadmap §1.5 (CL.C.2) — `lock` decorator.
    lock: Option<ResourceLock>,
    /// Roadmap §1.5 (CL.C.2) — `composite_key` block.
    composite_key: Option<ResourceCompositeKey>,
    /// `conventions [<name>, ...]` resource-level slot — closed catalog.
    /// See `docs/proposals/ir-resource-conventions-crud.md` §4.1.
    conventions: Vec<ResourceConventionAst>,
    /// Authored DDL constraints (`index on`, compound `unique`, `fts on`).
    constraints: Vec<ResourceConstraintAst>,
    /// router-w4 — `lifecycle_routes` block.
    lifecycle_routes: Option<crate::ast::ResourceLifecycleRoutesAst>,
}

type ResourceBodyHandler =
    for<'a> fn(&SourceLine<'a>, &str, &mut ResourceBodyState) -> Result<(), ParseError>;

fn resource_body_handlers() -> &'static [(&'static str, ResourceBodyHandler)] {
    &[
        ("previously ", handle_resource_previously),
        ("tenancy ", handle_resource_tenancy),
        ("retention ", handle_resource_retention),
        ("validates ", handle_resource_validates),
        ("has_many ", handle_resource_has_many),
        ("lifecycle ", handle_resource_lifecycle),
        ("index ", handle_resource_index),
        ("unique ", handle_resource_unique),
        ("fts ", handle_resource_fts),
    ]
}

fn handle_resource_lifecycle(
    line: &SourceLine<'_>,
    _rest: &str,
    _state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    Err(line_error(
        line,
        "internal: lifecycle should be dispatched inline before registry",
    ))
}

fn handle_resource_previously(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    state.previously.push(rest.trim().to_owned());
    Ok(())
}

fn handle_resource_tenancy(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let axis = rest.trim();
    if axis.is_empty() {
        return Err(line_error(
            line,
            "`resource tenancy` requires an axis (`org`, `team`, `none`, or a custom name)",
        ));
    }
    state.tenancy = Some(parse_defaults_tenancy(axis));
    Ok(())
}

fn handle_resource_retention(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    state.retention = Some(parse_resource_retention(line, rest)?);
    Ok(())
}

fn handle_resource_validates(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    state.validates.push(rest.trim().to_owned());
    Ok(())
}

fn handle_resource_has_many(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    state.has_many.push(parse_resource_has_many(line, rest)?);
    Ok(())
}

fn handle_resource_index(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    let Some(target) = rest.strip_prefix("on ") else {
        return Err(line_error(
            line,
            "`index` requires `on <field>` or `on (<field>, ...)`",
        ));
    };
    let (fields, method) = parse_resource_index_target(line, target.trim())?;
    state
        .constraints
        .push(ResourceConstraintAst::Index(ResourceIndexAst {
            fields,
            method,
            full_text: false,
            span: Span::new(line.start, line.end),
        }));
    Ok(())
}

fn handle_resource_unique(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    if !rest.starts_with('(') {
        return Err(line_error(
            line,
            "`unique` resource constraints use `unique (<field>, <field>, ...)`",
        ));
    }
    let (fields, trailing) = parse_parenthesized_field_list_with_trailing(line, rest)?;
    if !trailing.trim().is_empty() {
        return Err(line_error(
            line,
            "`unique (...)` does not accept trailing arguments",
        ));
    }
    state
        .constraints
        .push(ResourceConstraintAst::Unique(ResourceUniqueAst {
            fields,
            span: Span::new(line.start, line.end),
        }));
    Ok(())
}

fn handle_resource_fts(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    let Some(target) = rest.strip_prefix("on ") else {
        return Err(line_error(line, "`fts` requires `on (<field>, ...)`"));
    };
    let (fields, trailing) = parse_parenthesized_field_list_with_trailing(line, target.trim())?;
    let method = match trailing.trim() {
        "" => None,
        "gin" => Some(ResourceIndexMethodAst::Gin),
        other => {
            return Err(line_error_owned(
                line,
                format!("`fts on (...)` only accepts an optional `gin` modifier (got `{other}`)"),
            ));
        }
    };
    state
        .constraints
        .push(ResourceConstraintAst::Index(ResourceIndexAst {
            fields,
            method,
            full_text: true,
            span: Span::new(line.start, line.end),
        }));
    Ok(())
}

fn parse_lifecycle_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LifecycleBlockAst, usize), ParseError> {
    let header = &lines[start];
    let block_indent = header.indent;
    let rest = header
        .text
        .trim_start()
        .strip_prefix("lifecycle ")
        .unwrap_or("");
    let discriminator_field = rest.trim().to_owned();
    let child_indent = block_indent + 2;
    let grandchild_indent = block_indent + 4;

    let mut states = Vec::new();
    let mut transitions = Vec::new();
    let mut invariants = Vec::new();
    let mut invariant_handlers = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= block_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "lifecycle children use one indentation level deeper than the `lifecycle` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("state ") {
            let ((name, kind_keyword), span) = parse_lifecycle_state(line, rest)?;
            states.push(LifecycleStateAst {
                name,
                kind_keyword,
                span,
            });
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("transition ") {
            let transition_name = rest.trim();
            if transition_name.is_empty() {
                return Err(line_error(line, "`transition` requires a name"));
            }
            let (transition, next) =
                parse_lifecycle_transition(lines, i, transition_name, grandchild_indent)?;
            transitions.push(transition);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("invariant_handler ") {
            invariant_handlers.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("invariant ") {
            invariants.push(LifecycleInvariantAst {
                raw: rest.trim().to_owned(),
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "lifecycle children are `state`, `transition`, `invariant`, `invariant_handler`",
            ));
        }
    }

    if states.len() < 2 {
        return Err(line_error(
            header,
            "lifecycle requires at least 2 `state` declarations",
        ));
    }
    if transitions.is_empty() {
        return Err(line_error(
            header,
            "lifecycle requires at least 1 `transition`",
        ));
    }

    Ok((
        LifecycleBlockAst {
            discriminator_field,
            states,
            transitions,
            invariants,
            invariant_handlers,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_lifecycle_state(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<((String, Option<String>), Span), ParseError> {
    let mut parts = rest.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| line_error(line, "`state` requires a name"))?
        .to_owned();
    let kind_keyword = match parts.next() {
        None => None,
        Some(k @ ("initial" | "terminal")) => Some(k.to_owned()),
        Some(other) => {
            return Err(line_error_owned(
                line,
                format!("`state` modifier must be `initial` or `terminal`, got `{other}`"),
            ));
        }
    };
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`state` accepts at most one modifier (initial | terminal)",
        ));
    }
    Ok(((name, kind_keyword), Span::new(line.start, line.end)))
}

fn parse_lifecycle_transition(
    lines: &[SourceLine<'_>],
    start: usize,
    name: &str,
    child_indent: usize,
) -> Result<(LifecycleTransitionAst, usize), ParseError> {
    let header = &lines[start];
    let block_indent = header.indent;
    let tests_indent = child_indent + 2;
    let mut from = Vec::new();
    let mut to = None;
    let mut policy = None;
    let mut audit = None;
    let mut timestamps = None;
    let mut emits = Vec::new();
    let mut requires = None;
    let mut tests = Vec::new();
    let mut tests_seen = false;
    let mut previously = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= block_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "transition children use one indentation level deeper than the `transition` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("from ") {
            let states = split_lzx_list(rest);
            if states.is_empty() {
                return Err(line_error(line, "`from` requires at least one state"));
            }
            from.extend(states);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("to ") {
            ensure_lifecycle_once(line, "to", to.is_some())?;
            let target = parse_lifecycle_single_identifier(line, "`to`", rest)?;
            to = Some(target);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            ensure_lifecycle_once(line, "policy", policy.is_some())?;
            policy = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("audit ") {
            ensure_lifecycle_once(line, "audit", audit.is_some())?;
            audit = Some(format!("audit {}", rest.trim()));
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timestamps ") {
            ensure_lifecycle_once(line, "timestamps", timestamps.is_some())?;
            let field = parse_lifecycle_single_identifier(line, "`timestamps`", rest)?;
            timestamps = Some(field);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            let event = rest.trim();
            if event.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            emits.push(event.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("requires ") {
            ensure_lifecycle_once(line, "requires", requires.is_some())?;
            requires = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "tests" {
            ensure_lifecycle_once(line, "tests", tests_seen)?;
            tests_seen = true;
            last_end = line.end;
            i += 1;
            while i < lines.len() {
                let test_line = &lines[i];
                let test_trimmed = test_line.text.trim_start();
                if is_trivia(test_trimmed) {
                    i += 1;
                    continue;
                }
                if test_line.indent <= child_indent {
                    break;
                }
                if test_line.indent != tests_indent {
                    return Err(line_error(
                        test_line,
                        "`tests` children use one indentation level deeper than the `tests` header",
                    ));
                }
                tests.push(test_trimmed.to_owned());
                last_end = test_line.end;
                i += 1;
            }
        } else if let Some(rest) = trimmed.strip_prefix("previously ") {
            previously.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "transition children are `from`, `to`, `policy`, `audit`, `timestamps`, `emits`, `requires`, `tests`, or `previously`",
            ));
        }
    }

    if from.is_empty() {
        return Err(line_error(
            header,
            "`transition` requires at least one `from`",
        ));
    }
    let to = to.ok_or_else(|| line_error(header, "`transition` requires `to <state>`"))?;

    Ok((
        LifecycleTransitionAst {
            name: name.to_owned(),
            from,
            to,
            policy,
            audit,
            timestamps,
            emits,
            requires,
            tests,
            previously,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn ensure_lifecycle_once(
    line: &SourceLine<'_>,
    keyword: &'static str,
    already_seen: bool,
) -> Result<(), ParseError> {
    if already_seen {
        return Err(line_error_owned(
            line,
            format!("transition declares `{keyword}` at most once"),
        ));
    }
    Ok(())
}

fn parse_lifecycle_single_identifier(
    line: &SourceLine<'_>,
    keyword: &'static str,
    rest: &str,
) -> Result<String, ParseError> {
    let mut parts = rest.split_whitespace();
    let value = parts.next().ok_or_else(|| {
        line_error_owned(line, format!("{keyword} requires exactly one identifier"))
    })?;
    if parts.next().is_some() {
        return Err(line_error_owned(
            line,
            format!("{keyword} requires exactly one identifier"),
        ));
    }
    Ok(value.to_owned())
}

// ---------------------------------------------------------------------------
// CL.C.4 — `aggregate <Name>` + standalone `invariant <name>` parsers.
//
// `aggregate` lives at feature-child indent (sibling of `resource`,
// `command`). Closed body shape:
//
//     aggregate Order
//       root Order
//       contains OrderLine, Payment
//       invariants
//         invariant total_consistent
//           when sum(lines.amount) == total
//           message "Order total must match line items"
//
// `invariant` blocks also appear directly inside `resource` for
// resource-scoped invariants. The two parsers share `parse_invariant_decl`.
// ---------------------------------------------------------------------------

fn parse_aggregate_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AggregateDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let rest = header_trimmed
        .strip_prefix("aggregate ")
        .ok_or_else(|| line_error(header, "aggregate header must be `aggregate <Name>`"))?;
    let name = rest.trim();
    if name.is_empty() {
        return Err(line_error(
            header,
            "aggregate header requires a name (`aggregate <Name>`)",
        ));
    }
    let name = name.to_owned();
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut root: Option<String> = None;
    let mut contains: Vec<String> = Vec::new();
    let mut invariants: Vec<InvariantDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "aggregate body children use one indentation level deeper than the `aggregate` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("root ") {
            if root.is_some() {
                return Err(line_error(line, "aggregate declares `root` at most once"));
            }
            let target = rest.trim();
            if target.is_empty() {
                return Err(line_error(
                    line,
                    "`root` requires a resource name (`root <Resource>`)",
                ));
            }
            if target.split_whitespace().count() != 1 {
                return Err(line_error(line, "`root` accepts exactly one resource name"));
            }
            root = Some(target.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("contains ") {
            let parts = rest.split(',').map(str::trim).filter(|s| !s.is_empty());
            for member in parts {
                if member.split_whitespace().count() != 1 {
                    return Err(line_error(
                        line,
                        "`contains` accepts comma-separated resource names only",
                    ));
                }
                contains.push(member.to_owned());
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "invariants" {
            // Open-block form: child block of `invariant <name>` blocks
            // at grandchild_indent.
            i += 1;
            last_end = line.end;
            while i < lines.len() {
                let inv_line = &lines[i];
                let inv_trim = inv_line.text.trim_start();
                if is_trivia(inv_trim) {
                    i += 1;
                    continue;
                }
                if inv_line.indent <= child_indent {
                    break;
                }
                if inv_line.indent != grandchild_indent {
                    return Err(line_error(
                        inv_line,
                        "`invariants` children use one indentation level deeper than the `invariants` header",
                    ));
                }
                if let Some(inv_rest) = inv_trim.strip_prefix("invariant ") {
                    let (inv, next) = parse_invariant_decl(lines, i, inv_rest)?;
                    invariants.push(inv);
                    last_end = lines[next.saturating_sub(1).max(i)].end;
                    i = next;
                    continue;
                }
                return Err(line_error(
                    inv_line,
                    "`invariants` body accepts only `invariant <name>` blocks",
                ));
            }
            continue;
        }

        return Err(line_error(
            line,
            "`aggregate` children are `root`, `contains`, or `invariants`",
        ));
    }

    let root = root
        .ok_or_else(|| line_error(header, "aggregate requires a `root <Resource>` declaration"))?;

    Ok((
        AggregateDecl {
            name,
            root,
            contains,
            invariants,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse a single `invariant <name>` block. Reused by
/// `parse_aggregate_decl` (aggregate-scoped) and the resource parser
/// (resource-scoped). Closed body: `when <expr>` (required), `message
/// "<text>"` (optional).
///
/// `name_rest` is the substring after `invariant ` on the header line.
fn parse_invariant_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    name_rest: &str,
) -> Result<(InvariantDecl, usize), ParseError> {
    let header = &lines[start];
    let name = name_rest.trim();
    if name.is_empty() {
        return Err(line_error(
            header,
            "`invariant` requires a name (`invariant <name>`)",
        ));
    }
    if name.split_whitespace().count() != 1 {
        return Err(line_error(
            header,
            "`invariant` accepts exactly one name identifier",
        ));
    }
    let name = name.to_owned();
    let header_indent = header.indent;
    let child_indent = header_indent + 2;

    let mut when: Option<String> = None;
    let mut message: String = String::new();
    let mut message_seen = false;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "invariant body children use one indentation level deeper than the `invariant` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("when ") {
            if when.is_some() {
                return Err(line_error(line, "invariant declares `when` at most once"));
            }
            let expr = rest.trim();
            if expr.is_empty() {
                return Err(line_error(line, "`when` requires a predicate expression"));
            }
            when = Some(expr.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("message ") {
            if message_seen {
                return Err(line_error(
                    line,
                    "invariant declares `message` at most once",
                ));
            }
            message_seen = true;
            let raw = rest.trim();
            if let Some(quoted) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                message = quoted.to_owned();
            } else {
                message = raw.to_owned();
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`invariant` children are `when <predicate>` and optional `message \"<text>\"`",
        ));
    }

    let when =
        when.ok_or_else(|| line_error(header, "`invariant` requires a `when <predicate>` clause"))?;

    Ok((
        InvariantDecl {
            name,
            when,
            message,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// CL.C.4 — peel the `@slug` decorator off the raw type text. Returns
/// the cleaned type text + a bool indicating presence. `@slug` is
/// recognized as a standalone bare token (no parens) anywhere in the
/// decorator chain; other `@*` decorators (`@semantic.X`, `@pii.X`,
/// `@cap.Encrypted(...)`) stay inside the type text.
fn extract_slug_decorator(text: &str) -> (String, bool) {
    let mut parts: Vec<&str> = text.split_whitespace().collect();
    let mut slug = false;
    parts.retain(|tok| {
        if *tok == "@slug" {
            slug = true;
            false
        } else {
            true
        }
    });
    (parts.join(" "), slug)
}

// ---------------------------------------------------------------------------
// L0 #8 — `poller` block parser (docs/proposals/poller-vocab.md §3).
// Closed-catalog children; mirror of `parse_lifecycle_block` shape.
// ---------------------------------------------------------------------------

fn parse_poller_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(PollerBlockAst, usize), ParseError> {
    let header = &lines[start];
    let block_indent = header.indent;
    let child_indent = block_indent + 2;
    let grandchild_indent = block_indent + 4;
    let name = header
        .text
        .trim_start()
        .strip_prefix("poller ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "poller header must be `poller <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "poller header requires a name"));
    }

    let mut source: Option<String> = None;
    let mut cursor: Option<PollerCursorAst> = None;
    let mut retry: Option<PollerRetryAst> = None;
    let mut states: Vec<PollerStateAst> = Vec::new();
    let mut resolve_handler: Option<String> = None;
    let mut terminal_status_field: Option<String> = None;
    let mut terminal_result_field: Option<String> = None;
    let mut tick: Option<PollerTickAst> = None;
    let mut tenant_from: Option<String> = None;
    let mut idempotency: Vec<String> = Vec::new();
    let mut idempotency_seen = false;
    let mut audit: Option<String> = None;
    let mut emits: Vec<String> = Vec::new();
    let mut retry_quirks: Vec<PollerRetryQuirkAst> = Vec::new();

    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= block_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "poller children use one indentation level deeper than the `poller` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            if source.is_some() {
                return Err(line_error(line, "poller declares `source` at most once"));
            }
            let val = rest.trim();
            if val.is_empty() {
                return Err(line_error(
                    line,
                    "`source` requires a resource name (`source <Resource>`)",
                ));
            }
            source = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "cursor" {
            if cursor.is_some() {
                return Err(line_error(line, "poller declares `cursor` at most once"));
            }
            let (block, next) = parse_poller_cursor(lines, i, child_indent)?;
            cursor = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "retry" {
            if retry.is_some() {
                return Err(line_error(line, "poller declares `retry` at most once"));
            }
            let (block, next) = parse_poller_retry(lines, i, child_indent)?;
            retry = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "states" {
            if !states.is_empty() {
                return Err(line_error(line, "poller declares `states` at most once"));
            }
            let (block, next) = parse_poller_states(lines, i, child_indent)?;
            states = block;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("resolve via ") {
            if resolve_handler.is_some() {
                return Err(line_error(line, "poller declares `resolve` at most once"));
            }
            let val = rest.trim();
            let handler = val.strip_prefix("@fn.").ok_or_else(|| {
                line_error(
                    line,
                    "`resolve via` requires `@fn.<name>` handler reference",
                )
            })?;
            if handler.is_empty() {
                return Err(line_error(
                    line,
                    "`resolve via @fn.<name>` requires a handler name",
                ));
            }
            resolve_handler = Some(handler.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("terminal_status_field ") {
            if terminal_status_field.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `terminal_status_field` at most once",
                ));
            }
            terminal_status_field = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("terminal_result_field ") {
            if terminal_result_field.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `terminal_result_field` at most once",
                ));
            }
            terminal_result_field = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("tick ") {
            if tick.is_some() {
                return Err(line_error(line, "poller declares `tick` at most once"));
            }
            tick = Some(parse_poller_tick(line, rest)?);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            if tenant_from.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `tenant_from` at most once",
                ));
            }
            let val = rest.trim();
            if !val.starts_with("row.") {
                return Err(line_error(
                    line,
                    "`tenant_from` requires `row.<axis>_id` (poller cursor row is the producer)",
                ));
            }
            tenant_from = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            if idempotency_seen {
                return Err(line_error(
                    line,
                    "poller declares `idempotency` at most once",
                ));
            }
            idempotency_seen = true;
            for entry in rest.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                if !entry.starts_with("row.") {
                    return Err(line_error(
                        line,
                        "`idempotency by` entries are `row.<field>` paths",
                    ));
                }
                idempotency.push(entry.to_owned());
            }
            if idempotency.is_empty() {
                return Err(line_error(
                    line,
                    "`idempotency by` requires at least one `row.<field>`",
                ));
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("audit ") {
            if audit.is_some() {
                return Err(line_error(line, "poller declares `audit` at most once"));
            }
            audit = Some(format!("audit {}", rest.trim()));
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("emits ") {
            let event = rest.trim();
            if event.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            emits.push(event.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("retry_quirk ") {
            let (block, next) = parse_poller_retry_quirk(lines, i, rest, grandchild_indent)?;
            retry_quirks.push(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        return Err(line_error(
            line,
            "poller children are `source`, `cursor`, `retry`, `states`, `resolve via @fn.<name>`, \
             `terminal_status_field`, `terminal_result_field`, `tick every <duration>`, \
             `tenant_from row.<axis>_id`, `idempotency by row.<field>, ...`, `audit`, `emits`, `retry_quirk`",
        ));
    }

    let source =
        source.ok_or_else(|| line_error(header, "poller requires a `source <Resource>` child"))?;

    Ok((
        PollerBlockAst {
            name,
            source,
            cursor,
            retry,
            states,
            resolve_handler,
            terminal_status_field,
            terminal_result_field,
            tick,
            tenant_from,
            idempotency,
            audit,
            emits,
            retry_quirks,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_poller_cursor(
    lines: &[SourceLine<'_>],
    start: usize,
    child_indent: usize,
) -> Result<(PollerCursorAst, usize), ParseError> {
    let header = &lines[start];
    let grandchild_indent = child_indent + 2;
    let mut next_at_field: Option<String> = None;
    let mut resolved_at_field: Option<String> = None;
    let mut attempts_field: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= child_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`cursor` body uses one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("eligible_when ") {
            if next_at_field.is_some() {
                return Err(line_error(
                    line,
                    "`cursor` declares `eligible_when` at most once",
                ));
            }
            let mut parts = rest.split(',').map(str::trim);
            let na = parts.next().unwrap_or("");
            let ra = parts.next().unwrap_or("");
            if na.is_empty() || ra.is_empty() || parts.next().is_some() {
                return Err(line_error(
                    line,
                    "`eligible_when` requires two field names: \
                     `eligible_when <next_at_field>, <resolved_at_field>`",
                ));
            }
            next_at_field = Some(na.to_owned());
            resolved_at_field = Some(ra.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("attempts ") {
            if attempts_field.is_some() {
                return Err(line_error(
                    line,
                    "`cursor` declares `attempts` at most once",
                ));
            }
            let val = rest.trim();
            if val.is_empty() {
                return Err(line_error(line, "`attempts` requires a field name"));
            }
            attempts_field = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`cursor` body accepts only `eligible_when <a>, <b>` and `attempts <field>`",
        ));
    }

    let next_at_field = next_at_field
        .ok_or_else(|| line_error(header, "`cursor` requires an `eligible_when` child"))?;
    let resolved_at_field = resolved_at_field.expect("resolved_at parsed alongside next_at");
    let attempts_field = attempts_field
        .ok_or_else(|| line_error(header, "`cursor` requires an `attempts <field>` child"))?;

    Ok((
        PollerCursorAst {
            next_at_field,
            resolved_at_field,
            attempts_field,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_poller_retry(
    lines: &[SourceLine<'_>],
    start: usize,
    child_indent: usize,
) -> Result<(PollerRetryAst, usize), ParseError> {
    let header = &lines[start];
    let grandchild_indent = child_indent + 2;
    let mut max_attempts: Option<u32> = None;
    let mut backoff: Option<(String, Option<String>, Option<String>)> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= child_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`retry` body uses one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("max_attempts ") {
            if max_attempts.is_some() {
                return Err(line_error(
                    line,
                    "`retry` declares `max_attempts` at most once",
                ));
            }
            let val = rest.trim();
            let parsed = val
                .parse::<u32>()
                .map_err(|_| line_error(line, "`max_attempts` requires a non-negative integer"))?;
            max_attempts = Some(parsed);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("backoff ") {
            if backoff.is_some() {
                return Err(line_error(line, "`retry` declares `backoff` at most once"));
            }
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.is_empty() {
                return Err(line_error(
                    line,
                    "`backoff` requires a strategy (fixed | linear | exponential)",
                ));
            }
            let strategy = parts[0].to_owned();
            if !matches!(strategy.as_str(), "fixed" | "linear" | "exponential") {
                return Err(line_error(
                    line,
                    "`backoff` strategy must be `fixed`, `linear`, or `exponential`",
                ));
            }
            let mut base: Option<String> = None;
            let mut cap: Option<String> = None;
            let mut j = 1;
            while j < parts.len() {
                match parts[j] {
                    "base" => {
                        j += 1;
                        if j >= parts.len() {
                            return Err(line_error(line, "`base` requires a duration value"));
                        }
                        base = Some(parts[j].to_owned());
                        j += 1;
                    }
                    "cap" => {
                        j += 1;
                        if j >= parts.len() {
                            return Err(line_error(line, "`cap` requires a duration value"));
                        }
                        cap = Some(parts[j].to_owned());
                        j += 1;
                    }
                    other => {
                        return Err(line_error_owned(
                            line,
                            format!(
                                "`backoff` modifiers are `base <duration>` and `cap <duration>` \
                                 (got `{other}`)"
                            ),
                        ));
                    }
                }
            }
            backoff = Some((strategy, base, cap));
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`retry` body accepts only `max_attempts <int>` and `backoff <strategy> [base <d>] [cap <d>]`",
        ));
    }

    let (backoff_strategy, backoff_base, backoff_cap) = backoff
        .ok_or_else(|| line_error(header, "`retry` requires a `backoff <strategy>` child"))?;
    let max_attempts = max_attempts
        .ok_or_else(|| line_error(header, "`retry` requires a `max_attempts <int>` child"))?;

    Ok((
        PollerRetryAst {
            max_attempts,
            backoff_strategy,
            backoff_base,
            backoff_cap,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_poller_states(
    lines: &[SourceLine<'_>],
    start: usize,
    child_indent: usize,
) -> Result<(Vec<PollerStateAst>, usize), ParseError> {
    let header = &lines[start];
    let grandchild_indent = child_indent + 2;
    let mut states: Vec<PollerStateAst> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= child_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`states` body uses one indentation level deeper than the header",
            ));
        }

        let mut parts = trimmed.split_whitespace();
        let name = parts
            .next()
            .ok_or_else(|| line_error(line, "state entry requires a name"))?
            .to_owned();
        let kind_keyword = match parts.next() {
            None => None,
            Some(k @ ("initial" | "intermediate" | "terminal")) => Some(k.to_owned()),
            Some(other) => {
                return Err(line_error_owned(
                    line,
                    format!(
                        "state kind must be `initial`, `intermediate`, or `terminal` (got `{other}`)"
                    ),
                ));
            }
        };
        if parts.next().is_some() {
            return Err(line_error(
                line,
                "state entry accepts at most one kind modifier (initial | intermediate | terminal)",
            ));
        }
        states.push(PollerStateAst {
            name,
            kind_keyword,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    if states.len() < 2 {
        return Err(line_error(
            header,
            "poller `states` requires at least 2 entries",
        ));
    }

    Ok((states, i))
}

fn parse_poller_tick(line: &SourceLine<'_>, rest: &str) -> Result<PollerTickAst, ParseError> {
    let rest = rest.trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "every" {
        return Err(line_error(
            line,
            "`tick` requires `tick every <duration> [batch <int>]`",
        ));
    }
    let every = parts[1].to_owned();
    let mut batch: Option<u32> = None;
    if parts.len() > 2 {
        if parts.len() != 4 || parts[2] != "batch" {
            return Err(line_error(
                line,
                "`tick` modifier is `batch <int>` after `every <duration>`",
            ));
        }
        let parsed = parts[3]
            .parse::<u32>()
            .map_err(|_| line_error(line, "`batch` requires a non-negative integer"))?;
        batch = Some(parsed);
    }
    Ok(PollerTickAst {
        every,
        batch,
        span: Span::new(line.start, line.end),
    })
}

fn parse_poller_retry_quirk(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
    grandchild_indent: usize,
) -> Result<(PollerRetryQuirkAst, usize), ParseError> {
    let header = &lines[start];
    let kind = head_rest.trim().to_owned();
    if kind.is_empty() {
        return Err(line_error(
            header,
            "`retry_quirk` requires a catalog kind (e.g. `retry_quirk gender_flip_once`)",
        ));
    }
    let mut when: Option<String> = None;
    let mut counter_field: Option<String> = None;
    let mut mutate_field: Option<String> = None;
    let mut mutate_transform: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header.indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`retry_quirk` body uses one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("when ") {
            if when.is_some() {
                return Err(line_error(
                    line,
                    "`retry_quirk` declares `when` at most once",
                ));
            }
            when = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("counter ") {
            if counter_field.is_some() {
                return Err(line_error(
                    line,
                    "`retry_quirk` declares `counter` at most once",
                ));
            }
            counter_field = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("mutate ") {
            if mutate_field.is_some() {
                return Err(line_error(
                    line,
                    "`retry_quirk` declares `mutate` at most once",
                ));
            }
            let rest = rest.trim();
            let (lhs, rhs) = rest
                .split_once('=')
                .ok_or_else(|| line_error(line, "`mutate` requires `<field> = <transform>`"))?;
            let lhs = lhs.trim();
            let rhs = rhs.trim();
            let field = lhs.strip_prefix("row.").unwrap_or(lhs);
            if field.is_empty() {
                return Err(line_error(line, "`mutate` requires a field on `row.*`"));
            }
            mutate_field = Some(field.to_owned());
            mutate_transform = Some(rhs.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`retry_quirk` body accepts only `when <predicate>`, `counter <field>`, and `mutate <field> = <transform>`",
        ));
    }

    let when = when
        .ok_or_else(|| line_error(header, "`retry_quirk` requires a `when <predicate>` child"))?;
    let counter_field = counter_field
        .ok_or_else(|| line_error(header, "`retry_quirk` requires a `counter <field>` child"))?;
    let mutate_field = mutate_field.ok_or_else(|| {
        line_error(
            header,
            "`retry_quirk` requires a `mutate <field> = <transform>` child",
        )
    })?;
    let mutate_transform = mutate_transform.expect("transform parsed alongside field");

    Ok((
        PollerRetryQuirkAst {
            kind,
            when,
            counter_field,
            mutate_field,
            mutate_transform,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_resource_retention(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<ResourceRetention, ParseError> {
    let rest = rest.trim();
    let (duration, action) = rest.split_once(" then ").ok_or_else(|| {
        line_error(
            line,
            "`retention` requires `<duration> then <action>` (e.g. `retention 7y then anonymize`)",
        )
    })?;
    let action = match action.trim() {
        "anonymize" => ResourceRetentionAction::Anonymize,
        "delete" => ResourceRetentionAction::Delete,
        "archive" => ResourceRetentionAction::Archive,
        other => {
            return Err(line_error_owned(
                line,
                format!(
                    "`retention then` requires `anonymize`, `delete`, or `archive` (got `{other}`)"
                ),
            ));
        }
    };
    Ok(ResourceRetention {
        duration: duration.trim().to_owned(),
        action,
        span: Span::new(line.start, line.end),
    })
}

fn parse_resource_has_many(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<ResourceHasMany, ParseError> {
    let (name, after) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`has_many` requires `<name>: <Type> [inverse <field>]` (e.g. `has_many notes: CustomerNote inverse customer`)",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(
            line,
            "`has_many` requires a relation name before `:`",
        ));
    }
    let after = after.trim();
    let (type_text, inverse) = if let Some(idx) = after.find(" inverse ") {
        (
            after[..idx].trim().to_owned(),
            Some(after[idx + " inverse ".len()..].trim().to_owned()),
        )
    } else {
        (after.to_owned(), None)
    };
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "`has_many` requires a resource type after `:`",
        ));
    }
    Ok(ResourceHasMany {
        name: name.to_owned(),
        type_text,
        inverse,
        span: Span::new(line.start, line.end),
    })
}

fn parse_resource_index_target(
    line: &SourceLine<'_>,
    target: &str,
) -> Result<(Vec<String>, Option<ResourceIndexMethodAst>), ParseError> {
    let (fields, trailing) = if target.starts_with('(') {
        parse_parenthesized_field_list_with_trailing(line, target)?
    } else {
        let mut parts = target.splitn(2, char::is_whitespace);
        let field = parts.next().unwrap_or("").trim();
        if field.is_empty() {
            return Err(line_error(
                line,
                "`index on` requires a field name or parenthesized field list",
            ));
        }
        if !is_policy_identifier(field) {
            return Err(line_error_owned(
                line,
                format!("`{field}` is not a valid field name in `index on`"),
            ));
        }
        (vec![field.to_owned()], parts.next().unwrap_or("").trim())
    };
    let method = parse_resource_index_method(line, trailing.trim())?;
    Ok((fields, method))
}

fn parse_parenthesized_field_list_with_trailing<'a>(
    line: &SourceLine<'_>,
    text: &'a str,
) -> Result<(Vec<String>, &'a str), ParseError> {
    let text = text.trim();
    if !text.starts_with('(') {
        return Err(line_error(line, "expected parenthesized field list"));
    }
    let Some(end) = text.find(')') else {
        return Err(line_error(line, "field list is missing its closing `)`"));
    };
    let inner = &text[1..end];
    let fields = parse_resource_field_list(line, inner)?;
    Ok((fields, &text[end + 1..]))
}

fn parse_resource_field_list(
    line: &SourceLine<'_>,
    fields: &str,
) -> Result<Vec<String>, ParseError> {
    let parsed: Vec<String> = fields
        .split(',')
        .map(|field| field.trim().to_owned())
        .filter(|field| !field.is_empty())
        .collect();
    if parsed.is_empty() {
        return Err(line_error(
            line,
            "field list requires at least one field name",
        ));
    }
    for field in &parsed {
        if !is_policy_identifier(field) {
            return Err(line_error_owned(
                line,
                format!("`{field}` is not a valid field name in this list"),
            ));
        }
    }
    Ok(parsed)
}

fn parse_resource_index_method(
    line: &SourceLine<'_>,
    trailing: &str,
) -> Result<Option<ResourceIndexMethodAst>, ParseError> {
    let trailing = trailing.trim();
    if trailing.is_empty() {
        return Ok(None);
    }
    let method = trailing
        .strip_prefix("using ")
        .map(str::trim)
        .unwrap_or(trailing);
    let parsed = match method {
        "btree" => ResourceIndexMethodAst::Btree,
        "gin" => ResourceIndexMethodAst::Gin,
        "gist" => ResourceIndexMethodAst::Gist,
        other => {
            return Err(line_error_owned(
                line,
                format!(
                    "`index on` supports optional methods `btree`, `gin`, or `gist` (got `{other}`)"
                ),
            ));
        }
    };
    Ok(Some(parsed))
}

fn parse_resource_field_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(ResourceFieldDecl, usize), ParseError> {
    let header = &lines[start];
    let raw_trimmed = header.text.trim_start();
    let trimmed = strip_inline_comment(raw_trimmed).trim_end();
    let (name_part, after) = trimmed.split_once(':').ok_or_else(|| {
        line_error(
            header,
            "resource field must be `<name>: <Type> [modifiers...]`",
        )
    })?;
    let name = name_part.trim();
    if name.is_empty() {
        return Err(line_error(
            header,
            "resource field requires a name before `:`",
        ));
    }
    let after = after.trim();
    // Split the type text from trailing modifiers honouring parens.
    let (raw_type_text, modifiers_text, default, derived_from, constraints) =
        split_resource_field_after(header, after)?;
    let required = modifiers_text.contains("required");
    let optional = modifiers_text.contains("optional");
    let unique = modifiers_text.contains("unique");
    // CL.C.4 — `@slug` field decorator. Lives in the type/decorator
    // chain alongside `@semantic.X`/`@pii.X`. We peel it to a typed
    // `Field.slug` bool so codegen and doctor read it from the typed
    // slot without re-scanning `type_text`. Stripped from `type_text`
    // so `type_ref_from_*` does not see an unknown token.
    let (type_text, slug) = extract_slug_decorator(&raw_type_text);

    // Roadmap §1.5 (CL.C.2) — `@full_text` field decorator. Sits in
    // the type/decorator chain alongside `@slug`/`@semantic.X`/`@pii.X`.
    // We peel it to a typed `Field.full_text` bool. Detection is
    // depth-aware so it doesn't trip on parenthesised decorator args
    // (e.g. `@cap.Encrypted(key:@key.tenant)`).
    let (type_text, full_text) = extract_full_text_marker(header, &type_text)?;

    // `ir-resource-conventions-owner-scope` §7.1 — peel
    // `@owner_axis(through: <ident>)` out of the type text into a
    // typed `ResourceFieldDecl.owner_axis` slot. The analyzer
    // projects this onto `ir::Field.owner_axis`; the synth pass (O2)
    // builds the ownership-chain WHERE-clause predicate from it.
    let (type_text, owner_axis) = extract_owner_axis_decorator(header, &type_text)?;

    // Consume optional `previously migrated <old>` grandchild lines.
    let mut previously: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let inner = line.text.trim_start();
        if is_trivia(inner) {
            i += 1;
            continue;
        }
        if line.indent != grandchild_indent {
            break;
        }
        if let Some(rest) = inner.strip_prefix("previously ") {
            previously.push(rest.trim().to_owned());
            i += 1;
        } else {
            break;
        }
    }

    Ok((
        ResourceFieldDecl {
            name: name.to_owned(),
            type_text,
            required,
            optional,
            unique,
            slug,
            default,
            derived_from,
            constraints,
            full_text,
            owner_axis,
            previously,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

/// `ir-resource-conventions-owner-scope` §7.1 — peel
/// `@owner_axis(through: <ident>)` off a resource-field type text.
/// Returns the cleaned type text plus the optional axis payload.
///
/// Grammar:
/// - `@owner_axis(through: <ident>)` — keyword, open paren,
///   `through:`, bare identifier, close paren. Whitespace flexible.
/// - `<ident>` is a snake_case identifier; string literals (`"user"`)
///   are rejected with a parse error so authors don't accidentally
///   quote a column name into a heterogeneous shape.
/// - `@owner_axis` standalone (no parens) is a parse error.
/// - `@owner_axis()` with empty body is a parse error.
/// - Duplicate `@owner_axis(...)` on the same field is a parse error.
///
/// Detection is depth-aware (must sit at paren depth 0) so the marker
/// does not collide with paren-nested decorator args like
/// `@cap.Encrypted(key:@key.tenant)`.
fn extract_owner_axis_decorator(
    line: &SourceLine<'_>,
    type_text: &str,
) -> Result<(String, Option<OwnerAxisAst>), ParseError> {
    let bytes = type_text.as_bytes();
    const NEEDLE: &[u8] = b"@owner_axis";
    let mut depth = 0i32;
    let mut hit: Option<(usize, usize, String)> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if depth == 0 && i + NEEDLE.len() <= bytes.len() && &bytes[i..i + NEEDLE.len()] == NEEDLE {
            let before_ok = i == 0 || (bytes[i - 1] as char).is_whitespace();
            let after_idx = i + NEEDLE.len();
            // The keyword must be followed (after optional whitespace)
            // by `(` — bare `@owner_axis` is rejected so authors don't
            // accidentally ship the annotation without an axis column.
            let mut j = after_idx;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if before_ok {
                if j >= bytes.len() || bytes[j] as char != '(' {
                    return Err(line_error(
                        line,
                        "`@owner_axis` requires `(through: <ident>)` — bare keyword is not allowed",
                    ));
                }
                // Find the balanced closing paren.
                let mut d = 0i32;
                let mut k = j;
                let mut closed: Option<usize> = None;
                while k < bytes.len() {
                    match bytes[k] as char {
                        '(' => d += 1,
                        ')' => {
                            d -= 1;
                            if d == 0 {
                                closed = Some(k);
                                break;
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                let Some(close) = closed else {
                    return Err(line_error(
                        line,
                        "`@owner_axis(...)` is missing a closing `)`",
                    ));
                };
                let body = type_text[j + 1..close].trim();
                let through = parse_owner_axis_body(line, body)?;
                if hit.is_some() {
                    return Err(line_error(
                        line,
                        "duplicate `@owner_axis(...)` decorator on field",
                    ));
                }
                hit = Some((i, close + 1, through));
                i = close + 1;
                continue;
            }
        }
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    let Some((start, end, through_column)) = hit else {
        return Ok((type_text.to_owned(), None));
    };
    let before = type_text[..start].trim_end();
    let after = type_text[end..].trim_start();
    let mut cleaned = String::with_capacity(type_text.len());
    cleaned.push_str(before);
    if !before.is_empty() && !after.is_empty() {
        cleaned.push(' ');
    }
    cleaned.push_str(after);
    Ok((
        cleaned.trim().to_owned(),
        Some(OwnerAxisAst { through_column }),
    ))
}

/// Parse the body of `@owner_axis(<body>)`. Body must be exactly
/// `through: <ident>` per §7.1. String literals are rejected so the
/// authored shape stays homogenous with other identifier-valued slots
/// (`@slug`, `derived from`).
fn parse_owner_axis_body(line: &SourceLine<'_>, body: &str) -> Result<String, ParseError> {
    if body.is_empty() {
        return Err(line_error(
            line,
            "`@owner_axis()` requires `through: <ident>` — empty body is not allowed",
        ));
    }
    let (key, value) = body
        .split_once(':')
        .ok_or_else(|| line_error(line, "`@owner_axis(...)` body must be `through: <ident>`"))?;
    if key.trim() != "through" {
        return Err(line_error(
            line,
            "`@owner_axis(...)` only accepts the `through:` keyword argument",
        ));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err(line_error(
            line,
            "`@owner_axis(through:)` is missing the column identifier",
        ));
    }
    if value.starts_with('"') || value.starts_with('\'') {
        return Err(line_error(
            line,
            "`@owner_axis(through: <ident>)` requires a bare identifier, not a string literal",
        ));
    }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || value
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(true)
    {
        return Err(line_error(
            line,
            "`@owner_axis(through: <ident>)` identifier must match `[A-Za-z_][A-Za-z0-9_]*`",
        ));
    }
    Ok(value.to_owned())
}

/// Roadmap §1.5 (CL.C.2) — peel the `@full_text` decorator off the
/// type text. Returns the cleaned type text plus a boolean flag. The
/// marker is rejected if it appears more than once. Depth-aware so
/// paren-balanced decorator args (e.g. `@cap.Encrypted(key:@key.tenant)`)
/// are left alone.
fn extract_full_text_marker(
    line: &SourceLine<'_>,
    type_text: &str,
) -> Result<(String, bool), ParseError> {
    let bytes = type_text.as_bytes();
    let needle = b"@full_text";
    let mut depth = 0i32;
    let mut hit: Option<usize> = None;
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        let ch = bytes[i] as char;
        if ch == '(' || ch == '[' {
            depth += 1;
        } else if ch == ')' || ch == ']' {
            depth -= 1;
        }
        if depth == 0 && &bytes[i..i + needle.len()] == needle {
            // Boundary check: must be preceded by start/whitespace and
            // followed by end/whitespace so `@full_text_oops` doesn't
            // match.
            let before_ok = i == 0 || (bytes[i - 1] as char).is_whitespace();
            let end = i + needle.len();
            let after_ok = end == bytes.len() || (bytes[end] as char).is_whitespace();
            if before_ok && after_ok {
                if hit.is_some() {
                    return Err(line_error(
                        line,
                        "duplicate `@full_text` decorator on field",
                    ));
                }
                hit = Some(i);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    let Some(start) = hit else {
        return Ok((type_text.to_owned(), false));
    };
    let end = start + needle.len();
    let mut cleaned = String::with_capacity(type_text.len() - needle.len());
    cleaned.push_str(type_text[..start].trim_end());
    let tail = type_text[end..].trim_start();
    if !cleaned.is_empty() && !tail.is_empty() {
        cleaned.push(' ');
    }
    cleaned.push_str(tail);
    Ok((cleaned.trim().to_owned(), true))
}

/// Roadmap §1.5 (CL.C.2) — parse the `lock` strategy from the
/// single-line decorator. Closed catalog:
///
/// - `optimistic version_field: <field>`
/// - `pessimistic`
/// - `row_level`
/// Parse the bracketed identifier list following `conventions ` on a
/// resource body line. Implements the grammar from
/// `docs/proposals/ir-resource-conventions-crud.md` §4.1:
///
/// - `[` token, comma-separated identifiers, `]` token.
/// - One or more identifiers required — empty list (`conventions []`)
///   is a parse error.
/// - Each identifier must be in the closed catalog (today: `crud`).
/// - Unknown identifiers emit the `conventions_unknown` diagnostic
///   with a nearest-match suggestion per §4.3 (single-character
///   Levenshtein → `crud` for `crd`/`curd`/`cur`, etc.).
///
/// Duplicates are accepted at parse time per the brief; deduplication
/// (if needed) is the analyzer's job.
fn parse_resource_conventions_list(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<Vec<ResourceConventionAst>, ParseError> {
    let rest = rest.trim();
    let inner = rest
        .strip_prefix('[')
        .ok_or_else(|| {
            line_error(
                line,
                "`conventions` requires a bracketed identifier list: `conventions [crud]`",
            )
        })?
        .strip_suffix(']')
        .ok_or_else(|| line_error(line, "`conventions [<name>, ...]` must close with `]`"))?;
    let inner_trimmed = inner.trim();
    if inner_trimmed.is_empty() {
        return Err(line_error(
            line,
            "`conventions []` is not allowed — list at least one convention or omit the slot entirely",
        ));
    }
    let mut entries: Vec<ResourceConventionAst> = Vec::new();
    for raw in inner_trimmed.split(',') {
        let ident = raw.trim();
        if ident.is_empty() {
            return Err(line_error(
                line,
                "`conventions [...]` entries must be non-empty identifiers separated by commas",
            ));
        }
        match resource_convention_ident(ident) {
            Some(c) => entries.push(c),
            None => {
                let suggestion = nearest_resource_convention(ident);
                let msg = match suggestion {
                    Some(s) => format!(
                        "conventions_unknown: `{}` is not in the closed catalog. did you mean `{}`?",
                        ident, s,
                    ),
                    None => format!(
                        "conventions_unknown: `{}` is not in the closed catalog (known: `crud`)",
                        ident,
                    ),
                };
                return Err(line_error_owned(line, msg));
            }
        }
    }
    Ok(entries)
}

/// Map a parsed identifier to the closed catalog of resource-level
/// conventions. Returns `None` for any unknown identifier — the caller
/// raises `conventions_unknown` with a nearest-match suggestion.
fn resource_convention_ident(ident: &str) -> Option<ResourceConventionAst> {
    match ident {
        "crud" => Some(ResourceConventionAst::Crud),
        "me" => Some(ResourceConventionAst::Me),
        _ => None,
    }
}

/// Suggest the nearest closed-catalog convention identifier for an
/// unknown token. Single-character Levenshtein per crud §4.3 / me
/// §4.3 — returns the closest match within edit-distance 1.
fn nearest_resource_convention(ident: &str) -> Option<&'static str> {
    const CATALOG: &[&str] = &["crud", "me"];
    let mut best: Option<(&'static str, usize)> = None;
    for candidate in CATALOG {
        let d = levenshtein_distance(ident, candidate);
        if d <= 1 && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((candidate, d));
        }
    }
    best.map(|(s, _)| s)
}

/// Minimal Levenshtein distance used by `nearest_resource_convention`.
/// Lives next to its single caller and avoids a new dependency. Inputs
/// are short identifiers, so the dynamic-programming table is trivially
/// sized.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (n, m) = (a_chars.len(), b_chars.len());
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
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// router-w4 — parse a `lifecycle_routes` block. Children are at
/// grandchild indent (4 spaces from resource start): `<state> -> "<url>"`.
/// State must be a bare identifier, `none`, or `*`. URL is a
/// double-quoted string. Empty body is an error (an empty table has
/// no purpose; authors should omit the block).
fn parse_resource_lifecycle_routes(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::ResourceLifecycleRoutesAst, usize), ParseError> {
    let header = &lines[start];
    let mut arms = Vec::new();
    let mut i = start + 1;
    let body_indent = header.indent + 2;
    let mut last_end = header.end;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < body_indent {
            break;
        }
        if line.indent != body_indent {
            return Err(line_error(
                line,
                "`lifecycle_routes` arms use grandchild indentation (4 spaces)",
            ));
        }
        let (state, rest) = trimmed.split_once("->").ok_or_else(|| {
            line_error(
                line,
                "`lifecycle_routes` arms use `<state> -> \"<url>\"` (state name, `none`, or `*`)",
            )
        })?;
        let state = state.trim().to_owned();
        if state.is_empty() {
            return Err(line_error(
                line,
                "`lifecycle_routes` arm state must be a bare identifier, `none`, or `*`",
            ));
        }
        let url_text = rest.trim();
        if !url_text.starts_with('"') || !url_text.ends_with('"') || url_text.len() < 2 {
            return Err(line_error(
                line,
                "`lifecycle_routes` arm URL must be a double-quoted string",
            ));
        }
        let url = url_text[1..url_text.len() - 1].to_owned();
        arms.push(crate::ast::ResourceLifecycleRouteArmAst {
            state,
            url,
            span: Span::new(line.start, line.end),
        });
        last_end = line.end;
        i += 1;
    }
    if arms.is_empty() {
        return Err(line_error(
            header,
            "`lifecycle_routes` requires at least one `<state> -> \"<url>\"` arm",
        ));
    }
    Ok((
        crate::ast::ResourceLifecycleRoutesAst {
            arms,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_resource_lock(line: &SourceLine<'_>, rest: &str) -> Result<ResourceLock, ParseError> {
    let rest = rest.trim();
    if let Some(after) = rest.strip_prefix("optimistic") {
        let after = after.trim_start();
        let after = after.strip_prefix("version_field").ok_or_else(|| {
            line_error(
                line,
                "`lock optimistic` requires `version_field: <field>` (e.g. `lock optimistic version_field: lock_version`)",
            )
        })?;
        let after = after.trim_start();
        let after = after.strip_prefix(':').ok_or_else(|| {
            line_error(
                line,
                "`lock optimistic version_field` expects `:` followed by the column name",
            )
        })?;
        let version_field = after.trim().to_owned();
        if version_field.is_empty() {
            return Err(line_error(
                line,
                "`lock optimistic version_field:` requires a non-empty field name",
            ));
        }
        return Ok(ResourceLock::Optimistic { version_field });
    }
    match rest {
        "pessimistic" => Ok(ResourceLock::Pessimistic),
        "row_level" => Ok(ResourceLock::RowLevel),
        other => Err(line_error_owned(
            line,
            format!(
                "`lock` expects `optimistic version_field: <field>`, `pessimistic`, or `row_level` (got `{}`)",
                other
            ),
        )),
    }
}

/// Roadmap §1.5 (CL.C.2) — parse the `composite_key` block. Children
/// at `grandchild_indent`:
///
/// - `fields <a>, <b>, ...` (required, non-empty)
/// - `primary true|false` (optional; default `false`)
fn parse_resource_composite_key(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(ResourceCompositeKey, usize), ParseError> {
    let header = &lines[start];

    let mut fields: Vec<String> = Vec::new();
    let mut primary: bool = false;
    let mut saw_primary = false;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header.indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`composite_key` children use one indentation level deeper than the header",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("fields ") {
            if !fields.is_empty() {
                return Err(line_error(
                    line,
                    "duplicate `fields` line in `composite_key`",
                ));
            }
            let names: Vec<String> = rest
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            if names.is_empty() {
                return Err(line_error(
                    line,
                    "`fields` requires at least one field name (e.g. `fields order, line_number`)",
                ));
            }
            fields = names;
            last_end = line.end;
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("primary ") {
            if saw_primary {
                return Err(line_error(
                    line,
                    "duplicate `primary` line in `composite_key`",
                ));
            }
            primary = match rest.trim() {
                "true" => true,
                "false" => false,
                other => {
                    return Err(line_error_owned(
                        line,
                        format!("`primary` expects `true` or `false` (got `{}`)", other),
                    ));
                }
            };
            saw_primary = true;
            last_end = line.end;
            i += 1;
            continue;
        }
        return Err(line_error(
            line,
            "`composite_key` children are `fields <list>` and `primary true|false`",
        ));
    }

    if fields.is_empty() {
        return Err(line_error(
            header,
            "`composite_key` requires a `fields <a>, <b>, ...` child",
        ));
    }

    Ok((
        ResourceCompositeKey {
            fields,
            primary,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Split `<TypeRef> [decorators...] [required|optional|unique]
/// [<constraint>...] [= <default>] [derived from <expr>]` into
/// structured pieces. L0 #3 §10 adds the constraint axis between
/// modifiers and the default — but we peel from the right, so the
/// order doesn't matter on input. Constraint keywords (`min`, `max`,
/// `pattern`, `between`, `length`, `in`) are scanned via
/// `find_token` at depth 0 so they don't trip on parenthesised
/// decorator args.
fn split_resource_field_after(
    line: &SourceLine<'_>,
    after: &str,
) -> Result<
    (
        String,
        String,
        Option<String>,
        Option<String>,
        FieldConstraintsDecl,
    ),
    ParseError,
> {
    let after = after.trim();

    // Pull out `derived from <expr>` (always at the end).
    let (head, derived_from) = if let Some(idx) = find_token(after, " derived from ") {
        let derived = after[idx + " derived from ".len()..].trim().to_owned();
        (after[..idx].trim_end().to_owned(), Some(derived))
    } else {
        (after.to_owned(), None)
    };

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
    Ok((
        type_text,
        modifiers_text,
        default,
        derived_from,
        constraints,
    ))
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
fn extract_field_constraints(
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
            match before_keyword_after(&scan, &rest) {
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

/// Parse `<keyword> <integer> [tail...]`. Returns the parsed integer
/// and the tail after the integer (which may carry further
/// constraints or be empty).
fn parse_constraint_int(
    line: &SourceLine<'_>,
    text: &str,
    keyword: &str,
) -> Result<(i64, String), ParseError> {
    // text starts with `<keyword> ` already verified by caller.
    let rest = text.trim_start();
    let rest = rest
        .strip_prefix(keyword)
        .ok_or_else(|| {
            line_error_owned(line, format!("expected `{}` constraint keyword", keyword))
        })?
        .trim_start();
    // Take next whitespace-delimited token as the integer.
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let value_str = &rest[..end];
    let tail = rest[end..].to_owned();
    let n: i64 = value_str.parse().map_err(|_| {
        line_error_owned(
            line,
            format!(
                "`{}` constraint expects an integer, got `{}`",
                keyword, value_str
            ),
        )
    })?;
    Ok((n, tail))
}

/// Parse `pattern "<STRING>" [tail...]`. The string is delimited by
/// double quotes; embedded quotes are not supported (RE2 doesn't need
/// them in the common case — `\"` is rare).
fn parse_constraint_string(
    line: &SourceLine<'_>,
    text: &str,
    keyword: &str,
) -> Result<(String, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix(keyword)
        .ok_or_else(|| line_error_owned(line, format!("expected `{}` keyword", keyword)))?
        .trim_start();
    if !rest.starts_with('"') {
        return Err(line_error_owned(
            line,
            format!(
                "`{}` constraint expects a quoted string (e.g. `pattern \"^[a-z]+$\"`)",
                keyword
            ),
        ));
    }
    let body = &rest[1..];
    let end = body.find('"').ok_or_else(|| {
        line_error_owned(
            line,
            format!("`{}` constraint string is missing a closing `\"`", keyword),
        )
    })?;
    let value = body[..end].to_owned();
    let tail = body[end + 1..].to_owned();
    Ok((value, tail))
}

/// Parse `between <A> and <B> [tail...]`.
fn parse_constraint_between(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(i64, i64, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix("between")
        .ok_or_else(|| line_error(line, "expected `between` keyword"))?
        .trim_start();
    // Parse first integer.
    let end = rest
        .find(|c: char| c.is_whitespace())
        .ok_or_else(|| line_error(line, "`between` constraint requires `<A> and <B>`"))?;
    let lo_str = &rest[..end];
    let lo: i64 = lo_str.parse().map_err(|_| {
        line_error_owned(line, format!("`between` expects integer, got `{}`", lo_str))
    })?;
    let rest = rest[end..].trim_start();
    let rest = rest
        .strip_prefix("and")
        .ok_or_else(|| line_error(line, "`between <A> and <B>` requires the `and` keyword"))?
        .trim_start();
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let hi_str = &rest[..end];
    let hi: i64 = hi_str.parse().map_err(|_| {
        line_error_owned(line, format!("`between` expects integer, got `{}`", hi_str))
    })?;
    let tail = rest[end..].to_owned();
    Ok((lo, hi, tail))
}

/// Parse `in [a, b, c] [tail...]`. Returns the list values and the
/// tail. Quoted-string items are unquoted; bare integers stay as
/// their text form (the analyzer interprets per field type).
fn parse_constraint_in_list(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(Vec<String>, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix("in")
        .ok_or_else(|| line_error(line, "expected `in` keyword"))?
        .trim_start();
    if !rest.starts_with('[') {
        return Err(line_error(
            line,
            "`in` constraint expects a bracketed list (e.g. `in [\"a\", \"b\"]`)",
        ));
    }
    // Find matching `]`.
    let body = &rest[1..];
    let close = body
        .find(']')
        .ok_or_else(|| line_error(line, "`in` constraint list is missing a closing `]`"))?;
    let inner = &body[..close];
    let tail = body[close + 1..].to_owned();
    let values: Vec<String> = split_top_level_commas(inner)
        .into_iter()
        .map(|piece| {
            let trimmed = piece.trim();
            // Strip surrounding double quotes if present.
            if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
                trimmed[1..trimmed.len() - 1].to_owned()
            } else {
                trimmed.to_owned()
            }
        })
        .filter(|s| !s.is_empty())
        .collect();
    Ok((values, tail))
}

enum ParsedValidateConstraint {
    SanitizeHtml(String),
    Utf8Safe,
    MaxRecursion(u32),
    MaxSize(u64),
}

fn parse_constraint_validate(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(ParsedValidateConstraint, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix("validate ")
        .ok_or_else(|| line_error(line, "expected `validate <constraint>`"))?
        .trim_start();
    if let Some(inside) = rest.strip_prefix("sanitize_html(") {
        let end = inside.find(')').ok_or_else(|| {
            line_error(
                line,
                "`validate sanitize_html` profile is missing a closing `)`",
            )
        })?;
        let profile = inside[..end].trim();
        if profile.is_empty() {
            return Err(line_error(
                line,
                "`validate sanitize_html` requires a profile",
            ));
        }
        let tail = inside[end + 1..].to_owned();
        return Ok((
            ParsedValidateConstraint::SanitizeHtml(profile.to_owned()),
            tail,
        ));
    }
    if let Some(tail) = rest.strip_prefix("utf8_safe") {
        return Ok((ParsedValidateConstraint::Utf8Safe, tail.to_owned()));
    }
    if let Some(after) = rest.strip_prefix("max_recursion:") {
        let (raw, tail) = take_constraint_value(after);
        let value = raw.parse::<u32>().map_err(|_| {
            line_error_owned(
                line,
                format!("`validate max_recursion` expects u32, got `{}`", raw),
            )
        })?;
        return Ok((ParsedValidateConstraint::MaxRecursion(value), tail));
    }
    if let Some(after) = rest.strip_prefix("max_size:") {
        let (raw, tail) = take_constraint_value(after);
        let value = raw.parse::<u64>().map_err(|_| {
            line_error_owned(
                line,
                format!("`validate max_size` expects bytes as u64, got `{}`", raw),
            )
        })?;
        return Ok((ParsedValidateConstraint::MaxSize(value), tail));
    }
    Err(line_error(
        line,
        "`validate` supports `sanitize_html(<profile>)`, `utf8_safe`, `max_recursion:<n>`, or `max_size:<bytes>`",
    ))
}

fn parse_constraint_validator(
    line: &SourceLine<'_>,
    text: &str,
) -> Result<(String, String), ParseError> {
    let rest = text
        .trim_start()
        .strip_prefix("validator ")
        .ok_or_else(|| line_error(line, "expected `validator covers_pii`"))?
        .trim_start();
    let (raw, tail) = take_constraint_value(rest);
    let value = raw.strip_prefix("covers_pii:").unwrap_or(raw);
    if value != "covers_pii" && !value.starts_with("covers_pii_") {
        return Err(line_error(
            line,
            "`validator` currently supports `covers_pii` entries only",
        ));
    }
    Ok((value.to_owned(), tail))
}

fn take_constraint_value(text: &str) -> (&str, String) {
    let trimmed = text.trim_start();
    let end = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());
    (&trimmed[..end], trimmed[end..].to_owned())
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

// -----------------------------------------------------------------------------
// Phase L Tier 4d — `query.list` / `query.lookup` / `query.sql` /
// `query.view` and
// `record <Name>` block parsers.
//
// Queries live inside `domain` at indent 4. Children at indent 6 are
// `policy`, `modifier`, `params`, `scope`/`scope override`, `filters`,
// `search`, `cache`, `paginate`, `order`, `returns`, `sql`. Grandchildren
// (typed param slots, `scope override` body) at indent 8.
// -----------------------------------------------------------------------------

fn parse_query_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    if let Some(rest) = trimmed.strip_prefix("query.lookup ") {
        return parse_query_lookup_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.list ") {
        return parse_query_list_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.sql ") {
        return parse_query_sql_decl(lines, start, rest);
    }
    if let Some(rest) = trimmed.strip_prefix("query.view ") {
        return parse_query_view_decl(lines, start, rest);
    }
    Err(line_error(
        header,
        "query header must be `query.list <name>`, `query.lookup <name> by ...`, `query.sql <name>`, or `query.view <name>`",
    ))
}

fn parse_query_lookup_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    // Two shapes accepted today:
    //   - inline: `<name> by <field>: <Type>` (Cut A canonical).
    //   - block: `<name>` with `params` / `filters` / `policy` children.
    let (name, inline_key) = if let Some((name, after)) = rest.split_once(" by ") {
        (
            name.trim().to_owned(),
            Some(parse_lookup_key(header, after)?),
        )
    } else {
        (rest.to_owned(), None)
    };
    if name.is_empty() {
        return Err(line_error(header, "`query.lookup` requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    // `filters` lines are captured for cross-check but not lowered to
    // typed keys today; Cut A's contract is `keys` (from `by ...`) so
    // multi-key block lookups keep their filters in the AST sidecar
    // while IR uses `keys` from the inline form.
    let mut filters: Vec<String> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`query.lookup` body children use one indentation level deeper than the header",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "filters" {
            let (collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            filters = collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`query.lookup` children are `policy`, `params`, `filters`, or `gate behind/quota plan.*`",
            ));
        }
    }
    // Build the IR-facing keys list. Inline form contributes one
    // explicit key; block form synthesises a key per param so the IR
    // shape stays consistent.
    let keys: Vec<LookupKey> = if let Some(key) = inline_key {
        vec![key]
    } else {
        params
            .iter()
            .map(|p| LookupKey {
                name: p.name.clone(),
                type_text: p.type_text.clone(),
                span: p.span,
            })
            .collect()
    };
    Ok((
        QueryDecl::Lookup(LookupQueryDecl {
            name,
            public_contract: None,
            policy,
            policy_expr,
            keys,
            filters,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}

fn parse_lookup_key(line: &SourceLine<'_>, rest: &str) -> Result<LookupKey, ParseError> {
    let (name, type_text) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires `<field>: <Type>`",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires a field name before `:`",
        ));
    }
    let type_text = type_text.trim();
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires a type after `:`",
        ));
    }
    Ok(LookupKey {
        name: name.to_owned(),
        type_text: type_text.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

fn parse_query_list_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let name = rest.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(header, "`query.list` requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut modifier: Option<String> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    let mut scope_override = false;
    let mut scope_reason: Option<String> = None;
    let mut scope_assignments: Vec<String> = Vec::new();
    let mut scope_lines: Vec<String> = Vec::new();
    let mut filters: Vec<String> = Vec::new();
    let mut search: Option<QuerySearch> = None;
    let mut cache: Vec<String> = Vec::new();
    let mut cache_profile_ref: Option<String> = None;
    let mut paginate: Option<u32> = None;
    let mut order: Vec<String> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`query.list` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("modifier ") {
            modifier = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope override" {
            scope_override = true;
            let (reason, assignments, next) =
                parse_query_scope_override_block(lines, i, grandchild_indent)?;
            scope_reason = reason;
            scope_assignments = assignments;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            scope_lines = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "filters" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            filters = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("search ") {
            let (parsed, next) = parse_query_search(lines, i, rest, grandchild_indent)?;
            search = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "cache" {
            // Inline shape: `cache` followed by indented `key`/`ttl`/...
            if cache_profile_ref.is_some() {
                return Err(line_error(
                    line,
                    "`query.list` may declare either an inline `cache` block or a `cache <profile>` reference, not both",
                ));
            }
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            cache = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("cache ") {
            // Cache bucket cycle (CL.C.3) — `cache <profile_name>` reference
            // form. Single-line shape pointing at a feature-level
            // `cache <name>` profile.
            let name = rest.trim();
            if name.is_empty() {
                return Err(line_error(
                    line,
                    "`cache <profile>` requires a profile name (declare it as a feature-level `cache <name>` block)",
                ));
            }
            if !cache.is_empty() {
                return Err(line_error(
                    line,
                    "`query.list` may declare either an inline `cache` block or a `cache <profile>` reference, not both",
                ));
            }
            if cache_profile_ref.is_some() {
                return Err(line_error(
                    line,
                    "`query.list` may declare `cache <profile>` only once",
                ));
            }
            cache_profile_ref = Some(name.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("paginate ") {
            paginate = Some(rest.trim().parse::<u32>().map_err(|_| {
                line_error(line, "`paginate` requires a positive integer page size")
            })?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("order ") {
            order.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`query.list` children are `policy`, `modifier`, `params`, `scope`/`scope override`, `filters`, `search`, `cache`, `paginate`, `order`, or `gate behind/quota plan.*`",
            ));
        }
    }

    Ok((
        QueryDecl::List(ListQueryDecl {
            name,
            public_contract: None,
            policy,
            policy_expr,
            modifier,
            params,
            scope_override,
            scope_reason,
            scope_assignments,
            scope_lines,
            filters,
            search,
            cache,
            cache_profile_ref,
            paginate,
            order,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}

fn parse_query_sql_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    parse_sql_backed_query_decl(lines, start, rest, SqlQueryKind::Sql)
}

fn parse_query_view_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    parse_sql_backed_query_decl(lines, start, rest, SqlQueryKind::View)
}

fn parse_sql_backed_query_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    kind: SqlQueryKind,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let name = rest.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a name",
                SqlQueryKind::View => "`query.view` requires a name",
            },
        ));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    let mut scope_lines: Vec<String> = Vec::new();
    let mut returns: Option<String> = None;
    let mut sql_path: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                match kind {
                    SqlQueryKind::Sql => {
                        "`query.sql` body children use one indentation level deeper than the header"
                    }
                    SqlQueryKind::View => {
                        "`query.view` body children use one indentation level deeper than the header"
                    }
                },
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            scope_lines = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("returns ") {
            returns = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("sql ") {
            if kind == SqlQueryKind::View {
                return Err(line_error(
                    line,
                    "`query.view` uses `source @file.<name>.sql`; `sql \"./<path>.sql\"` is reserved for `query.sql`",
                ));
            }
            sql_path = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("source ") {
            if kind != SqlQueryKind::View {
                return Err(line_error(
                    line,
                    "`source @file.<name>.sql` is only valid on `query.view`; use `sql \"./<path>.sql\"` on `query.sql`",
                ));
            }
            let source = rest.trim();
            if !source.starts_with("@file.") || !source.ends_with(".sql") {
                return Err(line_error(
                    line,
                    "`query.view source` must be shaped `@file.<name>.sql`",
                ));
            }
            sql_path = Some(source.to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                match kind {
                    SqlQueryKind::Sql => {
                        "`query.sql` children are `policy`, `params`, `scope`, `returns`, `sql`, or `gate behind/quota plan.*`"
                    }
                    SqlQueryKind::View => {
                        "`query.view` children are `policy`, `returns`, `source`, `params`, `scope`, or `gate behind/quota plan.*`"
                    }
                },
            ));
        }
    }

    let returns = returns.ok_or_else(|| {
        line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a `returns <Type>` declaration",
                SqlQueryKind::View => "`query.view` requires a `returns <Type>` declaration",
            },
        )
    })?;
    let sql_path = sql_path.ok_or_else(|| {
        line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a `sql \"./<path>.sql\"` declaration",
                SqlQueryKind::View => {
                    "`query.view` requires a `source @file.<name>.sql` declaration"
                }
            },
        )
    })?;
    Ok((
        QueryDecl::Sql(SqlQueryDecl {
            name,
            kind,
            public_contract: None,
            policy,
            policy_expr,
            params,
            scope_lines,
            returns,
            sql_path,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}

fn parse_query_params_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(Vec<CommandInputSlot>, usize), ParseError> {
    let mut slots: Vec<CommandInputSlot> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "query `params` children use the deepest indentation level",
            ));
        }
        let (name_part, type_part) = trimmed.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "query `params` slots use `<name>: <Type> [required|optional]`",
            )
        })?;
        let name = name_part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "query `params` slot requires a name before `:`",
            ));
        }
        // L0 #3 §10 — query params share the inline-constraint catalog
        // with command inputs / resource fields.
        let (after_constraints, constraints) = extract_field_constraints(line, type_part.trim())?;
        let (type_text, required, optional) =
            split_command_input_modifiers(after_constraints.trim());
        slots.push(CommandInputSlot {
            name: name.to_owned(),
            type_text,
            required,
            optional,
            constraints,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((slots, i))
}

fn parse_query_scope_override_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(Option<String>, Vec<String>, usize), ParseError> {
    let mut reason: Option<String> = None;
    let mut assignments: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`scope override` children use the deepest indentation level",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("reason ") {
            reason = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            assignments.push(trimmed.to_owned());
        }
        i += 1;
    }
    Ok((reason, assignments, i))
}

fn parse_query_indented_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> (Vec<String>, usize) {
    let mut out: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        out.push(trimmed.to_owned());
        i += 1;
    }
    (out, i)
}

fn parse_query_search(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    grandchild_indent: usize,
) -> Result<(QuerySearch, usize), ParseError> {
    let header = &lines[start];
    let (source, fields) = rest.split_once(" over ").ok_or_else(|| {
        line_error(
            header,
            "`search` requires `<path> over <field>, <field>` (e.g. `search params.search over name, email`)",
        )
    })?;
    let fields: Vec<String> = fields
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    let mut mode: Option<String> = None;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`search` children use the deepest indentation level",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("mode ") {
            mode = Some(rest.trim().to_owned());
            i += 1;
        } else {
            return Err(line_error(line, "`search` children are `mode <kind>` only"));
        }
    }
    Ok((
        QuerySearch {
            source: source.trim().to_owned(),
            fields,
            mode,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}

fn parse_record_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(RecordDecl, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let name = trimmed
        .strip_prefix("record ")
        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
        .ok_or_else(|| line_error(header, "record header must be `record <Name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "record header requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut fields: Vec<ResourceFieldDecl> = Vec::new();
    let mut discriminator_field: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`record` body children use one indentation level deeper than the header",
            ));
        }
        if trimmed.contains(':') {
            let (field, next) = parse_resource_field_decl(lines, i, grandchild_indent)?;
            if field.type_text.contains("discriminator") {
                discriminator_field = Some(field.name.clone());
            }
            fields.push(field);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "`record` children are `<field>: <Type>` lines only",
            ));
        }
    }

    Ok((
        RecordDecl {
            name,
            public_contract: None,
            fields,
            discriminator_field,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

// -----------------------------------------------------------------------------
// Phase L — `auth` block parser
//
// The `auth` header sits at AGENT_INDENT_FEATURE_CHILD (2 spaces). Its
// direct children — `identity`, `password`, `sessions`, `mfa`, `oauth` —
// live at AGENT_INDENT_AGENT_CHILD (4 spaces). Grandchildren (the named
// options inside `password`/`sessions`/`mfa`/`oauth`) live at
// AGENT_INDENT_GRANDCHILD (6 spaces). This mirrors `parse_agent` so an
// LLM authoring auth has the same indentation contract as authoring an
// agent.
// -----------------------------------------------------------------------------

fn parse_auth(lines: &[SourceLine<'_>], start: usize) -> Result<(Auth, usize), ParseError> {
    let header = &lines[start];
    let mut identity: Option<AuthIdentity> = None;
    let mut password: Option<AuthPassword> = None;
    let mut sessions: Option<AuthSessions> = None;
    let mut mfa: Option<AuthMfa> = None;
    let mut oauth: Vec<AuthOAuthProvider> = Vec::new();
    let mut pending_identity_contract: Option<PublicContractDeclAst> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`auth` body children use four-space indentation",
            ));
        }

        // Cross-feature contract: `public contract identity as v<N>`
        // immediately above the `identity <Resource>.<field>` line per
        // docs/proposals/cross-feature-contracts.md §3.5 + §5.3.
        if let Some(contract) = parse_auth_identity_contract_line(line)? {
            if pending_identity_contract.is_some() {
                return Err(line_error(
                    line,
                    "duplicate `public contract identity` line; only one may precede each `identity` declaration",
                ));
            }
            if identity.is_some() {
                return Err(line_error(
                    line,
                    "`public contract identity` must appear ABOVE the `identity` line, not below",
                ));
            }
            pending_identity_contract = Some(contract);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("identity ") {
            if identity.is_some() {
                return Err(line_error(
                    line,
                    "`auth identity` may be declared at most once",
                ));
            }
            let field = rest.trim();
            if field.is_empty() {
                return Err(line_error(
                    line,
                    "`auth identity` requires `<Resource>.<field>`",
                ));
            }
            if !field.contains('.') {
                return Err(line_error(
                    line,
                    "`auth identity` requires `<Resource>.<field>` (dot-qualified)",
                ));
            }
            identity = Some(AuthIdentity {
                field: field.to_owned(),
                public_contract: pending_identity_contract.take(),
                span: Span::new(line.start, line.end),
            });
            last_end = line.end;
            i += 1;
        } else if trimmed == "password" {
            if password.is_some() {
                return Err(line_error(
                    line,
                    "`auth password` may be declared at most once",
                ));
            }
            let (parsed, next) = parse_auth_password(lines, i)?;
            password = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "sessions" {
            if sessions.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions` may be declared at most once",
                ));
            }
            let (parsed, next) = parse_auth_sessions(lines, i)?;
            sessions = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("mfa ") {
            if mfa.is_some() {
                return Err(line_error(line, "`auth mfa` may be declared at most once"));
            }
            let method = rest.trim();
            if method.is_empty() {
                return Err(line_error(
                    line,
                    "`auth mfa` requires a method id (`totp`, `sms`, `webauthn`, ...)",
                ));
            }
            let (parsed, next) = parse_auth_mfa(lines, i, method.to_owned())?;
            mfa = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("oauth ") {
            let provider = rest.trim();
            if provider.is_empty() {
                return Err(line_error(
                    line,
                    "`auth oauth` requires a provider id (`google`, `github`, ...)",
                ));
            }
            let (parsed, next) = parse_auth_oauth(lines, i, provider.to_owned())?;
            oauth.push(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "`auth` children are `identity`, `password`, `sessions`, `mfa`, or `oauth`",
            ));
        }
    }

    let identity = identity.ok_or_else(|| {
        line_error(
            header,
            "`auth` requires an `identity <Resource>.<field>` declaration",
        )
    })?;

    Ok((
        Auth {
            identity,
            password,
            sessions,
            mfa,
            oauth,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_password(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthPassword, usize), ParseError> {
    let header = &lines[start];
    let mut algorithm: Option<String> = None;
    let mut hash: Option<String> = None;
    let mut verify: Option<String> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth password` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("algorithm ") {
            algorithm = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("hash ") {
            hash = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("verify ") {
            verify = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
        } else {
            return Err(line_error(
                line,
                "`auth password` children are `algorithm`, `hash`, `verify`, or `rate_limit`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let algorithm = algorithm.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires an `algorithm <name>` declaration",
        )
    })?;
    let hash = hash.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires a `hash @fn.<name>` declaration",
        )
    })?;
    let verify = verify.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires a `verify @fn.<name>` declaration",
        )
    })?;

    Ok((
        AuthPassword {
            algorithm,
            hash,
            verify,
            rate_limit,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_sessions(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthSessions, usize), ParseError> {
    let header = &lines[start];
    let mut resource: Option<String> = None;
    let mut ttl: Option<String> = None;
    let mut refresh: Option<bool> = None;
    let mut access_ttl: Option<AuthDurationClause> = None;
    let mut rotation: Option<AuthSessionRotation> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = strip_inline_comment(line.text.trim_start()).trim_end();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth sessions` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("resource ") {
            resource = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("ttl ") {
            ttl = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("refresh ") {
            refresh = Some(
                parse_lzx_bool(rest.trim())
                    .ok_or_else(|| line_error(line, "`refresh` must be `true` or `false`"))?,
            );
        } else if let Some(rest) = trimmed.strip_prefix("access_ttl ") {
            if access_ttl.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions` may declare `access_ttl` at most once",
                ));
            }
            access_ttl = Some(AuthDurationClause {
                value: unquote_lzx_value(rest.trim()).to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if trimmed == "rotation" {
            if rotation.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions` may declare `rotation` at most once",
                ));
            }
            let (parsed, next) = parse_auth_session_rotation(lines, i)?;
            rotation = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        } else {
            return Err(line_error(
                line,
                "`auth sessions` children are `resource`, `ttl`, `refresh`, `access_ttl`, or `rotation`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let resource = resource.ok_or_else(|| {
        line_error(
            header,
            "`auth sessions` requires a `resource <Name>` declaration",
        )
    })?;
    let ttl = ttl.ok_or_else(|| {
        line_error(
            header,
            "`auth sessions` requires a `ttl \"<duration>\"` declaration",
        )
    })?;

    Ok((
        AuthSessions {
            resource,
            ttl,
            refresh: refresh.unwrap_or(false),
            access_ttl,
            rotation,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_session_rotation(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthSessionRotation, usize), ParseError> {
    let header = &lines[start];
    let mut refresh_ttl: Option<AuthDurationClause> = None;
    let mut grace: Option<AuthDurationClause> = None;
    let mut theft_detection_action: Option<AuthTheftDetectionActionClause> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = strip_inline_comment(line.text.trim_start()).trim_end();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_GRANDCHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GREAT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth sessions rotation` children use eight-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("refresh_ttl ") {
            if refresh_ttl.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions rotation` may declare `refresh_ttl` at most once",
                ));
            }
            refresh_ttl = Some(AuthDurationClause {
                value: unquote_lzx_value(rest.trim()).to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("grace ") {
            if grace.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions rotation` may declare `grace` at most once",
                ));
            }
            grace = Some(AuthDurationClause {
                value: unquote_lzx_value(rest.trim()).to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("theft_detection_action ") {
            if theft_detection_action.is_some() {
                return Err(line_error(
                    line,
                    "`auth sessions rotation` may declare `theft_detection_action` at most once",
                ));
            }
            theft_detection_action = Some(AuthTheftDetectionActionClause {
                action: parse_auth_theft_detection_action(line, rest)?,
                span: Span::new(line.start, line.end),
            });
        } else {
            return Err(line_error(
                line,
                "`auth sessions rotation` children are `refresh_ttl`, `grace`, or `theft_detection_action`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        AuthSessionRotation {
            refresh_ttl,
            grace,
            theft_detection_action,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_theft_detection_action(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<AuthTheftDetectionAction, ParseError> {
    let mut parts = rest.split_whitespace();
    let action = parts.next().ok_or_else(|| {
        line_error(
            line,
            "`theft_detection_action` requires `revoke_session_family` or `revoke_user`",
        )
    })?;
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`theft_detection_action` must be a single closed-catalog verb",
        ));
    }
    match action {
        "revoke_session_family" => Ok(AuthTheftDetectionAction::RevokeSessionFamily),
        "revoke_user" => Ok(AuthTheftDetectionAction::RevokeUser),
        other => Err(line_error_owned(
            line,
            format!(
                "unknown `theft_detection_action` `{other}` - closed catalog is `revoke_session_family` or `revoke_user`"
            ),
        )),
    }
}

fn parse_auth_mfa(
    lines: &[SourceLine<'_>],
    start: usize,
    method: String,
) -> Result<(AuthMfa, usize), ParseError> {
    let header = &lines[start];
    let mut enroll: Option<String> = None;
    let mut verify: Option<String> = None;
    let mut adapter: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth mfa` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("enroll ") {
            enroll = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("verify ") {
            verify = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("adapter ") {
            adapter = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`auth mfa` children are `enroll`, `verify`, or `adapter`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let enroll = enroll.ok_or_else(|| {
        line_error(
            header,
            "`auth mfa` requires an `enroll @fn.<name>` declaration",
        )
    })?;
    let verify = verify.ok_or_else(|| {
        line_error(
            header,
            "`auth mfa` requires a `verify @validator.<name>` declaration",
        )
    })?;

    Ok((
        AuthMfa {
            method,
            enroll,
            verify,
            adapter,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_auth_oauth(
    lines: &[SourceLine<'_>],
    start: usize,
    provider: String,
) -> Result<(AuthOAuthProvider, usize), ParseError> {
    let header = &lines[start];
    let mut adapter: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth oauth` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("adapter ") {
            adapter = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(line, "`auth oauth` children are `adapter`"));
        }

        last_end = line.end;
        i += 1;
    }

    let adapter = adapter.ok_or_else(|| {
        line_error(
            header,
            "`auth oauth` requires an `adapter @adapter.<name>` declaration",
        )
    })?;

    Ok((
        AuthOAuthProvider {
            provider,
            adapter,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

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

fn parse_job(lines: &[SourceLine<'_>], start: usize) -> Result<(Job, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("job ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "job header must be `job <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "job header requires a name"));
    }

    let mut trigger: Option<JobTrigger> = None;
    let mut queue: Option<String> = None;
    let mut tenant_from: Option<String> = None;
    let mut fanout: Option<JobFanout> = None;
    let mut idempotency_by: Option<String> = None;
    let mut retry: Option<JobRetry> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut timeout: Option<String> = None;
    let mut external_calls: Vec<JobExternalCall> = Vec::new();
    let mut handler: Option<JobHandler> = None;
    let mut declarative_target: Option<TargetExprDecl> = None;
    let mut declarative_lets: Vec<LetBindingDecl> = Vec::new();
    let mut declarative_effect: Option<CommandEffectDecl> = None;
    // `emits <event>` lines (with their optional indented payload child
    // block silently skipped — Tier 3 IR only carries event names).
    let mut emits: Vec<String> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "job body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("trigger ") {
            trigger = Some(parse_job_trigger(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("queue ") {
            queue = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            tenant_from = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("fanout ") {
            fanout = Some(parse_job_fanout(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            retry = Some(parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("calls ") {
            let (call, next) = parse_external_call(lines, i, rest)?;
            external_calls.push(call);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(parse_handler_line(rest));
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("target ") {
            declarative_target = Some(parse_target_expr(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("let ") {
            declarative_lets.push(parse_let_binding(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("creates ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Creates, rest)?;
            declarative_effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("updates ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Updates, rest)?;
            declarative_effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("deletes ") {
            let (parsed, next) =
                parse_command_effect(lines, i, CommandEffectKindDecl::Deletes, rest)?;
            declarative_effect = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            // Strip the optional ` from creates`/`from updates`/`from deletes`
            // suffix and consume any indented payload child block. The IR
            // only carries event names today; the child assignments stay
            // on the surface for Tier 3 doctor diagnostics that walk
            // source text directly.
            let raw = rest.trim();
            let name = if let Some(n) = raw.strip_suffix(" from creates") {
                n.trim()
            } else if let Some(n) = raw.strip_suffix(" from updates") {
                n.trim()
            } else if let Some(n) = raw.strip_suffix(" from deletes") {
                n.trim()
            } else {
                raw
            };
            emits.push(name.to_owned());
            last_end = line.end;
            i += 1;
            // Skip indented child lines (`<field> = <expr>`).
            while i < lines.len() {
                let child = &lines[i];
                let child_trim = child.text.trim_start();
                if is_trivia(child_trim) {
                    i += 1;
                    continue;
                }
                if child.indent <= AGENT_INDENT_AGENT_CHILD {
                    break;
                }
                last_end = child.end;
                i += 1;
            }
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass; tolerate here.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "job children are `trigger`, `queue`, `tenant_from`, `fanout`, `idempotency by`, `retry`, `policy`, `timeout`, `calls`, `handler`, `target`, `let`, `updates`/`creates`/`deletes`, `emits`, or `gate behind/quota plan.*`",
            ));
        }
    }

    let trigger = trigger.ok_or_else(|| {
        line_error(
            header,
            "`job` requires a `trigger event ...` or `trigger schedule ...` declaration",
        )
    })?;

    let body = if let Some(handler) = handler {
        JobBody::Handler(handler)
    } else if declarative_target.is_some() || declarative_effect.is_some() {
        JobBody::Declarative(JobDeclarativeTyped {
            target: declarative_target,
            lets: declarative_lets,
            effect: declarative_effect,
        })
    } else {
        JobBody::None
    };

    Ok((
        Job {
            name,
            trigger,
            queue,
            tenant_from,
            fanout,
            idempotency_by,
            retry,
            policy,
            policy_expr,
            timeout,
            external_calls,
            body,
            emits,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

pub(super) fn parse_job_trigger(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<JobTrigger, ParseError> {
    let rest = rest.trim();
    if let Some(ev) = rest.strip_prefix("event ") {
        let ev = ev.trim();
        if ev.is_empty() {
            return Err(line_error(line, "`trigger event` requires an event name"));
        }
        return Ok(JobTrigger::Event(ev.to_owned()));
    }
    if let Some(cron) = rest.strip_prefix("schedule ") {
        let cron = cron.trim();
        if cron.is_empty() {
            return Err(line_error(
                line,
                "`trigger schedule` requires a quoted cron expression",
            ));
        }
        return Ok(JobTrigger::Schedule(unquote_lzx_value(cron).to_owned()));
    }
    Err(line_error(
        line,
        "`trigger` requires `event <name>` or `schedule \"<cron>\"`",
    ))
}

fn parse_job_fanout(line: &SourceLine<'_>, rest: &str) -> Result<JobFanout, ParseError> {
    let rest = rest.trim();
    let (scope, axis) = rest.split_once(' ').ok_or_else(|| {
        line_error(
            line,
            "`fanout` requires `<scope> <axis>`, e.g. `fanout tenants org`",
        )
    })?;
    Ok(JobFanout {
        scope: scope.to_owned(),
        axis: axis.trim().to_owned(),
    })
}

pub(super) fn parse_job_retry(line: &SourceLine<'_>, rest: &str) -> Result<JobRetry, ParseError> {
    let rest = rest.trim();
    let (count_str, tail) = rest.split_once(' ').ok_or_else(|| {
        line_error(
            line,
            "`retry` requires `<count> backoff <strategy>` (e.g. `retry 3 backoff exponential`)",
        )
    })?;
    let count = count_str
        .parse::<u32>()
        .map_err(|_| line_error(line, "retry count must be a non-negative integer"))?;
    let tail = tail.trim();
    let backoff = tail.strip_prefix("backoff ").ok_or_else(|| {
        line_error(
            line,
            "`retry` requires `<count> backoff <strategy>` (e.g. `retry 3 backoff exponential`)",
        )
    })?;
    Ok(JobRetry {
        count,
        backoff: backoff.trim().to_owned(),
    })
}

fn parse_handler_line(rest: &str) -> JobHandler {
    let rest = rest.trim();
    // `"./path.go" returns Type` — split before the unquoted `returns`.
    let (path_part, returns_part) = if let Some(idx) = rest.find("\" returns ") {
        let end = idx + 1; // include closing quote
        (
            rest[..end].to_owned(),
            Some(rest[end + " returns ".len()..].trim().to_owned()),
        )
    } else {
        (rest.to_owned(), None)
    };
    JobHandler {
        path: unquote_lzx_value(path_part.trim()).to_owned(),
        returns: returns_part,
    }
}

fn parse_external_call(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
) -> Result<(JobExternalCall, usize), ParseError> {
    let header = &lines[start];
    let head = head_rest.trim();
    let (slot, op) = head.split_once('.').ok_or_else(|| {
        line_error(
            header,
            "`calls` requires `<slot>.<op>` (e.g. `calls crm.upsert_customer`)",
        )
    })?;
    let mut args: Vec<JobExternalCallArg> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`calls` argument lines use six-space indentation",
            ));
        }

        let (name, value) = trimmed
            .split_once('=')
            .ok_or_else(|| line_error(line, "`calls` argument lines must use `<name> = <expr>`"))?;
        args.push(JobExternalCallArg {
            name: name.trim().to_owned(),
            value: value.trim().to_owned(),
            span: Span::new(line.start, line.end),
        });
        last_end = line.end;
        i += 1;
    }

    Ok((
        JobExternalCall {
            slot: slot.trim().to_owned(),
            op: op.trim().to_owned(),
            args,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Migrations bucket cycle Route C — `tenant_migration <name>` block
/// parser. Body shape is closed: `target query.*|command.*`, `axis <name>`,
/// `idempotency <path>`, `retry`, `timeout`, `handler`. The older
/// `target tenants <axis>` and `idempotency by <path>` spellings remain
/// accepted for compatibility with existing fixtures.
fn parse_tenant_migration(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(TenantMigration, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("tenant_migration ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| {
            line_error(
                header,
                "tenant_migration header must be `tenant_migration <name>`",
            )
        })?;
    if name.is_empty() {
        return Err(line_error(
            header,
            "tenant_migration header requires a name",
        ));
    }

    let mut target_ref: Option<String> = None;
    let mut target_axis: Option<String> = None;
    let mut legacy_target_tenants = false;
    let mut idempotency_by: Option<String> = None;
    let mut retry: Option<JobRetry> = None;
    let mut timeout: Option<String> = None;
    let mut handler: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "tenant_migration body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("target tenants ") {
            target_axis = Some(rest.trim().to_owned());
            legacy_target_tenants = true;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("target ") {
            let target = rest.trim();
            if target.is_empty() {
                return Err(line_error(
                    line,
                    "`target` requires `query.<name>` or `command.<name>`",
                ));
            }
            target_ref = Some(target.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("axis ") {
            let axis = rest.trim();
            if axis.is_empty() {
                return Err(line_error(line, "`axis` requires a tenant axis name"));
            }
            target_axis = Some(axis.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            retry = Some(parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
            timeout = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "tenant_migration children are `target query.<name>|command.<name>`, `axis <name>`, `idempotency <path>`, `retry`, `timeout`, or `handler`",
            ));
        }
    }

    let target_axis = target_axis
        .ok_or_else(|| line_error(header, "`tenant_migration` requires `axis <name>`"))?;
    if target_ref.is_none() && !legacy_target_tenants {
        return Err(line_error(
            header,
            "`tenant_migration` requires `target query.<name>` or `target command.<name>`",
        ));
    }
    let handler = handler
        .ok_or_else(|| line_error(header, "`tenant_migration` requires `handler \"<path>\"`"))?;

    Ok((
        TenantMigration {
            name,
            target_ref,
            target_axis,
            idempotency_by,
            retry,
            timeout,
            handler,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_webhook(lines: &[SourceLine<'_>], start: usize) -> Result<(Webhook, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("webhook ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "webhook header must be `webhook <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "webhook header requires a name"));
    }

    let mut route: Option<String> = None;
    let mut verify: Option<WebhookVerify> = None;
    let mut tenant_from: Option<String> = None;
    let mut scope_global: Option<crate::ast::WebhookScopeGlobal> = None;
    let mut idempotency_by: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut handler: Option<WebhookHandler> = None;
    let mut emits: Vec<String> = Vec::new();
    let mut emits_predicates: Vec<Option<String>> = Vec::new();
    let mut payload_from: Option<String> = None;
    let mut replay: Option<WebhookReplay> = None;
    let mut dlq: Option<WebhookDlq> = None;
    let mut retry: Option<JobRetry> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "webhook body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("path ") {
            route = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("verify ") {
            let (parsed, next) = parse_webhook_verify(lines, i, rest)?;
            verify = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            tenant_from = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "scope global" || trimmed.starts_with("scope global ") {
            // Closes WAR-VOCAB-WEBHOOK-01. `scope global` followed by
            // a required `reason "..."` child line — for cases where
            // the provider doesn't send a tenant key in the payload
            // and the handler reconciles tenant elsewhere (e.g.
            // external_reference lookup). Reason is captured for
            // audit + doctor surfaces so the escape is explicit.
            let scope_line = line;
            let mut reason_text: Option<String> = None;
            let mut next = i + 1;
            while next < lines.len() {
                let next_line = &lines[next];
                let next_trim = next_line.text.trim_start();
                if is_trivia(next_trim) {
                    next += 1;
                    continue;
                }
                if next_line.indent <= AGENT_INDENT_AGENT_CHILD {
                    break;
                }
                if let Some(reason_rest) = next_trim.strip_prefix("reason ") {
                    let raw = reason_rest.trim();
                    let unquoted = unquote_lzx_value(raw);
                    if unquoted.is_empty() {
                        return Err(line_error(
                            next_line,
                            "`scope global` requires non-empty `reason \"...\"`",
                        ));
                    }
                    reason_text = Some(unquoted.to_owned());
                    last_end = next_line.end;
                    next += 1;
                    continue;
                }
                return Err(line_error(
                    next_line,
                    "`scope global` child must be `reason \"...\"`",
                ));
            }
            let reason = reason_text.ok_or_else(|| {
                line_error(
                    scope_line,
                    "`scope global` requires a `reason \"...\"` child explaining why the webhook escapes tenant_from",
                )
            })?;
            scope_global = Some(crate::ast::WebhookScopeGlobal {
                reason,
                span: Span {
                    start: scope_line.start,
                    end: last_end,
                },
            });
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            let handler_job = parse_handler_line(rest);
            handler = Some(WebhookHandler {
                path: handler_job.path,
                returns: handler_job.returns,
            });
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            // B5 framework gap 2 — single-line `emits <event> [when
            // <predicate>]` shape. Recognises the optional `when`
            // suffix and splits it from the event name. The flat
            // shape (no `when`) keeps the existing behaviour.
            let (event_name, predicate) = split_emits_when(rest.trim());
            if event_name.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            emits.push(event_name.to_owned());
            emits_predicates.push(predicate.map(str::to_owned));
            last_end = line.end;
            i += 1;
        } else if trimmed == "emits" {
            // B5 framework gap 2 — block-form `emits` with one
            // `<event> [when <predicate>]` per line. Each child line
            // sits one indentation level deeper than the header.
            let block_indent = line.indent + 2;
            i += 1;
            while i < lines.len() {
                let child = &lines[i];
                let child_trim = child.text.trim_start();
                if is_trivia(child_trim) {
                    i += 1;
                    continue;
                }
                if child.indent < block_indent {
                    break;
                }
                if child.indent != block_indent {
                    return Err(line_error(
                        child,
                        "`emits` block children use one indentation level deeper than the header",
                    ));
                }
                let (event_name, predicate) = split_emits_when(child_trim);
                if event_name.is_empty() {
                    return Err(line_error(
                        child,
                        "`emits` block entry requires an event name",
                    ));
                }
                emits.push(event_name.to_owned());
                emits_predicates.push(predicate.map(str::to_owned));
                last_end = child.end;
                i += 1;
            }
        } else if let Some(rest) = trimmed.strip_prefix("payload from ") {
            // `payload from webhook_events.<name>` — the `webhook_events.`
            // prefix is mandatory at the surface so the catalog is
            // obvious to a cold-reading author/LLM; only the suffix is
            // kept in the AST.
            let raw = rest.trim();
            let suffix = raw.strip_prefix("webhook_events.").ok_or_else(|| {
                line_error(
                    line,
                    "`payload from` requires `webhook_events.<name>` (catalog prefix is mandatory)",
                )
            })?;
            if suffix.is_empty() {
                return Err(line_error(
                    line,
                    "`payload from webhook_events.` requires an entry name",
                ));
            }
            payload_from = Some(suffix.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("replay") {
            // Two surface forms:
            //   `replay allow within "24h"`  (single-line short form)
            //   `replay\n  allow\n  within "24h"\n  dedupe by ...`
            // dispatched by inspecting the remainder of the header.
            let (parsed, next) = parse_webhook_replay(lines, i, rest)?;
            replay = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("dlq") {
            let (parsed, next) = parse_webhook_dlq(lines, i, rest)?;
            dlq = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            retry = Some(parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass; tolerate here.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "webhook children are `path`, `verify`, `tenant_from`, `scope global` + `reason`, `idempotency by`, `policy`, `handler`, `emits`, `payload from`, `replay`, `retry`, `dlq`, or `gate behind/quota plan.*`",
            ));
        }
    }

    let route = route
        .ok_or_else(|| line_error(header, "`webhook` requires a `path \"/...\"` declaration"))?;
    let verify = verify.ok_or_else(|| {
        line_error(
            header,
            "`webhook` requires a `verify hmac <alg>` declaration",
        )
    })?;

    // B5 framework gap 2 — only keep `emits_predicates` when at least
    // one entry carries a `when` clause; an all-None vec is the legacy
    // shape and serialises as empty.
    let emits_predicates = if emits_predicates.iter().any(|p| p.is_some()) {
        emits_predicates
    } else {
        Vec::new()
    };

    Ok((
        Webhook {
            name,
            route,
            verify,
            tenant_from,
            scope_global,
            idempotency_by,
            policy,
            policy_expr,
            handler,
            emits,
            emits_predicates,
            payload_from,
            replay,
            dlq,
            retry,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// B5 framework gap 2 — split an `emits <event> [when <predicate>]`
/// payload into its event name + optional predicate. The predicate
/// (everything after the ` when ` token) is trimmed but kept verbatim;
/// the analyzer is responsible for the typed lift.
fn split_emits_when(raw: &str) -> (&str, Option<&str>) {
    let text = raw.trim();
    // Match against ` when ` (with surrounding whitespace) so an
    // event name that contains `when` substring is not split. Use a
    // manual scan because `str::split_once` doesn't accept a closure
    // for whole-word boundaries.
    let mut search_from = 0;
    let bytes = text.as_bytes();
    while let Some(rel) = text[search_from..].find("when") {
        let abs = search_from + rel;
        let before_ok = abs > 0 && bytes[abs - 1].is_ascii_whitespace();
        let after_pos = abs + "when".len();
        let after_ok = after_pos < bytes.len() && bytes[after_pos].is_ascii_whitespace();
        if before_ok && after_ok {
            let head = text[..abs].trim_end();
            let predicate = text[after_pos..].trim();
            if predicate.is_empty() {
                // `emits foo when` (no predicate) — treat as a flat
                // emits line. The analyzer/doctor surfaces this as a
                // diagnostic if useful; the parser stays tolerant.
                return (head, None);
            }
            return (head, Some(predicate));
        }
        search_from = abs + "when".len();
    }
    (text, None)
}

/// Webhooks expanded cycle — parse `replay` in either short form
/// (`replay allow within "..."`) or long form (header + nested
/// `allow`/`deny` + `within "..."` + optional `dedupe by <path>`).
fn parse_webhook_replay(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
) -> Result<(WebhookReplay, usize), ParseError> {
    let header = &lines[start];
    let head = head_rest.trim();

    if !head.is_empty() {
        // Short form: `replay allow within "24h"` (`deny` has no
        // `within`, so allow/deny dispatch happens first).
        let mut tokens = head.split_whitespace();
        let mode = tokens
            .next()
            .ok_or_else(|| line_error(header, "`replay` requires `allow` or `deny`"))?;
        if mode != "allow" && mode != "deny" {
            return Err(line_error(
                header,
                "`replay` mode must be `allow` or `deny`",
            ));
        }
        let mut within: Option<String> = None;
        let rest_tail: Vec<&str> = tokens.collect();
        if !rest_tail.is_empty() {
            // Expect `within "<duration>"`.
            if rest_tail[0] != "within" || rest_tail.len() < 2 {
                return Err(line_error(
                    header,
                    "`replay <mode>` short form takes only `within \"<duration>\"`",
                ));
            }
            within = Some(unquote_lzx_value(rest_tail[1..].join(" ").trim()).to_owned());
        }
        return Ok((
            WebhookReplay {
                mode: mode.to_owned(),
                within,
                dedupe_by: None,
                span: Span::new(header.start, header.end),
            },
            start + 1,
        ));
    }

    // Long form: nested children at AGENT_INDENT_GRANDCHILD.
    let mut mode: Option<String> = None;
    let mut within: Option<String> = None;
    let mut dedupe_by: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`replay` children use six-space indentation",
            ));
        }

        if trimmed == "allow" || trimmed.starts_with("allow ") {
            if mode.is_some() {
                return Err(line_error(line, "`replay` declares `allow` or `deny` once"));
            }
            mode = Some("allow".to_owned());
            // Allow `allow within "..."` on the same nested line as a
            // shorthand inside the long form.
            if let Some(rest) = trimmed.strip_prefix("allow ") {
                let rest = rest.trim();
                if let Some(within_rest) = rest.strip_prefix("within ") {
                    within = Some(unquote_lzx_value(within_rest.trim()).to_owned());
                } else {
                    return Err(line_error(
                        line,
                        "`allow` only accepts a trailing `within \"<duration>\"`",
                    ));
                }
            }
        } else if trimmed == "deny" {
            if mode.is_some() {
                return Err(line_error(line, "`replay` declares `allow` or `deny` once"));
            }
            mode = Some("deny".to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("within ") {
            within = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("dedupe by ") {
            dedupe_by = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`replay` children are `allow`, `deny`, `within`, or `dedupe by`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let mode = mode.ok_or_else(|| line_error(header, "`replay` requires `allow` or `deny`"))?;

    Ok((
        WebhookReplay {
            mode,
            within,
            dedupe_by,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Webhooks expanded cycle — parse `dlq` with three mutually-exclusive
/// surface forms:
///
///   `dlq emit <event>`
///   `dlq handler "./..."`
///   `dlq drop` + nested `reason "..."`
fn parse_webhook_dlq(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
) -> Result<(WebhookDlq, usize), ParseError> {
    let header = &lines[start];
    let head = head_rest.trim();
    let span = Span::new(header.start, header.end);

    if let Some(rest) = head.strip_prefix("emit ") {
        let event = rest.trim();
        if event.is_empty() {
            return Err(line_error(header, "`dlq emit` requires an event name"));
        }
        return Ok((
            WebhookDlq::Emit {
                event: event.to_owned(),
                span,
            },
            start + 1,
        ));
    }

    if let Some(rest) = head.strip_prefix("handler ") {
        let path = unquote_lzx_value(rest.trim()).to_owned();
        if path.is_empty() {
            return Err(line_error(header, "`dlq handler` requires a quoted path"));
        }
        return Ok((WebhookDlq::Handler { path, span }, start + 1));
    }

    if head == "drop" || head.is_empty() && header.text.trim_start() == "dlq drop" {
        // Long form only: `dlq drop` + nested `reason "..."`.
        let mut reason: Option<String> = None;
        let mut last_end = header.end;
        let mut i = start + 1;

        while i < lines.len() {
            let line = &lines[i];
            let trimmed = line.text.trim_start();
            if is_trivia(trimmed) {
                i += 1;
                continue;
            }
            if line.indent <= AGENT_INDENT_AGENT_CHILD {
                break;
            }
            if line.indent != AGENT_INDENT_GRANDCHILD {
                return Err(line_error(
                    line,
                    "`dlq drop` children use six-space indentation",
                ));
            }
            if let Some(rest) = trimmed.strip_prefix("reason ") {
                reason = Some(unquote_lzx_value(rest.trim()).to_owned());
            } else {
                return Err(line_error(line, "`dlq drop` accepts only `reason \"...\"`"));
            }
            last_end = line.end;
            i += 1;
        }

        let reason = reason.ok_or_else(|| {
            line_error(
                header,
                "`dlq drop` requires `reason \"...\"` — silent drops on dead-letter must be explicit waivers",
            )
        })?;
        return Ok((
            WebhookDlq::Drop {
                reason,
                span: Span::new(header.start, last_end),
            },
            i,
        ));
    }

    Err(line_error(
        header,
        "`dlq` children are `emit <event>`, `handler \"...\"`, or `drop`",
    ))
}

fn parse_webhook_verify(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
) -> Result<(WebhookVerify, usize), ParseError> {
    let header = &lines[start];
    let head = head_rest.trim();
    let (scheme, algorithm) = head.split_once(' ').ok_or_else(|| {
        line_error(
            header,
            "`verify` requires `<scheme> <algorithm>` (e.g. `verify hmac sha256`)",
        )
    })?;
    let mut secret_env: Option<String> = None;
    let mut header_lit: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`verify` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("secret ") {
            // `secret env.CRM_WEBHOOK_SECRET` — record the env binding
            // verbatim (analyzer extracts the env name).
            secret_env = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("header ") {
            header_lit = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`verify` children are `secret` or `header`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        WebhookVerify {
            scheme: scheme.to_owned(),
            algorithm: algorithm.trim().to_owned(),
            secret_env,
            header: header_lit,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Realtime bucket cycle MVP — parse a `channel <name>` feature-level
/// block. Closed three-child body:
///
/// ```ignore
/// channel customer_activity
///   tenant_from org
///   policy @policy.read
///   payload CustomerActivityEvent
/// ```
///
/// All three children are required. Missing any one yields a parse
/// error citing the missing key. Unknown child keys are rejected so
/// the catalog stays closed; new realtime grammar (audit, rate_limit,
/// presence, broadcast wiring) must enter through a new cycle with
/// pilot evidence per `docs/scope-discipline.md`.
///
/// Doctor `CHANNEL-PAYLOAD-001` runs against the IR-lifted form; this
/// parser is purely syntactic.
///
/// Cache bucket cycle (CL.C.3) — parse a feature-level `cache <name>`
/// profile block.
///
/// Header: `cache <profile_name>` at feature-child indent (2 spaces).
/// Required body children at agent-child indent (4 spaces): `key
/// <expr>`, `ttl <literal>`.
/// Optional body children: `namespace <label>`, `tags <l1>[, <l2>...]`,
/// `stale_while_revalidate <literal>`, `coalesce <bool>`,
/// `sliding <bool>`.
fn parse_cache_profile(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(CacheProfileDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("cache ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "cache profile header must be `cache <name>`"))?;
    if name.is_empty() {
        return Err(line_error(
            header,
            "feature-level `cache` header requires a profile name",
        ));
    }

    let mut key: Option<String> = None;
    let mut ttl: Option<String> = None;
    let mut namespace: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut stale_while_revalidate: Option<String> = None;
    let mut coalesce: Option<bool> = None;
    let mut sliding: Option<bool> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`cache <name>` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("key ") {
            key = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("ttl ") {
            ttl = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("namespace ") {
            namespace = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("tags ") {
            for part in rest.split(',') {
                let label = part.trim();
                if !label.is_empty() {
                    tags.push(label.to_owned());
                }
            }
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("stale_while_revalidate ") {
            stale_while_revalidate = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("coalesce ") {
            coalesce = Some(parse_cache_bool(line, rest.trim())?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("sliding ") {
            sliding = Some(parse_cache_bool(line, rest.trim())?);
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`cache <name>` children are `key <expr>`, `ttl <literal>`, \
                 `namespace <label>`, `tags <l1>[, <l2>...]`, \
                 `stale_while_revalidate <literal>`, `coalesce <bool>`, \
                 or `sliding <bool>`",
            ));
        }
    }

    let key = key
        .ok_or_else(|| line_error(header, "`cache <name>` requires a `key <expr>` declaration"))?;
    let ttl = ttl.ok_or_else(|| {
        line_error(
            header,
            "`cache <name>` requires a `ttl <literal>` declaration",
        )
    })?;

    Ok((
        CacheProfileDecl {
            name,
            key,
            ttl,
            namespace,
            tags,
            stale_while_revalidate,
            coalesce,
            sliding,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_cache_bool(line: &SourceLine<'_>, value: &str) -> Result<bool, ParseError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(line_error_owned(
            line,
            format!(
                "`cache` boolean decorators (`coalesce`, `sliding`) accept `true` or `false`, found `{other}`"
            ),
        )),
    }
}

fn parse_agent_expose(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AgentExpose, usize), ParseError> {
    let header = &lines[start];
    let header_start = header.start;
    let mut method: Option<HttpMethod> = None;
    let mut path: Option<String> = None;
    let mut route_slots: Vec<AgentExposeRouteSlot> = Vec::new();
    let mut audience: Option<String> = None;
    let mut rate_limit_override: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`expose http` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("method ") {
            let token = rest.trim();
            let kind = HttpMethod::from_token(token).ok_or_else(|| {
                line_error(
                    line,
                    "`method` must be one of GET / POST / PUT / PATCH / DELETE",
                )
            })?;
            method = Some(kind);
        } else if let Some(rest) = trimmed.strip_prefix("path ") {
            path = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            route_slots.push(parse_agent_expose_route_slot(line, rest)?);
        } else if let Some(rest) = trimmed.strip_prefix("audience ") {
            audience = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            rate_limit_override = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`expose http` children are `method`, `path`, `route`, `audience`, or `rate_limit`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let method = method.ok_or_else(|| {
        line_error(
            header,
            "`expose http` requires `method <GET|POST|PUT|PATCH|DELETE>`",
        )
    })?;
    let path = path.ok_or_else(|| line_error(header, "`expose http` requires `path \"<url>\"`"))?;

    Ok((
        AgentExpose {
            method,
            path,
            route_slots,
            audience,
            rate_limit_override,
            span: Span::new(header_start, last_end),
        },
        i,
    ))
}

fn parse_agent_expose_route_slot(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<AgentExposeRouteSlot, ParseError> {
    let (name_part, type_part) = rest
        .split_once(':')
        .ok_or_else(|| line_error(line, "`route` slot must be `route <name>: <Type>`"))?;
    let name = name_part.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(line, "`route` slot is missing a name"));
    }
    let type_text = type_part.trim().to_owned();
    if type_text.is_empty() {
        return Err(line_error(line, "`route` slot is missing a type"));
    }
    Ok(AgentExposeRouteSlot {
        name,
        type_text,
        span: Span::new(line.start, line.end),
    })
}

fn parse_agent_input(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<AgentInputSlot>, usize), ParseError> {
    let mut slots = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "agent input slots use six-space indentation",
            ));
        }

        slots.push(parse_agent_input_slot(line)?);
        i += 1;
    }

    Ok((slots, i))
}

fn parse_agent_input_slot(line: &SourceLine<'_>) -> Result<AgentInputSlot, ParseError> {
    let trimmed = line.text.trim_start();
    let (name_part, rest) = trimmed.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "input slot must be `<name>: <type> [required|optional]`",
        )
    })?;

    let name = name_part.trim();
    if name.is_empty() {
        return Err(line_error(line, "input slot is missing a name"));
    }

    let mut required = false;
    let mut optional = false;
    let mut type_tokens: Vec<&str> = Vec::new();
    for token in rest.split_whitespace() {
        match token {
            "required" => required = true,
            "optional" => optional = true,
            other => type_tokens.push(other),
        }
    }

    if type_tokens.is_empty() {
        return Err(line_error(line, "input slot is missing a type"));
    }
    if required && optional {
        return Err(line_error(
            line,
            "input slot cannot be both `required` and `optional`",
        ));
    }

    Ok(AgentInputSlot {
        name: name.to_owned(),
        type_text: type_tokens.join(" "),
        required,
        optional,
        span: Span::new(line.start, line.end),
    })
}

/// Parse the value side of an `output ...` declaration. The leading
/// `output ` prefix has already been consumed by the caller.
fn parse_agent_output_value(line: &SourceLine<'_>, rest: &str) -> Result<AgentOutput, ParseError> {
    let trimmed = rest.trim();
    if let Some(rest) = trimmed.strip_prefix("stream ") {
        let type_ref = rest.trim();
        if type_ref.is_empty() {
            return Err(line_error(line, "`output stream` requires a type"));
        }
        if type_ref.split_whitespace().next() == Some("discriminator") {
            return Err(line_error(
                line,
                "`output stream` cannot also carry `discriminator`; pick one form",
            ));
        }
        Ok(AgentOutput::Stream(type_ref.to_owned()))
    } else if let Some(rest) = trimmed.strip_prefix("discriminator ") {
        let target = rest.trim();
        if target.is_empty() {
            return Err(line_error(line, "`output discriminator` requires a type"));
        }
        if target.split_whitespace().count() > 1 {
            return Err(line_error(
                line,
                "`output discriminator` accepts a single enum reference",
            ));
        }
        Ok(AgentOutput::Discriminator(target.to_owned()))
    } else if trimmed.is_empty() {
        Err(line_error(line, "`output` requires a value"))
    } else {
        // Bare type ref form: `output <Type>` (legacy default; lowering
        // disambiguates record-with-discriminator vs Text).
        if trimmed.split_whitespace().count() > 1 {
            return Err(line_error(
                line,
                "`output <Type>` accepts a single type reference",
            ));
        }
        Ok(AgentOutput::Plain(trimmed.to_owned()))
    }
}

fn parse_agent_tools(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<AgentTool>, usize), ParseError> {
    let mut tools = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "tool entries use six-space indentation, one reference per line",
            ));
        }

        if trimmed.split_whitespace().count() != 1 {
            return Err(line_error(
                line,
                "each tool entry is a single qualified reference",
            ));
        }

        tools.push(AgentTool {
            reference: trimmed.to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    Ok((tools, i))
}

fn parse_agent_evals(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<AgentEvalCase>, usize), ParseError> {
    let mut cases = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "eval `case` headers use six-space indentation",
            ));
        }

        let case_name = trimmed
            .strip_prefix("case ")
            .map(|rest| rest.trim().to_owned())
            .ok_or_else(|| line_error(line, "eval children must be `case <name>` blocks"))?;
        if case_name.is_empty() {
            return Err(line_error(line, "`case` requires a name"));
        }
        let case_start = line.start;
        let mut case_end = line.end;

        let mut assertions = Vec::new();
        let mut golden: Option<AgentEvalGolden> = None;
        i += 1;
        while i < lines.len() {
            let inner = &lines[i];
            let inner_trimmed = inner.text.trim_start();

            if is_trivia(inner_trimmed) {
                i += 1;
                continue;
            }

            if inner.indent <= AGENT_INDENT_GRANDCHILD {
                break;
            }

            if inner.indent != AGENT_INDENT_GREAT_GRANDCHILD {
                return Err(line_error(
                    inner,
                    "eval case children use eight-space indentation",
                ));
            }

            if let Some(rest) = inner_trimmed.strip_prefix("golden ") {
                if golden.is_some() {
                    return Err(line_error(
                        inner,
                        "`case` may declare at most one `golden` reference",
                    ));
                }
                golden = Some(parse_eval_golden(inner, rest)?);
            } else {
                assertions.push(parse_eval_assertion(inner)?);
            }
            case_end = inner.end;
            i += 1;
        }

        if assertions.is_empty() && golden.is_none() {
            return Err(line_error(
                line,
                "`case <name>` must declare at least one `requires`/`forbids` assertion or a `golden \"./path\"` reference",
            ));
        }

        cases.push(AgentEvalCase {
            name: case_name,
            assertions,
            golden,
            span: Span::new(case_start, case_end),
        });
    }

    Ok((cases, i))
}

/// Parse `golden "./path.jsonl"` or `golden "./path.jsonl" min_score 0.85`.
/// The path is required; `min_score` is optional and must parse as a
/// float when present. Adapter convention defaults to 0.85 when
/// omitted; the parser records `None` so authors can override at
/// adapter level without language-side ambiguity.
fn parse_eval_golden(line: &SourceLine<'_>, rest: &str) -> Result<AgentEvalGolden, ParseError> {
    let trimmed = rest.trim();
    if !trimmed.starts_with('"') {
        return Err(line_error(
            line,
            "`golden` requires a quoted file path: `golden \"./path.jsonl\"`",
        ));
    }
    // Find the closing quote without scanning past min_score.
    let body = &trimmed[1..];
    let Some(closing) = body.find('"') else {
        return Err(line_error(line, "`golden` path is missing a closing quote"));
    };
    let path = body[..closing].to_owned();
    let after = body[closing + 1..].trim();
    let min_score = if after.is_empty() {
        None
    } else if let Some(score_text) = after.strip_prefix("min_score ") {
        let value: f64 = score_text
            .trim()
            .parse()
            .map_err(|_| line_error(line, "`min_score` must be a decimal between 0.0 and 1.0"))?;
        if !(0.0..=1.0).contains(&value) {
            return Err(line_error(
                line,
                "`min_score` must be in the range 0.0..=1.0",
            ));
        }
        Some(value)
    } else {
        return Err(line_error(
            line,
            "trailing tokens after `golden \"./path\"` must be `min_score <N>`",
        ));
    };
    Ok(AgentEvalGolden {
        path,
        min_score,
        span: Span::new(line.start, line.end),
    })
}

fn parse_eval_assertion(line: &SourceLine<'_>) -> Result<AgentEvalAssertion, ParseError> {
    let trimmed = line.text.trim_start();
    let (kind, body) = if let Some(rest) = trimmed.strip_prefix("requires ") {
        (AgentEvalKind::Requires, rest.trim())
    } else if let Some(rest) = trimmed.strip_prefix("forbids ") {
        (AgentEvalKind::Forbids, rest.trim())
    } else {
        return Err(line_error(
            line,
            "eval assertions start with `requires` or `forbids`",
        ));
    };

    if body.is_empty() {
        return Err(line_error(line, "eval assertion requires a predicate"));
    }

    let predicate = parse_eval_predicate(line, body)?;
    Ok(AgentEvalAssertion {
        kind,
        predicate,
        span: Span::new(line.start, line.end),
    })
}

fn parse_eval_predicate(
    line: &SourceLine<'_>,
    body: &str,
) -> Result<AgentEvalPredicate, ParseError> {
    let body = body.trim();

    if let Some(rest) = body.strip_prefix("tools.calls ") {
        let mut parts = rest.split_whitespace();
        let op_token = parts.next().ok_or_else(|| {
            line_error(
                line,
                "`tools.calls` requires `includes` or `excludes` followed by a tool reference",
            )
        })?;
        let target = parts
            .next()
            .ok_or_else(|| line_error(line, "`tools.calls` requires a tool reference target"))?;
        if parts.next().is_some() {
            return Err(line_error(
                line,
                "`tools.calls <op> <ref>` accepts a single tool reference",
            ));
        }
        let op = match op_token {
            "includes" => ToolsCallsOp::Includes,
            "excludes" => ToolsCallsOp::Excludes,
            _ => {
                return Err(line_error(
                    line,
                    "`tools.calls` operator must be `includes` or `excludes`",
                ));
            }
        };
        return Ok(AgentEvalPredicate::ToolsCalls {
            op,
            target: target.to_owned(),
        });
    }

    if let Some(idx) = find_contains_keyword(body) {
        let lhs = body[..idx].trim().to_owned();
        let rhs = body[idx + " contains ".len()..].trim();
        if lhs.is_empty() {
            return Err(line_error(
                line,
                "`contains` predicate requires a left-hand reference",
            ));
        }
        let rhs = parse_contains_rhs(line, rhs)?;
        return Ok(AgentEvalPredicate::Contains { lhs, rhs });
    }

    Ok(AgentEvalPredicate::Closed {
        text: body.to_owned(),
    })
}

/// Locate the ` contains ` infix inside an eval predicate body. Returns the
/// byte index of the leading space so callers can split lhs/rhs without
/// re-scanning. Returns `None` when no `contains` keyword appears as a
/// stand-alone token (we never match inside quoted strings).
fn find_contains_keyword(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }
        if !in_quote && body[i..].starts_with(" contains ") {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_contains_rhs(line: &SourceLine<'_>, rhs: &str) -> Result<ContainsRhs, ParseError> {
    let rhs = rhs.trim();
    if rhs.is_empty() {
        return Err(line_error(line, "`contains` requires a right-hand value"));
    }
    if rhs.starts_with('"') {
        let stripped = rhs
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .ok_or_else(|| line_error(line, "`contains` string literal must be quoted"))?;
        return Ok(ContainsRhs::Literal(stripped.to_owned()));
    }
    if rhs.starts_with("@semantic.") {
        if rhs.split_whitespace().count() > 1 {
            return Err(line_error(
                line,
                "`contains @semantic.<Type>` accepts a single semantic-type reference",
            ));
        }
        return Ok(ContainsRhs::SemanticType(rhs.to_owned()));
    }
    Err(line_error(
        line,
        "`contains` rhs must be a quoted string literal or a `@semantic.<Type>` reference",
    ))
}

fn split_policy_atoms(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_float(line: &SourceLine<'_>, rest: &str) -> Result<f64, ParseError> {
    rest.trim()
        .parse::<f64>()
        .map_err(|_| line_error(line, "expected a decimal value (e.g. `0`, `0.2`)"))
}

fn parse_uint32(line: &SourceLine<'_>, rest: &str) -> Result<u32, ParseError> {
    rest.trim()
        .parse::<u32>()
        .map_err(|_| line_error(line, "expected a non-negative integer"))
}

fn parse_int64(line: &SourceLine<'_>, rest: &str) -> Result<i64, ParseError> {
    rest.trim()
        .parse::<i64>()
        .map_err(|_| line_error(line, "expected a signed integer"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum InvariantForm {
    TerminalImmutable,
    SingleStatePerScope { state: String, scope_field: String },
    NoJumpMoreThanOne,
}

/// Parses the closed-catalog invariant form from the raw tail after `invariant `.
/// Whitespace is significant only as a token separator; leading/trailing whitespace
/// is trimmed before matching.
#[allow(dead_code)]
pub(crate) fn parse_invariant_form(
    line: &SourceLine<'_>,
    raw: &str,
) -> Result<InvariantForm, ParseError> {
    let raw = raw.trim();
    if raw == "terminal_immutable" {
        return Ok(InvariantForm::TerminalImmutable);
    }
    if raw == "no_jump_more_than_one" {
        return Ok(InvariantForm::NoJumpMoreThanOne);
    }

    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.len() == 4 && parts[0] == "single" && parts[2] == "per" {
        let state = parts[1].to_owned();
        let scope_field = parts[3].to_owned();
        if state.is_empty() || scope_field.is_empty() {
            return Err(line_error(
                line,
                "`invariant single <state> per <scope_field>` requires both names",
            ));
        }
        return Ok(InvariantForm::SingleStatePerScope { state, scope_field });
    }

    Err(line_error_owned(
        line,
        format!(
            "unknown invariant form `{}` - closed catalog is `terminal_immutable`, `single <state> per <scope_field>`, `no_jump_more_than_one` (see docs/proposals/lifecycle-vocab.md §3.4)",
            raw
        ),
    ))
}

/// `ir-rate-limit-env-aware` cell 1 — parse the *body* of a single
/// `rate_limit` line (i.e. the slice after the `rate_limit ` prefix).
///
/// Recognises two shapes per proposal §4.2:
///
/// * unqualified — `"X per Y per Z"` → `(literal, None)`
/// * env-qualified — `"X per Y per Z" in dev, staging, test` →
///   `(literal, Some(["dev", "staging", "test"]))`
///
/// The proposal-defined `"unlimited"` shortcut (§4.4) passes through
/// verbatim — the analyzer lowers it to the empty-string sentinel
/// inside `ir::RateLimitByEnv.limit`.
///
/// Validation:
///   * empty literal (`rate_limit ""`) → error
///   * empty trailing `in` (`rate_limit "X" in`) → error
///   * empty env name in the list (`in dev,,test`) → error
fn parse_rate_limit_line_body(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<Vec<String>>), ParseError> {
    let trimmed = rest.trim_start();

    // The literal is a double-quoted string. Find the closing quote so
    // we can isolate the optional `in <env_list>` tail.
    if !trimmed.starts_with('"') {
        return Err(line_error(
            line,
            "`rate_limit` requires a quoted spec literal (e.g. `rate_limit \"5 per minute per ip\"`)",
        ));
    }
    let after_open = &trimmed[1..];
    let close_offset = after_open.find('"').ok_or_else(|| {
        line_error(
            line,
            "`rate_limit` spec literal is missing its closing quote",
        )
    })?;
    let literal = &after_open[..close_offset];
    if literal.is_empty() {
        return Err(line_error(
            line,
            "`rate_limit` spec literal must be non-empty",
        ));
    }

    let tail = after_open[close_offset + 1..].trim_start();
    if tail.is_empty() {
        return Ok((literal.to_owned(), None));
    }

    // The only legal tail today is `in <env_list>`.
    let Some(after_in) = tail.strip_prefix("in") else {
        return Err(line_error(
            line,
            "`rate_limit` literal may be followed only by `in <env_list>`",
        ));
    };
    // `in` must be separated from what follows by whitespace (so
    // `intermediate` doesn't accidentally parse). When the suffix is
    // empty (just `in` on its own), `strip_prefix` returns "", which
    // we explicitly reject below as an empty env list.
    let env_list_text = if let Some(stripped) = after_in.strip_prefix(char::is_whitespace) {
        stripped.trim()
    } else if after_in.is_empty() {
        ""
    } else {
        return Err(line_error(
            line,
            "`rate_limit` literal may be followed only by `in <env_list>`",
        ));
    };

    if env_list_text.is_empty() {
        return Err(line_error(
            line,
            "`rate_limit \"X\" in` requires at least one env name (e.g. `in dev, staging, test`)",
        ));
    }

    let mut envs: Vec<String> = Vec::new();
    for part in env_list_text.split(',') {
        let name = part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "`rate_limit` env list cannot contain empty entries (drop the trailing/extra comma)",
            ));
        }
        // The closed catalog is validated at the analyzer / Cell 3
        // doctor layer; the parser accepts any identifier here and
        // surfaces unknown names as a doctor warning later.
        envs.push(name.to_owned());
    }

    Ok((literal.to_owned(), Some(envs)))
}

/// `ir-rate-limit-env-aware` cell 1 — accumulate one `rate_limit` line
/// into the in-progress `RateLimitSpecAst` for a declaration.
///
/// The first unqualified line establishes the default; further
/// unqualified lines are rejected (`rate_limit_duplicate_default`).
/// Qualified lines are appended to `by_env` in source order.
fn fold_rate_limit_line(
    line: &SourceLine<'_>,
    spec: &mut Option<RateLimitSpecAst>,
    literal: String,
    envs: Option<Vec<String>>,
) -> Result<(), ParseError> {
    let entry_span = Span::new(line.start, line.end);
    let aggregate = spec.get_or_insert_with(|| RateLimitSpecAst {
        default: None,
        by_env: Vec::new(),
        span: entry_span,
    });
    aggregate.span = Span::new(aggregate.span.start, entry_span.end);

    match envs {
        None => {
            if aggregate.default.is_some() {
                return Err(line_error(
                    line,
                    "rate_limit_duplicate_default: declaration already carries an unqualified `rate_limit` line — add `in <env_list>` to differentiate or remove one",
                ));
            }
            aggregate.default = Some(unquote_lzx_value(&literal).to_owned());
        }
        Some(envs) => {
            aggregate.by_env.push(RateLimitByEnvAst {
                limit: literal,
                envs,
                span: entry_span,
            });
        }
    }

    Ok(())
}

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

    /// Wave 4 — parser must lift view `tests` into the typed
    /// `LzxViewTestAssertion` enum. `accepted by` / `rejected by` are
    /// the only admissible shapes; anything else is a hard parse error.
    #[test]
    fn lzx_view_tests_lift_to_typed_assertions() {
        let source = "experience customer
  view detail
    anchor @anchor.customer_detail
    extensible_by customer_tags, customer_import
    source customer.query.by_id(id: route.id)

    tests
      accepted by customer_tags
      accepted by customer_import
      rejected by billing
";
        let document = parse_lzx_document(source).unwrap();
        let view = &document.experiences[0].views[0];
        assert_eq!(view.tests.len(), 3);

        match &view.tests[0] {
            LzxViewTestAssertion::AcceptedBy { feature, .. } => {
                assert_eq!(feature, "customer_tags")
            }
            other => panic!("expected AcceptedBy, got {other:?}"),
        }
        match &view.tests[1] {
            LzxViewTestAssertion::AcceptedBy { feature, .. } => {
                assert_eq!(feature, "customer_import")
            }
            other => panic!("expected AcceptedBy, got {other:?}"),
        }
        match &view.tests[2] {
            LzxViewTestAssertion::RejectedBy { feature, .. } => {
                assert_eq!(feature, "billing")
            }
            other => panic!("expected RejectedBy, got {other:?}"),
        }
    }

    /// Wave 4 — the parser must reject any view test assertion outside
    /// the closed extensibility vocabulary (policy / predicate
    /// vocabulary belongs to commands, rules, and transitions).
    #[test]
    fn lzx_view_tests_reject_non_extensibility_shapes() {
        let source = "experience customer
  view detail
    anchor @anchor.customer_detail

    tests
      allows when target.status = active
";
        let err = parse_lzx_document(source).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("accepted by") && msg.contains("rejected by"),
            "expected guidance about closed catalog, got {msg}"
        );
    }

    /// Wave 4 — the live full-capsule fixture must still parse and its
    /// `accepted by` / `rejected by` lines must lift to the new typed
    /// shape (regression guard for the existing example).
    #[test]
    fn full_capsule_view_tests_round_trip() {
        let document = parse_lzx_document(include_str!(
            "../../../../examples/full-capsule/full-capsule.lzx"
        ))
        .unwrap();
        let detail_view = document
            .experiences
            .iter()
            .flat_map(|e| e.views.iter())
            .find(|v| v.name == "detail")
            .expect("detail view present in fixture");
        // Sanity-check the assertion shape — the fixture has two
        // `accepted by` and one `rejected by` under the `detail` view.
        let accepted: Vec<&str> = detail_view
            .tests
            .iter()
            .filter_map(|t| match t {
                LzxViewTestAssertion::AcceptedBy { feature, .. } => Some(feature.as_str()),
                _ => None,
            })
            .collect();
        let rejected: Vec<&str> = detail_view
            .tests
            .iter()
            .filter_map(|t| match t {
                LzxViewTestAssertion::RejectedBy { feature, .. } => Some(feature.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(accepted, vec!["customer_tags", "customer_import"]);
        assert_eq!(rejected, vec!["billing"]);
    }

    #[test]
    fn parses_lzx_experience_and_platform_surface() {
        let experience =
            parse_lzx_document(include_str!("../../../../examples/customer-capsule.lzx")).unwrap();
        assert_eq!(experience.experiences.len(), 1);
        assert_eq!(experience.experiences[0].name, "customer");
        assert_eq!(experience.experiences[0].imports, vec!["customer"]);
        assert_eq!(experience.experiences[0].views[0].name, "list");
        assert_eq!(
            experience.experiences[0].views[0].source.as_deref(),
            Some("customer.query.list")
        );
        assert_eq!(experience.experiences[0].views[1].anchor.as_deref(), None);

        let surface = parse_lzx_document(include_str!(
            "../../../../examples/customer-capsule.web.lzx"
        ))
        .unwrap();
        assert_eq!(surface.surfaces.len(), 1);
        assert_eq!(surface.surfaces[0].experience, "customer");
        assert_eq!(surface.surfaces[0].platform, LzxPlatform::Web);
        assert_eq!(
            surface.surfaces[0].uses_experience.as_deref(),
            Some("customer")
        );
        assert_eq!(surface.surfaces[0].audiences[0].name, "admin");
        assert_eq!(
            surface.surfaces[0].audiences[0].views[0].columns,
            vec!["name", "email", "status", "created_at"]
        );
        assert_eq!(
            surface.surfaces[0].audiences[0].views[0].filter,
            vec!["status"]
        );
        assert_eq!(
            surface.surfaces[0].audiences[0].views[0].cells,
            vec!["status @client.status_cell"]
        );
    }

    #[test]
    fn parses_lzx_app_manifest_and_routes() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  version "0.1.0"
  targets
    backend go
    web react
    mobile expo
  default_locale "pt-BR"
  default_timezone "America/Sao_Paulo"
  auth_failed_redirect public.login
  not_found public.not_found
  error_page 404
    template "./views/404.tmpl"
    audience public
  error_page 500
    template "./views/500.tmpl"
  uses customer, customer_auth

route customer_detail
  path "/customers/:id"
  route id: Customer.ID
  to customer.view.detail(id: route.id)
  surface customer web
  audience admin
  lazy true
"#;

        let document = parse_lzx_document(source).unwrap();
        let app = document.app.as_ref().unwrap();

        assert_eq!(app.name, "AcmeCRM");
        assert_eq!(app.title.as_deref(), Some("Acme CRM"));
        assert_eq!(app.targets, vec!["backend go", "web react", "mobile expo"]);
        assert_eq!(app.error_pages.len(), 2);
        assert_eq!(app.error_pages[0].status, 404);
        assert_eq!(app.error_pages[0].template, "./views/404.tmpl");
        assert_eq!(app.error_pages[0].audience.as_deref(), Some("public"));
        assert_eq!(app.error_pages[1].status, 500);
        assert_eq!(app.error_pages[1].template, "./views/500.tmpl");
        assert_eq!(app.error_pages[1].audience, None);
        assert_eq!(app.uses, vec!["customer", "customer_auth"]);
        assert_eq!(document.routes.len(), 1);
        assert_eq!(document.routes[0].path.as_deref(), Some("/customers/:id"));
        // ir+codegen(ts) §2.1 typed route_params landed (commit fe4d3a1c):
        // `route id: Customer.ID` now lifts to `route_params`, not `routes`.
        assert_eq!(document.routes[0].routes, Vec::<String>::new());
        assert_eq!(document.routes[0].route_params.len(), 1);
        assert_eq!(document.routes[0].route_params[0].name, "id");
        assert_eq!(
            document.routes[0].to.as_deref(),
            Some("customer.view.detail(id: route.id)")
        );
        assert_eq!(document.routes[0].lazy, Some(true));
    }

    #[test]
    fn parses_lzx_route_guard_clauses() {
        let source = r#"
app AcmeCRM
  actor_query "account.query.me"
  route_guard
    default_policy @scope.authenticated
    on_unauthenticated redirect "/sign-in"
    on_unauthorized redirect "/403"
    skeleton @client.route_guard_skeleton

route admin_home
  path "/admin"
  to customer.view.list
  surface customer web
  audience admin
  policy @policy.admin_only
    on_unauthenticated redirect "/sign-in"

experience customer
  view list
    policy @policy.admin_only
      on_unauthorized redirect "/"
    source customer.query.list

surface customer web
  uses experience customer

  audience admin
    policy @policy.admin_only
      on_unauthenticated redirect "/sign-in"
    view list Table
      policy @policy.admin_only
        on_unauthorized redirect "/"
      columns name
"#;

        let document = parse_lzx_document(source).unwrap();
        let app = document.app.as_ref().unwrap();
        let defaults = app.route_guard.as_ref().unwrap();
        assert_eq!(app.actor_query.as_deref(), Some("account.query.me"));
        assert_eq!(
            defaults.default_policy.as_deref(),
            Some("@scope.authenticated")
        );
        assert_eq!(defaults.on_unauthenticated.as_deref(), Some("/sign-in"));
        assert_eq!(defaults.on_unauthorized.as_deref(), Some("/403"));
        assert_eq!(
            defaults.skeleton.as_deref(),
            Some("@client.route_guard_skeleton")
        );

        let route_guard = document.routes[0].guard.as_ref().unwrap();
        assert_eq!(route_guard.policy, vec!["@policy.admin_only"]);
        assert_eq!(route_guard.on_unauthenticated.as_deref(), Some("/sign-in"));

        let experience_guard = document.experiences[0].views[0].guard.as_ref().unwrap();
        assert_eq!(experience_guard.policy, vec!["@policy.admin_only"]);
        assert_eq!(experience_guard.on_unauthorized.as_deref(), Some("/"));

        let audience = &document.surfaces[0].audiences[0];
        assert_eq!(
            audience
                .guard
                .as_ref()
                .and_then(|guard| guard.on_unauthenticated.as_deref()),
            Some("/sign-in")
        );
        assert_eq!(
            audience.views[0]
                .guard
                .as_ref()
                .and_then(|guard| guard.on_unauthorized.as_deref()),
            Some("/")
        );
    }

    #[test]
    fn parses_lzx_lifecycle_substep_on_view_and_resume_arm() {
        let source = r#"
experience host
  imports host

  view phone_verification
    policy @policy.host_only
    requires_lifecycle Host = basic_details_pending substep phone_verification
    on_lifecycle_pending @resume host_onboarding
    source host.query.lookup.my_host

  resume host_onboarding
    source query.lookup my_host
    none -> view phone_verification
    basic_details_pending substep phone_verification -> view phone_verification
    * -> view phone_verification
"#;

        let document = parse_lzx_document(source).expect("parses");
        let view = &document.experiences[0].views[0];
        let requires = view
            .guard
            .as_ref()
            .and_then(|guard| guard.requires_lifecycle.as_ref())
            .expect("requires_lifecycle");
        assert_eq!(requires.state, "basic_details_pending");
        assert_eq!(requires.substep.as_deref(), Some("phone_verification"));

        let arm = &document.experiences[0].resume_routers[0].arms[1];
        assert_eq!(
            arm.kind,
            crate::LzxResumeArmKind::State("basic_details_pending".to_string())
        );
        assert_eq!(arm.substep.as_deref(), Some("phone_verification"));
    }

    #[test]
    fn parses_lzx_audience_policy_single_and_list_rejects_trailing_comma() {
        let source = r#"
surface booking web
  audience host
    policy @policy.role.host
  audience signed_in
    policy [@policy.role.host, @policy.role.traveler]
"#;
        let document = parse_lzx_document(source).expect("parses");

        assert_eq!(
            document.surfaces[0].audiences[0]
                .guard
                .as_ref()
                .unwrap()
                .policy,
            vec!["@policy.role.host"]
        );
        assert_eq!(
            document.surfaces[0].audiences[1]
                .guard
                .as_ref()
                .unwrap()
                .policy,
            vec!["@policy.role.host", "@policy.role.traveler"]
        );

        let trailing = r#"
surface booking web
  audience signed_in
    policy [@policy.role.host,]
"#;
        let err = parse_lzx_document(trailing).unwrap_err();
        assert!(err.to_string().contains("empty entry"));
    }

    #[test]
    fn parses_lzx_error_page_maintenance_status() {
        let source = r#"
app AcmeCRM
  error_page 503
    template "./views/maintenance.tmpl"
    audience public
"#;

        let document = parse_lzx_document(source).unwrap();
        let page = &document.app.as_ref().unwrap().error_pages[0];
        assert_eq!(page.status, 503);
        assert_eq!(page.template, "./views/maintenance.tmpl");
        assert_eq!(page.audience.as_deref(), Some("public"));
    }

    #[test]
    fn rejects_lzx_error_page_without_template() {
        let source = r#"
app AcmeCRM
  error_page 404
    audience public
"#;

        let error = parse_lzx_document(source).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires a `template \"./...\"` declaration"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_lzx_partial_overrides() {
        let source = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
      columns += score
"#;

        let error = parse_lzx_document(source).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("partial overrides are not valid in `.lzx`")
        );
    }

    #[test]
    fn parses_lzx_view_anchor_child() {
        let source = r#"
experience customer
  imports customer

  view detail
    route id: Customer.ID
    anchor @anchor.customer_detail
    source customer.query.by_id(id: route.id)
"#;

        let document = parse_lzx_document(source).unwrap();

        assert_eq!(
            document.experiences[0].views[0].anchor.as_deref(),
            Some("@anchor.customer_detail")
        );
    }

    #[test]
    fn parses_lzx_extension_slots_with_order() {
        let source = r#"
experience customer_tags
  imports customer_tags, customer

  extends @anchor.customer_detail
    slot aside
      block @client.tag_editor
      platforms web, mobile
      audience admin, sales
    slot timeline after activity_timeline
      block @client.import_history
"#;

        let document = parse_lzx_document(source).unwrap();
        let extension = &document.experiences[0].extensions[0];

        assert_eq!(extension.anchor, "@anchor.customer_detail");
        assert!(extension.blocks.is_empty());
        assert_eq!(extension.slots.len(), 2);
        assert_eq!(extension.slots[0].name, "aside");
        assert_eq!(extension.slots[0].blocks, vec!["@client.tag_editor"]);
        assert_eq!(extension.slots[0].platforms, vec!["web", "mobile"]);
        assert_eq!(extension.slots[0].audiences, vec!["admin", "sales"]);
        assert_eq!(extension.slots[1].name, "timeline");
        assert_eq!(
            extension.slots[1]
                .order
                .as_ref()
                .map(|order| (order.relation.as_str(), order.target.as_str())),
            Some(("after", "activity_timeline"))
        );
    }

    // -------------------------------------------------------------------------
    // Cut A — feature skeleton + agent slice (§3.5 snapshot tests)
    // -------------------------------------------------------------------------

    use super::parse_feature_skeletons;
    use crate::{
        AgentEvalKind, AgentEvalPredicate, AgentOutput, ContainsRhs, DefaultsTenancy, ToolsCallsOp,
    };

    // -------------------------------------------------------------------------
    // RB.A — RBAC catalog parser smoke tests
    // -------------------------------------------------------------------------

    use super::parse_package_skeleton;
    use crate::RoleGrantsAst;

    #[test]
    fn parses_minimal_rbac_catalog() {
        let source = r#"
permission users:read
permission users:create
permission proposals:read:own

role viewer
  grants
    users:read
    proposals:read:own

role editor
  inherits viewer
  grants
    users:create

role admin
  grants_all
"#;
        let pkg = parse_package_skeleton(source).expect("parses");
        assert_eq!(pkg.permissions.len(), 3);
        assert_eq!(pkg.permissions[0].name, "users:read");
        assert_eq!(pkg.permissions[2].segments.len(), 3);

        assert_eq!(pkg.roles.len(), 3);
        assert_eq!(pkg.roles[0].name, "viewer");
        assert!(matches!(pkg.roles[0].grants, RoleGrantsAst::Explicit(_)));
        assert_eq!(pkg.roles[1].inherits.as_deref(), Some("viewer"));
        assert!(matches!(pkg.roles[2].grants, RoleGrantsAst::All));
    }

    #[test]
    fn rejects_invalid_permission_segments() {
        let source = "permission users\n";
        let err = parse_package_skeleton(source).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("2 to 4 colon-separated"));
    }

    #[test]
    fn rejects_too_many_permission_segments() {
        let source = "permission a:b:c:d:e\n";
        let err = parse_package_skeleton(source).unwrap_err();
        assert!(format!("{:?}", err).contains("2 to 4"));
    }

    #[test]
    fn rejects_empty_permission_segment() {
        let source = "permission users::read\n";
        let err = parse_package_skeleton(source).unwrap_err();
        assert!(format!("{:?}", err).contains("empty segments"));
    }

    #[test]
    fn rejects_multi_parent_inherits() {
        let source = r#"
role admin
  inherits a, b
"#;
        let err = parse_package_skeleton(source).unwrap_err();
        assert!(
            format!("{:?}", err).contains("Multi-parent")
                || format!("{:?}", err).contains("multi-parent")
        );
    }

    #[test]
    fn rejects_grants_and_grants_all() {
        let source = r#"
role admin
  grants_all
  grants
    users:read
"#;
        let err = parse_package_skeleton(source).unwrap_err();
        assert!(format!("{:?}", err).contains("either `grants` or `grants_all`"));
    }

    #[test]
    fn parses_inherited_only_role() {
        let source = r#"
role support_lead
  inherits support
"#;
        let pkg = parse_package_skeleton(source).expect("parses");
        assert_eq!(pkg.roles.len(), 1);
        assert!(matches!(pkg.roles[0].grants, RoleGrantsAst::InheritedOnly));
    }

    #[test]
    fn rbac_catalog_coexists_with_features() {
        let source = r#"
permission users:read

role viewer
  grants
    users:read

feature customer
  domain
    resource Customer
      email: Text required
"#;
        let pkg = parse_package_skeleton(source).expect("parses");
        assert_eq!(pkg.features.len(), 1);
        assert_eq!(pkg.features[0].name, "customer");
        assert_eq!(pkg.permissions.len(), 1);
        assert_eq!(pkg.roles.len(), 1);
    }

    #[test]
    fn agent_with_tools_block_parses() {
        let source = r#"
feature customer
  agent triage_customer
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./prompts/triage.md"
    tools
      customer.query.by_id
      customer.query.list
      @tool.web_search
"#;

        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].name, "customer");
        assert_eq!(features[0].agents.len(), 1);

        let agent = &features[0].agents[0];
        assert_eq!(agent.name, "triage_customer");
        assert_eq!(agent.input.len(), 1);
        assert_eq!(agent.input[0].name, "message");
        assert_eq!(agent.input[0].type_text, "Text");
        assert!(agent.input[0].required);
        assert_eq!(
            agent.policy.as_deref(),
            Some(&["@policy.read".to_owned()][..])
        );
        assert_eq!(agent.model.as_deref(), Some("@llm.default"));
        assert_eq!(agent.prompt.as_deref(), Some("./prompts/triage.md"));
        assert_eq!(agent.output, Some(AgentOutput::Stream("Text".to_owned())));
        assert_eq!(agent.tools.len(), 3);
        assert_eq!(agent.tools[0].reference, "customer.query.by_id");
        assert_eq!(agent.tools[1].reference, "customer.query.list");
        assert_eq!(agent.tools[2].reference, "@tool.web_search");
    }

    #[test]
    fn agent_with_evals_parses() {
        let source = r#"
feature customer
  agent summarize_customer
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./prompts/summarize.md"
    evals
      case short_for_active
        requires customer.lifecycle_stage = active
        requires output contains "active"

      case redacts_email
        requires customer.email = "ada@example.com"
        forbids output contains @semantic.Email

      case uses_lookup_when_id_known
        requires input.customer_id = "cus_123"
        requires tools.calls includes customer.query.by_id
"#;

        let features = parse_feature_skeletons(source).unwrap();
        let agent = &features[0].agents[0];

        assert_eq!(agent.temperature, Some(0.0));
        assert_eq!(agent.seed, Some(1));
        assert_eq!(agent.evals.len(), 3);

        let case0 = &agent.evals[0];
        assert_eq!(case0.name, "short_for_active");
        assert_eq!(case0.assertions.len(), 2);
        assert_eq!(case0.assertions[0].kind, AgentEvalKind::Requires);
        match &case0.assertions[1].predicate {
            AgentEvalPredicate::Contains { lhs, rhs } => {
                assert_eq!(lhs, "output");
                assert_eq!(rhs, &ContainsRhs::Literal("active".to_owned()));
            }
            other => panic!("expected Contains, got {other:?}"),
        }

        let case1 = &agent.evals[1];
        assert_eq!(case1.name, "redacts_email");
        assert_eq!(case1.assertions[1].kind, AgentEvalKind::Forbids);
        match &case1.assertions[1].predicate {
            AgentEvalPredicate::Contains { lhs, rhs } => {
                assert_eq!(lhs, "output");
                assert_eq!(
                    rhs,
                    &ContainsRhs::SemanticType("@semantic.Email".to_owned())
                );
            }
            other => panic!("expected SemanticType Contains, got {other:?}"),
        }

        let case2 = &agent.evals[2];
        assert_eq!(case2.name, "uses_lookup_when_id_known");
        match &case2.assertions[1].predicate {
            AgentEvalPredicate::ToolsCalls { op, target } => {
                assert_eq!(*op, ToolsCallsOp::Includes);
                assert_eq!(target, "customer.query.by_id");
            }
            other => panic!("expected ToolsCalls, got {other:?}"),
        }
    }

    #[test]
    fn agent_with_discriminator_output_parses() {
        let source = r#"
feature customer_support
  agent classify_intent
    input
      message: Text required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./prompts/classify_intent.md"
"#;

        let features = parse_feature_skeletons(source).unwrap();
        let agent = &features[0].agents[0];
        assert_eq!(
            agent.output,
            Some(AgentOutput::Discriminator("Intent".to_owned()))
        );
    }

    #[test]
    fn agent_with_discriminated_record_output_parses() {
        // The parser sees `output Action` as a bare type reference and emits
        // `Plain`. Lowering disambiguates record-with-discriminator vs Text.
        let source = r#"
feature customer
  agent extract_action
    input
      message: Text required
    policy @policy.read
    output Action
    model @llm.default
    prompt "./prompts/extract.md"
"#;

        let features = parse_feature_skeletons(source).unwrap();
        let agent = &features[0].agents[0];
        assert_eq!(agent.output, Some(AgentOutput::Plain("Action".to_owned())));
    }

    #[test]
    fn workflow_keyword_is_retired() {
        // docs/proposals/lifecycle-vocab.md v0.3 §2.1 — `workflow` was
        // retired in favor of `lifecycle` (a child of resource, not a
        // feature-level block). The parser raises an explicit error so
        // cold-readers see one canonical form.
        let source = r#"
feature customer
  workflow lifecycle on Customer.status
    activate: lead -> active
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("retired") && msg.contains("lifecycle"),
            "expected retired+lifecycle in error, got: {msg}"
        );
    }

    #[test]
    fn agent_rejects_unknown_output_kind() {
        let source = r#"
feature customer
  agent bad_output
    input
      message: Text required
    policy @policy.read
    output stream discriminator Intent
    model @llm.default
    prompt "./prompts/x.md"
"#;

        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("output stream"),
            "error should mention `output stream` mis-shape: {err}"
        );
    }

    #[test]
    fn agent_with_golden_eval_parses() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case golden_quality
        requires output contains "active"
        golden "./evals/summarize.jsonl" min_score 0.85

      case golden_only
        golden "./evals/summarize_minimal.jsonl"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let evals = &features[0].agents[0].evals;
        assert_eq!(evals.len(), 2);

        let g0 = evals[0].golden.as_ref().expect("golden 0");
        assert_eq!(g0.path, "./evals/summarize.jsonl");
        assert_eq!(g0.min_score, Some(0.85));

        let g1 = evals[1].golden.as_ref().expect("golden 1");
        assert_eq!(g1.path, "./evals/summarize_minimal.jsonl");
        assert!(g1.min_score.is_none());
        assert!(
            evals[1].assertions.is_empty(),
            "case with only golden has zero assertions"
        );
    }

    #[test]
    fn agent_golden_rejects_out_of_range_score() {
        let source = r#"
feature customer
  agent flaky
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case bad
        requires output contains "ok"
        golden "./x.jsonl" min_score 1.5
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("0.0..=1.0"),
            "error should reject out-of-range min_score: {err}"
        );
    }

    #[test]
    fn agent_input_optional_slot_parses() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
      hint: Text optional
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./prompts/triage.md"
"#;

        let features = parse_feature_skeletons(source).unwrap();
        let agent = &features[0].agents[0];
        assert_eq!(agent.input.len(), 2);
        assert!(agent.input[0].required);
        assert!(!agent.input[0].optional);
        assert!(!agent.input[1].required);
        assert!(agent.input[1].optional);
    }

    #[test]
    fn feature_with_no_agents_yields_empty_skeleton() {
        // Non-agent feature children (resources, queries, commands, ...) are
        // skipped silently by the slice; the legacy pipeline owns them.
        let source = r#"
feature customer
  purpose "test"
  defaults
    tenancy org

  domain
    resource Customer
      name: Text required
"#;

        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].name, "customer");
        assert!(features[0].agents.is_empty());
    }

    #[test]
    fn parses_canonical_full_capsule_fixture() {
        // Smoke-check against the real canonical fixture. Confirms the
        // line-walker tolerates the actual indent pattern (2/4/6/8 spaces),
        // the comments and blank lines scattered through the file, and
        // every non-agent feature child it should skip.
        let source = include_str!("../../../../examples/full-capsule/full-capsule.lzi");
        let features = parse_feature_skeletons(source).expect("parses");

        // The fixture declares five features; the slice surfaces them all
        // with at least one agent on `customer` (`summarize_customer`).
        let customer = features
            .iter()
            .find(|f| f.name == "customer")
            .expect("customer feature");
        assert!(
            customer
                .agents
                .iter()
                .any(|a| a.name == "summarize_customer"),
            "expected summarize_customer agent in customer feature"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A.7 — `expose http` parser slice
    // -------------------------------------------------------------------------

    use crate::HttpMethod;

    #[test]
    fn agent_with_expose_http_minimal_parses() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:id/summary"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let agent = &features[0].agents[0];
        let expose = agent.expose.as_ref().expect("expose");
        assert_eq!(expose.method, HttpMethod::Post);
        assert_eq!(expose.path, "/api/customers/:id/summary");
    }

    #[test]
    fn agent_with_expose_http_full_parses() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
      route customer_id: Customer.ID
      audience admin
      rate_limit "5 per minute per user"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let expose = features[0].agents[0].expose.as_ref().expect("expose");
        assert_eq!(expose.route_slots.len(), 1);
        assert_eq!(expose.route_slots[0].name, "customer_id");
        assert_eq!(expose.route_slots[0].type_text, "Customer.ID");
        assert_eq!(expose.audience.as_deref(), Some("admin"));
        assert_eq!(
            expose.rate_limit_override.as_deref(),
            Some("5 per minute per user")
        );
    }

    #[test]
    fn agent_rejects_unknown_method_in_expose() {
        let source = r#"
feature customer
  agent broken
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method FROBNICATE
      path "/x"
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("method"),
            "error should mention method: {err}"
        );
    }

    #[test]
    fn agent_rejects_expose_http_without_method() {
        let source = r#"
feature customer
  agent broken
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      path "/x"
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("method"),
            "error should require method: {err}"
        );
    }

    #[test]
    fn agent_rejects_expose_http_without_path() {
        let source = r#"
feature customer
  agent broken
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("path"),
            "error should require path: {err}"
        );
    }

    #[test]
    fn agent_rejects_duplicate_expose_http_blocks() {
        let source = r#"
feature customer
  agent broken
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/a"
    expose http
      method GET
      path "/b"
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("at most one"),
            "error should mention duplicate: {err}"
        );
    }

    #[test]
    fn multiple_features_per_file_parse() {
        let source = r#"
feature customer
  agent first_agent
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./a.md"

feature customer_outreach
  agent second_agent
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./b.md"
"#;

        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 2);
        assert_eq!(features[0].name, "customer");
        assert_eq!(features[0].agents[0].name, "first_agent");
        assert_eq!(features[1].name, "customer_outreach");
        assert_eq!(features[1].agents[0].name, "second_agent");
    }

    // -------------------------------------------------------------------------
    // Phase L — `auth` block parser slice
    // -------------------------------------------------------------------------

    #[test]
    fn auth_minimal_identity_only_parses() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let auth = features[0].auth.as_ref().expect("auth block");
        assert_eq!(auth.identity.field, "Customer.email");
        assert!(auth.password.is_none());
        assert!(auth.sessions.is_none());
        assert!(auth.mfa.is_none());
        assert!(auth.oauth.is_empty());
    }

    #[test]
    fn auth_full_block_parses() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"

    oauth google
      adapter @adapter.google_oauth

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp

    sessions
      resource CustomerSession
      ttl "7 days"
      refresh false
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let auth = features[0].auth.as_ref().expect("auth block");

        assert_eq!(auth.identity.field, "Customer.email");

        let password = auth.password.as_ref().expect("password");
        assert_eq!(password.algorithm, "argon2id");
        assert_eq!(password.hash, "@fn.hash_customer_password");
        assert_eq!(password.verify, "@fn.verify_customer_password");
        let rate_limit = password.rate_limit.as_ref().expect("password rate_limit");
        assert_eq!(rate_limit.default.as_deref(), Some("5 per 10 minutes"));
        assert!(rate_limit.by_env.is_empty());

        assert_eq!(auth.oauth.len(), 1);
        assert_eq!(auth.oauth[0].provider, "google");
        assert_eq!(auth.oauth[0].adapter, "@adapter.google_oauth");

        let mfa = auth.mfa.as_ref().expect("mfa");
        assert_eq!(mfa.method, "totp");
        assert_eq!(mfa.enroll, "@fn.enroll_customer_totp");
        assert_eq!(mfa.verify, "@validator.verify_customer_totp");

        let sessions = auth.sessions.as_ref().expect("sessions");
        assert_eq!(sessions.resource, "CustomerSession");
        assert_eq!(sessions.ttl, "7 days");
        assert!(!sessions.refresh);
    }

    #[test]
    fn auth_without_identity_errors() {
        let source = r#"
feature customer_auth
  auth
    password
      algorithm argon2id
      hash @fn.h
      verify @fn.v
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("identity"),
            "error should require identity: {message}"
        );
    }

    #[test]
    fn auth_password_without_algorithm_errors() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      hash @fn.h
      verify @fn.v
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("algorithm"),
            "error should require algorithm: {message}"
        );
    }

    #[test]
    fn auth_mfa_without_enroll_errors() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    mfa totp
      verify @validator.totp
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("enroll"),
            "error should require enroll: {message}"
        );
    }

    #[test]
    fn auth_duplicate_block_errors() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

  auth
    identity Customer.email
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("at most one"),
            "error should mention duplicate: {message}"
        );
    }

    #[test]
    fn auth_identity_without_dot_errors() {
        let source = r#"
feature customer_auth
  auth
    identity customeremail
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("dot-qualified"),
            "error should require Resource.field: {message}"
        );
    }

    // Per `docs/proposals/auth-lowering-scope.md` Route A: dedicated
    // happy-path tests for each `auth` child block so a regression in
    // `parse_auth_password` / `parse_auth_sessions` / `parse_auth_oauth` /
    // `parse_auth_mfa` is pinned to the specific child instead of being
    // detected only by the multi-child happy-path test.

    #[test]
    fn auth_password_child_parses_with_rate_limit() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let password = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .password
            .as_ref()
            .expect("password child");
        assert_eq!(password.algorithm, "argon2id");
        assert_eq!(password.hash, "@fn.hash_customer_password");
        assert_eq!(password.verify, "@fn.verify_customer_password");
        let rate_limit = password.rate_limit.as_ref().expect("password rate_limit");
        assert_eq!(rate_limit.default.as_deref(), Some("5 per 10 minutes"));
        assert!(rate_limit.by_env.is_empty());
    }

    #[test]
    fn auth_sessions_child_parses_with_refresh_true() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "30 days"
      refresh true
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let sessions = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child");
        assert_eq!(sessions.resource, "CustomerSession");
        assert_eq!(sessions.ttl, "30 days");
        assert!(sessions.refresh);
        assert!(sessions.access_ttl.is_none());
        assert!(sessions.rotation.is_none());
    }

    #[test]
    fn auth_sessions_child_defaults_legacy_refresh_false_when_omitted() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let sessions = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child");
        assert_eq!(sessions.resource, "CustomerSession");
        assert_eq!(sessions.ttl, "7 days");
        assert!(!sessions.refresh);
        assert!(sessions.access_ttl.is_none());
        assert!(sessions.rotation.is_none());
    }

    #[test]
    fn auth_sessions_child_parses_nested_rotation_block() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      access_ttl "15 minutes"
      rotation
        refresh_ttl "30 days"
        grace "30 seconds"
        theft_detection_action revoke_session_family
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let sessions = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child");
        assert_eq!(
            sessions.access_ttl.as_ref().map(|ttl| ttl.value.as_str()),
            Some("15 minutes")
        );
        assert!(sessions.access_ttl.as_ref().unwrap().span.end > 0);

        let rotation = sessions.rotation.as_ref().expect("rotation block");
        assert!(rotation.span.end > rotation.span.start);
        assert_eq!(
            rotation.refresh_ttl.as_ref().map(|ttl| ttl.value.as_str()),
            Some("30 days")
        );
        assert_eq!(
            rotation.grace.as_ref().map(|grace| grace.value.as_str()),
            Some("30 seconds")
        );
        assert_eq!(
            rotation
                .theft_detection_action
                .as_ref()
                .map(|action| action.action),
            Some(crate::AuthTheftDetectionAction::RevokeSessionFamily)
        );
    }

    #[test]
    fn auth_sessions_child_parses_empty_rotation_block() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      rotation
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let rotation = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .sessions
            .as_ref()
            .expect("sessions child")
            .rotation
            .as_ref()
            .expect("rotation block");
        assert!(rotation.refresh_ttl.is_none());
        assert!(rotation.grace.is_none());
        assert!(rotation.theft_detection_action.is_none());
    }

    #[test]
    fn auth_sessions_rotation_rejects_unknown_theft_action() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      rotation
        theft_detection_action quarantine_device
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("unknown `theft_detection_action`"),
            "error should mention closed-catalog theft action: {message}"
        );
    }

    #[test]
    fn auth_oauth_child_parses_multiple_providers() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    oauth google
      adapter @adapter.google_oauth

    oauth github
      adapter @adapter.github_oauth
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let oauth = &features[0].auth.as_ref().expect("auth").oauth;
        assert_eq!(oauth.len(), 2);
        assert_eq!(oauth[0].provider, "google");
        assert_eq!(oauth[0].adapter, "@adapter.google_oauth");
        assert_eq!(oauth[1].provider, "github");
        assert_eq!(oauth[1].adapter, "@adapter.github_oauth");
    }

    #[test]
    fn auth_mfa_child_parses_with_validator_verify() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let mfa = features[0]
            .auth
            .as_ref()
            .expect("auth")
            .mfa
            .as_ref()
            .expect("mfa child");
        assert_eq!(mfa.method, "totp");
        assert_eq!(mfa.enroll, "@fn.enroll_customer_totp");
        assert_eq!(mfa.verify, "@validator.verify_customer_totp");
        assert!(mfa.adapter.is_none());
    }

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
    // L0 #2 — `design.lzi` parser tests.
    // -------------------------------------------------------------------------

    #[test]
    fn design_parses_minimal_color_only() {
        let source = r##"
design example
  color
    success "#16a34a"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.name, "example");
        assert!(ast.extends.is_none());
        assert_eq!(ast.colors.len(), 1);
        assert_eq!(ast.colors[0].name, "success");
        assert_eq!(ast.colors[0].states.len(), 1);
        assert_eq!(ast.colors[0].states[0].kind, "base");
        assert_eq!(ast.colors[0].states[0].value, "#16a34a");
        assert!(ast.colors[0].states[0].dark.is_none());
    }

    #[test]
    fn design_parses_full_eight_group_fixture() {
        // Mirror of `docs/proposals/design-tokens.md` §8.1 (example brand
        // example). Exercises all eight closed groups + dark suffix + the
        // digit-leading `"2xl"` quoted name.
        let source = r##"
design example
  color
    primary
      base "#7c3aed"
      hover "#6d28d9"
      foreground "#ffffff"
    background
      base "#ffffff" dark "#09090b"
      muted "#f4f4f5" dark "#18181b"
    foreground
      base "#09090b" dark "#fafafa"
      muted "#71717a" dark "#a1a1aa"
    success "#16a34a"
    warning "#ea580c"
    danger  "#dc2626"

  typography
    family
      sans "Inter, system-ui, sans-serif"
      mono "JetBrains Mono, monospace"
    scale
      sm    size 0.875rem, line_height 1.25rem
      base  size 1rem,     line_height 1.5rem
      lg    size 1.125rem, line_height 1.75rem
      xl    size 1.25rem,  line_height 1.75rem
      "2xl" size 1.5rem,   line_height 2rem
    weight
      regular 400
      medium 500
      semibold 600
      bold 700
    tracking
      tight -0.025em
      normal 0
      wide 0.025em

  space
    "1" 0.25rem
    "2" 0.5rem
    "3" 0.75rem
    "4" 1rem

  radius
    sm 0.125rem
    base 0.25rem
    md 0.375rem

  shadow
    sm "0 1px 2px 0 rgb(0 0 0 / 0.05)"
    base "0 1px 3px 0 rgb(0 0 0 / 0.1)"
    md "0 4px 6px -1px rgb(0 0 0 / 0.1)"

  motion
    duration
      fast 150ms
      base 200ms
    easing
      out "cubic-bezier(0, 0, 0.2, 1)"

  breakpoint
    sm 640px
    md 768px
    lg 1024px

  z
    dropdown 1000
    modal 1300
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.name, "example");
        // Color group: primary + background + foreground + 3 flat semantic.
        assert_eq!(ast.colors.len(), 6);
        // Color sub-block lift.
        assert_eq!(ast.colors[0].name, "primary");
        assert_eq!(ast.colors[0].states.len(), 3);
        // Dark suffix lift.
        let bg = &ast.colors[1];
        assert_eq!(bg.name, "background");
        assert_eq!(bg.states[0].dark.as_deref(), Some("#09090b"));
        // Typography sub-groups.
        assert_eq!(ast.typography.families.len(), 2);
        assert_eq!(ast.typography.scale.len(), 5);
        assert_eq!(ast.typography.scale[4].name, "2xl");
        assert_eq!(ast.typography.scale[4].size, "1.5rem");
        assert_eq!(ast.typography.weights.len(), 4);
        assert_eq!(ast.typography.weights[0].value, "400");
        assert_eq!(ast.typography.tracking.len(), 3);
        // Scale groups.
        assert_eq!(ast.spaces.len(), 4);
        assert_eq!(ast.radii.len(), 3);
        assert_eq!(ast.breakpoints.len(), 3);
        // Shadow + motion + z.
        assert_eq!(ast.shadows.len(), 3);
        assert_eq!(ast.motion.durations.len(), 2);
        assert_eq!(ast.motion.easings.len(), 1);
        assert_eq!(ast.motion.easings[0].value, "cubic-bezier(0, 0, 0.2, 1)");
        assert_eq!(ast.z_indices.len(), 2);
    }

    #[test]
    fn design_color_sub_block_with_four_states() {
        let source = r##"
design example
  color
    primary
      base "#7c3aed"
      hover "#6d28d9"
      active "#5b21b6"
      foreground "#ffffff"
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.colors.len(), 1);
        assert_eq!(ast.colors[0].name, "primary");
        assert_eq!(ast.colors[0].states.len(), 4);
        let kinds: Vec<&str> = ast.colors[0]
            .states
            .iter()
            .map(|s| s.kind.as_str())
            .collect();
        assert_eq!(kinds, vec!["base", "hover", "active", "foreground"]);
        assert_eq!(ast.colors[0].states[0].value, "#7c3aed");
        assert_eq!(ast.colors[0].states[3].value, "#ffffff");
    }

    #[test]
    fn design_color_captures_dark_suffix() {
        let source = r##"
design example
  color
    background
      base "#ffffff" dark "#09090b"
      muted "#f4f4f5" dark "#18181b"
"##;
        let ast = super::parse_design_document(source).unwrap();
        let bg = &ast.colors[0];
        assert_eq!(bg.name, "background");
        assert_eq!(bg.states[0].value, "#ffffff");
        assert_eq!(bg.states[0].dark.as_deref(), Some("#09090b"));
        assert_eq!(bg.states[1].value, "#f4f4f5");
        assert_eq!(bg.states[1].dark.as_deref(), Some("#18181b"));
    }

    #[test]
    fn design_typography_scale_pairs_size_and_line_height() {
        let source = r##"
design example
  typography
    scale
      base size 1rem, line_height 1.5rem
      lg   size 1.125rem, line_height 1.75rem
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.typography.scale.len(), 2);
        let base = &ast.typography.scale[0];
        assert_eq!(base.name, "base");
        assert_eq!(base.size, "1rem");
        assert_eq!(base.line_height, "1.5rem");
        let lg = &ast.typography.scale[1];
        assert_eq!(lg.name, "lg");
        assert_eq!(lg.size, "1.125rem");
        assert_eq!(lg.line_height, "1.75rem");
    }

    #[test]
    fn design_extends_keyword_parses() {
        let source = r##"
design alpha
  extends base
  color
    primary
      base "#10b981"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.name, "alpha");
        assert_eq!(ast.extends.as_deref(), Some("base"));
    }

    #[test]
    fn design_digit_prefix_names_require_quotes() {
        let source = r##"
design example
  space
    "1" 0.25rem
    "2" 0.5rem
  breakpoint
    "2xl" 1536px
    "3xl" 1920px
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.spaces[0].name, "1");
        assert_eq!(ast.spaces[0].value, "0.25rem");
        assert_eq!(ast.spaces[1].name, "2");
        assert_eq!(ast.breakpoints[0].name, "2xl");
        assert_eq!(ast.breakpoints[0].value, "1536px");
        assert_eq!(ast.breakpoints[1].name, "3xl");
    }

    #[test]
    fn design_shadow_quoted_strings_preserved_intact() {
        let source = r##"
design example
  shadow
    sm "0 1px 2px 0 rgb(0 0 0 / 0.05)"
    base "0 1px 3px 0 rgb(0 0 0 / 0.1)"
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.shadows.len(), 2);
        assert_eq!(ast.shadows[0].name, "sm");
        assert_eq!(ast.shadows[0].value, "0 1px 2px 0 rgb(0 0 0 / 0.05)");
        assert_eq!(ast.shadows[1].value, "0 1px 3px 0 rgb(0 0 0 / 0.1)");
    }

    #[test]
    fn design_z_values_parsed_as_strings() {
        let source = r##"
design example
  z
    docked 10
    modal 1300
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.z_indices.len(), 2);
        assert_eq!(ast.z_indices[0].name, "docked");
        assert_eq!(ast.z_indices[0].value, "10");
        assert_eq!(ast.z_indices[1].value, "1300");
    }

    #[test]
    fn design_tracking_accepts_negative_value() {
        let source = r##"
design example
  typography
    tracking
      tight -0.025em
      normal 0
      wide 0.025em
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert_eq!(ast.typography.tracking.len(), 3);
        assert_eq!(ast.typography.tracking[0].name, "tight");
        assert_eq!(ast.typography.tracking[0].value, "-0.025em");
        assert_eq!(ast.typography.tracking[1].value, "0");
        assert_eq!(ast.typography.tracking[2].value, "0.025em");
    }

    #[test]
    fn design_empty_motion_block_skips_cleanly() {
        // `motion` header with no children should leave the AST defaults intact.
        let source = r##"
design example
  color
    success "#16a34a"
  motion
"##;
        let ast = super::parse_design_document(source).unwrap();
        assert!(ast.motion.durations.is_empty());
        assert!(ast.motion.easings.is_empty());
        // Sibling group still parsed.
        assert_eq!(ast.colors.len(), 1);
    }

    #[test]
    fn design_rejects_unknown_group_keyword() {
        let source = r##"
design example
  bogus
    foo bar
"##;
        let err = super::parse_design_document(source).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("design children"),
            "expected unknown-group diagnostic, got: {msg}"
        );
    }

    // ── `custom` 9th meta-group ──────────────────────────────────────────────
    // Per `docs/proposals/design-tokens-custom.md` §2.

    #[test]
    fn design_custom_group_parses_flat_entries() {
        let source = r##"
design hostpoint
  custom
    chat-bubble-mine "#dcf8c6"
    chat-bubble-other "#ffffff"
    map-marker-active "#ff5722"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.custom.len(), 3);
        assert_eq!(ast.custom[0].name, "chat-bubble-mine");
        assert_eq!(ast.custom[0].value, "#dcf8c6");
        assert!(ast.custom[0].dark.is_none());
        assert_eq!(ast.custom[1].name, "chat-bubble-other");
        assert_eq!(ast.custom[2].name, "map-marker-active");
    }

    #[test]
    fn design_custom_entry_captures_dark_suffix() {
        let source = r##"
design hostpoint
  custom
    chat-bubble-mine "#dcf8c6" dark "#005c4b"
    chat-bubble-other "#ffffff" dark "#202c33"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.custom.len(), 2);
        assert_eq!(ast.custom[0].value, "#dcf8c6");
        assert_eq!(ast.custom[0].dark.as_deref(), Some("#005c4b"));
        assert_eq!(ast.custom[1].dark.as_deref(), Some("#202c33"));
    }

    #[test]
    fn design_custom_group_coexists_with_color_group() {
        let source = r##"
design hostpoint
  color
    primary "#28bbdd"
  custom
    chat-bubble "#dcf8c6"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert_eq!(ast.colors.len(), 1);
        assert_eq!(ast.colors[0].name, "primary");
        assert_eq!(ast.custom.len(), 1);
        assert_eq!(ast.custom[0].name, "chat-bubble");
    }

    #[test]
    fn design_custom_entry_requires_value() {
        let source = r##"
design hostpoint
  custom
    chat-bubble
"##;
        let err = super::parse_design_document(source).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("custom entry requires"), "got: {msg}");
    }

    #[test]
    fn design_custom_empty_block_skips_cleanly() {
        // `custom` header with no children should not crash; the field
        // remains an empty Vec.
        let source = r##"
design hostpoint
  custom
  color
    primary "#28bbdd"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert!(ast.custom.is_empty());
        assert_eq!(ast.colors.len(), 1);
    }

    #[test]
    fn design_without_custom_group_still_parses() {
        // Regression: pre-Z2 `design.lzi` blocks must keep parsing.
        let source = r##"
design legacy
  color
    primary "#28bbdd"
"##;
        let ast = super::parse_design_document(source).expect("parses");
        assert!(ast.custom.is_empty());
        assert_eq!(ast.colors.len(), 1);
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
            Some(super::RouteRedirectTargetAst::View("sign_in".to_string()))
        );
        assert_eq!(route.role_mismatch.len(), 2);
        assert_eq!(route.role_mismatch[0].role, "traveler");
        assert_eq!(
            route.role_mismatch[0].target,
            super::RouteRedirectTargetAst::View("explore".to_string())
        );
        assert_eq!(
            route.default,
            Some(super::RouteRedirectTargetAst::Path("/welcome".to_string()))
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
        assert_eq!(errors.default, Some(super::ErrorExposureDefaultAst::Hide));
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
        let source = include_str!("../../../../examples/full-capsule/full-capsule.lzi");
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
        assert_eq!(errors.default, Some(super::ErrorExposureDefaultAst::Hide));
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
// Cache bucket cycle (CL.C.3) — feature-level `cache <name>` profile
// parser tests.
// =============================================================================
#[cfg(test)]
mod cache_profile_parser_tests {
    use super::parse_feature_skeletons;

    #[test]
    fn cache_profile_required_only_parses() {
        // Minimal profile: just `key` + `ttl`.
        let source = r#"
feature catalog
  cache product_view
    key "product:{product_id}"
    ttl 5m
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features[0].caches.len(), 1);
        let p = &features[0].caches[0];
        assert_eq!(p.name, "product_view");
        assert_eq!(p.key, "\"product:{product_id}\"");
        assert_eq!(p.ttl, "5m");
        assert!(p.namespace.is_none());
        assert!(p.tags.is_empty());
        assert!(p.stale_while_revalidate.is_none());
        assert!(p.coalesce.is_none());
        assert!(p.sliding.is_none());
    }

    #[test]
    fn cache_profile_full_body_parses() {
        // Every CL.C.3 decorator on one profile.
        let source = r#"
feature catalog
  cache product_view
    key "product:{product_id}"
    ttl 5m
    namespace catalog
    tags product, listing
    stale_while_revalidate 30s
    coalesce true
    sliding true
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let p = &features[0].caches[0];
        assert_eq!(p.namespace.as_deref(), Some("catalog"));
        assert_eq!(p.tags, vec!["product".to_owned(), "listing".to_owned()]);
        assert_eq!(p.stale_while_revalidate.as_deref(), Some("30s"));
        assert_eq!(p.coalesce, Some(true));
        assert_eq!(p.sliding, Some(true));
    }

    #[test]
    fn query_cache_reference_parses() {
        // A query opting into a profile via `cache <name>`.
        let source = r#"
feature catalog
  cache product_view
    key "product:{product_id}"
    ttl 5m

  domain
    query.list list
      cache product_view
"#;
        let features = parse_feature_skeletons(source).unwrap();
        match &features[0].queries[0] {
            crate::QueryDecl::List(q) => {
                assert_eq!(q.cache_profile_ref.as_deref(), Some("product_view"));
                assert!(q.cache.is_empty(), "inline cache must be empty");
            }
            other => panic!("expected query.list, got {other:?}"),
        }
    }

    #[test]
    fn query_cache_inline_and_reference_rejects() {
        // The mutually-exclusive guard rejects both forms on one query.
        let source = r#"
feature catalog
  cache product_view
    key "product:{product_id}"
    ttl 5m

  domain
    query.list list
      cache product_view
      cache
        key "extra"
        ttl 10m
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("inline `cache` block or a `cache <profile>` reference"),
            "expected exclusivity error, got: {msg}"
        );
    }
}

// =============================================================================
// Report vocab — `report <name>` parser tests.
// =============================================================================
#[cfg(test)]
mod report_parser_tests {
    use super::parse_feature_skeletons;
    use crate::ast::ReportColumnSourceAst;

    #[test]
    fn report_full_block_parses() {
        let source = r#"
feature customer
  report monthly_audit
    source customer.query.list
    columns
      id from row.id
      name from row.name
      tier from row.tier label "Plano"
      ltv from @fn.lifetime_value(row.id) label "Valor de vida"
      created_at from row.created_at format "yyyy-mm-dd"
    formats csv, xlsx
    storage object_storage.files
    visibility signed
    signed_ttl 1h
    filename "monthly_audit_{ctx.now:yyyymm}.{format}"
    policy @policy.global_read
    rate_limit "10 per hour per user"
    audit actor, ctx.now, source.params
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].reports.len(), 1);
        let report = &features[0].reports[0];
        assert_eq!(report.name, "monthly_audit");
        assert_eq!(report.source, "customer.query.list");
        assert_eq!(report.columns.len(), 5);
        assert!(matches!(
            &report.columns[0].source,
            ReportColumnSourceAst::RowField(f) if f == "id"
        ));
        assert!(matches!(
            &report.columns[3].source,
            ReportColumnSourceAst::FnCall { name, args }
                if name == "lifetime_value" && args == &["row.id"]
        ));
        assert_eq!(report.columns[2].label.as_deref(), Some("Plano"));
        assert_eq!(report.columns[4].format.as_deref(), Some("yyyy-mm-dd"));
        assert_eq!(report.formats, vec!["csv".to_owned(), "xlsx".to_owned()]);
        assert_eq!(report.storage.as_deref(), Some("object_storage.files"));
        assert_eq!(report.visibility.as_deref(), Some("signed"));
        assert_eq!(report.signed_ttl.as_deref(), Some("1h"));
        assert_eq!(report.policy.as_deref(), Some("@policy.global_read"));
        let audit = report.audit.as_ref().expect("audit");
        assert_eq!(
            audit.subjects,
            vec![
                "actor".to_owned(),
                "ctx.now".to_owned(),
                "source.params".to_owned()
            ]
        );
    }

    #[test]
    fn report_missing_source_errors() {
        let source = r#"
feature customer
  report broken
    columns
      id from row.id
    formats csv
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("source"));
    }

    #[test]
    fn report_missing_formats_errors() {
        let source = r#"
feature customer
  report broken
    source customer.query.list
    columns
      id from row.id
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("formats"));
    }

    #[test]
    fn report_column_unknown_source_errors() {
        let source = r#"
feature customer
  report broken
    source customer.query.list
    columns
      id from bogus.id
    formats csv
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("row.<field>") || err.to_string().contains("@fn"));
    }
}

// =============================================================================
// L0 #3 §10 — inline field constraint parser tests (Cell D.1).
// =============================================================================
#[cfg(test)]
mod field_constraint_parser_tests {
    use super::parse_feature_skeletons;

    /// `key: Text required min 2 max 80 pattern "^[a-z0-9-]+$"` —
    /// the canonical proposal §10 example. Constraints stack with
    /// `required` modifier; type_text remains `Text`.
    #[test]
    fn resource_field_text_min_max_pattern() {
        let source = r#"
feature slug
  domain
    resource Slug
      key: Text required min 2 max 80 pattern "^[a-z0-9-]+$"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let field = &features[0].resources[0].fields[0];
        assert_eq!(field.name, "key");
        assert_eq!(field.type_text, "Text");
        assert!(field.required);
        assert_eq!(field.constraints.min, Some(2));
        assert_eq!(field.constraints.max, Some(80));
        assert_eq!(field.constraints.pattern.as_deref(), Some("^[a-z0-9-]+$"));
    }

    /// `between A and B` on Integer parses as a two-tuple.
    #[test]
    fn resource_field_integer_between() {
        let source = r#"
feature person
  domain
    resource Person
      age: Integer between 0 and 150
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let field = &features[0].resources[0].fields[0];
        assert_eq!(field.name, "age");
        assert_eq!(field.constraints.between, Some((0, 150)));
        assert!(field.constraints.min.is_none());
        assert!(field.constraints.max.is_none());
    }

    /// `in ["admin", "editor", "viewer"]` on Text parses the
    /// list and strips surrounding quotes.
    #[test]
    fn resource_field_text_in_list() {
        let source = r#"
feature acl
  domain
    resource Member
      role: Text in ["admin", "editor", "viewer"]
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let field = &features[0].resources[0].fields[0];
        assert_eq!(field.name, "role");
        assert_eq!(
            field.constraints.r#in.as_deref(),
            Some(&["admin".to_owned(), "editor".to_owned(), "viewer".to_owned()][..])
        );
    }

    /// `length N` on Text captures exact length.
    #[test]
    fn resource_field_text_length() {
        let source = r#"
feature post
  domain
    resource Post
      title: Text length 120
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let field = &features[0].resources[0].fields[0];
        assert_eq!(field.constraints.length, Some(120));
    }

    /// Constraints before the default literal parse correctly.
    #[test]
    fn resource_field_constraints_before_default() {
        let source = r#"
feature counter
  domain
    resource Counter
      score: Integer min 0 max 100 = 50
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let field = &features[0].resources[0].fields[0];
        assert_eq!(field.constraints.min, Some(0));
        assert_eq!(field.constraints.max, Some(100));
        assert_eq!(field.default.as_deref(), Some("50"));
    }

    /// Command input slots pick up the same constraint catalog.
    #[test]
    fn command_input_slot_min_max_pattern() {
        let source = r#"
feature slug
  command create
    policy @policy.create
    input
      key: Text required min 2 max 80 pattern "^[a-z]+$"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let cmd = &features[0].commands[0];
        let slots = match &cmd.input {
            crate::CommandInputDecl::Typed(s) => s,
            _ => panic!("expected typed input"),
        };
        assert_eq!(slots[0].name, "key");
        assert_eq!(slots[0].constraints.min, Some(2));
        assert_eq!(slots[0].constraints.max, Some(80));
        assert_eq!(slots[0].constraints.pattern.as_deref(), Some("^[a-z]+$"));
        assert!(slots[0].required);
    }

    #[test]
    fn command_write_window_parses_duration_literal() {
        let source = r#"
feature billing
  command create_invoice
    input customer, issued_at
    write_window by input.issued_at within 30d
    policy @policy.create
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let write_window = features[0].commands[0]
            .write_window
            .as_ref()
            .expect("write_window");
        assert_eq!(write_window.by, "input.issued_at");
        assert_eq!(write_window.within, "30d");
    }

    #[test]
    fn command_write_window_requires_by() {
        let source = r#"
feature billing
  command create_invoice
    write_window input.issued_at within 30d
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("write_window"));
    }

    #[test]
    fn command_write_window_requires_within() {
        let source = r#"
feature billing
  command create_invoice
    write_window by input.issued_at
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("within"));
    }

    #[test]
    fn command_triggers_transition_parses_canonical_and_legacy_shapes() {
        let source = r#"
feature order
  command submit
    triggers transition approve
  command fulfill
    triggers transition approve, capture_payment, ship
  command legacy_inline
    triggers approve, capture_payment
  command legacy_block
    triggers
      transition approve
      transition capture_payment, ship
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(features[0].commands[0].triggers, vec!["approve".to_owned()]);
        assert_eq!(
            features[0].commands[1].triggers,
            vec![
                "approve".to_owned(),
                "capture_payment".to_owned(),
                "ship".to_owned()
            ]
        );
        assert_eq!(
            features[0].commands[2].triggers,
            vec!["approve".to_owned(), "capture_payment".to_owned()]
        );
        assert_eq!(
            features[0].commands[3].triggers,
            vec![
                "approve".to_owned(),
                "capture_payment".to_owned(),
                "ship".to_owned()
            ]
        );

        let trailing = r#"
feature order
  command broken
    triggers transition approve,
"#;
        let err = parse_feature_skeletons(trailing).unwrap_err();
        assert!(err.to_string().contains("empty entry"));
    }
}

// =============================================================================
// L0 #3 — `.lzx` surface parser tests.
// =============================================================================

#[cfg(test)]
mod deprecated_parser_tests {
    use super::parse_feature_skeletons;

    #[test]
    fn command_deprecated_block_parses() {
        let source = r#"feature customer
  command legacy_update
    policy @policy.update
    creates Customer
    deprecated
      since "2026-03-01"
      replacement command.update_v2
      sunset "2026-12-31"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let dep = features[0].commands[0].deprecated.as_ref().unwrap();
        assert_eq!(dep.since.as_deref(), Some("2026-03-01"));
        assert_eq!(dep.replacement.as_deref(), Some("command.update_v2"));
        assert_eq!(dep.sunset.as_deref(), Some("2026-12-31"));
    }

    #[test]
    fn command_deprecated_inline_parses() {
        let source = r#"feature customer
  command legacy_update
    policy @policy.update
    deprecated since "2026-03-01" replacement command.update_v2 sunset "2026-12-31"
    creates Customer
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(
            features[0].commands[0]
                .deprecated
                .as_ref()
                .unwrap()
                .replacement
                .as_deref(),
            Some("command.update_v2")
        );
    }

    #[test]
    fn command_deprecated_bare_parses() {
        let source = "feature customer\n  command legacy_update\n    policy @policy.update\n    deprecated\n    creates Customer\n";
        let features = parse_feature_skeletons(source).unwrap();
        let dep = features[0].commands[0].deprecated.as_ref().unwrap();
        assert!(dep.since.is_none());
        assert!(dep.replacement.is_none());
        assert!(dep.sunset.is_none());
    }

    #[test]
    fn api_deprecated_block_parses() {
        let source = r#"feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    policy @policy.read
    deprecated
      since "2026-04-01"
      replacement api.export_v2
      sunset "2026-09-30"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let dep = features[0].apis[0].deprecated.as_ref().unwrap();
        assert_eq!(dep.since.as_deref(), Some("2026-04-01"));
        assert_eq!(dep.replacement.as_deref(), Some("api.export_v2"));
        assert_eq!(dep.sunset.as_deref(), Some("2026-09-30"));
    }

    #[test]
    fn api_deprecated_inline_parses() {
        let source = r#"feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated since "2026-04-01" replacement api.export_v2 sunset "2026-09-30"
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(
            features[0].apis[0]
                .deprecated
                .as_ref()
                .unwrap()
                .replacement
                .as_deref(),
            Some("api.export_v2")
        );
    }

    #[test]
    fn api_deprecated_bare_parses() {
        let source = "feature customer\n  api legacy_export\n    method GET\n    path \"/api/customers/export-v1\"\n    output [Customer]\n    deprecated\n";
        let features = parse_feature_skeletons(source).unwrap();
        let dep = features[0].apis[0].deprecated.as_ref().unwrap();
        assert!(dep.since.is_none());
        assert!(dep.replacement.is_none());
        assert!(dep.sunset.is_none());
    }
}

// =============================================================================
// codegen-correctness-cycle-2 cell IA1 — `invalidates` block parser tests.
//
// Authoring syntax (cross-resource cache invalidation under `command`):
//
//   command save_X
//     ...
//     effect updates X
//     invalidates
//       query.lookup_my_X
//       feature_y.query.list_by_x
//
// The block form is the canonical authoring shape — each entry is a
// qualified query reference (bare `<query_name>` segments today; the
// optional leading feature segment routes to a sibling feature).
// Same-feature single-line form (`invalidates query.<name>`) is kept
// for backward compatibility but not exercised here.
// =============================================================================
#[cfg(test)]
mod invalidates_parser_tests {
    use super::parse_feature_skeletons;

    #[test]
    fn invalidates_block_parses_same_and_cross_feature() {
        let source = "feature customer\n  command save_X\n    input\n      id: ID required\n    policy @policy.update\n    updates Customer\n      tier = input.tier\n    invalidates\n      query.lookup_my_X\n      feature_y.query.list_by_x\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let command = &features[0].commands[0];

        assert_eq!(command.name, "save_X");
        assert_eq!(command.invalidates.len(), 2);
        // Same-feature entry — bare `query.<name>` form.
        assert_eq!(command.invalidates[0].query, "query.lookup_my_X");
        assert!(command.invalidates[0].args.is_empty());
        // Cross-feature entry — `<feature>.query.<name>` form.
        assert_eq!(command.invalidates[1].query, "feature_y.query.list_by_x");
        assert!(command.invalidates[1].args.is_empty());
    }

    #[test]
    fn invalidates_block_parses_with_named_args() {
        let source = "feature customer\n  command reassign\n    route id: ID\n    input\n      owner_id: ID required\n    policy @policy.update\n    updates Customer\n      owner_id = input.owner_id\n    invalidates\n      query.list\n      query.by_id(id: route.id)\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let invalidates = &features[0].commands[0].invalidates;

        assert_eq!(invalidates.len(), 2);
        assert_eq!(invalidates[0].query, "query.list");
        assert_eq!(invalidates[1].query, "query.by_id");
        assert_eq!(invalidates[1].args.len(), 1);
        assert_eq!(invalidates[1].args[0].name, "id");
        assert_eq!(invalidates[1].args[0].value, "route.id");
    }

    #[test]
    fn invalidates_block_requires_grandchild_indent() {
        // Entries at indent 4 (sibling indent) fall back to the
        // command-child dispatcher and surface its "children are …"
        // diagnostic. The grammar gate is: only indent-6 (grandchild)
        // lines after `invalidates` are entries.
        let source = "feature customer\n  command save_X\n    policy @policy.update\n    updates Customer\n    invalidates\n    query.list\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("invalidates") || msg.contains("`command` children"),
            "expected grammar diagnostic, got: {msg}"
        );
    }

    #[test]
    fn invalidates_block_rejects_unclosed_call() {
        // A call expression that opens `(` but never closes is rejected
        // up-front — covers the per-entry parse without depending on the
        // analyzer's downstream resolution pass.
        let source = "feature customer\n  command save_X\n    policy @policy.update\n    updates Customer\n    invalidates\n      query.by_id(id: route.id\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains(")") || msg.contains("call expression"),
            "expected unclosed-call diagnostic, got: {msg}"
        );
    }
}

// =============================================================================
// L0 #8 — `poller` block parser tests (docs/proposals/poller-vocab.md §3)
// =============================================================================
#[cfg(test)]
mod poller_parser_tests {
    use super::parse_feature_skeletons;

    #[test]
    fn poller_block_parses_minimal() {
        let source = "feature multi_bank\n  poller v8_consult_resolver\n    source V8PendingConsult\n    cursor\n      eligible_when next_check_at, resolved_at\n      attempts attempts\n    retry\n      max_attempts 30\n      backoff exponential base 30s cap 10m\n    states\n      pending initial\n      resolved terminal\n      failed terminal\n    resolve via @fn.poll_v8\n    idempotency by row.id, row.attempts\n";
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        let pollers = &features[0].pollers;
        assert_eq!(pollers.len(), 1);
        let p = &pollers[0];
        assert_eq!(p.name, "v8_consult_resolver");
        assert_eq!(p.source, "V8PendingConsult");
        let cursor = p.cursor.as_ref().unwrap();
        assert_eq!(cursor.next_at_field, "next_check_at");
        assert_eq!(cursor.resolved_at_field, "resolved_at");
        assert_eq!(cursor.attempts_field, "attempts");
        let retry = p.retry.as_ref().unwrap();
        assert_eq!(retry.max_attempts, 30);
        assert_eq!(retry.backoff_strategy, "exponential");
        assert_eq!(retry.backoff_base.as_deref(), Some("30s"));
        assert_eq!(retry.backoff_cap.as_deref(), Some("10m"));
        assert_eq!(p.states.len(), 3);
        assert_eq!(p.states[0].name, "pending");
        assert_eq!(p.states[0].kind_keyword.as_deref(), Some("initial"));
        assert_eq!(p.resolve_handler.as_deref(), Some("poll_v8"));
        assert_eq!(p.idempotency, vec!["row.id", "row.attempts"]);
    }

    #[test]
    fn poller_block_parses_full_with_quirk() {
        let source = "feature multi_bank\n  poller v8_consult_resolver\n    source V8PendingConsult\n    cursor\n      eligible_when next_check_at, resolved_at\n      attempts attempts\n    retry\n      max_attempts 30\n      backoff exponential base 30s cap 10m\n    states\n      pending initial\n      gender_ambiguous intermediate\n      resolved terminal\n      failed terminal\n    resolve via @fn.poll_v8\n    terminal_status_field final_status\n    terminal_result_field final_resultado\n    tick every 15s batch 100\n    tenant_from row.org_id\n    idempotency by row.id, row.attempts\n    audit default\n    emits v8_consult_resolved\n    emits v8_consult_failed\n    retry_quirk gender_flip_once\n      when row.status_v8 == \"gender_ambiguous\"\n      counter gender_retry_count\n      mutate row.gender = flip(row.gender)\n";
        let features = parse_feature_skeletons(source).unwrap();
        let p = &features[0].pollers[0];
        assert_eq!(p.tick.as_ref().unwrap().every, "15s");
        assert_eq!(p.tick.as_ref().unwrap().batch, Some(100));
        assert_eq!(p.tenant_from.as_deref(), Some("row.org_id"));
        assert_eq!(p.terminal_status_field.as_deref(), Some("final_status"));
        assert_eq!(p.terminal_result_field.as_deref(), Some("final_resultado"));
        assert_eq!(p.audit.as_deref(), Some("audit default"));
        assert_eq!(p.emits.len(), 2);
        assert_eq!(p.retry_quirks.len(), 1);
        let q = &p.retry_quirks[0];
        assert_eq!(q.kind, "gender_flip_once");
        assert_eq!(q.counter_field, "gender_retry_count");
        assert_eq!(q.mutate_field, "gender");
        assert_eq!(q.mutate_transform, "flip(row.gender)");
    }

    #[test]
    fn poller_requires_source() {
        let source = "feature multi_bank\n  poller bad\n    cursor\n      eligible_when next_check_at, resolved_at\n      attempts attempts\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("source"));
    }

    #[test]
    fn poller_states_min_two() {
        let source =
            "feature multi_bank\n  poller bad\n    source X\n    states\n      only_one terminal\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("at least 2"));
    }

    #[test]
    fn poller_backoff_strategy_closed() {
        let source = "feature multi_bank\n  poller bad\n    source X\n    retry\n      max_attempts 3\n      backoff weibull\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("fixed"));
    }

    #[test]
    fn poller_tenant_from_requires_row_prefix() {
        let source =
            "feature multi_bank\n  poller bad\n    source X\n    tenant_from payload.org_id\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("row."));
    }

    // -----------------------------------------------------------------
    // Realtime bucket cycle MVP — `channel` parser tests.
    //
    // Three cases per the cycle proposal: minimal happy path; rejects
    // when a required child is missing; rejects unknown child key. The
    // doctor diagnostic `CHANNEL-PAYLOAD-001` covers payload-resolution
    // separately (in `lazuli_cli`'s doctor suite).
    // -----------------------------------------------------------------

    #[test]
    fn channel_parses_minimal() {
        let source = "feature customer\n  channel customer_activity\n    tenant_from org\n    policy @policy.read\n    payload CustomerActivityEvent\n";
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        let channels = &features[0].channels;
        assert_eq!(channels.len(), 1);
        let c = &channels[0];
        assert_eq!(c.name, "customer_activity");
        assert_eq!(c.tenant_from, "org");
        assert_eq!(c.policy, "@policy.read");
        assert_eq!(c.payload, "CustomerActivityEvent");
    }

    #[test]
    fn channel_rejects_missing_payload() {
        let source = "feature customer\n  channel customer_activity\n    tenant_from org\n    policy @policy.read\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("payload"),
            "error should mention missing payload, got: {err}"
        );
    }

    #[test]
    fn channel_rejects_unknown_child_key() {
        // `transport ws` is one of the explicitly rejected anti-proposals
        // — transport is adapter-resolved, never authored on the channel.
        let source = "feature customer\n  channel customer_activity\n    tenant_from org\n    policy @policy.read\n    payload CustomerActivityEvent\n    transport ws\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("channel children"),
            "error should mention closed catalog, got: {err}"
        );
    }
}

// =============================================================================
// Notifications expanded bucket cycle — digest/throttle parser tests.
// =============================================================================
#[cfg(test)]
mod notification_digest_throttle_parser_tests {
    use super::parse_feature_skeletons;

    fn source_with_notification(children: &str) -> String {
        format!(
            "feature customer_outreach\n  notification booking_confirmed\n    channel email, push\n    recipient target.user.email\n    trigger event payments.transaction_completed\n    template \"./templates/booking_confirmed.<locale>.tmpl\"\n    policy @policy.dispatch\n{children}"
        )
    }

    #[test]
    fn notification_digest_parses_full_surface() {
        let source = source_with_notification(
            "    digest\n      every 1h\n      group_by payload.user_id\n      max_size 50\n      template_strategy merge\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let digest = features[0].notifications[0]
            .digest
            .as_ref()
            .expect("digest");
        assert_eq!(digest.every, "1h");
        assert_eq!(digest.group_by.as_deref(), Some("payload.user_id"));
        assert_eq!(digest.max_size, Some(50));
        assert_eq!(digest.template_strategy.as_deref(), Some("merge"));
    }

    #[test]
    fn notification_digest_requires_every() {
        let source = source_with_notification(
            "    digest\n      group_by payload.user_id\n      max_size 50\n",
        );
        let err = parse_feature_skeletons(&source).unwrap_err();
        assert!(err.to_string().contains("every"), "{err}");
    }

    #[test]
    fn notification_digest_rejects_unknown_child() {
        let source = source_with_notification("    digest\n      every 1h\n      mode batch\n");
        let err = parse_feature_skeletons(&source).unwrap_err();
        assert!(err.to_string().contains("digest"), "{err}");
    }

    #[test]
    fn notification_throttle_parses_full_surface() {
        let source = source_with_notification(
            "    throttle\n      per_recipient\n      per_channel\n      burst 3\n      max_per 1h\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let throttle = features[0].notifications[0]
            .throttle
            .as_ref()
            .expect("throttle");
        assert_eq!(throttle.max_per, "1h");
        assert!(throttle.per_recipient);
        assert!(throttle.per_channel);
        assert_eq!(throttle.burst, Some(3));
    }

    #[test]
    fn notification_throttle_requires_max_per() {
        let source = source_with_notification("    throttle\n      per_recipient\n");
        let err = parse_feature_skeletons(&source).unwrap_err();
        assert!(err.to_string().contains("max_per"), "{err}");
    }

    #[test]
    fn notification_throttle_rejects_unknown_child() {
        let source = source_with_notification("    throttle\n      max_per 1h\n      per_user\n");
        let err = parse_feature_skeletons(&source).unwrap_err();
        assert!(err.to_string().contains("throttle"), "{err}");
    }
}

#[cfg(test)]
mod enum_metadata_parser_tests {
    use super::parse_feature_skeletons;
    use crate::ast::EnumStorageValueDecl;

    #[test]
    fn enum_metadata_parses_label_hint_icon_combinations() {
        let source = r#"
feature account
  domain
    enum Gender
      male: label @translation.gender_male
      female: label @translation.gender_female, icon "user"
      non_binary: label gender_non_binary, hint @translation.gender_non_binary_hint
      prefer_not: label @translation.gender_prefer_not, hint @translation.gender_prefer_not_hint, icon "eye-off"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let variants = &features[0].enums[0].variants;

        assert_eq!(variants[0].name, "male");
        assert_eq!(variants[0].label_key.as_deref(), Some("gender_male"));
        assert_eq!(variants[0].hint_key, None);
        assert_eq!(variants[0].icon_key, None);

        assert_eq!(variants[1].label_key.as_deref(), Some("gender_female"));
        assert_eq!(variants[1].icon_key.as_deref(), Some("user"));
        assert_eq!(variants[1].hint_key, None);

        assert_eq!(variants[2].label_key.as_deref(), Some("gender_non_binary"));
        assert_eq!(
            variants[2].hint_key.as_deref(),
            Some("gender_non_binary_hint")
        );
        assert_eq!(variants[2].icon_key, None);

        assert_eq!(variants[3].label_key.as_deref(), Some("gender_prefer_not"));
        assert_eq!(
            variants[3].hint_key.as_deref(),
            Some("gender_prefer_not_hint")
        );
        assert_eq!(variants[3].icon_key.as_deref(), Some("eye-off"));
    }

    #[test]
    fn enum_metadata_preserves_bare_variants_and_storage_values() {
        let source = r#"
feature account
  domain
    enum Status
      draft
      active = "live": label @translation.status_active, icon "check"
      archived = 9: label status_archived
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let variants = &features[0].enums[0].variants;

        assert_eq!(variants[0].name, "draft");
        assert_eq!(variants[0].storage, None);
        assert_eq!(variants[0].label_key, None);

        assert_eq!(variants[1].name, "active");
        assert_eq!(
            variants[1].storage,
            Some(EnumStorageValueDecl::String("live".to_owned()))
        );
        assert_eq!(variants[1].label_key.as_deref(), Some("status_active"));
        assert_eq!(variants[1].icon_key.as_deref(), Some("check"));

        assert_eq!(variants[2].name, "archived");
        assert_eq!(variants[2].storage, Some(EnumStorageValueDecl::Integer(9)));
        assert_eq!(variants[2].label_key.as_deref(), Some("status_archived"));
    }

    #[test]
    fn enum_metadata_rejects_hint_or_icon_without_label() {
        let source = r#"
feature account
  domain
    enum Status
      active: icon "check"
"#;
        let err = parse_feature_skeletons(source).expect_err("rejects missing label");
        assert!(err.to_string().contains("requires `label <key>`"), "{err}");
    }
}

#[cfg(test)]
mod public_contract_tests {
    use super::*;

    #[test]
    fn parse_public_contract_attaches_to_enum() {
        let source = r#"
feature account
  domain
    public contract Gender as v1
    enum Gender
      female = 1
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let contract = features[0].enums[0].public_contract.as_ref().unwrap();
        assert_eq!(contract.version, 1);
    }

    #[test]
    fn parse_public_contract_attaches_to_resource() {
        let source = r#"
feature account
  domain
    public contract User as v2
    resource User
      email: @semantic.Email required
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let contract = features[0].resources[0].public_contract.as_ref().unwrap();
        assert_eq!(contract.version, 2);
    }

    #[test]
    fn parse_public_contract_attaches_to_command() {
        let source = r#"
feature account
  public contract command.create_user as v3
  command create_user
    input
      email: Text required
    returns User
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let contract = features[0].commands[0].public_contract.as_ref().unwrap();
        assert_eq!(contract.version, 3);
    }

    #[test]
    fn parse_public_contract_attaches_to_record() {
        let source = r#"
feature account
  domain
    public contract Address as v4
    record Address
      line1: Text required
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let contract = features[0].records[0].public_contract.as_ref().unwrap();
        assert_eq!(contract.version, 4);
    }

    #[test]
    fn parse_public_contract_mismatched_name_errors() {
        let source = r#"
feature account
  domain
    public contract Gender as v1
    enum Status
      active = 1
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("public contract `Gender` precedes a `enum Status` declaration"),
            "got {message}"
        );
    }

    #[test]
    fn parse_public_contract_trailing_no_symbol_errors() {
        let source = r#"
feature account
  domain
    public contract Gender as v1
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("trailing `public contract` declaration"),
            "got {message}"
        );
    }

    #[test]
    fn parse_public_contract_identity_attaches_to_auth() {
        // Per docs/proposals/cross-feature-contracts.md §5.3 row 7 —
        // `public contract identity as v<N>` is a special singleton form
        // recognized inside the `auth` block (NOT at feature level).
        let source = r#"
feature account
  auth
    public contract identity as v1
    identity Customer.email
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let auth = features[0].auth.as_ref().expect("auth block");
        let contract = auth
            .identity
            .public_contract
            .as_ref()
            .expect("identity contract attached");
        assert_eq!(contract.version, 1);
        assert_eq!(auth.identity.field, "Customer.email");
    }

    #[test]
    fn parse_public_contract_identity_below_identity_errors() {
        // The contract MUST appear ABOVE the identity line; below is an
        // ordering error.
        let source = r#"
feature account
  auth
    identity Customer.email
    public contract identity as v1
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("ABOVE the `identity` line"),
            "got {message}"
        );
    }

    // -------------------------------------------------------------------------
    // Cross-feature contracts §5.4 — feature-level `uses` line parsing,
    // optionally with consumer-side `version v<N>` pin.
    // -------------------------------------------------------------------------

    #[test]
    fn parses_uses_single_feature_no_pin() {
        let source = r#"
feature billing
  uses account
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].uses_clauses.len(), 1);
        assert_eq!(features[0].uses_clauses[0].feature, "account");
        assert_eq!(features[0].uses_clauses[0].version, None);
    }

    #[test]
    fn parses_uses_with_version_pin() {
        let source = r#"
feature billing
  uses account version v2
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let clauses = &features[0].uses_clauses;
        assert_eq!(clauses.len(), 1);
        assert_eq!(clauses[0].feature, "account");
        assert_eq!(clauses[0].version, Some(2));
    }

    #[test]
    fn parses_uses_comma_list_shares_line_level_pin() {
        // The trailing `version v<N>` applies to ALL entries on the line.
        let source = r#"
feature billing
  uses org, user, account version v1
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let clauses = &features[0].uses_clauses;
        assert_eq!(clauses.len(), 3);
        assert_eq!(clauses[0].feature, "org");
        assert_eq!(clauses[1].feature, "user");
        assert_eq!(clauses[2].feature, "account");
        for clause in clauses {
            assert_eq!(clause.version, Some(1));
        }
    }

    #[test]
    fn parses_multiple_uses_lines_independently() {
        // Each `uses` line carries its own pin (or none).
        let source = r#"
feature billing
  uses account version v1
  uses notifications
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let clauses = &features[0].uses_clauses;
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0].feature, "account");
        assert_eq!(clauses[0].version, Some(1));
        assert_eq!(clauses[1].feature, "notifications");
        assert_eq!(clauses[1].version, None);
    }

    #[test]
    fn parses_uses_empty_entry_errors() {
        let source = r#"
feature billing
  uses account, , billing
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("empty entry"), "got {message}");
    }

    #[test]
    fn parses_uses_bad_version_errors() {
        let source = r#"
feature billing
  uses account version 1
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("`v` prefix"), "got {message}");
    }

    #[test]
    fn parses_uses_zero_version_errors() {
        let source = r#"
feature billing
  uses account version v0
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("positive u16"), "got {message}");
    }
}

#[cfg(test)]
mod resource_conventions_tests {
    //! Parser tests for the `conventions [<name>, ...]` resource slot.
    //! Grammar + diagnostics anchored in
    //! `docs/proposals/ir-resource-conventions-crud.md` §4.1 / §4.3.

    use super::parse_feature_skeletons;
    use crate::ast::ResourceConventionAst;

    fn customer_source(slot_line: &str) -> String {
        // Anchor the slot inside a minimal resource block. The trailing
        // `\n` keeps the indentation parser happy when `slot_line` is
        // empty (missing-slot test).
        let mut src = String::from(
            "\nfeature customer\n  resource Customer\n    org: Org required\n    email: Text required\n",
        );
        if !slot_line.is_empty() {
            src.push_str("    ");
            src.push_str(slot_line);
            src.push('\n');
        }
        src
    }

    #[test]
    fn parses_conventions_crud() {
        let src = customer_source("conventions [crud]");
        let features = parse_feature_skeletons(&src).expect("parses");
        let resource = &features[0].resources[0];
        assert_eq!(resource.conventions, vec![ResourceConventionAst::Crud]);
    }

    #[test]
    fn missing_conventions_is_empty() {
        let src = customer_source("");
        let features = parse_feature_skeletons(&src).expect("parses");
        let resource = &features[0].resources[0];
        assert!(resource.conventions.is_empty());
    }

    #[test]
    fn parses_conventions_with_duplicates() {
        // Per §4.1: duplicates are permissive at parse time —
        // deduplication is the analyzer's responsibility if needed.
        let src = customer_source("conventions [crud, crud]");
        let features = parse_feature_skeletons(&src).expect("parses");
        let resource = &features[0].resources[0];
        assert_eq!(
            resource.conventions,
            vec![ResourceConventionAst::Crud, ResourceConventionAst::Crud]
        );
    }

    #[test]
    fn empty_conventions_list_errors() {
        let src = customer_source("conventions []");
        let err = parse_feature_skeletons(&src).expect_err("empty list rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("`conventions []` is not allowed"),
            "expected empty-list diagnostic, got: {msg}",
        );
    }

    #[test]
    fn unknown_convention_errors_with_suggestion() {
        // §4.3 — single-character Levenshtein → `crud` for `crd`.
        let src = customer_source("conventions [crd]");
        let err = parse_feature_skeletons(&src).expect_err("unknown rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("conventions_unknown"),
            "expected `conventions_unknown` code, got: {msg}",
        );
        assert!(
            msg.contains("did you mean `crud`?"),
            "expected `crud` suggestion verbatim, got: {msg}",
        );
    }

    #[test]
    fn far_unknown_convention_errors_without_suggestion() {
        // Identifier far enough from `crud` that single-char Levenshtein
        // does not propose a match — diagnostic still fires.
        let src = customer_source("conventions [foo]");
        let err = parse_feature_skeletons(&src).expect_err("unknown rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("conventions_unknown"),
            "expected `conventions_unknown` code, got: {msg}",
        );
        assert!(
            msg.contains("`foo`"),
            "expected offending ident, got: {msg}"
        );
    }

    #[test]
    fn unbracketed_conventions_errors() {
        let src = customer_source("conventions crud");
        let err = parse_feature_skeletons(&src).expect_err("missing brackets rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("bracketed identifier list"),
            "expected bracket-required diagnostic, got: {msg}",
        );
    }

    #[test]
    fn bare_conventions_keyword_errors() {
        let src = customer_source("conventions");
        let err = parse_feature_skeletons(&src).expect_err("bare keyword rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("bracketed identifier list"),
            "expected bracket-required diagnostic, got: {msg}",
        );
    }

    #[test]
    fn duplicate_conventions_slot_errors() {
        // Two `conventions [...]` lines on one resource — reject.
        let mut src =
            String::from("\nfeature customer\n  resource Customer\n    org: Org required\n");
        src.push_str("    conventions [crud]\n");
        src.push_str("    conventions [crud]\n");
        let err = parse_feature_skeletons(&src).expect_err("duplicate slot rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("at most one `conventions` slot"),
            "expected duplicate-slot diagnostic, got: {msg}",
        );
    }
}

#[cfg(test)]
mod resource_ddl_authoring_tests {
    use super::parse_feature_skeletons;
    use crate::ast::{ResourceConstraintAst, ResourceIndexMethodAst};

    fn resource_with(lines: &[&str]) -> crate::ast::ResourceDecl {
        let mut source = String::from(
            "\nfeature customer\n  resource Customer\n    workspace: Workspace required\n    email: Text required\n    tags: list of Text\n",
        );
        for line in lines {
            source.push_str("    ");
            source.push_str(line);
            source.push('\n');
        }
        parse_feature_skeletons(&source)
            .expect("resource DDL authoring should parse")
            .remove(0)
            .resources
            .remove(0)
    }

    #[test]
    fn parses_single_column_index_on_parenthesized_field() {
        let resource = resource_with(&["index on (workspace)"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Index(index) => {
                assert_eq!(index.fields, vec!["workspace"]);
                assert_eq!(index.method, None);
                assert!(!index.full_text);
            }
            other => panic!("expected index constraint, got {other:?}"),
        }
    }

    #[test]
    fn parses_single_column_index_with_gin_modifier() {
        let resource = resource_with(&["index on tags gin"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Index(index) => {
                assert_eq!(index.fields, vec!["tags"]);
                assert_eq!(index.method, Some(ResourceIndexMethodAst::Gin));
                assert!(!index.full_text);
            }
            other => panic!("expected index constraint, got {other:?}"),
        }
    }

    #[test]
    fn parses_compound_unique_constraint() {
        let resource = resource_with(&["unique (workspace, email)"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Unique(unique) => {
                assert_eq!(unique.fields, vec!["workspace", "email"]);
            }
            other => panic!("expected unique constraint, got {other:?}"),
        }
    }

    #[test]
    fn parses_resource_fts_as_full_text_gin_index() {
        let resource = resource_with(&["fts on (email, tags)"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Index(index) => {
                assert_eq!(index.fields, vec!["email", "tags"]);
                assert_eq!(index.method, None);
                assert!(index.full_text);
            }
            other => panic!("expected full-text index constraint, got {other:?}"),
        }
    }
}

// =============================================================================
// `ir-rate-limit-env-aware` cell 1 — parser tests for the env-qualified
// `rate_limit` shape (proposal §4.2, §9). Cover both back-compat
// (single-line `rate_limit "X"`) and the new multi-line shape with
// optional `in <env_list>` qualification.
// =============================================================================

#[cfg(test)]
mod rate_limit_env_aware_tests {
    use super::parse_feature_skeletons;

    fn feature_with_command_body(body: &str) -> String {
        format!(
            "\nfeature customer\n  resource Customer\n    name: Text required\n\n  command create\n    input\n      name: Text required\n    policy @policy.public\n{body}    creates Customer\n      name = params.name\n",
        )
    }

    fn single_command<'a>(
        features: &'a [crate::ast::FeatureSkeleton],
    ) -> &'a crate::ast::CommandDecl {
        let feature = features.first().expect("one feature parsed");
        feature.commands.first().expect("one command parsed")
    }

    #[test]
    fn single_unqualified_rate_limit_is_back_compat() {
        // Proposal §8 — the existing `rate_limit "X per Y per Z"` shape
        // must still parse and lower into `{ default: "X...", by_env: [] }`.
        let source = feature_with_command_body("    rate_limit \"5 per 10 minutes per ip\"\n");
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.default.as_deref(), Some("5 per 10 minutes per ip"));
        assert!(spec.by_env.is_empty());
    }

    #[test]
    fn default_plus_single_qualified_line_parses() {
        // Proposal §5.1 — `account.register` shape after the migration.
        let source = feature_with_command_body(
            "    rate_limit \"5 per 10 minutes per ip\"\n    rate_limit \"unlimited\" in dev, test\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.default.as_deref(), Some("5 per 10 minutes per ip"));
        assert_eq!(spec.by_env.len(), 1);
        let entry = &spec.by_env[0];
        assert_eq!(entry.limit, "unlimited");
        assert_eq!(entry.envs, vec!["dev".to_owned(), "test".to_owned()]);
    }

    #[test]
    fn single_line_with_three_envs_folds_into_one_entry() {
        // Proposal §5.4 — one qualified line, multiple envs.
        let source = feature_with_command_body(
            "    rate_limit \"5 per 1 minutes per user\"\n    rate_limit \"1000 per 1 minutes per user\" in dev, staging, test\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.by_env.len(), 1);
        let entry = &spec.by_env[0];
        assert_eq!(entry.limit, "1000 per 1 minutes per user");
        assert_eq!(
            entry.envs,
            vec!["dev".to_owned(), "staging".to_owned(), "test".to_owned()]
        );
    }

    #[test]
    fn unlimited_keyword_in_qualified_line_preserves_literal_for_analyzer() {
        // Proposal §4.4 — `"unlimited"` is preserved verbatim at AST
        // level; the analyzer lowers it to the empty-string sentinel in
        // `ir::RateLimitByEnv.limit`.
        let source = feature_with_command_body(
            "    rate_limit \"5 per minute per ip\"\n    rate_limit \"unlimited\" in test\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.by_env.len(), 1);
        let entry = &spec.by_env[0];
        assert_eq!(entry.limit, "unlimited");
        assert_eq!(entry.envs, vec!["test".to_owned()]);
    }

    #[test]
    fn duplicate_default_lines_are_rejected() {
        // Proposal §9.2 — two unqualified declarations is the
        // `rate_limit_duplicate_default` error.
        let source = feature_with_command_body(
            "    rate_limit \"5 per minute per ip\"\n    rate_limit \"10 per minute per ip\"\n",
        );
        let err = parse_feature_skeletons(&source).expect_err("duplicate default rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("rate_limit_duplicate_default"),
            "expected `rate_limit_duplicate_default` code, got: {msg}",
        );
    }

    #[test]
    fn empty_in_tail_is_rejected() {
        // Proposal §12 Cell 1 — `rate_limit "X" in` (empty list) errors.
        let source = feature_with_command_body("    rate_limit \"5 per minute per ip\" in\n");
        let err = parse_feature_skeletons(&source).expect_err("empty `in` tail rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("requires at least one env name"),
            "expected empty-env-list diagnostic, got: {msg}",
        );
    }

    #[test]
    fn unknown_env_identifier_parses_at_ast_level() {
        // Proposal §4.3 / §9.2 — the parser is forgiving; the doctor
        // (Cell 3) emits the `rate_limit_unknown_env` warning later.
        let source = feature_with_command_body(
            "    rate_limit \"5 per minute per ip\"\n    rate_limit \"unlimited\" in dev, qa\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let command = single_command(&features);
        let spec = command.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(spec.by_env.len(), 1);
        // Raw identifiers as authored; analyzer normalises closed-catalog
        // names and surfaces unknowns via `ir::RateLimitByEnv.unknown_envs`.
        assert_eq!(spec.by_env[0].envs, vec!["dev".to_owned(), "qa".to_owned()]);
    }

    #[test]
    fn trailing_garbage_after_literal_is_rejected() {
        // Defensive — anything other than `in <env_list>` after the
        // quoted literal must fail (catches typos like `for` or `on`).
        let source =
            feature_with_command_body("    rate_limit \"5 per minute per ip\" on production\n");
        let err = parse_feature_skeletons(&source).expect_err("trailing garbage rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("`in <env_list>`"),
            "expected `in <env_list>` diagnostic, got: {msg}",
        );
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
