use thiserror::Error;

use crate::ast::OwnerAxisAst;
use crate::ast::{
    Agent, AgentEvalAssertion, AgentEvalCase, AgentEvalGolden, AgentEvalKind, AgentEvalPredicate,
    AgentExpose, AgentExposeRouteSlot, AgentInputSlot, AgentOutput, AgentTool, AggregateDecl,
    ApiDecl, ApprovalThenDecl, AssignmentDecl, AudienceAst, Auth, AuthDurationClause, AuthIdentity,
    AuthMfa, AuthOAuthProvider, AuthPassword, AuthSessionRotation, AuthSessions,
    AuthTheftDetectionAction, AuthTheftDetectionActionClause, BindingRefAst, CacheProfileDecl,
    CellBindingAst, Channel, ColorStateAst, ColorTokenAst, CommandApproval, CommandAudit,
    CommandDecl, CommandDeprecatedDecl, CommandEffectDecl, CommandEffectKindDecl, CommandEmit,
    CommandInputDecl, CommandInputSlot, CommandRouteSlot, CommandRouteSlotKind, CommandWriteWindow,
    ContainsRhs, CustomTokenAst, DefaultsPolicyFor, DefaultsTenancy, DesignDeclAst,
    DrawerBindingSourceAst, DrawerRouteBindingAst, DrawerSubViewAst, DrawerTriggerAst,
    EasingTokenAst, EnumDeclAst, EnumStorageValueDecl, EnumVariantDecl, ErrorExposureDefaultAst,
    EventGroup, EventVariantFieldDecl, EventVariantKindAst, FamilyTokenAst, FeatureDefaults,
    FeatureErrorExposeRuleDecl, FeatureErrorMessageDecl, FeatureErrorsDecl, FeatureGatesAst,
    FeatureSkeleton, FieldConstraintsDecl, FieldPoliciesDecl, FieldPolicyDecl,
    FilterCardinalityAst, FilterDeclAst, GateDirectiveAst, HttpMethod, InvalidatesDecl,
    InvariantDecl, Job, JobBody, JobDeclarativeTyped, JobExternalCall, JobExternalCallArg,
    JobFanout, JobHandler, JobRetry, JobTrigger, LetBindingDecl, ListQueryDecl,
    LocaleNegotiateDecl, LookupKey, LookupQueryDecl, LzxAction, LzxApp, LzxAudience, LzxDocument,
    LzxErrorPage, LzxExperience, LzxExperienceView, LzxExtensionOrder, LzxExtensionSlot,
    LzxPlatform, LzxPlatformView, LzxRequiresLifecycle, LzxResumeArm, LzxResumeArmKind,
    LzxResumeRouter, LzxRoute, LzxRouteGuardDefaults, LzxSurface, LzxViewExtension, LzxViewGuard,
    MotionAst, Notification, NotificationDigest, NotificationThrottle, PackageSkeleton,
    PermissionDeclAst, PlanBlockAst, PlanFeatureRefAst, PlanLimitRefAst, PlanTrialAst,
    PoliciesDecl, PolicyAtomAst, PolicyCategoryDecl, PolicyExprAst, PublicContractDeclAst,
    QueryDecl, QuerySearch, RateLimitByEnvAst, RateLimitSpecAst, RecordDecl, ReportColumnAst,
    ReportColumnSourceAst, ReportDecl, ResourceCompositeKey, ResourceConstraintAst,
    ResourceConventionAst, ResourceDecl, ResourceFieldDecl, ResourceHasMany, ResourceIndexAst,
    ResourceIndexMethodAst, ResourceLock, ResourceRetention, ResourceRetentionAction,
    ResourceUniqueAst, RoleDeclAst, RoleGrantsAst, RouteParamAst, ScaleTokenAst, SearchDeclAst,
    SearchFieldAst, SearchModeAst, SelectionDeclAst, SelectionModeAst, SettingDeclAst,
    SettingPersistenceAst, SettingValueSpaceAst, ShadowTokenAst, SortDeclAst, SortDirAst, Span,
    SqlQueryDecl, SurfaceAst, SurfaceTargetAst, TargetArgDecl, TargetExprDecl, TenantMigration,
    TextScaleTokenAst, ToolsCallsOp, TrackingTokenAst, TranslationDecl, TranslationKeyDecl,
    TranslationKeyRefAst, TranslationPluralArmDecl, TranslationVariantDecl, TypographyAst,
    UsesClauseAst, ViewAst, ViewCreateAst, ViewDetailAst, ViewListAst, Webhook, WebhookDlq,
    WebhookHandler, WebhookReplay, WebhookVerify, WeightTokenAst, ZTokenAst,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleBlockAst {
    pub discriminator_field: String,
    pub states: Vec<LifecycleStateAst>,
    pub transitions: Vec<LifecycleTransitionAst>,
    pub invariants: Vec<LifecycleInvariantAst>,
    pub invariant_handlers: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleStateAst {
    pub name: String,
    pub kind_keyword: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleTransitionAst {
    pub name: String,
    pub from: Vec<String>,
    pub to: String,
    pub policy: Option<String>,
    pub audit: Option<String>,
    pub timestamps: Option<String>,
    pub emits: Vec<String>,
    pub requires: Option<String>,
    pub tests: Vec<String>,
    pub previously: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleInvariantAst {
    /// Raw tail after `invariant `; lowering tokenizes the closed catalog.
    pub raw: String,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// L0 #8 — `poller` vocabulary (docs/proposals/poller-vocab.md).
// Top-level feature kind, parallel to `job` / `webhook` / `notification`.
// AST is closed-catalog: only the children listed in §3.1 of the proposal
// are accepted; any other keyword is a parse error.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerBlockAst {
    pub name: String,
    pub source: String,
    pub cursor: Option<PollerCursorAst>,
    pub retry: Option<PollerRetryAst>,
    pub states: Vec<PollerStateAst>,
    pub resolve_handler: Option<String>,
    pub terminal_status_field: Option<String>,
    pub terminal_result_field: Option<String>,
    pub tick: Option<PollerTickAst>,
    pub tenant_from: Option<String>,
    pub idempotency: Vec<String>,
    pub audit: Option<String>,
    pub emits: Vec<String>,
    pub retry_quirks: Vec<PollerRetryQuirkAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerCursorAst {
    pub next_at_field: String,
    pub resolved_at_field: String,
    pub attempts_field: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerRetryAst {
    pub max_attempts: u32,
    pub backoff_strategy: String,
    pub backoff_base: Option<String>,
    pub backoff_cap: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerStateAst {
    pub name: String,
    pub kind_keyword: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerTickAst {
    pub every: String,
    pub batch: Option<u32>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerRetryQuirkAst {
    /// Catalog form name (`gender_flip_once` in v0.1).
    pub kind: String,
    /// Raw predicate after `when ` — closed predicate language;
    /// analyzer cross-checks.
    pub when: String,
    /// Counter field on `source`.
    pub counter_field: String,
    /// `mutate <field> = <transform>` raw rhs.
    pub mutate_field: String,
    pub mutate_transform: String,
    pub span: Span,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("{message}")]
    Pest { message: String, span: Span },

    #[error("internal parser error: expected {expected}")]
    Expected { expected: &'static str },
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            Self::Pest { span, .. } => *span,
            Self::Expected { .. } => Span::new(0, 1),
        }
    }
}

pub fn parse_lzx_document(source: &str) -> Result<LzxDocument, ParseError> {
    let lines = source_lines(source);
    let mut app = None;
    let mut routes = Vec::new();
    let mut experiences = Vec::new();
    let mut surfaces = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent != 0 {
            return Err(line_error(
                line,
                "top-level `.lzx` declarations are not indented",
            ));
        }

        if trimmed.starts_with("app ") {
            if app.is_some() {
                return Err(line_error(
                    line,
                    "`.lzx` files can declare only one `app` manifest",
                ));
            }
            let (parsed_app, next) = parse_lzx_app(&lines, index)?;
            app = Some(parsed_app);
            index = next;
        } else if trimmed.starts_with("route ") {
            let (route, next) = parse_lzx_route(&lines, index)?;
            routes.push(route);
            index = next;
        } else if trimmed.starts_with("experience ") {
            let (experience, next) = parse_lzx_experience(&lines, index)?;
            experiences.push(experience);
            index = next;
        } else if trimmed.starts_with("surface ") {
            let (surface, next) = parse_lzx_surface(&lines, index)?;
            surfaces.push(surface);
            index = next;
        } else {
            return Err(line_error(
                line,
                "expected `app <name>`, `route <name>`, `experience <name>`, or `surface <experience> <platform>`",
            ));
        }
    }

    Ok(LzxDocument {
        app,
        routes,
        experiences,
        surfaces,
        span: Span::new(0, source.len()),
    })
}

fn parse_lzx_app(lines: &[SourceLine<'_>], start: usize) -> Result<(LzxApp, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "app manifests use `app <name>`"));
    }

    let mut title = None;
    let mut version = None;
    let mut targets = Vec::new();
    let mut default_locale = None;
    let mut default_timezone = None;
    let mut auth_failed_redirect = None;
    let mut route_guard = None;
    let mut actor_query = None;
    let mut not_found = None;
    let mut error_pages = Vec::new();
    let mut uses = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "app manifest children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("title ") {
            title = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("version ") {
            version = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if trimmed == "targets" {
            index += 1;
            while index < lines.len() {
                let target_line = &lines[index];
                let target_trimmed = target_line.text.trim_start();
                if is_trivia(target_trimmed) {
                    index += 1;
                    continue;
                }
                if target_line.indent <= 2 {
                    break;
                }
                if target_line.indent != 4 {
                    return Err(line_error(
                        target_line,
                        "app targets use four-space indentation",
                    ));
                }
                targets.push(target_trimmed.to_owned());
                index += 1;
            }
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("targets ") {
            targets.extend(split_lzx_list(rest));
        } else if let Some(rest) = trimmed.strip_prefix("default_locale ") {
            default_locale = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("default_timezone ") {
            default_timezone = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("auth_failed_redirect ") {
            auth_failed_redirect = Some(rest.trim().to_owned());
        } else if trimmed == "route_guard" {
            if route_guard.is_some() {
                return Err(line_error(
                    line,
                    "app manifest declares `route_guard` at most once",
                ));
            }
            let (parsed, next) = parse_lzx_route_guard_defaults(lines, index, 2)?;
            route_guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("actor_query ") {
            actor_query = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("not_found ") {
            not_found = Some(rest.trim().to_owned());
        } else if trimmed.starts_with("error_page ") {
            let (error_page, next) = parse_lzx_error_page(lines, index)?;
            error_pages.push(error_page);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("uses ") {
            uses = split_lzx_list(rest);
        } else {
            return Err(line_error(
                line,
                "app manifest children are `title`, `version`, `targets`, `default_locale`, `default_timezone`, `auth_failed_redirect`, `route_guard`, `actor_query`, `not_found`, `error_page <status>`, or `uses` declarations",
            ));
        }
        index += 1;
    }

    Ok((
        LzxApp {
            name: parts[1].to_owned(),
            title,
            version,
            targets,
            default_locale,
            default_timezone,
            auth_failed_redirect,
            route_guard,
            actor_query,
            not_found,
            error_pages,
            uses,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_route_guard_defaults(
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

fn parse_lzx_view_guard(
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
    let mut on_unauthenticated = None;
    let mut on_unauthorized = None;
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
        } else if let Some(rest) = trimmed.strip_prefix("on_unauthorized ") {
            if on_unauthorized.is_some() {
                return Err(line_error(
                    line,
                    "`policy` declares `on_unauthorized` at most once",
                ));
            }
            on_unauthorized = Some(parse_lzx_redirect_clause(line, rest.trim())?);
        } else {
            return Err(line_error(
                line,
                "`policy` children are `on_unauthenticated redirect \"<path>\"` or `on_unauthorized redirect \"<path>\"`",
            ));
        }

        last_end = line.end;
        index += 1;
    }

    Ok((
        LzxViewGuard {
            policy,
            on_unauthenticated,
            on_unauthorized,
            requires_lifecycle: None,
            on_lifecycle_pending: None,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

fn parse_lzx_view_guard_policy(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<Vec<String>, ParseError> {
    if value.is_empty() {
        return Err(line_error(line, "`policy` requires a policy reference"));
    }

    if !value.starts_with('[') {
        return Ok(vec![value.to_owned()]);
    }

    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Err(line_error(
            line,
            "`policy` list form is `policy [@policy.a, @policy.b]`",
        ));
    };
    if inner.trim().is_empty() {
        return Err(line_error(
            line,
            "`policy` list requires at least one policy reference",
        ));
    }

    let mut policies = Vec::new();
    for atom in inner.split(',') {
        let atom = atom.trim();
        if atom.is_empty() {
            return Err(line_error(
                line,
                "`policy` list has an empty entry; check for trailing/duplicate commas",
            ));
        }
        policies.push(atom.to_owned());
    }
    Ok(policies)
}

fn parse_lzx_redirect_clause(line: &SourceLine<'_>, value: &str) -> Result<String, ParseError> {
    let Some(rest) = value.strip_prefix("redirect ") else {
        return Err(line_error(
            line,
            "route guard redirect clauses use `redirect \"<path>\"`",
        ));
    };
    let target = rest.trim();
    if !target.starts_with('"') || !target.ends_with('"') {
        return Err(line_error(
            line,
            "route guard redirect targets must be quoted strings",
        ));
    }
    Ok(unquote_lzx_value(target).to_owned())
}

fn parse_lzx_requires_lifecycle(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<LzxRequiresLifecycle, ParseError> {
    let Some((resource, state)) = rest.split_once('=') else {
        return Err(line_error(
            line,
            "`requires_lifecycle` uses `requires_lifecycle <Resource> = <state>`",
        ));
    };
    let resource = resource.trim();
    let state = state.trim();
    if resource.is_empty()
        || !resource
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        || !is_lzx_bare_ident(resource)
    {
        return Err(line_error(
            line,
            "`requires_lifecycle` resource must be an upper-case resource identifier",
        ));
    }
    if !is_lzx_bare_ident(state) {
        return Err(line_error(
            line,
            "`requires_lifecycle` state must be a bare lifecycle state identifier",
        ));
    }
    Ok(LzxRequiresLifecycle {
        resource: resource.to_owned(),
        state: state.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

fn parse_lzx_on_lifecycle_pending(line: &SourceLine<'_>, rest: &str) -> Result<String, ParseError> {
    let target = if let Some(target) = rest.trim().strip_prefix("@resume ") {
        target.trim()
    } else if let Some(target) = rest.trim().strip_prefix("@resume.") {
        target.trim()
    } else {
        return Err(line_error(
            line,
            "`on_lifecycle_pending` uses `on_lifecycle_pending @resume <name>`",
        ));
    };
    if !is_lzx_resume_ref(target) {
        return Err(line_error(
            line,
            "`on_lifecycle_pending` resume reference must be `<name>` or `<feature>.<name>`",
        ));
    }
    Ok(target.to_owned())
}

fn attach_lzx_requires_lifecycle(
    line: &SourceLine<'_>,
    guard: &mut LzxViewGuard,
    parsed: LzxRequiresLifecycle,
) -> Result<(), ParseError> {
    if guard.requires_lifecycle.is_some() {
        return Err(line_error(
            line,
            "view declares `requires_lifecycle` at most once",
        ));
    }
    guard.span.end = guard.span.end.max(parsed.span.end);
    guard.requires_lifecycle = Some(parsed);
    Ok(())
}

fn attach_lzx_on_lifecycle_pending(
    line: &SourceLine<'_>,
    guard: &mut LzxViewGuard,
    parsed: String,
    span_end: usize,
) -> Result<(), ParseError> {
    if guard.on_lifecycle_pending.is_some() {
        return Err(line_error(
            line,
            "view declares `on_lifecycle_pending` at most once",
        ));
    }
    guard.span.end = guard.span.end.max(span_end);
    guard.on_lifecycle_pending = Some(parsed);
    Ok(())
}

fn parse_lzx_error_page(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxErrorPage, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 || parts[0] != "error_page" {
        return Err(line_error(header, "error pages use `error_page <status>`"));
    }
    let status = parts[1]
        .parse::<u16>()
        .map_err(|_| line_error(header, "error page status must be an HTTP status code"))?;

    let mut template = None;
    let mut audience = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            index += 1;
            continue;
        }
        if line.indent <= 2 {
            break;
        }
        if line.indent != 4 {
            return Err(line_error(
                line,
                "error_page children use four-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("template ") {
            template = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("audience ") {
            audience = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "error_page children are `template \"./...\"` or `audience <name>` declarations",
            ));
        }
        index += 1;
    }

    let template = template.ok_or_else(|| {
        line_error(
            header,
            "`error_page` requires a `template \"./...\"` declaration",
        )
    })?;

    Ok((
        LzxErrorPage {
            status,
            template,
            audience,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_route(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxRoute, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "routes use `route <name>`"));
    }

    let mut path = None;
    let mut routes = Vec::new();
    let mut to = None;
    let mut surface = None;
    let mut audience = None;
    let mut lazy = None;
    let mut prerender = None;
    let mut guard = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(line, "route children use two-space indentation"));
        }

        if let Some(rest) = trimmed.strip_prefix("path ") {
            path = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            routes.extend(split_lzx_list(rest));
        } else if let Some(rest) = trimmed.strip_prefix("to ") {
            to = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("surface ") {
            surface = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("audience ") {
            audience = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("lazy ") {
            lazy =
                Some(parse_lzx_bool(rest.trim()).ok_or_else(|| {
                    line_error(line, "route lazy uses `lazy true` or `lazy false`")
                })?);
        } else if let Some(rest) = trimmed.strip_prefix("prerender ") {
            prerender = Some(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "route declares `policy` at most once"));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 2)?;
            guard = Some(parsed);
            index = next;
            continue;
        } else {
            return Err(line_error(
                line,
                "route children are `path`, `route <name>: <Type>`, `to`, `surface`, `audience`, `lazy`, `prerender`, or `policy` declarations",
            ));
        }
        index += 1;
    }

    Ok((
        LzxRoute {
            name: parts[1].to_owned(),
            path,
            routes,
            to,
            surface,
            audience,
            lazy,
            prerender,
            guard,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_experience(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExperience, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "`experience` uses `experience <name>`"));
    }

    let mut imports = Vec::new();
    let mut views = Vec::new();
    let mut resume_routers = Vec::new();
    let mut extensions = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "experience children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("imports ") {
            imports.extend(split_lzx_list(rest));
            index += 1;
        } else if trimmed.starts_with("view ") {
            let (view, next) = parse_lzx_experience_view(lines, index)?;
            views.push(view);
            index = next;
        } else if trimmed.starts_with("resume ") {
            let (resume, next) = parse_lzx_resume_router(lines, index)?;
            resume_routers.push(resume);
            index = next;
        } else if trimmed.starts_with("extends @anchor.") {
            let (extension, next) = parse_lzx_view_extension(lines, index)?;
            extensions.push(extension);
            index = next;
        } else {
            return Err(line_error(
                line,
                "experience children are `imports`, `view`, `resume`, or `extends @anchor.*` declarations",
            ));
        }
    }

    Ok((
        LzxExperience {
            name: parts[1].to_owned(),
            imports,
            views,
            resume_routers,
            extensions,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_resume_router(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxResumeRouter, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "resume blocks use `resume <name>`"));
    }
    if !is_lzx_bare_ident(parts[1]) {
        return Err(line_error(
            header,
            "`resume` name must be a bare identifier",
        ));
    }

    let mut source_query = None;
    let mut arms = Vec::new();
    let mut index = start + 1;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(
                line,
                "resume children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source query.lookup ") {
            if source_query.is_some() {
                return Err(line_error(
                    line,
                    "resume declares `source query.lookup` at most once",
                ));
            }
            let query = rest.trim();
            if !is_lzx_resume_ref(query) {
                return Err(line_error(
                    line,
                    "`source query.lookup` requires `<query>` or `<feature>.<query>`",
                ));
            }
            source_query = Some(query.to_owned());
        } else {
            arms.push(parse_lzx_resume_arm(line, trimmed)?);
        }

        last_end = line.end;
        index += 1;
    }

    let Some(source_query) = source_query else {
        return Err(line_error(
            header,
            "resume blocks require `source query.lookup <query>`",
        ));
    };
    if arms.is_empty() {
        return Err(line_error(header, "resume blocks require at least one arm"));
    }

    Ok((
        LzxResumeRouter {
            name: parts[1].to_owned(),
            source_query,
            arms,
            span: Span::new(header.start, last_end),
        },
        index,
    ))
}

fn parse_lzx_resume_arm(line: &SourceLine<'_>, trimmed: &str) -> Result<LzxResumeArm, ParseError> {
    let Some((left, right)) = split_lzx_arrow(trimmed) else {
        return Err(line_error(
            line,
            "resume arms use `<state> -> view <name>` or `<state> → view <name>`",
        ));
    };
    let arm = left.trim();
    let kind = match arm {
        "none" => LzxResumeArmKind::None,
        "*" => LzxResumeArmKind::Wildcard,
        state if is_lzx_bare_ident(state) => LzxResumeArmKind::State(state.to_owned()),
        _ => {
            return Err(line_error(
                line,
                "resume arm state must be a lifecycle state, `none`, or `*`",
            ));
        }
    };

    let Some(target_view) = right.trim().strip_prefix("view ") else {
        return Err(line_error(
            line,
            "resume arms target views with `view <name>`",
        ));
    };
    let target_view = target_view.trim();
    if !is_lzx_bare_ident(target_view) {
        return Err(line_error(
            line,
            "resume arm target view must be a bare identifier",
        ));
    }

    Ok(LzxResumeArm {
        kind,
        target_view: target_view.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

fn split_lzx_arrow(text: &str) -> Option<(&str, &str)> {
    let unicode = text.find('→').map(|idx| (idx, '→'.len_utf8()));
    let ascii = text.find("->").map(|idx| (idx, 2));
    let (idx, len) = match (unicode, ascii) {
        (Some(u), Some(a)) => {
            if u.0 <= a.0 {
                u
            } else {
                a
            }
        }
        (Some(u), None) => u,
        (None, Some(a)) => a,
        (None, None) => return None,
    };
    Some((&text[..idx], &text[idx + len..]))
}

fn parse_lzx_experience_view(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExperienceView, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 && !(parts.len() == 4 && parts[2] == "id") {
        return Err(line_error(
            header,
            "experience views use `view <name>` or `view <name> id @anchor.<name>`",
        ));
    }

    let mut anchor = (parts.len() == 4).then(|| parts[3].to_owned());
    let mut source = None;
    let mut submit = None;
    let mut routes = Vec::new();
    let mut extensible_by = Vec::new();
    let mut blocks = Vec::new();
    let mut actions = Vec::new();
    let mut opens = Vec::new();
    let mut tests = Vec::new();
    let mut guard = None;
    let mut pending_requires_lifecycle: Option<(usize, LzxRequiresLifecycle)> = None;
    let mut pending_on_lifecycle_pending: Option<(usize, String)> = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(line, "view children use four-space indentation"));
        }

        if let Some(rest) = trimmed.strip_prefix("route ") {
            routes.push(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("anchor ") {
            anchor = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("source ") {
            source = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            submit = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("extensible_by ") {
            extensible_by = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("action ") {
            let Some((name, target)) = rest.split_once(" -> ") else {
                return Err(line_error(line, "actions use `action <name> -> <target>`"));
            };
            actions.push(LzxAction {
                name: name.trim().to_owned(),
                target: target.trim().to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("opens ") {
            opens.push(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "view declares `policy` at most once"));
            }
            let (mut parsed, next) = parse_lzx_view_guard(lines, index, 4)?;
            if let Some((pending_index, pending)) = pending_requires_lifecycle.take() {
                attach_lzx_requires_lifecycle(&lines[pending_index], &mut parsed, pending)?;
            }
            if let Some((pending_index, pending)) = pending_on_lifecycle_pending.take() {
                let pending_line = &lines[pending_index];
                attach_lzx_on_lifecycle_pending(
                    pending_line,
                    &mut parsed,
                    pending,
                    pending_line.end,
                )?;
            }
            guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("requires_lifecycle ") {
            let parsed = parse_lzx_requires_lifecycle(line, rest.trim())?;
            if let Some(guard) = guard.as_mut() {
                attach_lzx_requires_lifecycle(line, guard, parsed)?;
            } else {
                if pending_requires_lifecycle.is_some() {
                    return Err(line_error(
                        line,
                        "view declares `requires_lifecycle` at most once",
                    ));
                }
                pending_requires_lifecycle = Some((index, parsed));
            }
        } else if let Some(rest) = trimmed.strip_prefix("on_lifecycle_pending ") {
            let parsed = parse_lzx_on_lifecycle_pending(line, rest.trim())?;
            if let Some(guard) = guard.as_mut() {
                attach_lzx_on_lifecycle_pending(line, guard, parsed, line.end)?;
            } else {
                if pending_on_lifecycle_pending.is_some() {
                    return Err(line_error(
                        line,
                        "view declares `on_lifecycle_pending` at most once",
                    ));
                }
                pending_on_lifecycle_pending = Some((index, parsed));
            }
        } else if trimmed == "tests" {
            index += 1;
            while index < lines.len() {
                let test_line = &lines[index];
                let test_trimmed = test_line.text.trim_start();
                if is_trivia(test_trimmed) {
                    index += 1;
                    continue;
                }
                if test_line.indent <= 4 {
                    break;
                }
                if test_line.indent != 6 {
                    return Err(line_error(
                        test_line,
                        "test assertions inside experience views use six-space indentation",
                    ));
                }
                tests.push(test_trimmed.to_owned());
                index += 1;
            }
            continue;
        } else {
            return Err(line_error(
                line,
                "view children are `route`, `anchor`, `source`, `submit`, `extensible_by`, `block`, `action`, `opens`, `policy`, `requires_lifecycle`, `on_lifecycle_pending`, or `tests`",
            ));
        }
        index += 1;
    }

    if let Some((pending_index, _)) = pending_requires_lifecycle.as_ref() {
        return Err(line_error(
            &lines[*pending_index],
            "`requires_lifecycle` currently requires a sibling `policy` guard",
        ));
    }
    if let Some((pending_index, _)) = pending_on_lifecycle_pending.as_ref() {
        return Err(line_error(
            &lines[*pending_index],
            "`on_lifecycle_pending` currently requires a sibling `policy` guard",
        ));
    }

    Ok((
        LzxExperienceView {
            name: parts[1].to_owned(),
            anchor,
            routes,
            extensible_by,
            source,
            submit,
            blocks,
            actions,
            opens,
            tests,
            guard,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_view_extension(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxViewExtension, usize), ParseError> {
    let header = &lines[start];
    let anchor = header
        .text
        .trim_start()
        .strip_prefix("extends ")
        .ok_or_else(|| line_error(header, "view extensions use `extends @anchor.<name>`"))?
        .trim()
        .to_owned();
    let mut blocks = Vec::new();
    let mut slots = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(
                line,
                "view extension children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if trimmed.starts_with("slot ") {
            let (slot, next) = parse_lzx_extension_slot(lines, index)?;
            slots.push(slot);
            index = next;
            continue;
        } else {
            return Err(line_error(
                line,
                "view extension children are `slot` declarations or legacy `block` declarations",
            ));
        }

        index += 1;
    }

    Ok((
        LzxViewExtension {
            anchor,
            blocks,
            slots,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_extension_slot(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExtensionSlot, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();

    if parts.len() != 2 && parts.len() != 4 {
        return Err(line_error(
            header,
            "extension slots use `slot <name>` or `slot <name> before|after <block>`",
        ));
    }

    let order = if parts.len() == 4 {
        if !matches!(parts[2], "before" | "after") {
            return Err(line_error(
                header,
                "extension slot ordering uses `before` or `after`",
            ));
        }
        Some(LzxExtensionOrder {
            relation: parts[2].to_owned(),
            target: parts[3].to_owned(),
        })
    } else {
        None
    };

    let mut blocks = Vec::new();
    let mut platforms = Vec::new();
    let mut audiences = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let child = line.text.trim_start();

        if is_trivia(child) {
            index += 1;
            continue;
        }

        if line.indent <= 4 {
            break;
        }

        if line.indent != 6 {
            return Err(line_error(
                line,
                "extension slot children use six-space indentation",
            ));
        }

        if let Some(rest) = child.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if let Some(rest) = child.strip_prefix("platforms ") {
            platforms = split_lzx_list(rest);
        } else if let Some(rest) = child.strip_prefix("audience ") {
            audiences = split_lzx_list(rest);
        } else {
            return Err(line_error(
                line,
                "extension slot children are `block`, `platforms`, or `audience` declarations",
            ));
        }

        index += 1;
    }

    if blocks.is_empty() {
        return Err(line_error(
            header,
            "extension slots must declare at least one `block`",
        ));
    }

    Ok((
        LzxExtensionSlot {
            name: parts[1].to_owned(),
            order,
            blocks,
            platforms,
            audiences,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_surface(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxSurface, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 3 {
        return Err(line_error(
            header,
            "surfaces use `surface <experience> web|mobile`",
        ));
    }

    let platform = match parts[2] {
        "web" => LzxPlatform::Web,
        "mobile" => LzxPlatform::Mobile,
        _ => {
            return Err(line_error(
                header,
                "surface platform must be `web` or `mobile`",
            ));
        }
    };

    let mut uses_experience = None;
    let mut audiences = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "surface children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("uses experience ") {
            uses_experience = Some(rest.trim().to_owned());
            index += 1;
        } else if trimmed.starts_with("audience ") {
            let (audience, next) = parse_lzx_audience(lines, index)?;
            audiences.push(audience);
            index = next;
        } else if trimmed.starts_with("view ") {
            return Err(line_error(
                line,
                "concrete platform views live under `audience ...` blocks",
            ));
        } else {
            return Err(line_error(
                line,
                "surface children are `uses experience` or `audience` declarations",
            ));
        }
    }

    Ok((
        LzxSurface {
            experience: parts[1].to_owned(),
            platform,
            uses_experience,
            audiences,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_audience(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxAudience, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() < 2 {
        return Err(line_error(header, "audience blocks use `audience <name>`"));
    }

    let mut views = Vec::new();
    let mut guard = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(
                line,
                "audience children are complete `view <name> <type>` declarations or `policy <policy>`",
            ));
        }

        if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "audience declares `policy` at most once"));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 4)?;
            guard = Some(parsed);
            index = next;
        } else if trimmed.starts_with("view ") {
            let (view, next) = parse_lzx_platform_view(lines, index)?;
            views.push(view);
            index = next;
        } else {
            return Err(line_error(
                line,
                "audience children are complete `view <name> <type>` declarations or `policy <policy>`",
            ));
        }
    }

    Ok((
        LzxAudience {
            name: parts[1].to_owned(),
            qualifiers: parts[2..].iter().map(|part| (*part).to_owned()).collect(),
            views,
            guard,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_platform_view(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxPlatformView, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 3 {
        return Err(line_error(
            header,
            "platform views use `view <name> <type>`",
        ));
    }

    let mut columns = Vec::new();
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut search = Vec::new();
    let mut filter = Vec::new();
    let mut cells = Vec::new();
    let mut actions = Vec::new();
    let mut submit = None;
    let mut blocks = Vec::new();
    let mut guard = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 4 {
            break;
        }

        if line.indent != 6 {
            return Err(line_error(
                line,
                "platform view children use six-space indentation",
            ));
        }

        if trimmed.contains("+=") || trimmed.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole view",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("columns ") {
            columns = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("fields ") {
            fields = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("sections ") {
            sections = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("search ") {
            search = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("filter ") {
            filter = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("cells ") {
            cells = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("actions ") {
            actions = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            submit = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(
                    line,
                    "platform view declares `policy` at most once",
                ));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 6)?;
            guard = Some(parsed);
            index = next;
            continue;
        } else {
            return Err(line_error(
                line,
                "platform view children are `columns`, `fields`, `sections`, `search`, `filter`, `cells`, `actions`, `submit`, `block`, or `policy`",
            ));
        }

        index += 1;
    }

    Ok((
        LzxPlatformView {
            name: parts[1].to_owned(),
            view_type: parts[2].to_owned(),
            columns,
            fields,
            sections,
            search,
            filter,
            cells,
            actions,
            submit,
            blocks,
            guard,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

// =============================================================================
// L0 #3 — lzx ViewModel surface parser.
// -----------------------------------------------------------------------------
// Hand-written line-walker for `features/<feat>/<feat>.{web,mobile}.lzx`
// per `docs/proposals/lzx-integration-codegen.md` §5. Mirrors the
// `parse_design_decl` pattern (L0 #2 Cell A) and the legacy
// `parse_lzx_*` helpers. Indentation is two spaces per level.
//
// Top-level entry point is `parse_surface_document` (source text) which
// dispatches to `parse_surface_decl` (line slice). The helper is `pub`
// so the analyzer and CLI can drive it from already-loaded
// `SourceLine` slices when needed.
// =============================================================================

/// Parse a full `.lzx` ViewModel file. Expects exactly one
/// `surface <feature> web|mobile` declaration at indent 0.
pub fn parse_surface_document(source: &str) -> Result<SurfaceAst, ParseError> {
    let lines = source_lines(source);
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent != 0 {
            return Err(line_error(
                line,
                "top-level `surface` declaration must start at indent 0",
            ));
        }
        if trimmed.starts_with("surface ") {
            let (parsed, _next) = parse_surface_decl(&lines, i)?;
            return Ok(parsed);
        }
        return Err(line_error(
            line,
            "`.lzx` ViewModel files must begin with `surface <feature> web|mobile`",
        ));
    }
    Err(ParseError::Expected {
        expected: "surface <feature> web|mobile declaration",
    })
}

/// Parse a `surface <feature> web|mobile` block starting at `lines[start]`.
/// Returns the AST + the index of the first line not consumed. Module-private
/// to match `SourceLine`'s scope; callers use the `parse_surface_document`
/// source-text entry point.
fn parse_surface_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(SurfaceAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let parts: Vec<_> = header_text.split_whitespace().collect();
    if parts.len() != 3 || parts[0] != "surface" {
        return Err(line_error(
            header,
            "surface header is `surface <feature> web|mobile`",
        ));
    }
    let feature = parts[1].to_owned();
    let target = match parts[2] {
        "web" => SurfaceTargetAst::Web,
        "mobile" => SurfaceTargetAst::Mobile,
        _ => {
            return Err(line_error(
                header,
                "surface target must be `web` or `mobile`",
            ));
        }
    };
    let header_indent = header.indent;
    let body_indent = header_indent + 2;

    let mut uses_feature: Option<String> = None;
    let mut audiences: Vec<AudienceAst> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != body_indent {
            return Err(line_error(
                line,
                "surface body lines use one indentation level deeper than the `surface` header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("uses feature ") {
            let value = rest.trim();
            if value.is_empty() {
                return Err(line_error(line, "`uses feature` requires a feature name"));
            }
            uses_feature = Some(value.to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("audience ") || trimmed == "audience" {
            let (audience, next) = parse_lzx_audience_block(lines, i, body_indent)?;
            audiences.push(audience);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "surface body lines are `uses feature <feature>` or `audience <name>` declarations",
            ));
        }
    }

    Ok((
        SurfaceAst {
            feature,
            target,
            uses_feature,
            audiences,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse an `audience <name>` block. `requires @scope.<name>` lines may
/// appear at the same indentation as `view` children; both are captured.
fn parse_lzx_audience_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(AudienceAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let parts: Vec<_> = header_text.split_whitespace().collect();
    if parts.len() != 2 || parts[0] != "audience" {
        return Err(line_error(header, "audience header is `audience <name>`"));
    }
    let name = parts[1].to_owned();
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error(
            header,
            "audience names use kebab-case or snake_case identifiers",
        ));
    }
    let body_indent = parent_indent + 2;
    let view_indent = body_indent;

    let mut requires: Vec<PolicyAtomAst> = Vec::new();
    let mut views: Vec<ViewAst> = Vec::new();
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
        if line.indent != view_indent {
            return Err(line_error(
                line,
                "audience body lines use one indentation level deeper than the `audience` header",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("requires ") {
            let atom = parse_policy_atom(line, rest.trim())?;
            requires.push(atom);
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("view list ")
            || trimmed.starts_with("view detail ")
            || trimmed.starts_with("view create ")
        {
            let (view, next) = parse_view_block(lines, i, view_indent)?;
            views.push(view);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "audience body lines are `requires @scope.<name>` or `view list|detail|create <name>` declarations",
            ));
        }
    }

    Ok((
        AudienceAst {
            name,
            requires,
            views,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse one of `view list`, `view detail`, `view create` blocks.
fn parse_view_block(
    lines: &[SourceLine<'_>],
    start: usize,
    parent_indent: usize,
) -> Result<(ViewAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let (kind, after_kind) = if let Some(rest) = header_text.strip_prefix("view list ") {
        ("list", rest)
    } else if let Some(rest) = header_text.strip_prefix("view detail ") {
        ("detail", rest)
    } else if let Some(rest) = header_text.strip_prefix("view create ") {
        ("create", rest)
    } else {
        return Err(line_error(
            header,
            "view header is `view list|detail|create <name> [at \"<path>\"]`",
        ));
    };

    let (name, route) = parse_view_header_tail(header, after_kind)?;
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            header,
            format!("view name `{}` must be kebab-case or snake_case", name),
        ));
    }
    let body_indent = parent_indent + 2;

    // Collect raw children; dispatch into the kind-specific builder.
    let mut state = ViewBodyState::default();
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
                "view body lines use one indentation level deeper than the `view` header",
            ));
        }
        if raw.contains("+=") || raw.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole view",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();

        if let Some(rest) = trimmed.strip_prefix("drawer ") {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`drawer` is only valid in `view list` bodies",
                ));
            }
            if state.drawer.is_some() {
                return Err(line_error(
                    line,
                    "view list declares at most one `drawer` block",
                ));
            }
            let (drawer, next) = parse_drawer_block(lines, i, body_indent, rest.trim())?;
            last_end = drawer.span.end;
            state.drawer = Some(drawer);
            i = next;
            continue;
        }

        if trimmed == "filters" {
            if kind != "list" {
                return Err(line_error(
                    line,
                    "`filters` block is only valid in `view list`",
                ));
            }
            let (next, block_end) = parse_filters_block(lines, i, body_indent, &mut state)?;
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed.starts_with("filters ") {
            return Err(line_error(
                line,
                "`filters` is a block keyword and does not accept inline content",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("search ") {
            if state.search.is_some() {
                return Err(line_error(line, "view declares `search` at most once"));
            }
            let (search, next) = parse_view_search_decl(lines, i, rest.trim(), body_indent)?;
            state.search = Some(search);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "sort" {
            if state.sort.is_some() {
                return Err(line_error(line, "view declares `sort` at most once"));
            }
            let (sort, next, block_end) = parse_view_sort_block(lines, i, body_indent)?;
            state.sort = Some(sort);
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed == "settings" {
            if !state.settings.is_empty() {
                return Err(line_error(line, "view declares `settings` at most once"));
            }
            let (settings, next, block_end) = parse_view_settings_block(lines, i, body_indent)?;
            state.settings = settings;
            last_end = block_end;
            i = next;
            continue;
        }
        if trimmed.starts_with("persist ") {
            return Err(line_error(
                line,
                "`persist` is valid only as a child of a `settings` declaration",
            ));
        }

        let mut matched = false;
        for (prefix, handler) in view_body_handlers() {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                handler(line, rest.trim(), &mut state)?;
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(line_error_owned(
                line,
                format!(
                    "view body lines are `source`, `submit`, `columns`, `fields`, `search`, `filter`, `sections`, `cells`, `route`, or `actions` declarations (got `{}`)",
                    trimmed
                ),
            ));
        }
        last_end = line.end;
        i += 1;
    }

    let span = Span::new(header.start, last_end);
    let view = match kind {
        "list" => {
            let selection = assemble_selection_decl(&state, span);
            ViewAst::List(ViewListAst {
                name,
                route,
                source: state.source.ok_or_else(|| {
                    line_error(
                        header,
                        "view list requires a `source <feature>.query.<name>` line",
                    )
                })?,
                columns: state.columns,
                search: state.search,
                filter: state.filter,
                filters: state.filters,
                cells_slot: state.cells_slot,
                cells: state.cells,
                drawer: state.drawer,
                sort: state.sort,
                selection,
                settings: state.settings,
                actions: state.actions,
                redacted_fields: state.redacted_fields,
                span,
            })
        }
        "detail" => {
            reject_list_only_view_body(header, &state, "view detail")?;
            ViewAst::Detail(ViewDetailAst {
                name,
                route,
                source: state.source.ok_or_else(|| {
                    line_error(
                        header,
                        "view detail requires a `source <feature>.query.<name>` line",
                    )
                })?,
                route_params: state.route_params,
                sections: state.sections,
                cells: state.cells,
                actions: state.actions,
                redacted_fields: state.redacted_fields,
                span,
            })
        }
        "create" => {
            reject_list_only_view_body(header, &state, "view create")?;
            ViewAst::Create(ViewCreateAst {
                name,
                route,
                submit: state.submit.ok_or_else(|| {
                    line_error(
                        header,
                        "view create requires a `submit <feature>.command.<name>` line",
                    )
                })?,
                fields: state.fields,
                cells: state.cells,
                redacted_fields: state.redacted_fields,
                span,
            })
        }
        _ => unreachable!(),
    };
    Ok((view, i))
}

#[derive(Default)]
struct ViewBodyState {
    source: Option<String>,
    submit: Option<String>,
    columns: Vec<String>,
    search: Option<SearchDeclAst>,
    filter: Vec<String>,
    filters: Vec<FilterDeclAst>,
    has_filters_block: bool,
    fields: Vec<String>,
    sections: Vec<String>,
    cells_slot: Option<String>,
    cells: Vec<CellBindingAst>,
    actions: Vec<String>,
    route_params: Vec<RouteParamAst>,
    drawer: Option<DrawerSubViewAst>,
    sort: Option<SortDeclAst>,
    selection: Option<SelectionDeclAst>,
    bulk_actions: Vec<String>,
    bulk_actions_seen: bool,
    settings: Vec<SettingDeclAst>,
    redacted_fields: Vec<String>,
}

type ViewBodyLineHandler =
    for<'a> fn(&SourceLine<'a>, &str, &mut ViewBodyState) -> Result<(), ParseError>;

fn view_body_handlers() -> &'static [(&'static str, ViewBodyLineHandler)] {
    &[
        ("source ", parse_view_source_line),
        ("submit ", parse_view_submit_line),
        ("columns ", parse_view_columns_line),
        ("fields ", parse_view_fields_line),
        ("filter ", parse_view_filter_line),
        ("sections ", parse_view_sections_line),
        ("selection ", parse_view_selection_line),
        ("bulk_actions ", parse_view_bulk_actions_line),
        ("actions ", parse_view_actions_line),
        ("cells ", parse_view_cells_line),
        ("route ", parse_view_route_line),
    ]
}

fn parse_view_source_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.source.is_some() {
        return Err(line_error(line, "view declares `source` at most once"));
    }
    state.source = Some(rest.to_owned());
    Ok(())
}

fn parse_view_submit_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.submit.is_some() {
        return Err(line_error(line, "view declares `submit` at most once"));
    }
    state.submit = Some(rest.to_owned());
    Ok(())
}

fn parse_view_columns_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.columns.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_fields_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if let Some(fields) = rest.strip_suffix(" redacted") {
        let fields = split_lzx_list(fields);
        state.redacted_fields.extend(fields.iter().cloned());
        state.fields.extend(fields);
    } else {
        state.fields.extend(split_lzx_list(rest));
    }
    Ok(())
}

fn parse_view_filter_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.filter.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_sections_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.sections.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_actions_line(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    state.actions.extend(split_lzx_list(rest));
    Ok(())
}

fn parse_view_cells_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    if let Some(slot_rest) = rest.strip_prefix("@client.") {
        let slot = slot_rest.trim();
        if slot.is_empty() {
            return Err(line_error(
                line,
                "`cells @client.<slot>` requires a slot identifier after `@client.`",
            ));
        }
        if slot.split_whitespace().count() > 1 {
            return Err(line_error_owned(
                line,
                format!(
                    "`cells @client.<slot>` accepts only one slot identifier (got `{}`); per-column form is `cells <field> @client.<slot>` and binds a single field",
                    slot
                ),
            ));
        }
        if state.cells_slot.is_some() {
            return Err(line_error(
                line,
                "view declares `cells @client.<slot>` (grid form) at most once",
            ));
        }
        if !is_kebab_or_snake_ident(slot) {
            return Err(line_error_owned(
                line,
                format!("cell slot `{}` must be a kebab/snake identifier", slot),
            ));
        }
        state.cells_slot = Some(slot.to_owned());
        Ok(())
    } else {
        let binding = parse_cell_binding(line, rest)?;
        state.cells.push(binding);
        Ok(())
    }
}

fn parse_view_route_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    let param = parse_route_param(line, rest)?;
    state.route_params.push(param);
    Ok(())
}

fn parse_drawer_block(
    lines: &[SourceLine<'_>],
    start: usize,
    drawer_indent: usize,
    header_rest: &str,
) -> Result<(DrawerSubViewAst, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header_rest.split_whitespace().collect();
    if parts.len() != 3 || parts[1] != "on" {
        return Err(line_error(
            header,
            "drawer blocks use `drawer <name> on select|open`",
        ));
    }
    let name = parts[0].to_owned();
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            header,
            format!("drawer name `{}` must be kebab/snake identifier", name),
        ));
    }
    let trigger = match parts[2] {
        "select" => DrawerTriggerAst::Select,
        "open" => DrawerTriggerAst::ManualOpen,
        _ => {
            return Err(line_error(
                header,
                "drawer trigger must be `select` or `open`",
            ));
        }
    };

    let child_indent = drawer_indent + 2;
    let mut state = ViewBodyState::default();
    let mut route_binding = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= drawer_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "drawer body lines use one indentation level deeper than the `drawer` header",
            ));
        }
        if raw.contains("+=") || raw.contains("-=") {
            return Err(line_error(
                line,
                "partial overrides are not valid in `.lzx`; redeclare the whole drawer",
            ));
        }

        let trimmed = strip_inline_comment(raw).trim_end();
        if trimmed.starts_with("drawer ") {
            return Err(line_error(line, "drawer cannot be nested"));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            parse_view_source_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            if route_binding.is_some() {
                return Err(line_error(line, "drawer declares `route` at most once"));
            }
            route_binding = Some(parse_drawer_route_binding(line, rest.trim())?);
        } else if let Some(rest) = trimmed.strip_prefix("sections ") {
            parse_view_sections_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("cells ") {
            parse_drawer_cells_line(line, rest.trim(), &mut state)?;
        } else if let Some(rest) = trimmed.strip_prefix("actions ") {
            parse_view_actions_line(line, rest.trim(), &mut state)?;
        } else {
            return Err(line_error_owned(
                line,
                format!(
                    "drawer body lines are `source`, `route <key> from selection`, `sections`, `cells <field> @client.<slot>`, or `actions` declarations (got `{}`)",
                    trimmed
                ),
            ));
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        DrawerSubViewAst {
            name,
            trigger,
            source: state.source.ok_or_else(|| {
                line_error(
                    header,
                    "drawer requires a `source <feature>.query.<name>` line",
                )
            })?,
            route_binding,
            sections: state.sections,
            cells: state.cells,
            actions: state.actions,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_drawer_cells_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if rest.split_whitespace().count() != 2 {
        return Err(line_error(
            line,
            "drawer cells use `cells <field> @client.<slot>`",
        ));
    }
    parse_view_cells_line(line, rest, state)
}

fn parse_drawer_route_binding(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<DrawerRouteBindingAst, ParseError> {
    let (target, source) = value.rsplit_once(" from ").ok_or_else(|| {
        line_error(
            line,
            "drawer route binding must be `route <key> from selection`",
        )
    })?;
    let target = target.trim();
    if target.is_empty() {
        return Err(line_error(
            line,
            "drawer route binding requires a target key",
        ));
    }
    if !is_kebab_or_snake_ident(target) {
        return Err(line_error_owned(
            line,
            format!(
                "drawer route target `{}` must be kebab/snake identifier",
                target
            ),
        ));
    }
    if source.trim() != "selection" {
        return Err(line_error(
            line,
            "drawer route binding source must be `from selection`",
        ));
    }
    Ok(DrawerRouteBindingAst {
        target: target.to_owned(),
        source: DrawerBindingSourceAst::Selection,
    })
}

fn parse_filters_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
    state: &mut ViewBodyState,
) -> Result<(usize, usize), ParseError> {
    let header = &lines[start];
    if state.has_filters_block {
        return Err(line_error(
            header,
            "view list declares `filters` at most once",
        ));
    }
    state.has_filters_block = true;

    let child_indent = body_indent + 2;
    let mut block_filters = Vec::new();
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
                "filters declarations use one indentation level deeper than the `filters` header",
            ));
        }

        let trimmed = strip_inline_comment(raw).trim_end();
        let filter = parse_filter_decl(line, trimmed)?;
        if block_filters
            .iter()
            .any(|existing: &FilterDeclAst| existing.name == filter.name)
        {
            return Err(line_error_owned(
                line,
                format!("duplicate filter `{}` in `filters` block", filter.name),
            ));
        }
        last_end = line.end;
        block_filters.push(filter);
        i += 1;
    }

    if block_filters.is_empty() {
        return Err(line_error(
            header,
            "filters block requires at least one filter declaration",
        ));
    }

    state.filters.extend(block_filters);
    Ok((i, last_end))
}

fn parse_filter_decl(line: &SourceLine<'_>, value: &str) -> Result<FilterDeclAst, ParseError> {
    let (name_raw, type_raw) = value.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "filter declaration must be `<name>: [list of] <Type> [from query]`",
        )
    })?;
    let name = name_raw.trim().to_owned();
    if !is_lzx_bare_ident(&name) {
        return Err(line_error_owned(
            line,
            format!(
                "filter name `{}` must start with a letter and contain only letters, digits, or `_`",
                name
            ),
        ));
    }

    let mut rest = type_raw.trim();
    let mut url_sync = false;
    if let Some((head, source)) = rest.rsplit_once(" from ") {
        if source.trim() != "query" {
            return Err(line_error(line, "filter URL source must be `from query`"));
        }
        rest = head.trim();
        url_sync = true;
    }

    let (cardinality, type_ref) = if let Some(type_ref) = rest.strip_prefix("list of ") {
        (FilterCardinalityAst::Multi, type_ref.trim())
    } else {
        (FilterCardinalityAst::Single, rest)
    };
    if type_ref.is_empty() {
        return Err(line_error(line, "filter declaration requires a type"));
    }
    if !is_lzx_bare_ident(type_ref) {
        return Err(line_error_owned(
            line,
            format!("filter type `{}` must be a bare identifier", type_ref),
        ));
    }

    Ok(FilterDeclAst {
        name,
        type_ref: type_ref.to_owned(),
        cardinality,
        url_sync,
        span: Span::new(line.start, line.end),
    })
}

fn parse_view_search_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    body_indent: usize,
) -> Result<(SearchDeclAst, usize), ParseError> {
    let header = &lines[start];
    if rest == "segmented" {
        parse_view_segmented_search(lines, start, body_indent)
    } else if rest.starts_with("segmented ") {
        Err(line_error(
            header,
            "the `segmented` form takes no inline list — use child `field` declarations",
        ))
    } else {
        Ok((
            SearchDeclAst {
                mode: SearchModeAst::Columns(split_lzx_list(rest)),
                fields: Vec::new(),
                free_text_target: None,
                span: Span::new(header.start, header.end),
            },
            start + 1,
        ))
    }
}

fn parse_view_segmented_search(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
) -> Result<(SearchDeclAst, usize), ParseError> {
    let header = &lines[start];
    let child_indent = body_indent + 2;
    let mut fields: Vec<SearchFieldAst> = Vec::new();
    let mut free_text_target = None;
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
                "`search segmented` child lines use one indentation level deeper than `search segmented`",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("field ") {
            let field = parse_view_search_field(line, rest.trim())?;
            if fields.iter().any(|existing| existing.key == field.key) {
                return Err(line_error_owned(
                    line,
                    format!(
                        "`search segmented` declares field `{}` more than once",
                        field.key
                    ),
                ));
            }
            fields.push(field);
        } else if let Some(rest) = trimmed.strip_prefix("free text into ") {
            if free_text_target.is_some() {
                return Err(line_error(
                    line,
                    "`search segmented` declares `free text into` at most once",
                ));
            }
            free_text_target = Some(parse_binding_ref(line, rest.trim())?);
        } else {
            return Err(line_error(
                line,
                "`search segmented` children are `field <key> binds <BindingRef>` or `free text into <BindingRef>`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    Ok((
        SearchDeclAst {
            mode: SearchModeAst::Segmented,
            fields,
            free_text_target,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_view_search_field(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<SearchFieldAst, ParseError> {
    let Some((key, target)) = rest.split_once(" binds ") else {
        return Err(line_error(
            line,
            "`search segmented` fields use `field <key> binds <BindingRef>`",
        ));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(line_error(
            line,
            "`search segmented` field key cannot be empty",
        ));
    }
    Ok(SearchFieldAst {
        key: key.to_owned(),
        binds_to: parse_binding_ref(line, target.trim())?,
        span: Span::new(line.start, line.end),
    })
}

fn parse_binding_ref(line: &SourceLine<'_>, raw: &str) -> Result<BindingRefAst, ParseError> {
    if raw == "selection" {
        return Ok(BindingRefAst::SelectionScalar);
    }
    if let Some(name) = raw.strip_prefix("filters.") {
        if !name.is_empty() {
            return Ok(BindingRefAst::Filter {
                name: name.to_owned(),
            });
        }
    }
    if let Some(name) = raw.strip_prefix("source.") {
        if !name.is_empty() {
            return Ok(BindingRefAst::SourceInput {
                name: name.to_owned(),
            });
        }
    }
    Err(line_error(
        line,
        "binding references are `filters.<name>`, `source.<name>`, or `selection`",
    ))
}

fn parse_view_selection_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.selection.is_some() {
        return Err(line_error(line, "view declares `selection` at most once"));
    }
    let mode = match rest {
        "single" => SelectionModeAst::Single,
        "multi" => SelectionModeAst::Multi,
        "none" => {
            return Err(line_error(
                line,
                "`selection none` is not valid; omit the line for no selection",
            ));
        }
        _ => {
            return Err(line_error(
                line,
                "`selection` must be `selection single` or `selection multi`",
            ));
        }
    };
    state.selection = Some(SelectionDeclAst {
        mode,
        bulk_actions: Vec::new(),
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

fn parse_view_bulk_actions_line(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ViewBodyState,
) -> Result<(), ParseError> {
    if state.bulk_actions_seen {
        return Err(line_error(
            line,
            "view declares `bulk_actions` at most once",
        ));
    }
    let actions = split_lzx_list(rest);
    if actions.is_empty() {
        return Err(line_error(
            line,
            "`bulk_actions` requires at least one command name",
        ));
    }
    state.bulk_actions = actions;
    state.bulk_actions_seen = true;
    Ok(())
}

fn assemble_selection_decl(state: &ViewBodyState, view_span: Span) -> Option<SelectionDeclAst> {
    if let Some(mut selection) = state.selection.clone() {
        selection.bulk_actions = state.bulk_actions.clone();
        Some(selection)
    } else if state.bulk_actions_seen {
        Some(SelectionDeclAst {
            mode: SelectionModeAst::None,
            bulk_actions: state.bulk_actions.clone(),
            span: view_span,
        })
    } else {
        None
    }
}

fn reject_list_only_view_body(
    header: &SourceLine<'_>,
    state: &ViewBodyState,
    kind: &str,
) -> Result<(), ParseError> {
    if state.sort.is_some()
        || state.selection.is_some()
        || state.bulk_actions_seen
        || !state.settings.is_empty()
    {
        return Err(line_error_owned(
            header,
            format!(
                "`sort`, `selection`, `bulk_actions`, and `settings` are valid only in `view list`, not `{}`",
                kind
            ),
        ));
    }
    Ok(())
}

/// Split the `<name> [at "<path>"]` tail of a view header. The optional
/// `at "<...>"` clause carries a quoted route path.
fn parse_view_header_tail(
    header: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<String>), ParseError> {
    let rest = rest.trim();
    if let Some(at_idx) = find_top_level_token(rest, " at ") {
        let name = rest[..at_idx].trim().to_owned();
        if name.is_empty() {
            return Err(line_error(header, "view header requires a name"));
        }
        let after = rest[at_idx + " at ".len()..].trim();
        if !after.starts_with('"') {
            return Err(line_error(
                header,
                "`at` route must be a quoted string (e.g. `at \"/slugs\"`)",
            ));
        }
        let route = unquote_lzx_value(after).to_owned();
        if !route.starts_with('/') {
            return Err(line_error(header, "`at` route path must begin with `/`"));
        }
        Ok((name, Some(route)))
    } else {
        let name = rest.trim().to_owned();
        if name.is_empty() {
            return Err(line_error(header, "view header requires a name"));
        }
        Ok((name, None))
    }
}

/// Parse `cells <field> @client.<slot>` — `value` is the text after the
/// `cells ` prefix.
fn parse_cell_binding(line: &SourceLine<'_>, value: &str) -> Result<CellBindingAst, ParseError> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(
            line,
            "cell bindings use `cells <field> @client.<slot>`",
        ));
    }
    let field = parts[0].to_owned();
    let slot = parts[1]
        .strip_prefix("@client.")
        .ok_or_else(|| line_error(line, "cell slot must be `@client.<slot>`"))?
        .to_owned();
    if !is_kebab_or_snake_ident(&field) {
        return Err(line_error_owned(
            line,
            format!("cell field `{}` must be a kebab/snake identifier", field),
        ));
    }
    if !is_kebab_or_snake_ident(&slot) {
        return Err(line_error_owned(
            line,
            format!("cell slot `{}` must be a kebab/snake identifier", slot),
        ));
    }
    Ok(CellBindingAst {
        field,
        slot,
        span: Span::new(line.start, line.end),
    })
}

/// Parse `route <name>: <Type> from path` — the path-source clause is
/// mandatory; the lzx grammar reserves `route ... from path` for typed
/// path parameters.
fn parse_route_param(line: &SourceLine<'_>, value: &str) -> Result<RouteParamAst, ParseError> {
    // Pattern: `<name>: <Type> from path`. Split on `from` first so
    // any `:` inside `<Type>` is preserved.
    let (head, source) = value
        .rsplit_once(" from ")
        .ok_or_else(|| line_error(line, "route param must be `route <name>: <Type> from path`"))?;
    if source.trim() != "path" {
        return Err(line_error(line, "route param source must be `from path`"));
    }
    let (name_raw, type_raw) = head
        .split_once(':')
        .ok_or_else(|| line_error(line, "route param must be `route <name>: <Type> from path`"))?;
    let name = name_raw.trim().to_owned();
    let type_ref = type_raw.trim().to_owned();
    if name.is_empty() || type_ref.is_empty() {
        return Err(line_error(
            line,
            "route param requires both a name and a type",
        ));
    }
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            line,
            format!("route param name `{}` must be kebab/snake case", name),
        ));
    }
    Ok(RouteParamAst {
        name,
        type_ref,
        span: Span::new(line.start, line.end),
    })
}

fn parse_view_sort_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
) -> Result<(SortDeclAst, usize, usize), ParseError> {
    let header = &lines[start];
    let child_indent = body_indent + 2;
    let mut index = start + 1;
    let mut allowed: Option<Vec<String>> = None;
    let mut default: Option<(String, SortDirAst)> = None;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            index += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`sort` children use one indentation level deeper than `sort`",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("by ") {
            if allowed.is_some() {
                return Err(line_error(line, "`sort` declares `by` at most once"));
            }
            let fields = split_lzx_list(rest);
            if fields.is_empty() {
                return Err(line_error(line, "`sort by` requires at least one field"));
            }
            allowed = Some(fields);
        } else if let Some(rest) = trimmed.strip_prefix("default ") {
            if default.is_some() {
                return Err(line_error(line, "`sort` declares `default` at most once"));
            }
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() != 2 {
                return Err(line_error(
                    line,
                    "`sort default` uses `default <field> <asc|desc>`",
                ));
            }
            default = Some((parts[0].to_owned(), parse_sort_dir(line, parts[1])?));
        } else {
            return Err(line_error(
                line,
                "`sort` children are `by <field>, ...` or `default <field> <asc|desc>`",
            ));
        }
        last_end = line.end;
        index += 1;
    }

    let allowed = allowed.ok_or_else(|| line_error(header, "`sort` requires a `by` line"))?;
    let (default_field, default_dir) =
        default.ok_or_else(|| line_error(header, "`sort` requires a `default` line"))?;
    if !allowed.iter().any(|field| field == &default_field) {
        return Err(line_error_owned(
            header,
            format!(
                "`sort default` field `{}` must be listed in `sort by`",
                default_field
            ),
        ));
    }

    Ok((
        SortDeclAst {
            allowed,
            default_field,
            default_dir,
            span: Span::new(header.start, last_end),
        },
        index,
        last_end,
    ))
}

fn parse_sort_dir(line: &SourceLine<'_>, value: &str) -> Result<SortDirAst, ParseError> {
    match value {
        "asc" => Ok(SortDirAst::Asc),
        "desc" => Ok(SortDirAst::Desc),
        _ => Err(line_error(
            line,
            "`sort default` dir must be `asc` or `desc`",
        )),
    }
}

fn parse_view_settings_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
) -> Result<(Vec<SettingDeclAst>, usize, usize), ParseError> {
    let header = &lines[start];
    let setting_indent = body_indent + 2;
    let persist_indent = body_indent + 4;
    let mut index = start + 1;
    let mut settings = Vec::new();
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            index += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != setting_indent {
            return Err(line_error(
                line,
                "`settings` children use one indentation level deeper than `settings`",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if trimmed.starts_with("persist ") {
            return Err(line_error(
                line,
                "`persist` is valid only as a child of a setting declaration",
            ));
        }
        let mut setting = parse_setting_decl_line(line, trimmed)?;
        if settings
            .iter()
            .any(|existing: &SettingDeclAst| existing.name == setting.name)
        {
            return Err(line_error_owned(
                line,
                format!("duplicate setting `{}`", setting.name),
            ));
        }
        last_end = line.end;
        index += 1;

        let mut persistence_seen = false;
        while index < lines.len() {
            let child = &lines[index];
            let child_raw = child.text.trim_start();
            if is_trivia(child_raw) {
                index += 1;
                continue;
            }
            if child.indent <= setting_indent {
                break;
            }
            if child.indent != persist_indent {
                return Err(line_error(
                    child,
                    "setting children use one indentation level deeper than the setting declaration",
                ));
            }
            let child_trimmed = strip_inline_comment(child_raw).trim_end();
            if let Some(rest) = child_trimmed.strip_prefix("persist ") {
                if persistence_seen {
                    return Err(line_error(child, "setting declares `persist` at most once"));
                }
                persistence_seen = true;
                setting.persistence = parse_setting_persistence(child, rest.trim())?;
            } else {
                return Err(line_error(
                    child,
                    "setting children are `persist local`, `persist workspace`, or `persist none`",
                ));
            }
            setting.span = Span::new(setting.span.start, child.end);
            last_end = child.end;
            index += 1;
        }

        settings.push(setting);
    }

    if settings.is_empty() {
        return Err(line_error(
            header,
            "`settings` requires at least one setting",
        ));
    }
    Ok((settings, index, last_end))
}

fn parse_setting_decl_line(
    line: &SourceLine<'_>,
    trimmed: &str,
) -> Result<SettingDeclAst, ParseError> {
    let (name_raw, rest_raw) = trimmed.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "setting declarations use `<name>: <Type> [constraints] default <value>`",
        )
    })?;
    let name = name_raw.trim().to_owned();
    if !is_kebab_or_snake_ident(&name) {
        return Err(line_error_owned(
            line,
            format!("setting name `{}` must be kebab/snake case", name),
        ));
    }
    let rest = rest_raw.trim();
    let (value_space, default) = if let Some(after_enum) = rest.strip_prefix("Enum ") {
        parse_enum_setting(line, after_enum.trim())?
    } else if let Some(after_bool) = rest.strip_prefix("Bool ") {
        parse_bool_setting(line, after_bool.trim())?
    } else if let Some(after_int) = rest.strip_prefix("Int ") {
        parse_int_setting(line, after_int.trim())?
    } else {
        return Err(line_error(
            line,
            "setting type must be `Enum [...]`, `Bool`, or `Int`",
        ));
    };

    Ok(SettingDeclAst {
        name,
        value_space,
        default,
        persistence: SettingPersistenceAst::None,
        span: Span::new(line.start, line.end),
    })
}

fn parse_enum_setting(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(SettingValueSpaceAst, String), ParseError> {
    if !rest.starts_with('[') {
        return Err(line_error(line, "enum settings use `Enum [value, ...]`"));
    }
    let values_end = rest.find(']').ok_or_else(|| {
        line_error(
            line,
            "enum settings use `Enum [value, ...] default <value>`",
        )
    })?;
    let values = split_lzx_list(&rest[1..values_end]);
    if values.is_empty() {
        return Err(line_error(line, "enum settings require at least one value"));
    }
    let default = parse_required_default(line, rest[values_end + 1..].trim())?;
    if !values.iter().any(|value| value == &default) {
        return Err(line_error_owned(
            line,
            format!(
                "enum setting default `{}` is not in the enum values",
                default
            ),
        ));
    }
    Ok((SettingValueSpaceAst::Enum(values), default))
}

fn parse_bool_setting(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(SettingValueSpaceAst, String), ParseError> {
    let default = parse_required_default(line, rest)?;
    if !matches!(default.as_str(), "true" | "false") {
        return Err(line_error(
            line,
            "bool setting default must be `true` or `false`",
        ));
    }
    Ok((SettingValueSpaceAst::Bool, default))
}

fn parse_int_setting(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(SettingValueSpaceAst, String), ParseError> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut min = None;
    let mut max = None;
    let mut default = None;
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "min" => {
                index += 1;
                let value = parts.get(index).ok_or_else(|| {
                    line_error(line, "int setting `min` requires an integer value")
                })?;
                min = Some(parse_i64_token(line, value, "min")?);
            }
            "max" => {
                index += 1;
                let value = parts.get(index).ok_or_else(|| {
                    line_error(line, "int setting `max` requires an integer value")
                })?;
                max = Some(parse_i64_token(line, value, "max")?);
            }
            "default" => {
                index += 1;
                let value = parts.get(index).ok_or_else(|| {
                    line_error(line, "int setting `default` requires an integer value")
                })?;
                if default.is_some() {
                    return Err(line_error(line, "setting declares `default` at most once"));
                }
                default = Some((*value).to_owned());
            }
            _ => {
                return Err(line_error(
                    line,
                    "int settings use `Int [min N] [max N] default V`",
                ));
            }
        }
        index += 1;
    }
    let default = default.ok_or_else(|| line_error(line, "setting requires `default <value>`"))?;
    let default_value = default.parse::<i64>().map_err(|_| {
        line_error(
            line,
            "int setting default must be an integer within the declared range",
        )
    })?;
    if let Some(min) = min {
        if default_value < min {
            return Err(line_error(
                line,
                "int setting default is below the declared `min`",
            ));
        }
    }
    if let Some(max) = max {
        if default_value > max {
            return Err(line_error(
                line,
                "int setting default is above the declared `max`",
            ));
        }
    }
    Ok((SettingValueSpaceAst::Int { min, max }, default))
}

fn parse_required_default(line: &SourceLine<'_>, rest: &str) -> Result<String, ParseError> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 2 || parts[0] != "default" {
        return Err(line_error(line, "setting requires `default <value>`"));
    }
    Ok(parts[1].to_owned())
}

fn parse_i64_token(
    line: &SourceLine<'_>,
    value: &str,
    label: &'static str,
) -> Result<i64, ParseError> {
    value
        .parse::<i64>()
        .map_err(|_| line_error_owned(line, format!("int setting `{}` must be an integer", label)))
}

fn parse_setting_persistence(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<SettingPersistenceAst, ParseError> {
    match value {
        "local" => Ok(SettingPersistenceAst::Local),
        "workspace" => Ok(SettingPersistenceAst::Workspace),
        "none" => Ok(SettingPersistenceAst::None),
        _ => Err(line_error(
            line,
            "`persist` must be `persist local`, `persist workspace`, or `persist none`",
        )),
    }
}

/// Parse a `@<namespace>.<name>` policy atom, with an optional raw
/// parenthesized argument suffix for step-up atoms such as
/// `@mfa.required(within:15m)`.
fn parse_policy_atom(line: &SourceLine<'_>, value: &str) -> Result<PolicyAtomAst, ParseError> {
    let atom = value.trim();
    let body = atom.strip_prefix('@').ok_or_else(|| {
        line_error(
            line,
            "policy atoms start with `@` (e.g. `@scope.workspace_admin`)",
        )
    })?;
    let (body, args) = if let Some((head, tail)) = body.split_once('(') {
        if !tail.ends_with(')') {
            return Err(line_error(
                line,
                "policy atom arguments must be closed with `)`",
            ));
        }
        let args = tail[..tail.len() - 1].trim();
        if args.is_empty() {
            return Err(line_error(line, "policy atom arguments cannot be empty"));
        }
        (head.trim(), Some(args.to_owned()))
    } else {
        (body.trim(), None)
    };
    let (namespace, name) = body.split_once('.').ok_or_else(|| {
        line_error(
            line,
            "policy atom must include a namespace and name (`@<ns>.<name>`)",
        )
    })?;
    if !matches!(
        namespace,
        "scope" | "role" | "actor" | "mfa" | "session" | "rate_budget" | "time"
    ) {
        return Err(line_error_owned(
            line,
            format!(
                "policy atom namespace `{}` is not in the closed catalog (`scope` | `role` | `actor` | `mfa` | `session` | `rate_budget` | `time`)",
                namespace
            ),
        ));
    }
    if !is_kebab_or_snake_ident(name) {
        return Err(line_error_owned(
            line,
            format!("policy atom name `{}` must be kebab/snake case", name),
        ));
    }
    Ok(PolicyAtomAst {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        args,
        span: Span::new(line.start, line.end),
    })
}

/// RB.S6 — recognize the new `has_role` / `has_permission` /
/// `authenticated` predicates within a `policy <expr>` payload. The
/// caller passes the raw payload (`rest.trim()` from `policy <rest>`);
/// the helper returns:
///
/// - `Ok(Some(expr))` when the payload is a structured expression
///   (contains `has_role` / `has_permission` / `authenticated` /
///   `and` / `or` / `not` / parens).
/// - `Ok(None)` when the payload is a bare legacy atom
///   (`@policy.<name>` / `@role.<name>` / etc.) — back-compat path,
///   caller keeps the raw string and skips the expression form.
/// - `Err(_)` when the payload looks expression-shaped but is
///   malformed (unknown predicate, bad permission ref, etc.).
fn try_parse_policy_expr(
    line: &SourceLine<'_>,
    payload: &str,
) -> Result<Option<PolicyExprAst>, ParseError> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    // Back-compat fast path: bare atom (no spaces, no parens, no keyword
    // boundaries). Examples: `@policy.create`, `@role.admin`,
    // `@scope.same_company`. The caller keeps the raw string for the
    // legacy single-atom rendering.
    if !looks_like_policy_expr(trimmed) {
        return Ok(None);
    }
    let mut parser = PolicyExprParser::new(trimmed, line);
    let expr = parser.parse_or()?;
    if !parser.is_at_end() {
        return Err(line_error_owned(
            line,
            format!(
                "unexpected trailing input in policy expression: `{}`",
                parser.remaining()
            ),
        ));
    }
    Ok(Some(expr))
}

/// Cheap surface heuristic: does the payload contain any of the closed
/// expression keywords or grouping punctuation?
fn looks_like_policy_expr(payload: &str) -> bool {
    if payload.contains('(') || payload.contains(')') {
        return true;
    }
    // Tokenize on whitespace; any token equal to a reserved keyword
    // qualifies as expression-shaped.
    for tok in payload.split_whitespace() {
        match tok {
            "authenticated" | "has_role" | "has_permission" | "and" | "or" | "not" => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Hand-rolled recursive-descent parser for the closed policy
/// expression grammar:
///
/// ```text
/// or_expr   := and_expr ("or" and_expr)*
/// and_expr  := unary_expr ("and" unary_expr)*
/// unary_expr := "not" unary_expr | atom_expr
/// atom_expr := "(" or_expr ")"
///            | "authenticated"
///            | "has_role" <ident>
///            | "has_permission" <perm_ref>
///            | <policy_atom>     # @<ns>.<name>
/// ```
struct PolicyExprParser<'a, 'src> {
    input: &'a str,
    pos: usize,
    line: &'a SourceLine<'src>,
}

impl<'a, 'src> PolicyExprParser<'a, 'src> {
    fn new(input: &'a str, line: &'a SourceLine<'src>) -> Self {
        Self {
            input,
            pos: 0,
            line,
        }
    }

    fn is_at_end(&self) -> bool {
        self.skip_ws_peek();
        self.pos >= self.input.len()
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    fn skip_ws_peek(&self) -> usize {
        let bytes = self.input.as_bytes();
        let mut p = self.pos;
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        p
    }

    fn skip_ws(&mut self) {
        self.pos = self.skip_ws_peek();
    }

    /// Consume the literal `kw` if it appears next (followed by
    /// whitespace, `(`, or end). Returns true on success.
    fn consume_keyword(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let rest = &self.input[self.pos..];
        if !rest.starts_with(kw) {
            return false;
        }
        let after = &rest[kw.len()..];
        if !after.is_empty() {
            let c = after.as_bytes()[0];
            if !(c.is_ascii_whitespace() || c == b'(' || c == b')') {
                return false;
            }
        }
        self.pos += kw.len();
        true
    }

    fn consume_char(&mut self, c: char) -> bool {
        self.skip_ws();
        let rest = &self.input[self.pos..];
        if rest.starts_with(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    /// Read a bare ident token (lowercase + digits + `_`). Used for
    /// `has_role <ident>`.
    fn read_ident(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let start = self.pos;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase() || c == b'_' || (self.pos > start && c.is_ascii_digit()) {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(self.input[start..self.pos].to_owned())
        }
    }

    /// Read a permission ref: 2-4 colon-separated lowercase segments.
    /// Mirrors `parse_permission_decl` validation; centralised here so
    /// `has_permission` malformed args raise a parse error
    /// (RBAC-POLICY-PREDICATE-FORM-001 spec).
    fn read_permission_ref(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let start = self.pos;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase()
                || c == b'_'
                || c == b':'
                || (self.pos > start && c.is_ascii_digit())
            {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(self.input[start..self.pos].to_owned())
        }
    }

    /// Read a `@<ns>.<name>` atom token, including one optional
    /// parenthesized argument suffix.
    fn read_atom_token(&mut self) -> Option<&str> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        if self.pos >= bytes.len() || bytes[self.pos] != b'@' {
            return None;
        }
        let start = self.pos;
        self.pos += 1;
        while self.pos < bytes.len() {
            let c = bytes[self.pos];
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-' || c == b'.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start + 1 {
            // Just `@` with nothing after.
            self.pos = start;
            return None;
        }
        if self.pos < bytes.len() && bytes[self.pos] == b'(' {
            self.pos += 1;
            while self.pos < bytes.len() && bytes[self.pos] != b')' {
                self.pos += 1;
            }
            if self.pos < bytes.len() && bytes[self.pos] == b')' {
                self.pos += 1;
            }
        }
        Some(&self.input[start..self.pos])
    }

    fn parse_or(&mut self) -> Result<PolicyExprAst, ParseError> {
        let mut terms = vec![self.parse_and()?];
        while self.consume_keyword("or") {
            terms.push(self.parse_and()?);
        }
        Ok(if terms.len() == 1 {
            terms.into_iter().next().unwrap()
        } else {
            PolicyExprAst::Or(terms)
        })
    }

    fn parse_and(&mut self) -> Result<PolicyExprAst, ParseError> {
        let mut terms = vec![self.parse_unary()?];
        while self.consume_keyword("and") {
            terms.push(self.parse_unary()?);
        }
        Ok(if terms.len() == 1 {
            terms.into_iter().next().unwrap()
        } else {
            PolicyExprAst::And(terms)
        })
    }

    fn parse_unary(&mut self) -> Result<PolicyExprAst, ParseError> {
        if self.consume_keyword("not") {
            let inner = self.parse_unary()?;
            return Ok(PolicyExprAst::Not(Box::new(inner)));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<PolicyExprAst, ParseError> {
        self.skip_ws();
        if self.consume_char('(') {
            let inner = self.parse_or()?;
            if !self.consume_char(')') {
                return Err(line_error(
                    self.line,
                    "unbalanced parens in policy expression (expected `)`)",
                ));
            }
            return Ok(inner);
        }
        if self.consume_keyword("authenticated") {
            return Ok(PolicyExprAst::Authenticated);
        }
        if self.consume_keyword("has_role") {
            let name = self.read_ident().ok_or_else(|| {
                line_error(
                    self.line,
                    "`has_role` requires an identifier (e.g. `has_role manager`)",
                )
            })?;
            return Ok(PolicyExprAst::HasRole(name));
        }
        if self.consume_keyword("has_permission") {
            let perm = self.read_permission_ref().ok_or_else(|| {
                line_error(
                    self.line,
                    "`has_permission` requires a permission ref (e.g. `has_permission users:read`)",
                )
            })?;
            // Validate shape: 2-4 colon-separated lowercase segments,
            // each non-empty. Mirrors the RBAC catalog grammar.
            if !is_valid_permission_ref(&perm) {
                return Err(line_error_owned(
                    self.line,
                    format!(
                        "`has_permission` argument `{}` must be 2-4 colon-separated lowercase segments",
                        perm
                    ),
                ));
            }
            return Ok(PolicyExprAst::HasPermission(perm));
        }
        if let Some(tok) = self.read_atom_token() {
            // Re-parse via parse_policy_atom to enforce the closed
            // namespace catalog. `tok` includes the leading `@`.
            let owned = tok.to_owned();
            let atom = parse_policy_atom(self.line, &owned)?;
            return Ok(PolicyExprAst::Atom(atom));
        }
        Err(line_error_owned(
            self.line,
            format!(
                "expected `authenticated`, `has_role`, `has_permission`, `not`, `(`, or `@<ns>.<name>` in policy expression; found `{}`",
                self.remaining()
            ),
        ))
    }
}

/// Permission ref shape: 2-4 colon-separated lowercase segments, each
/// non-empty, alphanumeric + `_`, first char lowercase. Mirrors the
/// `permission <ref>` catalog grammar (`parse_permission_decl`).
fn is_valid_permission_ref(s: &str) -> bool {
    let segments: Vec<&str> = s.split(':').collect();
    if segments.len() < 2 || segments.len() > 4 {
        return false;
    }
    for seg in segments {
        if seg.is_empty() {
            return false;
        }
        let mut chars = seg.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() {
            return false;
        }
        for c in chars {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return false;
            }
        }
    }
    true
}

/// Identifier check used across audience / view / cell / route names:
/// kebab-case (`workspace-admin`) and snake_case (`workspace_admin`)
/// both pass; anything else (PascalCase, spaces, leading digit, etc.)
/// rejects.
fn is_kebab_or_snake_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() {
        return false;
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
            return false;
        }
    }
    true
}

fn is_lzx_bare_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_lzx_resume_ref(s: &str) -> bool {
    let parts: Vec<_> = s.split('.').collect();
    matches!(parts.as_slice(), [name] if is_lzx_bare_ident(name))
        || matches!(parts.as_slice(), [feature, name] if is_lzx_bare_ident(feature) && is_lzx_bare_ident(name))
}

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

const AGENT_INDENT_FEATURE_CHILD: usize = 2;
const AGENT_INDENT_AGENT_CHILD: usize = 4;
const AGENT_INDENT_GRANDCHILD: usize = 6;
const AGENT_INDENT_GREAT_GRANDCHILD: usize = 8;

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
            let (parsed, next) = parse_notification(lines, i)?;
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
            let (parsed, next) = parse_channel(lines, i)?;
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
            let (parsed, next) = parse_event_group(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            event_groups.push(parsed);
            i = next;
            continue;
        }

        // MCP bucket cycle — `mcp_server <name>` block. Closed-catalog
        // children: transport / scope / auth / metadata / tool /
        // resource / prompt. See `docs/proposals/bucket-mcp-cycle.md`.
        if line.indent == AGENT_INDENT_FEATURE_CHILD && trimmed.starts_with("mcp_server ") {
            let (parsed, next) = parse_mcp_server(lines, i)?;
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
        // blocks. Authored inside `domain` at indent 4. Header is
        // recognised unambiguously by the keyword prefix.
        if trimmed.starts_with("query.list ")
            || trimmed.starts_with("query.lookup ")
            || trimmed.starts_with("query.sql ")
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
            let (parsed, next) = parse_translation_decl(lines, i)?;
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
            q.public_contract =
                take_matching_public_contract(line, pending_contract, "query.sql", &q.name)?;
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
                return Err(line_error(
                    inner,
                    "policy category children are `when_denied @translation.<key>` only",
                ));
            }
            categories.push(PolicyCategoryDecl {
                name: name.to_owned(),
                atoms,
                when_denied,
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
// where the value is a bare integer or a quoted string.
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
        let (variant_name, storage) = match body.split_once('=') {
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
            None => (body.to_owned(), None),
        };
        if variant_name.is_empty() {
            return Err(line_error(line, "enum variant requires a name"));
        }
        variants.push(EnumVariantDecl {
            name: variant_name,
            storage,
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
        return Err(line_error(
            line,
            "`triggers` children use `triggers transition <name>[, <name>]`",
        ));
    };
    if names.is_empty() {
        return Err(line_error(
            line,
            "`triggers transition` requires at least one transition name",
        ));
    }

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

fn parse_invalidates_entry(
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

fn find_token(text: &str, needle: &str) -> Option<usize> {
    // Find `needle` at depth 0 (not inside parens / brackets).
    let bytes = text.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        let ch = bytes[i] as char;
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        if depth == 0 && &bytes[i..i + needle_bytes.len()] == needle_bytes {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find ` = ` outside of parens/brackets. The default literal may itself
/// contain `=` (rare), but the fixture's default literals are simple
/// (`= lead`, `= 0`).
fn find_default_assignment(text: &str) -> Option<usize> {
    find_token(text, " = ")
}

// -----------------------------------------------------------------------------
// Phase L Tier 4d — `query.list` / `query.lookup` / `query.sql` and
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
    Err(line_error(
        header,
        "query header must be `query.list <name>`, `query.lookup <name> by ...`, or `query.sql <name>`",
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
    let header = &lines[start];
    let name = rest.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(header, "`query.sql` requires a name"));
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
                "`query.sql` body children use one indentation level deeper than the header",
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
            sql_path = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`query.sql` children are `policy`, `params`, `scope`, `returns`, `sql`, or `gate behind/quota plan.*`",
            ));
        }
    }

    let returns = returns.ok_or_else(|| {
        line_error(
            header,
            "`query.sql` requires a `returns <Type>` declaration",
        )
    })?;
    let sql_path = sql_path.ok_or_else(|| {
        line_error(
            header,
            "`query.sql` requires a `sql \"./<path>.sql\"` declaration",
        )
    })?;
    Ok((
        QueryDecl::Sql(SqlQueryDecl {
            name,
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

fn parse_job_trigger(line: &SourceLine<'_>, rest: &str) -> Result<JobTrigger, ParseError> {
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

fn parse_job_retry(line: &SourceLine<'_>, rest: &str) -> Result<JobRetry, ParseError> {
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

fn parse_channel(lines: &[SourceLine<'_>], start: usize) -> Result<(Channel, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("channel ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "channel header must be `channel <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "channel header requires a name"));
    }

    let mut tenant_from: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut payload: Option<String> = None;
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
                "channel body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            tenant_from = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("payload ") {
            payload = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "channel children are `tenant_from <axis>`, `policy @policy.<name>`, \
                 and `payload <RecordType>` (additional kinds — audit, rate_limit, \
                 broadcast, presence — deferred per docs/scope-discipline.md)",
            ));
        }
    }

    let tenant_from = tenant_from.ok_or_else(|| {
        line_error(
            header,
            "`channel` requires a `tenant_from <axis>` declaration",
        )
    })?;
    let policy = policy.ok_or_else(|| {
        line_error(
            header,
            "`channel` requires a `policy @policy.<name>` declaration",
        )
    })?;
    let payload = payload.ok_or_else(|| {
        line_error(
            header,
            "`channel` requires a `payload <RecordType>` declaration",
        )
    })?;

    Ok((
        Channel {
            name,
            tenant_from,
            policy,
            payload,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_notification(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Notification, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("notification ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "notification header must be `notification <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "notification header requires a name"));
    }

    let mut channels: Vec<String> = Vec::new();
    let mut recipient: Option<String> = None;
    let mut trigger: Option<JobTrigger> = None;
    let mut tenant_from: Option<String> = None;
    let mut idempotency_by: Option<String> = None;
    let mut retry: Option<JobRetry> = None;
    let mut template: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut emits: Vec<String> = Vec::new();
    let mut digest: Option<NotificationDigest> = None;
    let mut throttle: Option<NotificationThrottle> = None;
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
                "notification body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("channel ") {
            channels = split_lzx_list(rest);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("recipient ") {
            recipient = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("trigger ") {
            trigger = Some(parse_job_trigger(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            tenant_from = Some(rest.trim().to_owned());
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
        } else if let Some(rest) = trimmed.strip_prefix("template ") {
            template = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            emits.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "digest" {
            if digest.is_some() {
                return Err(line_error(
                    line,
                    "`notification` may declare at most one `digest` sub-block",
                ));
            }
            let (parsed, next) = parse_notification_digest(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            digest = Some(parsed);
            i = next;
        } else if trimmed == "throttle" {
            if throttle.is_some() {
                return Err(line_error(
                    line,
                    "`notification` may declare at most one `throttle` sub-block",
                ));
            }
            let (parsed, next) = parse_notification_throttle(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            throttle = Some(parsed);
            i = next;
        } else {
            return Err(line_error(
                line,
                "notification children are `channel`, `recipient`, `trigger`, `tenant_from`, `idempotency by`, `retry`, `template`, `policy`, `emits`, `digest`, or `throttle`",
            ));
        }
    }

    let recipient = recipient.ok_or_else(|| {
        line_error(
            header,
            "`notification` requires a `recipient <path>` declaration",
        )
    })?;
    let trigger = trigger.ok_or_else(|| {
        line_error(
            header,
            "`notification` requires a `trigger event ...` or `trigger schedule ...` declaration",
        )
    })?;
    let template = template.ok_or_else(|| {
        line_error(
            header,
            "`notification` requires a `template \"./...\"` declaration",
        )
    })?;

    Ok((
        Notification {
            name,
            channels,
            recipient,
            trigger,
            tenant_from,
            idempotency_by,
            retry,
            template,
            policy,
            policy_expr,
            emits,
            digest,
            throttle,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Notifications expanded bucket cycle — parse the `digest` sub-block
/// of a `notification`. Header line is bare `digest` at indent 4;
/// children at indent 6 are `every "<duration>"` (required),
/// `group_by <path>` (optional), `max_size <N>` (optional), and
/// `template_strategy <merge|append>` (optional). All other child
/// keys are rejected to keep the catalog closed.
fn parse_notification_digest(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(NotificationDigest, usize), ParseError> {
    let header = &lines[start];
    let mut every: Option<String> = None;
    let mut group_by: Option<String> = None;
    let mut max_size: Option<u32> = None;
    let mut template_strategy: Option<String> = None;
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
                "`digest` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("every ") {
            every = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("group_by ") {
            group_by = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("max_size ") {
            let raw = rest.trim();
            match raw.parse::<u32>() {
                Ok(value) => max_size = Some(value),
                Err(_) => {
                    return Err(line_error(
                        line,
                        "`max_size` requires an unsigned 32-bit integer",
                    ));
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("template_strategy ") {
            template_strategy = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`digest` children are `every \"<duration>\"`, `group_by <path>`, `max_size <N>`, or `template_strategy merge|append`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let every = every.ok_or_else(|| {
        line_error(
            header,
            "`digest` requires an `every \"<duration>\"` declaration",
        )
    })?;

    Ok((
        NotificationDigest {
            every,
            group_by,
            max_size,
            template_strategy,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Notifications expanded bucket cycle — parse the `throttle`
/// sub-block of a `notification`. Header line is bare `throttle` at
/// indent 4; children at indent 6 are `max_per "<duration>"`
/// (required), `per_recipient` (bare flag), `per_channel` (bare
/// flag), and `burst <N>` (optional). Distinct keyword from scalar
/// `rate_limit` — the throttle keys on recipient/channel, not on the
/// caller.
fn parse_notification_throttle(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(NotificationThrottle, usize), ParseError> {
    let header = &lines[start];
    let mut max_per: Option<String> = None;
    let mut per_recipient = false;
    let mut per_channel = false;
    let mut burst: Option<u32> = None;
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
                "`throttle` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("max_per ") {
            max_per = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if trimmed == "per_recipient" {
            per_recipient = true;
        } else if trimmed == "per_channel" {
            per_channel = true;
        } else if let Some(rest) = trimmed.strip_prefix("burst ") {
            let raw = rest.trim();
            match raw.parse::<u32>() {
                Ok(value) => burst = Some(value),
                Err(_) => {
                    return Err(line_error(
                        line,
                        "`burst` requires an unsigned 32-bit integer",
                    ));
                }
            }
        } else {
            return Err(line_error(
                line,
                "`throttle` children are `max_per \"<duration>\"`, `per_recipient`, `per_channel`, or `burst <N>`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let max_per = max_per.ok_or_else(|| {
        line_error(
            header,
            "`throttle` requires a `max_per \"<duration>\"` declaration",
        )
    })?;

    Ok((
        NotificationThrottle {
            max_per,
            per_recipient,
            per_channel,
            burst,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// MCP bucket cycle — parse a feature-scoped `mcp_server <name>` block.
/// Header line is `mcp_server <ident>` at indent 2. Children at indent 4:
/// `transport <stdio|http_sse|http_streamable>` (required, closed-catalog),
/// `scope feature.<name>` (optional), `auth bearer env.<NAME>` (optional),
/// `metadata` sub-block (optional, single occurrence), `tool <name>`
/// sub-blocks (repeatable), `resource <name>` sub-blocks (repeatable),
/// `prompt <name>` sub-blocks (repeatable).
fn parse_mcp_server(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpServer, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("mcp_server ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "mcp_server header must be `mcp_server <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "mcp_server header requires a name"));
    }

    let mut transport: Option<String> = None;
    let mut scope_feature: Option<String> = None;
    let mut auth: Option<String> = None;
    let mut metadata = crate::ast::McpServerMetadata::default();
    let mut tools: Vec<crate::ast::McpTool> = Vec::new();
    let mut resources: Vec<crate::ast::McpResource> = Vec::new();
    let mut prompts: Vec<crate::ast::McpPrompt> = Vec::new();
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
                "`mcp_server` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("transport ") {
            let value = rest.trim().to_owned();
            transport = Some(value);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("scope ") {
            scope_feature = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("auth ") {
            auth = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "metadata" {
            let (parsed, next) = parse_mcp_server_metadata(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            metadata = parsed;
            i = next;
        } else if trimmed.starts_with("tool ") {
            let (parsed, next) = parse_mcp_tool(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            tools.push(parsed);
            i = next;
        } else if trimmed.starts_with("resource ") {
            let (parsed, next) = parse_mcp_resource(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            resources.push(parsed);
            i = next;
        } else if trimmed.starts_with("prompt ") {
            let (parsed, next) = parse_mcp_prompt(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            prompts.push(parsed);
            i = next;
        } else {
            return Err(line_error(
                line,
                "`mcp_server` children are `transport`, `scope`, `auth`, `metadata`, `tool <name>`, `resource <name>`, or `prompt <name>`",
            ));
        }
    }

    let transport = transport.ok_or_else(|| {
        line_error(
            header,
            "`mcp_server` requires a `transport <stdio|http_sse|http_streamable>` declaration",
        )
    })?;

    Ok((
        crate::ast::McpServer {
            name,
            transport,
            scope_feature,
            auth,
            metadata,
            tools,
            resources,
            prompts,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse `metadata` sub-block at indent 4 inside an `mcp_server`.
/// Children at indent 6: `name "..."`, `description "..."`, `version "..."`.
fn parse_mcp_server_metadata(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpServerMetadata, usize), ParseError> {
    let mut out = crate::ast::McpServerMetadata::default();
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
                "`metadata` children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("name ") {
            out.name = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("description ") {
            out.description = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("version ") {
            out.version = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`metadata` children are `name \"...\"`, `description \"...\"`, or `version \"...\"`",
            ));
        }
        i += 1;
    }
    Ok((out, i))
}

/// Parse `tool <name>` sub-block at indent 4 inside an `mcp_server`.
fn parse_mcp_tool(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpTool, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("tool ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "tool header must be `tool <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "tool header requires a name"));
    }
    let mut description: Option<String> = None;
    let mut params: Vec<crate::ast::McpParam> = Vec::new();
    let mut returns: Option<String> = None;
    let mut handler: Option<String> = None;
    let mut policy: Option<String> = None;
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
                "`tool` body children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("description ") {
            description = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_mcp_params(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            params = parsed;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("returns ") {
            returns = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`tool` children are `description`, `params`, `returns`, `handler`, or `policy`",
            ));
        }
    }
    let handler = handler
        .ok_or_else(|| line_error(header, "`tool` requires a `handler @fn.<name>` declaration"))?;
    Ok((
        crate::ast::McpTool {
            name,
            description,
            params,
            returns,
            handler,
            policy,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse `resource <name>` sub-block at indent 4 inside an `mcp_server`.
fn parse_mcp_resource(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpResource, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("resource ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "resource header must be `resource <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "resource header requires a name"));
    }
    let mut uri_template: Option<String> = None;
    let mut mime: Option<String> = None;
    let mut handler: Option<String> = None;
    let mut policy: Option<String> = None;
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
                "`resource` body children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("uri_template ") {
            uri_template = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("mime ") {
            mime = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`resource` children are `uri_template`, `mime`, `handler`, or `policy`",
            ));
        }
        last_end = line.end;
        i += 1;
    }
    let uri_template = uri_template.ok_or_else(|| {
        line_error(
            header,
            "`resource` requires a `uri_template \"...\"` declaration",
        )
    })?;
    let handler = handler.ok_or_else(|| {
        line_error(
            header,
            "`resource` requires a `handler @fn.<name>` declaration",
        )
    })?;
    Ok((
        crate::ast::McpResource {
            name,
            uri_template,
            mime,
            handler,
            policy,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse `prompt <name>` sub-block at indent 4 inside an `mcp_server`.
fn parse_mcp_prompt(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpPrompt, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("prompt ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "prompt header must be `prompt <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "prompt header requires a name"));
    }
    let mut description: Option<String> = None;
    let mut params: Vec<crate::ast::McpParam> = Vec::new();
    let mut template: Option<String> = None;
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
                "`prompt` body children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("description ") {
            description = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_mcp_params(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            params = parsed;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("template ") {
            template = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`prompt` children are `description`, `params`, or `template`",
            ));
        }
    }
    let template = template.ok_or_else(|| {
        line_error(
            header,
            "`prompt` requires a `template \"./...\"` declaration",
        )
    })?;
    Ok((
        crate::ast::McpPrompt {
            name,
            description,
            params,
            template,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse `params` sub-block at indent 6 inside a `tool` or `prompt`.
/// Children at indent 8 are `<name>: <type> [required|optional]`.
fn parse_mcp_params(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<crate::ast::McpParam>, usize), ParseError> {
    let mut out: Vec<crate::ast::McpParam> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
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
                "`params` rows use eight-space indentation",
            ));
        }
        let (name_part, after_colon) = trimmed.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "`params` row must be `<name>: <type> [required|optional]`",
            )
        })?;
        let after = after_colon.trim();
        let (ty, required) = if let Some(stripped) = after.strip_suffix(" required") {
            (stripped.trim().to_owned(), true)
        } else if let Some(stripped) = after.strip_suffix(" optional") {
            (stripped.trim().to_owned(), false)
        } else {
            (after.to_owned(), false)
        };
        out.push(crate::ast::McpParam {
            name: name_part.trim().to_owned(),
            ty,
            required,
        });
        i += 1;
    }
    Ok((out, i))
}

/// i18n bucket cycle — parse a `translation` block. Header is the
/// bare keyword (no name). Children at indent 4: `catalog "<path>"`
/// (required, exactly one) and `key <name>` (repeatable). Inside a
/// `key <name>`, indent 6 carries BCP-47 variants (`pt-BR "..."`) and
/// optional `plural <arm>` blocks; inside `plural <arm>`, indent 8
/// carries another set of BCP-47 variants.
fn parse_translation_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(TranslationDecl, usize), ParseError> {
    let header = &lines[start];
    let mut catalog: Option<String> = None;
    let mut keys: Vec<TranslationKeyDecl> = Vec::new();
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
                "translation body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("catalog ") {
            if catalog.is_some() {
                return Err(line_error(
                    line,
                    "`translation` may declare at most one `catalog` line",
                ));
            }
            catalog = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("key ") {
            let name = rest.trim().to_owned();
            if name.is_empty() {
                return Err(line_error(line, "`key` requires a name"));
            }
            let (key, next) = parse_translation_key(lines, i, name)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            keys.push(key);
            i = next;
        } else {
            return Err(line_error(
                line,
                "translation children are `catalog \"<path>\"` and `key <name>`",
            ));
        }
    }

    let catalog = catalog.ok_or_else(|| {
        line_error(
            header,
            "`translation` requires a `catalog \"<path>\"` declaration",
        )
    })?;

    Ok((
        TranslationDecl {
            catalog,
            keys,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_translation_key(
    lines: &[SourceLine<'_>],
    start: usize,
    name: String,
) -> Result<(TranslationKeyDecl, usize), ParseError> {
    let header = &lines[start];
    let mut variants: Vec<TranslationVariantDecl> = Vec::new();
    let mut plurals: Vec<TranslationPluralArmDecl> = Vec::new();
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
                "translation key children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("plural ") {
            let arm = rest.trim().to_owned();
            if arm.is_empty() {
                return Err(line_error(line, "`plural` requires an arm name"));
            }
            let (plural, next) = parse_translation_plural(lines, i, arm)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            plurals.push(plural);
            i = next;
        } else if let Some((locale, rest)) = trimmed.split_once(' ') {
            let text = unquote_lzx_value(rest.trim()).to_owned();
            variants.push(TranslationVariantDecl {
                locale: locale.to_owned(),
                text,
            });
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "translation key body lines are `<bcp47-tag> \"<text>\"` or `plural <arm>`",
            ));
        }
    }

    Ok((
        TranslationKeyDecl {
            name,
            variants,
            plurals,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_translation_plural(
    lines: &[SourceLine<'_>],
    start: usize,
    arm: String,
) -> Result<(TranslationPluralArmDecl, usize), ParseError> {
    let header = &lines[start];
    let mut variants: Vec<TranslationVariantDecl> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

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
                "plural arm body uses eight-space indentation",
            ));
        }
        if let Some((locale, rest)) = trimmed.split_once(' ') {
            let text = unquote_lzx_value(rest.trim()).to_owned();
            variants.push(TranslationVariantDecl {
                locale: locale.to_owned(),
                text,
            });
            i += 1;
        } else {
            return Err(line_error(
                line,
                "plural arm body lines are `<bcp47-tag> \"<text>\"`",
            ));
        }
    }

    Ok((TranslationPluralArmDecl { arm, variants }, i))
}

fn parse_event_group(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(EventGroup, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let rest = header_trimmed
        .strip_prefix("event_group ")
        .ok_or_else(|| line_error(header, "event_group header must be `event_group <pattern>`"))?
        .trim();
    if rest.is_empty() {
        return Err(line_error(header, "event_group header requires a pattern"));
    }
    let (pattern, on_resource) = if let Some(idx) = rest.find(" on ") {
        let (lhs, rhs) = rest.split_at(idx);
        let resource = rhs[" on ".len()..].trim().to_owned();
        if resource.is_empty() {
            return Err(line_error(
                header,
                "`event_group ... on <Resource>` requires a resource name",
            ));
        }
        (lhs.trim().to_owned(), Some(resource))
    } else {
        (rest.to_owned(), None)
    };

    // event_group sits inside `domain`, so its children typically live
    // at `header.indent + 2`. We track that floor here rather than the
    // global agent indent because the group can appear at any depth
    // depending on whether `domain` is nested.
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut payload: Vec<String> = Vec::new();
    let mut audit: Option<String> = None;
    let mut events: Vec<String> = Vec::new();
    let mut events_outbox_guaranteed: Vec<bool> = Vec::new();
    let mut event_variants: Vec<Vec<EventVariantFieldDecl>> = Vec::new();
    let mut event_variant_kinds: Vec<EventVariantKindAst> = Vec::new();
    let mut in_payload = false;
    // EVENT-OUTBOX §3.3 — when set, grandchild `outbox guaranteed` lines
    // toggle the most recently authored event's outbox flag.
    let mut current_event_idx: Option<usize> = None;
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

        if line.indent == child_indent {
            in_payload = false;
            current_event_idx = None;
            if trimmed == "payload" {
                in_payload = true;
            } else if let Some(rest) = trimmed.strip_prefix("audit ") {
                audit = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("event ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !name.is_empty() {
                    events.push(name);
                    events_outbox_guaranteed.push(false);
                    event_variants.push(Vec::new());
                    event_variant_kinds.push(EventVariantKindAst::Committed);
                    current_event_idx = Some(events.len() - 1);
                }
            } else if let Some(rest) = trimmed.strip_prefix("event.trace ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !name.is_empty() {
                    events.push(name);
                    events_outbox_guaranteed.push(false);
                    event_variants.push(Vec::new());
                    event_variant_kinds.push(EventVariantKindAst::Trace);
                    // Trace events cannot carry an outbox guarantee;
                    // grandchild `outbox` lines are rejected below, but
                    // typed field rows are still collected.
                    current_event_idx = Some(events.len() - 1);
                }
            } else {
                // Unknown child — Tier 4 may extend this; skip silently
                // to match Phase L's existing fall-through behaviour.
            }
        } else if line.indent >= grandchild_indent && in_payload {
            payload.push(trimmed.to_owned());
        } else if line.indent >= grandchild_indent
            && let Some(idx) = current_event_idx
            && let Some(rest) = trimmed.strip_prefix("outbox ")
        {
            if matches!(event_variant_kinds[idx], EventVariantKindAst::Trace) {
                return Err(line_error(
                    line,
                    "`event.trace` cannot carry an `outbox` guarantee; trace events bypass the outbox",
                ));
            }
            // EVENT-OUTBOX §3.3 — accept `outbox guaranteed` under
            // `event <name>`. Closed catalog: only `guaranteed` is
            // authored; `outbox none` is implicit (the default).
            match rest.trim() {
                "guaranteed" => events_outbox_guaranteed[idx] = true,
                "none" => events_outbox_guaranteed[idx] = false,
                other => {
                    return Err(line_error_owned(
                        line,
                        format!(
                            "`outbox` only accepts `guaranteed` (or `none`); got `{}`",
                            other
                        ),
                    ));
                }
            }
        } else if line.indent >= grandchild_indent
            && let Some(idx) = current_event_idx
            && trimmed.contains(':')
            && !trimmed.starts_with('#')
        {
            // B5 framework gap 1 — typed payload field row under an
            // event variant. Surface shape mirrors resource fields:
            // `<name>: <Type> [required|optional]`. The type literal
            // is kept verbatim; the analyzer lifts to `ir::TypeRef`.
            let field = parse_event_variant_field(line, trimmed)?;
            event_variants[idx].push(field);
        } else {
            // Continuation of a non-payload child (anything else falls
            // through here; legacy fallthrough kept for fixtures that
            // author lines we haven't taught the parser to lift yet).
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        EventGroup {
            pattern,
            on_resource,
            payload,
            audit,
            events,
            events_outbox_guaranteed,
            event_variants,
            event_variant_kinds,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// B5 framework gap 1 — parse a single `<name>: <Type> [required|optional]`
/// row inside an `event_group`'s `event <name>` body. Mirrors the
/// minimum slot count of `ResourceFieldDecl` but rejects modifiers
/// outside the closed `required` / `optional` pair (no defaults, no
/// constraints, no `unique`/`slug`/`@full_text` because event payloads
/// are projection-only).
fn parse_event_variant_field(
    line: &SourceLine<'_>,
    trimmed: &str,
) -> Result<EventVariantFieldDecl, ParseError> {
    let raw = strip_inline_comment(trimmed).trim_end();
    let (name_part, after) = raw.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "event variant field must be `<name>: <Type> [required|optional]`",
        )
    })?;
    let name = name_part.trim();
    if name.is_empty() {
        return Err(line_error(
            line,
            "event variant field requires a name before `:`",
        ));
    }
    let after = after.trim();
    if after.is_empty() {
        return Err(line_error(
            line,
            "event variant field requires a type after `:`",
        ));
    }
    // Strip trailing modifiers (`required` / `optional`) from the type
    // literal so the analyzer's `type_ref_from_syntax` sees the bare
    // type token. Keep the split conservative: only the last
    // whitespace-separated token is considered a modifier candidate,
    // matching the resource-field convention.
    let mut required = false;
    let mut optional = false;
    let mut type_text = after.to_owned();
    loop {
        let trimmed_type = type_text.trim_end();
        // Last whitespace-separated token without scanning forward.
        let tail = match trimmed_type.rfind(|c: char| c.is_whitespace()) {
            Some(idx) => &trimmed_type[idx + 1..],
            None => trimmed_type,
        };
        match tail {
            "required" => {
                required = true;
                let cut = trimmed_type.len() - tail.len();
                type_text = trimmed_type[..cut].trim_end().to_owned();
            }
            "optional" => {
                optional = true;
                let cut = trimmed_type.len() - tail.len();
                type_text = trimmed_type[..cut].trim_end().to_owned();
            }
            _ => break,
        }
    }
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "event variant field requires a type literal before the modifier",
        ));
    }
    Ok(EventVariantFieldDecl {
        name: name.to_owned(),
        type_text,
        required,
        optional,
        span: Span::new(line.start, line.end),
    })
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

// -----------------------------------------------------------------------------
// L0 #2 — `design.lzi` parser.
//
// `design.lzi` lives at project root. The file declares one `design <name>`
// block at indent 0; children sit at indent 2 (one of the eight closed
// groups), grandchildren at indent 4 (token entries inside groups), and
// great-grandchildren at indent 6 (state entries inside a color sub-block,
// or sub-group entries inside `typography` / `motion`).
//
// Surface forms (parser, not lowering):
//
//   color
//     primary
//       base "#7c3aed"
//       hover "#6d28d9"
//       active "#5b21b6" dark "#7c3aed"
//       foreground "#ffffff"
//     success "#16a34a"             # flat (single state, treated as `base`)
//
//   typography
//     family
//       sans "Inter, system-ui, sans-serif"
//     scale
//       base size 1rem, line_height 1.5rem
//       "2xl" size 1.5rem, line_height 2rem
//     weight
//       regular 400
//     tracking
//       tight -0.025em
//
//   space     <name> <value>
//   radius    <name> <value>
//   shadow    <name> "<value>"
//   motion
//     duration <name> <value>
//     easing   <name> "<value>"
//   breakpoint <name> <value>
//   z          <name> <integer>
//
// Names that start with a digit (`"2xl"`, `"3xl"`, `"16"`) MUST be quoted
// per the lexical rule in §3.1. Unquoted idents preserve the existing
// `IDENT_LOWER` snake_case convention.
// -----------------------------------------------------------------------------

/// Entry point: parse a complete `design.lzi` source. Skips trivia,
/// expects exactly one `design <name>` block at indent 0.
pub fn parse_design_document(source: &str) -> Result<DesignDeclAst, ParseError> {
    let lines = source_lines(source);
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent != 0 {
            return Err(line_error(
                line,
                "top-level `design` declaration must start at indent 0",
            ));
        }
        if trimmed.starts_with("design ") || trimmed == "design" {
            let (parsed, _next) = parse_design_decl(&lines, i)?;
            return Ok(parsed);
        }
        return Err(line_error(
            line,
            "`design.lzi` must begin with a `design <name>` declaration",
        ));
    }
    Err(ParseError::Expected {
        expected: "design <name> declaration",
    })
}

/// Parse a `design <name>` block starting at `lines[start]`. Returns the
/// AST + the index of the first line not consumed. Module-private to
/// match `SourceLine`'s scope; callers use the `parse_design_document`
/// source-text entry point.
fn parse_design_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(DesignDeclAst, usize), ParseError> {
    let header = &lines[start];
    let header_text = strip_inline_comment(header.text.trim_start()).trim_end();
    let name = header_text
        .strip_prefix("design ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "design header must be `design <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "design header requires a name"));
    }
    let header_indent = header.indent;
    let group_indent = header_indent + 2;

    let mut extends: Option<String> = None;
    let mut colors: Vec<ColorTokenAst> = Vec::new();
    let mut typography = TypographyAst::default();
    let mut spaces: Vec<ScaleTokenAst> = Vec::new();
    let mut radii: Vec<ScaleTokenAst> = Vec::new();
    let mut shadows: Vec<ShadowTokenAst> = Vec::new();
    let mut motion = MotionAst::default();
    let mut breakpoints: Vec<ScaleTokenAst> = Vec::new();
    let mut z_indices: Vec<ZTokenAst> = Vec::new();
    let mut custom: Vec<CustomTokenAst> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != group_indent {
            return Err(line_error(
                line,
                "design body children use one indentation level deeper than the `design` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();

        if let Some(rest) = trimmed.strip_prefix("extends ") {
            let target = rest.trim().to_owned();
            if target.is_empty() {
                return Err(line_error(
                    line,
                    "`extends` requires a base design name (e.g. `extends base`)",
                ));
            }
            extends = Some(target);
            last_end = line.end;
            i += 1;
        } else if trimmed == "color" {
            let (parsed, next) = parse_design_color_group(lines, i, line.indent + 2)?;
            colors.extend(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "typography" {
            let (parsed, next) = parse_design_typography(lines, i, line.indent + 2)?;
            typography = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "space" {
            let (parsed, next) = parse_design_scale_group(lines, i, line.indent + 2)?;
            spaces = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "radius" {
            let (parsed, next) = parse_design_scale_group(lines, i, line.indent + 2)?;
            radii = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "shadow" {
            let (parsed, next) = parse_design_shadow_group(lines, i, line.indent + 2)?;
            shadows = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "motion" {
            let (parsed, next) = parse_design_motion(lines, i, line.indent + 2)?;
            motion = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "breakpoint" {
            let (parsed, next) = parse_design_scale_group(lines, i, line.indent + 2)?;
            breakpoints = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "z" {
            let (parsed, next) = parse_design_z_group(lines, i, line.indent + 2)?;
            z_indices = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "custom" {
            // L0 #2 — `custom` 9th meta-group per
            // `docs/proposals/design-tokens-custom.md` §2. Flat sub-grammar.
            let (parsed, next) = parse_design_custom_group(lines, i, line.indent + 2)?;
            custom = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "design children are `extends`, `color`, `typography`, `space`, `radius`, `shadow`, `motion`, `breakpoint`, `z`, or `custom`",
            ));
        }
    }

    Ok((
        DesignDeclAst {
            name,
            extends,
            colors,
            typography,
            spaces,
            radii,
            shadows,
            motion,
            breakpoints,
            z_indices,
            custom,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse the body of `color` (group of named entries, each either flat
/// `<name> "<hex>"` or sub-block with state lines).
fn parse_design_color_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ColorTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let state_indent = child_indent + 2;
    let mut colors: Vec<ColorTokenAst> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "color entries use one indentation level deeper than the `color` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;

        // Disambiguate flat vs sub-block: if `after` is empty (after stripping
        // trailing whitespace), this is a sub-block header; otherwise the
        // remainder is the flat-form hex value (with optional `dark <hex>`).
        let after = after.trim();
        if after.is_empty() {
            let entry_start = line.start;
            let (states, next, last_end) =
                parse_design_color_states(lines, i + 1, state_indent, child_indent)?;
            if states.is_empty() {
                return Err(line_error(
                    line,
                    "color sub-block requires at least one of `base`, `hover`, `active`, `foreground`",
                ));
            }
            colors.push(ColorTokenAst {
                name,
                states,
                span: Span::new(entry_start, last_end),
            });
            i = next;
        } else {
            // Flat form: `<name> "<hex>" [dark "<hex>"]`. Treat the value as
            // an implicit `base` state.
            let (value, dark) = parse_color_value_with_dark(line, after)?;
            colors.push(ColorTokenAst {
                name,
                states: vec![ColorStateAst {
                    kind: "base".to_owned(),
                    value,
                    dark,
                }],
                span: Span::new(line.start, line.end),
            });
            i += 1;
        }
    }

    Ok((colors, i))
}

/// Parse a sequence of `base | hover | active | foreground "<hex>" [dark
/// "<hex>"]` lines at `state_indent` until we leave the parent block.
fn parse_design_color_states(
    lines: &[SourceLine<'_>],
    start: usize,
    state_indent: usize,
    parent_indent: usize,
) -> Result<(Vec<ColorStateAst>, usize, usize), ParseError> {
    let mut states: Vec<ColorStateAst> = Vec::new();
    let mut i = start;
    let mut last_end = if start == 0 { 0 } else { lines[start - 1].end };
    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != state_indent {
            return Err(line_error(
                line,
                "color state entries use one indentation level deeper than the color sub-block name",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (kind, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        if after.is_empty() {
            return Err(line_error(
                line,
                "color state requires a hex value (e.g. `base \"#7c3aed\"`)",
            ));
        }
        let (value, dark) = parse_color_value_with_dark(line, after)?;
        states.push(ColorStateAst { kind, value, dark });
        last_end = line.end;
        i += 1;
    }
    Ok((states, i, last_end))
}

/// Parse the `<value> [dark <value>]` tail of a color line. The values
/// are typically quoted hex literals; we preserve quotes verbatim so the
/// analyzer can validate.
fn parse_color_value_with_dark(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<String>), ParseError> {
    let rest = rest.trim();
    // `dark ` may appear after the primary value; we honor a top-level
    // (paren-depth-0) match so embedded `dark` inside an unlikely literal
    // stays put. In practice values are short hex strings.
    if let Some(idx) = find_top_level_token(rest, " dark ") {
        let primary = rest[..idx].trim();
        let dark_part = rest[idx + " dark ".len()..].trim();
        if primary.is_empty() {
            return Err(line_error(
                line,
                "color value missing before `dark` modifier",
            ));
        }
        if dark_part.is_empty() {
            return Err(line_error(
                line,
                "`dark` modifier requires a hex value (e.g. `dark \"#09090b\"`)",
            ));
        }
        Ok((
            strip_design_quotes(primary).to_owned(),
            Some(strip_design_quotes(dark_part).to_owned()),
        ))
    } else {
        Ok((strip_design_quotes(rest).to_owned(), None))
    }
}

/// Parse `typography` body: family / scale / weight / tracking sub-groups.
fn parse_design_typography(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(TypographyAst, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let entry_indent = child_indent + 2;
    let mut typo = TypographyAst::default();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "typography sub-groups use one indentation level deeper than the `typography` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let sub_header_index = i;
        match trimmed {
            "family" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.families = entries
                    .into_iter()
                    .map(|(name, value)| FamilyTokenAst { name, value })
                    .collect();
                i = next;
            }
            "scale" => {
                let (entries, next) =
                    parse_design_scale_block(lines, sub_header_index, entry_indent)?;
                typo.scale = entries;
                i = next;
            }
            "weight" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.weights = entries
                    .into_iter()
                    .map(|(name, value)| WeightTokenAst { name, value })
                    .collect();
                i = next;
            }
            "tracking" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.tracking = entries
                    .into_iter()
                    .map(|(name, value)| TrackingTokenAst { name, value })
                    .collect();
                i = next;
            }
            other => {
                return Err(line_error_owned(
                    line,
                    format!(
                        "typography sub-groups are `family`, `scale`, `weight`, or `tracking` (got `{other}`)"
                    ),
                ));
            }
        }
    }
    Ok((typo, i))
}

/// Parse the body of a flat `<group>` like `space` / `radius` /
/// `breakpoint`, where each child line is `<name> <value>`.
fn parse_design_scale_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ScaleTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ScaleTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse `shadow` body: each child is `<name> "<value>"` where the value is
/// a CSS box-shadow string (lowering validates single-layer).
fn parse_design_shadow_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ShadowTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ShadowTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse the body of `motion` (duration + easing sub-groups).
fn parse_design_motion(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(MotionAst, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let entry_indent = child_indent + 2;
    let mut motion = MotionAst::default();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "motion sub-groups use one indentation level deeper than the `motion` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let sub_header_index = i;
        match trimmed {
            "duration" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                motion.durations = entries
                    .into_iter()
                    .map(|(name, value)| ScaleTokenAst { name, value })
                    .collect();
                i = next;
            }
            "easing" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                motion.easings = entries
                    .into_iter()
                    .map(|(name, value)| EasingTokenAst { name, value })
                    .collect();
                i = next;
            }
            other => {
                return Err(line_error_owned(
                    line,
                    format!("motion sub-groups are `duration` or `easing` (got `{other}`)"),
                ));
            }
        }
    }
    Ok((motion, i))
}

/// Parse `z` body: each child line is `<name> <integer>`.
fn parse_design_z_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ZTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ZTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse `custom` body: each child line is `<kebab-name> "<hex>" [dark "<hex>"]`.
/// Flat sub-grammar — no state sub-blocks. Lowering enforces hex validity
/// + reserved-name + collision rules. See
/// `docs/proposals/design-tokens-custom.md` §2.
fn parse_design_custom_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<CustomTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<CustomTokenAst> = Vec::new();
    let mut i = header_index + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "custom entries use one indentation level deeper than the `custom` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        if after.is_empty() {
            return Err(line_error(
                line,
                "custom entry requires a hex value (e.g. `chat-bubble \"#dcf8c6\"`)",
            ));
        }
        let (value, dark) = parse_color_value_with_dark(line, after)?;
        entries.push(CustomTokenAst {
            name,
            value,
            dark,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((entries, i))
}

/// Generic `<name> <value>` block parser used by space/radius/shadow/
/// breakpoint/z plus motion.duration/easing plus typography.family/
/// weight/tracking. Values are captured verbatim with surrounding quotes
/// stripped if present; the analyzer applies type-specific validation.
fn parse_design_named_value_block(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<(String, String)>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "design value entries use one indentation level deeper than the group header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, rest) = split_design_name(line, trimmed)?;
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(line_error(
                line,
                "design value entry requires `<name> <value>`",
            ));
        }
        entries.push((name, strip_design_quotes(rest).to_owned()));
        i += 1;
    }
    Ok((entries, i))
}

/// Parse `typography.scale` body: `<name> size <size>, line_height <lh>`.
fn parse_design_scale_block(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<TextScaleTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<TextScaleTokenAst> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "typography.scale entries use one indentation level deeper than the `scale` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        // Expected: `size <size>, line_height <lh>`.
        let after = after.strip_prefix("size ").ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry must be `<name> size <size>, line_height <lh>`",
            )
        })?;
        let comma_idx = after.find(',').ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry requires `, line_height <value>` after the size",
            )
        })?;
        let size = strip_design_quotes(after[..comma_idx].trim()).to_owned();
        let after_comma = after[comma_idx + 1..].trim();
        let lh = after_comma.strip_prefix("line_height ").ok_or_else(|| {
            line_error(
                line,
                "typography.scale entry expects `line_height <value>` after the comma",
            )
        })?;
        let line_height = strip_design_quotes(lh.trim()).to_owned();
        entries.push(TextScaleTokenAst {
            name,
            size,
            line_height,
        });
        i += 1;
    }
    Ok((entries, i))
}

/// Split `<name> <rest>` where `<name>` may be a bare ident or a quoted
/// string (needed for digit-leading names like `"2xl"`). The split
/// happens at the first whitespace following the (possibly-quoted) name.
fn split_design_name<'a>(
    line: &SourceLine<'_>,
    trimmed: &'a str,
) -> Result<(String, &'a str), ParseError> {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() {
        return Err(line_error(line, "expected a token name"));
    }
    let (name_text, rest) = if bytes[0] == b'"' {
        // Scan to matching closing quote.
        let mut i = 1;
        while i < bytes.len() && bytes[i] != b'"' {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            i += 1;
        }
        if i >= bytes.len() {
            return Err(line_error(line, "unterminated quoted token name"));
        }
        let name = &trimmed[1..i];
        let after = trimmed[i + 1..].trim_start();
        (name.to_owned(), after)
    } else {
        let end = bytes
            .iter()
            .position(|b| b.is_ascii_whitespace())
            .unwrap_or(bytes.len());
        let name = trimmed[..end].to_owned();
        let after = trimmed[end..].trim_start();
        (name, after)
    };
    if name_text.is_empty() {
        return Err(line_error(line, "token name cannot be empty"));
    }
    Ok((name_text, rest))
}

/// Strip surrounding `"..."` quotes if present, returning the inner slice.
fn strip_design_quotes(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

/// Find `needle` outside of quoted strings and outside parenthesized
/// regions. Mirrors the depth/quote-aware logic in `find_token` but also
/// suppresses matches inside `"..."` literals — design values often carry
/// `"#hex"` and `"cubic-bezier(...)"` strings that we never want to scan
/// into.
fn find_top_level_token(text: &str, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        let b = bytes[i];
        match b {
            b'"' => in_quote = !in_quote,
            b'(' | b'[' if !in_quote => depth += 1,
            b')' | b']' if !in_quote => depth -= 1,
            _ => {}
        }
        if !in_quote && depth == 0 && &bytes[i..i + needle_bytes.len()] == needle_bytes {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[derive(Debug)]
pub(crate) struct SourceLine<'a> {
    text: &'a str,
    indent: usize,
    start: usize,
    end: usize,
}

fn source_lines(source: &str) -> Vec<SourceLine<'_>> {
    let mut offset = 0;
    let mut lines = Vec::new();

    for text in source.lines() {
        let start = offset;
        let end = start + text.len();
        lines.push(SourceLine {
            text,
            indent: text.bytes().take_while(|byte| *byte == b' ').count(),
            start,
            end,
        });
        offset = end + 1;
    }

    lines
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

fn is_trivia(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Strip a `# ...` inline comment from the tail of a source line,
/// honoring `"..."` and `'...'` quoted strings (a `#` inside a quoted
/// literal stays put). Returns the input unchanged when the line carries
/// no comment.
fn strip_inline_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match in_quote {
            Some(q) if b == q => in_quote = None,
            Some(_) if b == b'\\' && i + 1 < bytes.len() => i += 1,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => in_quote = Some(b),
            None if b == b'#' => return line[..i].trim_end(),
            None => {}
        }
        i += 1;
    }
    line
}

fn split_lzx_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn unquote_lzx_value(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
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

/// IR Error-Vocab (Cell PARSE-1) — extract the bare key from a literal
/// `@translation.<key>` token. Trailing whitespace and trailing tokens
/// past the first whitespace are rejected; the surface intentionally
/// keeps the form single-token to stay inside the existing
/// `Rule.message @translation.<key>` precedent.
fn parse_translation_key_token(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<TranslationKeyRefAst, ParseError> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return Err(line_error(
            line,
            "`@translation.<key>` reference requires a key",
        ));
    }
    let mut parts = trimmed.split_whitespace();
    let first = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(line_error(
            line,
            "`@translation.<key>` reference must be a single token",
        ));
    }
    let key = first.strip_prefix("@translation.").ok_or_else(|| {
        line_error(
            line,
            "expected `@translation.<key>` reference (must start with `@translation.`)",
        )
    })?;
    if key.is_empty() {
        return Err(line_error(
            line,
            "`@translation.` reference requires a non-empty key after the dot",
        ));
    }
    Ok(TranslationKeyRefAst {
        key: key.to_owned(),
        span: Span::new(line.start, line.end),
    })
}

fn parse_lzx_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn line_error(line: &SourceLine<'_>, message: &'static str) -> ParseError {
    ParseError::Pest {
        message: message.to_owned(),
        span: Span::new(line.start, line.end.max(line.start + 1)),
    }
}

/// Owned-message variant of `line_error`. Used by parsers that need to
/// interpolate user-supplied tokens into the diagnostic.
fn line_error_owned(line: &SourceLine<'_>, message: String) -> ParseError {
    ParseError::Pest {
        message,
        span: Span::new(line.start, line.end.max(line.start + 1)),
    }
}

// =============================================================================
// PG.A — top-level plan-catalog parser + side-channel gate scanner.
// -----------------------------------------------------------------------------
// These two passes are deliberately additive: they walk the raw source
// independently and never touch the existing per-callable struct
// literals (CommandDecl, ApiDecl, Job, Webhook, ListQueryDecl, ...).
// The analyzer (PG.B) consumes both outputs and threads them through
// IR + doctor as a parallel side-table.
// =============================================================================

/// Parse top-level `plan <name>` blocks. Returns one entry per authored
/// block in source order. Plan blocks are siblings of `feature`/`app`
/// at indent 0; their children sit at indent 2.
///
/// Validation of cross-plan references and feature catalog union is
/// the analyzer's job; this parser only enforces the closed grammar
/// (identifier shape, value types, single trial block per plan).
pub fn parse_plan_blocks(source: &str) -> Result<Vec<PlanBlockAst>, ParseError> {
    let lines = source_lines(source);
    let mut plans = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent == 0 {
            if let Some(rest) = trimmed.strip_prefix("plan ") {
                let name = rest.trim().to_owned();
                if name.is_empty() {
                    return Err(line_error(
                        line,
                        "plan header requires a name: `plan <name>`",
                    ));
                }
                if !is_plan_ident(&name) {
                    return Err(line_error(line, "plan name must match `[a-z][a-z0-9_]*`"));
                }
                let (block, next) = parse_plan_block(&lines, i, name)?;
                plans.push(block);
                i = next;
                continue;
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    Ok(plans)
}

fn parse_plan_block(
    lines: &[SourceLine<'_>],
    start: usize,
    name: String,
) -> Result<(PlanBlockAst, usize), ParseError> {
    let header = &lines[start];
    let mut features: Vec<PlanFeatureRefAst> = Vec::new();
    let mut limits: Vec<PlanLimitRefAst> = Vec::new();
    let mut trial: Option<PlanTrialAst> = None;
    let mut last_end = header.end;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent == 0 {
            break;
        }
        if line.indent != 2 {
            return Err(line_error(
                line,
                "`plan` children use two-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("features ") {
            features.extend(parse_plan_feature_refs(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("limits ") {
            limits.extend(parse_plan_limit_refs(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("trial ") {
            if trial.is_some() {
                return Err(line_error(
                    line,
                    "`plan` may declare at most one `trial` block",
                ));
            }
            trial = Some(parse_plan_trial(line, rest)?);
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`plan` children are `features`, `limits`, or `trial duration ..., then ...`",
            ));
        }
    }
    if features.is_empty() && limits.is_empty() && trial.is_none() {
        return Err(line_error(
            header,
            "`plan` requires at least one of `features`, `limits`, or `trial`",
        ));
    }
    Ok((
        PlanBlockAst {
            name,
            features,
            limits,
            trial,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_plan_feature_refs(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<Vec<PlanFeatureRefAst>, ParseError> {
    let mut out = Vec::new();
    for piece in rest.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if let Some(target) = piece.strip_suffix(".features") {
            if !is_plan_ident(target) {
                return Err(line_error(
                    line,
                    "cross-plan `<other>.features` reference requires a lowercase plan id",
                ));
            }
            out.push(PlanFeatureRefAst::CrossPlan(target.to_owned()));
        } else if is_plan_ident(piece) {
            out.push(PlanFeatureRefAst::Ident(piece.to_owned()));
        } else {
            return Err(line_error(
                line,
                "`features` entries must be `[a-z][a-z0-9_]*` or `<other>.features`",
            ));
        }
    }
    Ok(out)
}

fn parse_plan_limit_refs(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<Vec<PlanLimitRefAst>, ParseError> {
    let mut out = Vec::new();
    for piece in rest.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if let Some(target) = piece.strip_suffix(".limits") {
            if !is_plan_ident(target) {
                return Err(line_error(
                    line,
                    "cross-plan `<other>.limits` reference requires a lowercase plan id",
                ));
            }
            out.push(PlanLimitRefAst::CrossPlan(target.to_owned()));
            continue;
        }
        let mut tokens = piece.split_whitespace();
        let name = tokens
            .next()
            .ok_or_else(|| line_error(line, "`limits` entry requires `<name> <value>`"))?;
        let value = tokens
            .next()
            .ok_or_else(|| line_error(line, "`limits` entry requires a value after the name"))?;
        if tokens.next().is_some() {
            return Err(line_error(
                line,
                "`limits` entry must be `<name> <integer>` or `<name> unlimited`",
            ));
        }
        if !is_plan_ident(name) {
            return Err(line_error(
                line,
                "`limits` entry name must match `[a-z][a-z0-9_]*`",
            ));
        }
        if value == "unlimited" {
            out.push(PlanLimitRefAst::Unlimited {
                name: name.to_owned(),
            });
        } else if let Ok(v) = value.parse::<u64>() {
            out.push(PlanLimitRefAst::Integer {
                name: name.to_owned(),
                value: v,
            });
        } else {
            return Err(line_error(
                line,
                "`limits` value must be a positive integer or `unlimited`",
            ));
        }
    }
    Ok(out)
}

fn parse_plan_trial(line: &SourceLine<'_>, rest: &str) -> Result<PlanTrialAst, ParseError> {
    let body = rest.trim();
    let (left, right) = body.split_once(',').ok_or_else(|| {
        line_error(
            line,
            "`trial` requires `duration <d>, then <plan>` (comma separator)",
        )
    })?;
    let duration = left
        .trim()
        .strip_prefix("duration ")
        .ok_or_else(|| {
            line_error(
                line,
                "`trial` left side must be `duration <integer><s|m|h|d>`",
            )
        })?
        .trim()
        .to_owned();
    if !is_valid_trial_duration(&duration) {
        return Err(line_error(
            line,
            "`trial duration` must be `<integer><s|m|h|d>` (e.g. `14d`, `48h`)",
        ));
    }
    let then_plan = right
        .trim()
        .strip_prefix("then ")
        .ok_or_else(|| line_error(line, "`trial` right side must be `then <plan>`"))?
        .trim()
        .to_owned();
    if !is_plan_ident(&then_plan) {
        return Err(line_error(
            line,
            "`trial then <plan>` requires a lowercase plan id",
        ));
    }
    Ok(PlanTrialAst {
        duration,
        then_plan,
        span: Span::new(line.start, line.end),
    })
}

fn is_valid_trial_duration(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    let last = s.chars().last().unwrap();
    if !matches!(last, 's' | 'm' | 'h' | 'd') {
        return false;
    }
    let head = &s[..s.len() - 1];
    !head.is_empty() && head.chars().all(|c| c.is_ascii_digit())
}

/// PG.A — plan/feature/limit ident regex: `[a-z][a-z0-9_]*`.
fn is_plan_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// PG.A — scan a feature `.lzi` source for `gate behind plan.feature: ...`
/// and `gate quota plan.limit: ...` directives inside callable bodies.
/// Returns a map keyed by qualified callable id
/// (`command:<name>` / `job:<name>` / `webhook:<name>` /
/// `api:<name>` / `query.list:<name>` / `query.lookup:<name>` /
/// `query.sql:<name>`) into the gate directives declared in source
/// order for that callable.
///
/// The scanner is a single-pass text walker. It tracks the current
/// callable header so a `gate ...` line indented under that header is
/// attributed correctly. Gates outside any callable body are reported
/// as a parse error.
pub fn parse_feature_gates(source: &str) -> Result<FeatureGatesAst, ParseError> {
    let lines = source_lines(source);
    let mut callables: std::collections::BTreeMap<String, Vec<GateDirectiveAst>> =
        std::collections::BTreeMap::new();
    let mut current: Option<(String, usize)> = None; // (qualified-key, header-indent)

    for line in &lines {
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            continue;
        }

        // Has the current callable scope ended? (Sibling header at the
        // same indent or shallower.)
        if let Some((_, header_indent)) = &current {
            if line.indent <= *header_indent {
                current = None;
            }
        }

        // Detect a new callable header.
        if let Some(key) = callable_header_key(trimmed) {
            current = Some((key, line.indent));
            continue;
        }

        // Inside a callable scope, recognise gate lines.
        if let Some(rest) = trimmed.strip_prefix("gate ") {
            let directive = parse_gate_directive_line(line, rest)?;
            match &current {
                Some((key, _)) => {
                    callables
                        .entry(key.clone())
                        .or_insert_with(Vec::new)
                        .push(directive);
                }
                None => {
                    return Err(line_error(
                        line,
                        "`gate` directive must appear inside a callable body",
                    ));
                }
            }
        }
    }

    Ok(FeatureGatesAst { callables })
}

/// Recognise `command <name>` / `job <name>` / `webhook <name>` /
/// `api <name>` / `query.list <name>` / `query.lookup <name>` /
/// `query.sql <name>` headers and produce the qualified key the
/// downstream side-table uses.
fn callable_header_key(trimmed: &str) -> Option<String> {
    let prefixes: &[(&str, &str)] = &[
        ("command ", "command"),
        ("job ", "job"),
        ("webhook ", "webhook"),
        ("api ", "api"),
        ("query.list ", "query.list"),
        ("query.lookup ", "query.lookup"),
        ("query.sql ", "query.sql"),
    ];
    for (prefix, kind) in prefixes {
        if let Some(rest) = trimmed.strip_prefix(*prefix) {
            // Take only the first whitespace-separated token (header may
            // carry trailing `by <field>: <Type>` etc.).
            let name = rest.split_whitespace().next().unwrap_or_default();
            if !name.is_empty() {
                return Some(format!("{}:{}", kind, name));
            }
        }
    }
    None
}

fn parse_gate_directive_line(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<GateDirectiveAst, ParseError> {
    let body = rest.trim();
    if let Some(after) = body.strip_prefix("behind plan.feature:") {
        let feature = after.trim();
        if feature.is_empty() {
            return Err(line_error(
                line,
                "`gate behind plan.feature:` requires a feature identifier",
            ));
        }
        if !is_plan_ident(feature) {
            return Err(line_error(
                line,
                "`gate behind plan.feature:` requires a lowercase identifier ([a-z][a-z0-9_]*)",
            ));
        }
        Ok(GateDirectiveAst::Behind {
            feature: feature.to_owned(),
            span: Span::new(line.start, line.end),
        })
    } else if let Some(after) = body.strip_prefix("quota plan.limit:") {
        let limit = after.trim();
        if limit.is_empty() {
            return Err(line_error(
                line,
                "`gate quota plan.limit:` requires a limit identifier",
            ));
        }
        if !is_plan_ident(limit) {
            return Err(line_error(
                line,
                "`gate quota plan.limit:` requires a lowercase identifier ([a-z][a-z0-9_]*)",
            ));
        }
        Ok(GateDirectiveAst::Quota {
            limit: limit.to_owned(),
            span: Span::new(line.start, line.end),
        })
    } else {
        Err(line_error(
            line,
            "`gate` directives are `gate behind plan.feature: <name>` or `gate quota plan.limit: <name>`",
        ))
    }
}

// =============================================================================
// RB.A — top-level RBAC catalog parser (`permission` / `role`).
// -----------------------------------------------------------------------------
// Package-scoped vocab declared at indent 0, sibling to `feature` /
// `app` / `workspace`. The parse is additive: existing top-level
// constructs are untouched. See `docs/proposals/rbac-catalog-vocab.md`.
// =============================================================================

/// Parse a `.lzi` source for the full top-level package skeleton —
/// features (delegating to `parse_feature_skeletons`) plus RBAC
/// catalog declarations (`permission <ident>`, `role <name>`).
///
/// This is the new canonical entry point for package-level parsing.
/// Existing callers using `parse_feature_skeletons` directly continue
/// to work (the function is unchanged); new callers wanting the
/// catalog go through `parse_package_skeleton`.
pub fn parse_package_skeleton(source: &str) -> Result<PackageSkeleton, ParseError> {
    let features = parse_feature_skeletons(source)?;
    let (permissions, roles) = parse_rbac_catalog_decls(source)?;
    Ok(PackageSkeleton {
        features,
        permissions,
        roles,
    })
}

/// Walk `source` and harvest top-level `permission <ident>` and
/// `role <name>` declarations. Skips lines inside any indented block
/// (features, agents, etc.); recognises only indent-0 directives.
fn parse_rbac_catalog_decls(
    source: &str,
) -> Result<(Vec<PermissionDeclAst>, Vec<RoleDeclAst>), ParseError> {
    let lines = source_lines(source);
    let mut permissions = Vec::new();
    let mut roles = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent == 0 {
            if let Some(rest) = trimmed.strip_prefix("permission ") {
                let name = rest.trim().to_owned();
                let decl = parse_permission_decl(line, name)?;
                permissions.push(decl);
                i += 1;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("role ") {
                let header_name = rest.trim().to_owned();
                if header_name.is_empty() {
                    return Err(line_error(line, "`role` requires a name: `role <name>`"));
                }
                if !is_rbac_role_ident(&header_name) {
                    return Err(line_error(line, "role name must match `[a-z][a-z0-9_]*`"));
                }
                let (decl, next) = parse_role_decl(&lines, i, header_name)?;
                roles.push(decl);
                i = next;
                continue;
            }
        }

        i += 1;
    }

    Ok((permissions, roles))
}

fn parse_permission_decl(
    line: &SourceLine<'_>,
    raw_name: String,
) -> Result<PermissionDeclAst, ParseError> {
    let name = raw_name.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(
            line,
            "`permission` requires an identifier: `permission <resource>:<action>`",
        ));
    }
    if name.contains(char::is_whitespace) {
        return Err(line_error(
            line,
            "`permission` accepts a single colon-separated identifier per line",
        ));
    }
    let segments: Vec<String> = name.split(':').map(str::to_owned).collect();
    if segments.len() < 2 || segments.len() > 4 {
        return Err(line_error(
            line,
            "permission identifier requires 2 to 4 colon-separated segments \
             (`<resource>:<action>` ... `<resource>:<action>:<scope>:<qualifier>`)",
        ));
    }
    for seg in &segments {
        if seg.is_empty() {
            return Err(line_error(
                line,
                "permission identifier cannot have empty segments (no leading, trailing, or doubled colons)",
            ));
        }
        if !is_rbac_segment_ident(seg) {
            return Err(line_error(
                line,
                "permission segments must match `[a-z][a-z0-9_]*`",
            ));
        }
    }
    Ok(PermissionDeclAst {
        name,
        segments,
        span: Span::new(line.start, line.end),
    })
}

fn parse_role_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    name: String,
) -> Result<(RoleDeclAst, usize), ParseError> {
    let header = &lines[start];
    let mut inherits: Option<String> = None;
    let mut grants: Option<Vec<String>> = None;
    let mut grants_all = false;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent == 0 {
            break;
        }
        if line.indent != 2 {
            return Err(line_error(
                line,
                "`role` children use two-space indentation (`inherits`, `grants`, `grants_all`)",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("inherits ") {
            if inherits.is_some() {
                return Err(line_error(
                    line,
                    "`role` may declare at most one `inherits` clause",
                ));
            }
            let parent = rest.trim();
            if parent.contains(',') {
                return Err(line_error(
                    line,
                    "multi-parent inheritance (`inherits A, B`) is not supported in v0.1; \
                     declare a single parent role per `role`",
                ));
            }
            if !is_rbac_role_ident(parent) {
                return Err(line_error(
                    line,
                    "`inherits` parent must be a single `[a-z][a-z0-9_]*` role name",
                ));
            }
            inherits = Some(parent.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "grants_all" {
            if grants_all {
                return Err(line_error(
                    line,
                    "`role` may declare `grants_all` at most once",
                ));
            }
            if grants.is_some() {
                return Err(line_error(
                    line,
                    "`role` may declare either `grants` or `grants_all`, not both",
                ));
            }
            grants_all = true;
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "grants" {
            if grants.is_some() {
                return Err(line_error(
                    line,
                    "`role` may declare at most one `grants` block",
                ));
            }
            if grants_all {
                return Err(line_error(
                    line,
                    "`role` may declare either `grants` or `grants_all`, not both",
                ));
            }
            let (entries, next) = parse_role_grants_block(lines, i)?;
            grants = Some(entries);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        return Err(line_error(
            line,
            "`role` children are `inherits <role>`, `grants` block, or `grants_all`",
        ));
    }

    let grants_kind = if grants_all {
        RoleGrantsAst::All
    } else if let Some(list) = grants {
        RoleGrantsAst::Explicit(list)
    } else {
        RoleGrantsAst::InheritedOnly
    };

    Ok((
        RoleDeclAst {
            name,
            inherits,
            grants: grants_kind,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_role_grants_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<String>, usize), ParseError> {
    let header = &lines[start];
    let mut entries = Vec::new();
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
        if line.indent != header.indent + 2 {
            return Err(line_error(
                line,
                "`grants` entries use four-space indentation (one permission ref per line)",
            ));
        }
        let tok = trimmed;
        if tok.contains(char::is_whitespace) {
            return Err(line_error(
                line,
                "`grants` entries are bare permission identifiers (one per line)",
            ));
        }
        let segments: Vec<&str> = tok.split(':').collect();
        if segments.len() < 2 || segments.len() > 4 || segments.iter().any(|s| s.is_empty()) {
            return Err(line_error(
                line,
                "permission ref must be 2-4 non-empty colon-separated segments",
            ));
        }
        for seg in &segments {
            if !is_rbac_segment_ident(seg) {
                return Err(line_error(
                    line,
                    "permission ref segments must match `[a-z][a-z0-9_]*`",
                ));
            }
        }
        entries.push(tok.to_owned());
        i += 1;
    }
    if entries.is_empty() {
        return Err(line_error(
            header,
            "`grants` block requires at least one permission ref \
             (use `grants_all` or omit the block to inherit only)",
        ));
    }
    Ok((entries, i))
}

fn is_rbac_role_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_rbac_segment_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::{InvariantForm, SourceLine, parse_invariant_form, parse_lzx_document};
    use crate::LzxPlatform;

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

    #[test]
    fn parses_lzx_experience_and_platform_surface() {
        let experience =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.lzx")).unwrap();
        assert_eq!(experience.experiences.len(), 1);
        assert_eq!(experience.experiences[0].name, "customer");
        assert_eq!(experience.experiences[0].imports, vec!["customer"]);
        assert_eq!(experience.experiences[0].views[0].name, "list");
        assert_eq!(
            experience.experiences[0].views[0].source.as_deref(),
            Some("customer.query.list")
        );
        assert_eq!(experience.experiences[0].views[1].anchor.as_deref(), None);

        let surface =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.web.lzx")).unwrap();
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
        assert_eq!(document.routes[0].routes, vec!["id: Customer.ID"]);
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
        let source = include_str!("../../../examples/full-capsule/full-capsule.lzi");
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
                assert_eq!(s.name, "lifetime_value");
                assert_eq!(s.returns, "CustomerLtv[]");
                assert_eq!(s.sql_path, "./queries/customer_lifetime_value.sql");
                assert_eq!(s.scope_lines.len(), 1);
            }
            other => panic!("expected query.sql, got {other:?}"),
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
        let source = include_str!("../../../examples/full-capsule/full-capsule.lzi");
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
// RB.S6 — policy-expression parser tests (`has_role`, `has_permission`,
// `authenticated`, with boolean combinators).
// =============================================================================
#[cfg(test)]
mod policy_expr_parser_tests {
    use super::{SourceLine, looks_like_policy_expr, try_parse_policy_expr};
    use crate::ast::PolicyExprAst;

    fn line(text: &'static str) -> SourceLine<'static> {
        SourceLine {
            text,
            indent: 0,
            start: 0,
            end: text.len(),
        }
    }

    #[test]
    fn legacy_atom_falls_back_to_none() {
        let l = line("policy @policy.create");
        assert_eq!(
            try_parse_policy_expr(&l, "@policy.create").unwrap(),
            None,
            "bare @policy.* atom must remain raw-string back-compat"
        );
        assert!(!looks_like_policy_expr("@policy.create"));
        assert!(!looks_like_policy_expr("@role.admin"));
    }

    #[test]
    fn authenticated_alone_parses() {
        let l = line("policy authenticated");
        let expr = try_parse_policy_expr(&l, "authenticated").unwrap().unwrap();
        assert_eq!(expr, PolicyExprAst::Authenticated);
    }

    #[test]
    fn has_role_parses() {
        let l = line("policy has_role manager");
        let expr = try_parse_policy_expr(&l, "has_role manager")
            .unwrap()
            .unwrap();
        assert_eq!(expr, PolicyExprAst::HasRole("manager".into()));
    }

    #[test]
    fn has_permission_parses() {
        let l = line("policy has_permission queries:start");
        let expr = try_parse_policy_expr(&l, "has_permission queries:start")
            .unwrap()
            .unwrap();
        assert_eq!(expr, PolicyExprAst::HasPermission("queries:start".into()));
    }

    #[test]
    fn has_permission_three_segments_parses() {
        let l = line("policy has_permission report:repasse:mark");
        let expr = try_parse_policy_expr(&l, "has_permission report:repasse:mark")
            .unwrap()
            .unwrap();
        assert_eq!(
            expr,
            PolicyExprAst::HasPermission("report:repasse:mark".into())
        );
    }

    #[test]
    fn malformed_permission_ref_errors() {
        let l = line("policy has_permission users");
        let err = try_parse_policy_expr(&l, "has_permission users").unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("must be 2-4 colon-separated"),
            "expected segment-count error, got: {msg}"
        );
    }

    #[test]
    fn missing_has_role_arg_errors() {
        let l = line("policy has_role");
        let err = try_parse_policy_expr(&l, "has_role").unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("`has_role` requires an identifier"),
            "expected missing-ident error, got: {msg}"
        );
    }

    #[test]
    fn and_combinator_parses() {
        let l = line("policy authenticated and has_role manager");
        let expr = try_parse_policy_expr(&l, "authenticated and has_role manager")
            .unwrap()
            .unwrap();
        match expr {
            PolicyExprAst::And(terms) => {
                assert_eq!(terms.len(), 2);
                assert_eq!(terms[0], PolicyExprAst::Authenticated);
                assert_eq!(terms[1], PolicyExprAst::HasRole("manager".into()));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn or_combinator_parses() {
        let l = line("policy has_role manager or has_role admin");
        let expr = try_parse_policy_expr(&l, "has_role manager or has_role admin")
            .unwrap()
            .unwrap();
        match expr {
            PolicyExprAst::Or(terms) => {
                assert_eq!(terms.len(), 2);
                assert_eq!(terms[0], PolicyExprAst::HasRole("manager".into()));
                assert_eq!(terms[1], PolicyExprAst::HasRole("admin".into()));
            }
            other => panic!("expected Or, got {other:?}"),
        }
    }

    #[test]
    fn not_combinator_parses() {
        let l = line("policy not has_role viewer");
        let expr = try_parse_policy_expr(&l, "not has_role viewer")
            .unwrap()
            .unwrap();
        match expr {
            PolicyExprAst::Not(inner) => {
                assert_eq!(*inner, PolicyExprAst::HasRole("viewer".into()));
            }
            other => panic!("expected Not, got {other:?}"),
        }
    }

    #[test]
    fn parens_override_precedence() {
        // `and` binds tighter than `or`; parens force the alternative
        // grouping. We expect `Or([authenticated, And([X,Y])])` without
        // parens vs `And([Or([authenticated, X]), Y])` with parens.
        let l = line("policy");
        let raw = "(authenticated or has_role manager) and has_permission queries:start";
        let expr = try_parse_policy_expr(&l, raw).unwrap().unwrap();
        match expr {
            PolicyExprAst::And(terms) => {
                assert_eq!(terms.len(), 2);
                match &terms[0] {
                    PolicyExprAst::Or(_) => {}
                    other => panic!("expected Or under And, got {other:?}"),
                }
                assert_eq!(
                    terms[1],
                    PolicyExprAst::HasPermission("queries:start".into())
                );
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn embedded_atom_parses() {
        // `has_role X or @actor.system` mixes a predicate with an atom.
        let l = line("policy");
        let expr = try_parse_policy_expr(&l, "has_role admin or @actor.system")
            .unwrap()
            .unwrap();
        match expr {
            PolicyExprAst::Or(terms) => {
                assert_eq!(terms.len(), 2);
                assert_eq!(terms[0], PolicyExprAst::HasRole("admin".into()));
                match &terms[1] {
                    PolicyExprAst::Atom(atom) => {
                        assert_eq!(atom.namespace, "actor");
                        assert_eq!(atom.name, "system");
                    }
                    other => panic!("expected Atom, got {other:?}"),
                }
            }
            other => panic!("expected Or, got {other:?}"),
        }
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
    fn command_triggers_transition_parses_single_and_list_rejects_trailing_comma() {
        let source = r#"
feature order
  command submit
    triggers transition approve
  command fulfill
    triggers transition approve, capture_payment, ship
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
mod surface_parser_tests {
    use super::parse_surface_document;
    use crate::{
        BindingRefAst, DrawerBindingSourceAst, DrawerTriggerAst, FilterCardinalityAst,
        SearchModeAst, SelectionModeAst, SettingPersistenceAst, SettingValueSpaceAst, SortDirAst,
        SurfaceTargetAst, ViewAst,
    };

    #[test]
    fn minimal_surface_one_audience_one_view_list() {
        let source = r#"
surface slug web
  audience admin
    view list slug_list
      source slug.query.mine
      columns key, title
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, SurfaceTargetAst::Web);
        assert_eq!(surface.uses_feature, None);
        assert_eq!(surface.audiences.len(), 1);
        let audience = &surface.audiences[0];
        assert_eq!(audience.name, "admin");
        assert_eq!(audience.requires.len(), 0);
        assert_eq!(audience.views.len(), 1);
        let view = match &audience.views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected ViewAst::List, got {:?}", other),
        };
        assert_eq!(view.name, "slug_list");
        assert_eq!(view.route, None);
        assert_eq!(view.source, "slug.query.mine");
        assert_eq!(view.columns, vec!["key", "title"]);
    }

    #[test]
    fn parses_full_section_13_1_demo_fixture() {
        // Section 13.1 verbatim from
        // `docs/proposals/lzx-integration-codegen.md`.
        let source = r#"surface slug web
  uses feature slug

  audience admin
    requires @scope.workspace_admin

    view list slug_list at "/slugs"
      source slug.query.mine
      columns key, title, tags, created_at
      search key, title
      filter tags
      cells tags @client.type_badge
      actions create, update, delete

    view detail slug_detail at "/slugs/:key"
      source slug.query.by_key
      route key: Text from path
      sections header, metadata, related_items
      cells tags @client.type_badge
      actions update, delete

    view create slug_create at "/slugs/new"
      submit slug.command.create
      fields key, title, description, tags
      cells tags @client.type_badge

  audience public
    requires @scope.workspace_member

    view list public_slug_list at "/browse"
      source slug.query.mine
      columns key, title
      search key, title
"#;
        let surface = parse_surface_document(source).expect("parses §13.1 fixture");
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, SurfaceTargetAst::Web);
        assert_eq!(surface.uses_feature.as_deref(), Some("slug"));
        assert_eq!(surface.audiences.len(), 2);

        // admin audience.
        let admin = &surface.audiences[0];
        assert_eq!(admin.name, "admin");
        assert_eq!(admin.requires.len(), 1);
        assert_eq!(admin.requires[0].namespace, "scope");
        assert_eq!(admin.requires[0].name, "workspace_admin");
        assert_eq!(admin.views.len(), 3);

        let list = match &admin.views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(list.name, "slug_list");
        assert_eq!(list.route.as_deref(), Some("/slugs"));
        assert_eq!(list.columns, vec!["key", "title", "tags", "created_at"]);
        match &list.search.as_ref().expect("search").mode {
            SearchModeAst::Columns(columns) => assert_eq!(columns, &vec!["key", "title"]),
            other => panic!("expected columns search, got {other:?}"),
        }
        assert_eq!(list.filter, vec!["tags"]);
        assert_eq!(list.cells.len(), 1);
        assert_eq!(list.cells[0].field, "tags");
        assert_eq!(list.cells[0].slot, "type_badge");
        assert_eq!(list.actions, vec!["create", "update", "delete"]);

        let detail = match &admin.views[1] {
            ViewAst::Detail(v) => v,
            other => panic!("expected detail, got {:?}", other),
        };
        assert_eq!(detail.name, "slug_detail");
        assert_eq!(detail.route.as_deref(), Some("/slugs/:key"));
        assert_eq!(detail.source, "slug.query.by_key");
        assert_eq!(detail.route_params.len(), 1);
        assert_eq!(detail.route_params[0].name, "key");
        assert_eq!(detail.route_params[0].type_ref, "Text");
        assert_eq!(detail.sections, vec!["header", "metadata", "related_items"]);
        assert_eq!(detail.actions, vec!["update", "delete"]);

        let create = match &admin.views[2] {
            ViewAst::Create(v) => v,
            other => panic!("expected create, got {:?}", other),
        };
        assert_eq!(create.name, "slug_create");
        assert_eq!(create.route.as_deref(), Some("/slugs/new"));
        assert_eq!(create.submit, "slug.command.create");
        assert_eq!(create.fields, vec!["key", "title", "description", "tags"]);

        // public audience.
        let public = &surface.audiences[1];
        assert_eq!(public.name, "public");
        assert_eq!(public.requires.len(), 1);
        assert_eq!(public.requires[0].name, "workspace_member");
        assert_eq!(public.views.len(), 1);
    }

    #[test]
    fn search_segmented_block_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key
      search segmented
        field slug binds filters.slug
        field type binds filters.type
        field tag binds filters.tags
        free text into source.q
"#;
        let surface = parse_surface_document(source).expect("parses segmented search");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {other:?}"),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, SearchModeAst::Segmented);
        assert_eq!(search.fields.len(), 3);
        assert_eq!(search.fields[0].key, "slug");
        assert_eq!(
            search.fields[0].binds_to,
            BindingRefAst::Filter {
                name: "slug".to_owned()
            }
        );
        assert_eq!(
            search.free_text_target,
            Some(BindingRefAst::SourceInput {
                name: "q".to_owned()
            })
        );
    }

    #[test]
    fn search_columns_v1_form_still_parses() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search key, title\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        match &search.mode {
            SearchModeAst::Columns(columns) => assert_eq!(columns, &vec!["key", "title"]),
            other => panic!("expected columns search, got {other:?}"),
        }
        assert!(search.fields.is_empty());
        assert!(search.free_text_target.is_none());
    }

    #[test]
    fn search_segmented_rejects_inline_content() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented foo\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("takes no inline list"));
    }

    #[test]
    fn search_at_most_once() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search key
      search segmented
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn search_field_rejects_duplicate_key() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search segmented
        field slug binds filters.slug
        field slug binds source.slug
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("more than once"));
    }

    #[test]
    fn search_free_text_at_most_once() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
      search segmented
        free text into source.q
        free text into source.query
"#;
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("free text into"));
    }

    #[test]
    fn search_binding_ref_filter_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field slug binds filters.slug\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::Filter {
                name: "slug".to_owned()
            }
        );
    }

    #[test]
    fn search_binding_ref_source_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field q binds source.q\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::SourceInput {
                name: "q".to_owned()
            }
        );
    }

    #[test]
    fn search_binding_ref_selection_form() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field selected binds selection\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.search.as_ref().unwrap().fields[0].binds_to,
            BindingRefAst::SelectionScalar
        );
    }

    #[test]
    fn search_binding_ref_invalid() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n        field slug binds foo.bar\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("binding references"));
    }

    #[test]
    fn search_segmented_empty_block() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      search segmented\n";
        let surface = parse_surface_document(source).expect("parses empty segmented search");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, SearchModeAst::Segmented);
        assert!(search.fields.is_empty());
        assert!(search.free_text_target.is_none());
    }

    #[test]
    fn view_list_requires_source() {
        let source = "surface slug web\n  audience admin\n    view list bad\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("view list requires"));
    }

    #[test]
    fn view_list_no_columns_is_not_parse_time_error() {
        let source =
            "surface slug web\n  audience admin\n    view list bad\n      source slug.query.mine\n";
        let surface = parse_surface_document(source).expect("parses without columns");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert!(view.columns.is_empty());
        assert!(view.cells_slot.is_none());
    }

    #[test]
    fn view_create_requires_submit() {
        let source = "surface slug web\n  audience admin\n    view create bad\n      fields key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("view create requires"));
    }

    #[test]
    fn mobile_target_recognised() {
        let source = "surface item mobile\n  audience kiosk\n    view list item_list\n      source item.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses mobile");
        assert_eq!(surface.target, SurfaceTargetAst::Mobile);
    }

    #[test]
    fn rejects_unknown_target() {
        let source = "surface slug desktop\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("surface target must be"));
    }

    #[test]
    fn rejects_top_level_indentation() {
        let source = "  surface slug web\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("top-level"));
    }

    #[test]
    fn cells_binding_parses() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      columns tags\n      cells tags @client.type_badge\n";
        let surface = parse_surface_document(source).expect("parses cells");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.cells.len(), 1);
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn view_list_accepts_cells_at_client_slot_grid_form() {
        let source = "surface item web\n  audience admin\n    view list foo at \"/\"\n      source f.query.q\n      cells @client.item_card\n";
        let surface = parse_surface_document(source).expect("parses cells grid form");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot.as_deref(), Some("item_card"));
        assert!(view.columns.is_empty());
        assert!(view.cells.is_empty());
    }

    #[test]
    fn view_list_rejects_cells_at_client_slot_with_trailing_tokens() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.foo extra\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("accepts only one slot identifier"));
    }

    #[test]
    fn view_list_rejects_double_cells_grid_form() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.item_card\n      cells @client.other_card\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn view_list_v1_per_column_cells_still_works() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      cells tags @client.type_badge\n      columns key, title\n";
        let surface = parse_surface_document(source).expect("parses per-column cells");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot, None);
        assert_eq!(view.columns, vec!["key", "title"]);
        assert_eq!(view.cells.len(), 1);
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn view_list_no_longer_requires_columns_if_cells_slot_present() {
        let source = "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n      cells @client.item_card\n";
        let surface = parse_surface_document(source).expect("parses without columns");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert_eq!(view.cells_slot.as_deref(), Some("item_card"));
        assert!(view.columns.is_empty());
    }

    #[test]
    fn view_list_empty_grid_and_no_columns_does_not_error_at_parse_time() {
        let source =
            "surface item web\n  audience admin\n    view list foo\n      source f.query.q\n";
        let surface = parse_surface_document(source).expect("parses without render declaration");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        assert!(view.cells_slot.is_none());
        assert!(view.columns.is_empty());
    }

    #[test]
    fn cells_binding_requires_at_client_prefix() {
        let source = "surface slug web\n  audience admin\n    view list slug_list\n      source slug.query.mine\n      columns tags\n      cells tags @server.type_badge\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("cell slot must be `@client."));
    }

    #[test]
    fn view_list_with_drawer_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key, title
      drawer item_detail on select
        source item.query.by_id
        route key from selection
        sections header, content, metadata
        cells related @client.related_items
        actions update, delete
"#;
        let surface = parse_surface_document(source).expect("parses drawer");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            other => panic!("expected list, got {:?}", other),
        };
        let drawer = view.drawer.as_ref().expect("drawer populated");
        assert_eq!(drawer.name, "item_detail");
        assert_eq!(drawer.trigger, DrawerTriggerAst::Select);
        assert_eq!(drawer.source, "item.query.by_id");
        let route = drawer.route_binding.as_ref().expect("route binding");
        assert_eq!(route.target, "key");
        assert_eq!(route.source, DrawerBindingSourceAst::Selection);
        assert_eq!(drawer.sections, vec!["header", "content", "metadata"]);
        assert_eq!(drawer.cells.len(), 1);
        assert_eq!(drawer.cells[0].field, "related");
        assert_eq!(drawer.cells[0].slot, "related_items");
        assert_eq!(drawer.actions, vec!["update", "delete"]);
    }

    #[test]
    fn drawer_rejects_unknown_trigger() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on hover\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer trigger must be `select` or `open`")
        );
    }

    #[test]
    fn drawer_rejects_columns_inside() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        columns a, b\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer body lines are"));
    }

    #[test]
    fn drawer_rejects_filters_inside() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        filters status\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer body lines are"));
    }

    #[test]
    fn drawer_rejects_nested() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        drawer bar on select\n          source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("drawer cannot be nested"));
    }

    #[test]
    fn view_list_at_most_one_drawer() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n      drawer bar on open\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most one `drawer`"));
    }

    #[test]
    fn drawer_grid_form_cells_rejected() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        cells @client.item_card\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer cells use `cells <field> @client.<slot>`")
        );
    }

    #[test]
    fn view_detail_rejects_drawer() {
        let source = "surface item web\n  audience admin\n    view detail item_detail\n      source item.query.by_id\n      route key: Text from path\n      drawer foo on select\n        source item.query.by_id\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("`drawer` is only valid in `view list` bodies")
        );
    }

    #[test]
    fn route_key_from_selection_parses() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        route key from selection\n";
        let surface = parse_surface_document(source).expect("parses drawer route");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let route = view
            .drawer
            .as_ref()
            .and_then(|drawer| drawer.route_binding.as_ref())
            .expect("route binding");
        assert_eq!(route.target, "key");
        assert_eq!(route.source, DrawerBindingSourceAst::Selection);
    }

    #[test]
    fn route_key_from_path_inside_drawer_rejected() {
        let source = "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer foo on select\n        source item.query.by_id\n        route key from path\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(
            err.to_string()
                .contains("drawer route binding source must be `from selection`")
        );
    }

    #[test]
    fn view_list_filters_block_parses() {
        let source = r#"surface item web
  audience admin
    view list item_terminal at "/"
      source item.query.search
      columns key
      filters
        type: ItemType
        status: ItemStatus
        confidence: Confidence
        tags: list of Text
        slug: Text from query
"#;
        let surface = parse_surface_document(source).expect("parses filters");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters.len(), 5);
        assert_eq!(view.filters[0].name, "type");
        assert_eq!(view.filters[0].type_ref, "ItemType");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Single);
        assert!(!view.filters[0].url_sync);
        assert_eq!(view.filters[3].name, "tags");
        assert_eq!(view.filters[3].cardinality, FilterCardinalityAst::Multi);
        assert!(!view.filters[3].url_sync);
        assert_eq!(view.filters[4].name, "slug");
        assert_eq!(view.filters[4].cardinality, FilterCardinalityAst::Single);
        assert!(view.filters[4].url_sync);
    }

    #[test]
    fn filters_single_from_query() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from query\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters[0].name, "slug");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Single);
        assert!(view.filters[0].url_sync);
    }

    #[test]
    fn filters_multi_from_query() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        tags: list of Text from query\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filters[0].name, "tags");
        assert_eq!(view.filters[0].cardinality, FilterCardinalityAst::Multi);
        assert!(view.filters[0].url_sync);
    }

    #[test]
    fn filters_rejects_from_path() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from path\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("from query"));
    }

    #[test]
    fn filters_rejects_duplicate_name() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        tags: list of Text\n        tags: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("duplicate filter `tags`"));
    }

    #[test]
    fn filters_rejects_empty_block() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n      actions update\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires at least one"));
    }

    #[test]
    fn view_detail_rejects_filters() {
        let source = "surface item web\n  audience admin\n    view detail a\n      source item.query.by_id\n      filters\n        slug: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("only valid in `view list`"));
    }

    #[test]
    fn view_create_rejects_filters() {
        let source = "surface item web\n  audience admin\n    view create a\n      submit item.command.create\n      fields key\n      filters\n        slug: Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("only valid in `view list`"));
    }

    #[test]
    fn view_list_at_most_one_filters_block() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text\n      filters\n        tags: list of Text\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn filters_missing_type_ref() {
        let source = "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug:\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires a type"));
    }

    #[test]
    fn multiple_audiences_per_surface() {
        let source = r#"surface slug web
  audience admin
    requires @scope.workspace_admin
    view list a
      source slug.query.mine
      columns key

  audience public
    requires @scope.workspace_member
    view list b
      source slug.query.mine
      columns key
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.audiences.len(), 2);
        assert_eq!(surface.audiences[0].name, "admin");
        assert_eq!(surface.audiences[1].name, "public");
    }

    #[test]
    fn multiple_views_per_audience() {
        let source = r#"surface slug web
  audience admin
    view list a
      source slug.query.mine
      columns key
    view list b
      source slug.query.mine
      columns key
    view detail c at "/x/:id"
      source slug.query.by_key
      route id: Text from path
"#;
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.audiences[0].views.len(), 3);
    }

    #[test]
    fn empty_audience_parses_cleanly() {
        let source = "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n";
        let surface = parse_surface_document(source).expect("parses empty audience");
        assert_eq!(surface.audiences.len(), 1);
        assert_eq!(surface.audiences[0].views.len(), 0);
        assert_eq!(surface.audiences[0].requires.len(), 1);
    }

    #[test]
    fn actions_comma_separated_list() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions create, update, delete\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions, vec!["create", "update", "delete"]);
    }

    #[test]
    fn at_path_optional() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        let view = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.route, None);
    }

    #[test]
    fn rejects_partial_overrides() {
        let source = "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      columns += score\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("partial overrides"));
    }

    #[test]
    fn route_param_captures_type_text() {
        let source = "surface slug web\n  audience admin\n    view detail d at \"/s/:id\"\n      source slug.query.by_key\n      route id: Customer.ID from path\n";
        let surface = parse_surface_document(source).expect("parses");
        let detail = match &surface.audiences[0].views[0] {
            ViewAst::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.route_params[0].name, "id");
        assert_eq!(detail.route_params[0].type_ref, "Customer.ID");
    }

    #[test]
    fn uses_feature_override_captured() {
        let source = "surface slug web\n  uses feature slug\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        assert_eq!(surface.uses_feature.as_deref(), Some("slug"));
    }

    #[test]
    fn requires_scope_atom_captured() {
        let source = "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n    view list a\n      source slug.query.mine\n      columns key\n";
        let surface = parse_surface_document(source).expect("parses");
        let atom = &surface.audiences[0].requires[0];
        assert_eq!(atom.namespace, "scope");
        assert_eq!(atom.name, "workspace_admin");
    }

    #[test]
    fn requires_rejects_unknown_namespace() {
        let source = "surface slug web\n  audience admin\n    requires @group.workspace_admin\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("namespace"));
    }

    #[test]
    fn rejects_blank_document() {
        let source = "\n\n# comment only\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(matches!(err, super::ParseError::Expected { .. }));
    }

    #[test]
    fn comments_and_blank_lines_skipped() {
        let source = r#"# header comment

surface slug web
  # mid comment
  audience admin

    view list a
      # explanatory
      source slug.query.mine
      columns key
"#;
        let surface = parse_surface_document(source).expect("parses with comments");
        assert_eq!(surface.audiences[0].views.len(), 1);
    }

    #[test]
    fn at_path_requires_leading_slash() {
        let source = "surface slug web\n  audience admin\n    view list a at \"slugs\"\n      source slug.query.mine\n      columns key\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("must begin with `/`"));
    }

    #[test]
    fn view_create_with_route_at() {
        let source = "surface slug web\n  audience admin\n    view create new at \"/slugs/new\"\n      submit slug.command.create\n      fields key\n";
        let surface = parse_surface_document(source).expect("parses");
        let create = match &surface.audiences[0].views[0] {
            ViewAst::Create(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(create.route.as_deref(), Some("/slugs/new"));
        assert_eq!(create.submit, "slug.command.create");
    }

    #[test]
    fn sort_block_parses() {
        let source = r#"surface item web
  audience admin
    view list terminal
      source item.query.search
      columns title
      sort
        by title, type, priority, updated
        default updated desc
"#;
        let surface = parse_surface_document(source).expect("parses sort");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        let sort = list.sort.as_ref().expect("sort");
        assert_eq!(sort.allowed, vec!["title", "type", "priority", "updated"]);
        assert_eq!(sort.default_field, "updated");
        assert_eq!(sort.default_dir, SortDirAst::Desc);
    }

    #[test]
    fn sort_requires_by_line() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        default title asc\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("requires a `by`"));
    }

    #[test]
    fn sort_default_field_must_be_allowed() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title\n        default updated desc\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("must be listed"));
    }

    #[test]
    fn sort_default_requires_dir() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title\n        default title\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("default <field>"));
    }

    #[test]
    fn selection_single_and_multi_parse() {
        let source = r#"surface item web
  audience admin
    view list single_view
      source item.query.search
      columns title
      selection single
    view list multi_view
      source item.query.search
      columns title
      selection multi
"#;
        let surface = parse_surface_document(source).expect("parses selection");
        let single = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        let multi = match &surface.audiences[0].views[1] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(single.mode, SelectionModeAst::Single);
        assert_eq!(multi.mode, SelectionModeAst::Multi);
    }

    #[test]
    fn selection_none_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      selection none\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("omit the line"));
    }

    #[test]
    fn selection_unknown_mode_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      selection foo\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("selection single"));
    }

    #[test]
    fn bulk_actions_single_and_multi_parse() {
        let source = r#"surface item web
  audience admin
    view list one
      source item.query.search
      columns title
      selection multi
      bulk_actions delete
    view list many
      source item.query.search
      columns title
      selection multi
      bulk_actions delete, archive
"#;
        let surface = parse_surface_document(source).expect("parses bulk actions");
        let one = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        let many = match &surface.audiences[0].views[1] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(one.bulk_actions, vec!["delete"]);
        assert_eq!(many.bulk_actions, vec!["delete", "archive"]);
    }

    #[test]
    fn bulk_actions_duplicate_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      bulk_actions delete\n      bulk_actions archive\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("bulk_actions"));
    }

    #[test]
    fn bulk_actions_without_selection_is_not_parser_error() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      bulk_actions delete\n";
        let surface = parse_surface_document(source).expect("bulk-only parses");
        let selection = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v.selection.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(selection.mode, SelectionModeAst::None);
        assert_eq!(selection.bulk_actions, vec!["delete"]);
    }

    #[test]
    fn settings_full_example_parses() {
        let source = r#"surface item web
  audience admin
    view list terminal
      source item.query.search
      columns title
      settings
        grid_size: Enum [sm, md, lg] default sm
          persist local
        show_metadata: Bool default true
        page_size: Int min 10 max 200 default 25
          persist workspace
"#;
        let surface = parse_surface_document(source).expect("parses settings");
        let list = match &surface.audiences[0].views[0] {
            ViewAst::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(list.settings.len(), 3);
        assert_eq!(list.settings[0].name, "grid_size");
        assert_eq!(
            list.settings[0].value_space,
            SettingValueSpaceAst::Enum(vec!["sm".into(), "md".into(), "lg".into()])
        );
        assert_eq!(list.settings[0].default, "sm");
        assert_eq!(list.settings[0].persistence, SettingPersistenceAst::Local);
        assert_eq!(list.settings[1].value_space, SettingValueSpaceAst::Bool);
        assert_eq!(
            list.settings[2].value_space,
            SettingValueSpaceAst::Int {
                min: Some(10),
                max: Some(200)
            }
        );
        assert_eq!(
            list.settings[2].persistence,
            SettingPersistenceAst::Workspace
        );
    }

    #[test]
    fn persist_outside_setting_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      persist local\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("persist"));
    }

    #[test]
    fn duplicate_setting_name_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        grid_size: Bool default true\n        grid_size: Bool default false\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("duplicate setting"));
    }

    #[test]
    fn enum_default_must_be_member() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        grid_size: Enum [sm, md] default lg\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("not in the enum"));
    }

    #[test]
    fn int_default_must_be_in_range() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n        page_size: Int min 10 max 200 default 5\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("below"));
    }

    #[test]
    fn settings_empty_block_rejected() {
        let source = "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      settings\n";
        let err = parse_surface_document(source).unwrap_err();
        assert!(err.to_string().contains("at least one setting"));
    }

    #[test]
    fn list_only_keywords_rejected_in_detail_and_create() {
        let detail = "surface item web\n  audience admin\n    view detail terminal\n      source item.query.by_id\n      sort\n        by title\n        default title asc\n";
        let create = "surface item web\n  audience admin\n    view create terminal\n      submit item.command.create\n      selection multi\n";
        let detail_err = parse_surface_document(detail).unwrap_err();
        let create_err = parse_surface_document(create).unwrap_err();
        assert!(detail_err.to_string().contains("valid only in `view list`"));
        assert!(create_err.to_string().contains("valid only in `view list`"));
    }
}

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
