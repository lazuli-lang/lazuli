//! Lowering from `lazuli_syntax` canonical AST slices into `lazuli_ir`.
//!
//! ## Role in the compile pipeline
//!
//! `lazuli_analyzer` sits between `lazuli_syntax` (canonical AST) and
//! `lazuli_ir` (typed lowered shape). Its job is **mechanical
//! projection plus structural validation**: lift the parser's verbatim
//! AST onto the IR shape that downstream consumers (codegen, doctor,
//! LSP, inspect) read. Anything that needs cross-module reasoning
//! lives in `lazuli_cli` (the `expand` pass) or `lazuli_doctor`;
//! anything per-file lives here.
//!
//! ## Submodule layout (R3-E — rails-style refactor)
//!
//! The lowering pipeline is organised into per-concern sibling
//! modules. Each one carries the projection rules for a single
//! "slot" in the vocabulary:
//!
//! ### Cross-cutting primitives
//!
//! * [`helpers`] — pure utility predicates (case conversion, span
//!   bridging, edit-distance, balanced-paren walkers). No AST shape,
//!   no IR shape larger than `SpanRef`. Shared by every slice.
//! * [`expr`] — pure mechanical "text → IR atom" projections
//!   (paths, qualified names, raw exprs, policy atoms, translation
//!   keys). Every other slice calls into this slot.
//! * [`source_map`] — source-position bookkeeping consumed by LSP.
//! * [`symbol_origin`] — origin tagging (handwritten vs synthesized
//!   vs pack-derived) used by inspect and doctor.
//!
//! ### Per-domain lowering (R2 — Wave 4.6)
//!
//! * [`command`] — command effect cluster (`creates|updates|deletes`),
//!   target / let / named-arg / assignment leaves, and the
//!   `invalidates query.<name>` cross-feature reference resolver.
//! * [`workflow`] — async-work leaf lowerings shared by `job`,
//!   `poller`, `webhook`, `tenant_migration`, `channel`,
//!   `notification`, `mcp_server`, `event_group`: retry, fanout,
//!   external-call refs, emit predicates, MCP leaves, digest /
//!   throttle, event-variant fields, job body / trigger.
//! * [`lzx`] — `.lzx` *app layer* (routes, experiences, platform
//!   surfaces). One entry point: `lower_lzx_document`.
//! * [`surface`] — `.lzx` *ViewModel layer* (per-feature audiences +
//!   views + cells + drawers + route params). One entry point:
//!   `lower_surface`.
//!
//! ### Per-domain lowering (R3-E)
//!
//! * [`resource`] — `resource <Foo> { ... }` decl + field-level
//!   lowering (`@cap.PII` extraction, modifier recovery,
//!   inline-validator constraint lift, the four `validate_constraint_*`
//!   gates) + rate-limit literal projection.
//! * [`query`] — `query.list` / `query.lookup` / `query.sql` lowering,
//!   filter line parser (WAR-VOCAB-QUERY-ENUM-01), cache profile
//!   resolution (CL.C.3), and `lower_command_input_to_typed` for
//!   typed query/command input slots.
//! * [`auth`] — `auth { identity | password | sessions | mfa | oauth }`
//!   lowering. The non-trivial bit is `<Resource>.<field>` ->
//!   `FieldRef` splitting; the rest is structural.
//! * [`agent`] — LLM capability lowering: input slots, policy atom,
//!   output projection (text|stream|enum|record-discriminator),
//!   tool reference resolution (Adapter|Local|CrossFeature), eval
//!   case + closed-predicate parser, HTTP expose.
//! * [`design`] — closed-catalog design token lowering (colors,
//!   typography, spaces, radii, shadows, motion, breakpoints,
//!   z-indices, custom). Cheap structural validation per group.
//! * [`plan_gate`] — package-wide `PlanGateFacts` aggregator
//!   (subscription anchor + plan catalog + per-callable gates)
//!   and the six PG diagnostic codes.
//! * [`lifecycle`] — resource lifecycle synthesis hooks.
//! * [`checks`] — public per-file structural checks invoked by
//!   `lazuli_cli` / `lazuli_doctor`. Stays public because external
//!   tools depend on it.
//! * [`rbac`] — RBAC closure construction over a feature's policies.
//!
//! Per-feature orchestration (`lower_feature_skeleton`, jobs / pollers
//! / webhooks / notifications / channels / event groups orchestration,
//! reports, conventions / CRUD synthesis, auto-photo synthesis) still
//! lives in this file. The per-domain leaves above are called from
//! there.
//!
//! ## Vocabulary cross-reference
//!
//! Source AST shapes are defined in `lazuli_syntax::ast` (Wave 4.4).
//! Destination IR shapes are defined in `lazuli_ir` (Wave 4.1). When
//! a lowering function feels like it's "thinking" rather than just
//! "translating", the design pressure belongs upstream (parser
//! enforcement, IR shape change) — not here.
//!
//! ## ABI guarantee
//!
//! Public items historically reachable at `lazuli_analyzer::Foo`
//! remain reachable at the same path. Internal helpers used across
//! sibling modules are `pub(crate)`.

mod agent;
mod auth;
mod auto_photo;
pub mod checks;
mod command;
mod command_decl;
mod conventions;
mod design;
mod errors;
mod expr;
mod feature;
mod feature_meta;
mod helpers;
mod lifecycle;
mod lzx;
mod plan_gate;
mod query;
pub mod rbac;
mod report;
mod resource;
pub mod source_map;
mod surface;
pub mod symbol_origin;
mod types;
mod workflow;

pub use agent::lower_agent;
pub use auth::lower_auth;
pub(crate) use command_decl::{DeprecationTarget, lower_command_decl, lower_deprecated};
pub use conventions::{
    ConventionSynthDiagnostic, CrudSynthDiagnostic, build_owner_scope_cte_prefix_for_test,
    build_owner_scope_where_for_test, synthesize_conventions,
};
pub use design::lower_design;
pub use errors::{AnalyzeError, CONVENTION_CATALOG, conventions_unknown_suggestion};
pub use feature::lower_feature_skeleton;
pub(crate) use feature_meta::{
    lower_aggregate_decl, lower_api_decl, lower_defaults, lower_enum_decl,
    lower_feature_errors_decl, lower_invariant_decl, lower_public_contract, lower_record_decl,
    lower_translation_decl,
};
pub use lzx::lower_lzx_document;
pub use plan_gate::{
    PlanGateCode, PlanGateDiagnostic, PlanGateFacts, aggregate_plan_gate_facts,
    diagnose_plan_gate_facts, parse_subscription_anchor,
};
pub use surface::lower_surface;
pub use symbol_origin::build_symbol_origin_index;
#[cfg(test)]
pub(crate) use types::parse_cap_file_type;
pub use types::type_ref_from_syntax_public;
pub(crate) use types::{
    parse_cap_pii_type, parse_default, type_ref_from_syntax, type_ref_from_text,
};

use expr::{
    lower_path_string, lower_policy_atom, lower_policy_expr, lower_qualified_name,
    lower_translation_key_ref,
};
use workflow::{
    lower_emit_predicate, lower_event_variant_field, lower_external_call, lower_fanout,
    lower_job_body, lower_job_trigger, lower_mcp_prompt, lower_mcp_resource, lower_mcp_tool,
    lower_notification_digest, lower_notification_throttle, lower_retry,
    lower_tenant_migration_target,
};

use helpers::span_of;

use lazuli_ir as ir;
use lazuli_syntax as syntax;

// `lower_lzx_document` + `lower_surface` and the entire `.lzx`
// surface family moved to `lzx.rs` (app layer) and `surface.rs`
// (ViewModel layer).
//
// `type_ref_from_syntax` + `type_ref_from_text` + the `@cap.*`
// capability parsers (`@cap.File`, `@cap.PII`, `@cap.Hashed`,
// `@cap.Encrypted`, `@cap.E2ee`, `@cap.Token`), `@semantic.Money`
// parsing, `parse_default`, and the closed-catalog primitive-type
// match moved to `types.rs`.

/// Phase L Tier 4 follow-up — lower a canonical-indent `policies` block
/// into `ir::Policies`. The AST mirrors the IR shape 1:1 so this is a
/// structural copy: category atoms and per-resource field overrides
/// project directly. Closed-catalog validation lives in doctor.
pub(crate) fn lower_policies_decl(decl: &syntax::PoliciesDecl) -> ir::Policies {
    let categories = decl
        .categories
        .iter()
        .map(|c| ir::PolicyCategory {
            name: c.name.clone(),
            atoms: c.atoms.clone(),
            previous_names: Vec::new(),
            // IR Error-Vocab (Cell PARSE-1) — lower the optional
            // `when_denied @translation.<key>` child onto the typed IR
            // slot. Same-feature scope; cross-feature key resolution
            // lives in doctor (`translation_key_unknown` + ERR-VOCAB-002).
            when_denied: c.when_denied.as_ref().map(lower_translation_key_ref),
            when_denied_route: c.when_denied_route.as_ref().map(lower_when_denied_route),
        })
        .collect();
    let fields = decl
        .fields
        .iter()
        .map(|f| ir::FieldPolicies {
            resource: lower_qualified_name(&f.resource),
            fields: f
                .fields
                .iter()
                .map(|fp| ir::FieldPolicy {
                    field: fp.field.clone(),
                    read: fp.read.clone(),
                    write: fp.write.clone(),
                    previous_names: Vec::new(),
                })
                .collect(),
        })
        .collect();
    ir::Policies {
        categories,
        fields,
        span_ref: Some(span_of(decl.span)),
    }
}

pub(crate) fn lower_when_denied_route(route: &syntax::WhenDeniedRouteAst) -> ir::WhenDeniedRoute {
    ir::WhenDeniedRoute {
        unauthenticated: route
            .unauthenticated
            .as_ref()
            .map(lower_route_redirect_target),
        role_mismatch: route
            .role_mismatch
            .iter()
            .map(|arm| ir::RoleMismatchArm {
                role: arm.role.clone(),
                target: lower_route_redirect_target(&arm.target),
                span_ref: Some(span_of(arm.span)),
            })
            .collect(),
        default: route.default.as_ref().map(lower_route_redirect_target),
        span_ref: Some(span_of(route.span)),
    }
}

pub(crate) fn lower_route_redirect_target(
    target: &syntax::RouteRedirectTargetAst,
) -> ir::RouteRedirectTarget {
    match target {
        syntax::RouteRedirectTargetAst::View(view) => ir::RouteRedirectTarget::View(view.clone()),
        syntax::RouteRedirectTargetAst::Path(path) => ir::RouteRedirectTarget::Path(path.clone()),
    }
}

/// Phase L Tier 3 — lower a canonical-indent `job` block into `ir::Job`.
/// Handler-backed bodies lower fully; declarative bodies preserve the
/// raw spine (`raw_target`, `raw_lets`, `raw_effect`) until Tier 4.
pub fn lower_job(feature: &str, job: &syntax::Job) -> Result<ir::Job, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &job.trigger);
    let idempotency = job
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = job.retry.as_ref().map(lower_retry);
    let tenant_from = job
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let fanout = job.fanout.as_ref().map(lower_fanout);
    let external_calls = job.external_calls.iter().map(lower_external_call).collect();
    let policy = job
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let policy = match policy {
        ir::PolicyRef::None => None,
        other => Some(other),
    };
    let body = lower_job_body(&job.body);

    let policy_expr = job.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Job {
        name: job.name.clone(),
        trigger,
        queue: job.queue.clone(),
        idempotency,
        retry,
        policy,
        policy_expr,
        policy_when_denied: None,
        tenant_from,
        fanout,
        timeout: job.timeout.clone(),
        external_calls,
        body,
        emits: job.emits.clone(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(job.span)),
    })
}

// =============================================================================
// L0 #8 — poller lowering (docs/proposals/poller-vocab.md §4).
//
// AST → IR is purely structural; doctor rules enforce the closed-catalog
// validity invariants (cursor field shapes, terminal-state existence,
// handler orphan, etc.). The lowering never fails on AST alone — it
// applies the defaults (`tick.every = 30s`, `tick.batch = 100`) and
// surfaces structurally well-formed IR for downstream consumers.
// =============================================================================

/// Default tick interval when `tick every <duration>` is omitted in source.
/// Per proposal §3.8.
const POLLER_DEFAULT_TICK_EVERY: &str = "30s";
const POLLER_DEFAULT_TICK_BATCH: u32 = 100;

pub fn lower_poller(poller: &syntax::PollerBlockAst) -> Result<ir::Poller, AnalyzeError> {
    let cursor_ast = poller
        .cursor
        .as_ref()
        .ok_or_else(|| AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "cursor".to_owned(),
        })?;
    let retry_ast = poller
        .retry
        .as_ref()
        .ok_or_else(|| AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "retry".to_owned(),
        })?;
    let resolve_name =
        poller
            .resolve_handler
            .as_deref()
            .ok_or_else(|| AnalyzeError::MissingField {
                kind: "poller".to_owned(),
                name: poller.name.clone(),
                field: "resolve via @fn.<name>".to_owned(),
            })?;
    if poller.idempotency.is_empty() {
        return Err(AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "idempotency".to_owned(),
        });
    }
    if poller.states.is_empty() {
        return Err(AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "states".to_owned(),
        });
    }

    let cursor = ir::PollerCursor {
        next_at_field: cursor_ast.next_at_field.clone(),
        resolved_at_field: cursor_ast.resolved_at_field.clone(),
        attempts_field: cursor_ast.attempts_field.clone(),
        span_ref: Some(span_of(cursor_ast.span)),
    };

    let backoff = match retry_ast.backoff_strategy.as_str() {
        "fixed" => ir::PollerBackoff::Fixed {
            base: retry_ast.backoff_base.clone(),
        },
        "linear" => ir::PollerBackoff::Linear {
            base: retry_ast
                .backoff_base
                .clone()
                .unwrap_or_else(|| "30s".to_owned()),
            cap: retry_ast.backoff_cap.clone(),
        },
        "exponential" => ir::PollerBackoff::Exponential {
            base: retry_ast
                .backoff_base
                .clone()
                .unwrap_or_else(|| "30s".to_owned()),
            cap: retry_ast.backoff_cap.clone(),
        },
        other => {
            return Err(AnalyzeError::UnknownEnum {
                kind: format!("poller `{}` backoff", poller.name),
                value: other.to_owned(),
            });
        }
    };
    let retry = ir::PollerRetry {
        max_attempts: retry_ast.max_attempts,
        backoff,
        span_ref: Some(span_of(retry_ast.span)),
    };

    let states = poller
        .states
        .iter()
        .map(|s| ir::PollerState {
            name: s.name.clone(),
            kind: match s.kind_keyword.as_deref() {
                Some("initial") => ir::PollerStateKind::Initial,
                Some("terminal") => ir::PollerStateKind::Terminal,
                Some("intermediate") | None => ir::PollerStateKind::Intermediate,
                Some(_) => ir::PollerStateKind::Intermediate,
            },
            span_ref: Some(span_of(s.span)),
        })
        .collect::<Vec<_>>();

    let tick = match poller.tick.as_ref() {
        Some(t) => ir::PollerTick {
            every: t.every.clone(),
            batch: t.batch.unwrap_or(POLLER_DEFAULT_TICK_BATCH),
        },
        None => ir::PollerTick {
            every: POLLER_DEFAULT_TICK_EVERY.to_owned(),
            batch: POLLER_DEFAULT_TICK_BATCH,
        },
    };

    let tenant_from = poller
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });

    let idempotency = ir::IdempotencyKey {
        by: ir::Path {
            segments: poller.idempotency.iter().cloned().collect(),
        },
    };

    let audit = poller.audit.as_deref().map(|raw: &str| {
        let rest = raw.strip_prefix("audit ").unwrap_or(raw).trim();
        if rest == "default" {
            ir::AuditSpec {
                subjects: vec!["actor".to_owned(), "target.id".to_owned()],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        } else if let Some(reason) = rest.strip_prefix("none ") {
            ir::AuditSpec {
                subjects: vec![format!("none {}", reason)],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        } else {
            ir::AuditSpec {
                subjects: rest
                    .split(',')
                    .map(str::trim)
                    .filter(|s: &&str| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        }
    });

    let retry_quirks = poller
        .retry_quirks
        .iter()
        .filter_map(|q| match q.kind.as_str() {
            "gender_flip_once" => Some(ir::PollerRetryQuirk::GenderFlipOnce {
                when: q.when.clone(),
                counter_field: q.counter_field.clone(),
                gender_field: q.mutate_field.clone(),
            }),
            // Unknown catalog entries are dropped during lowering;
            // doctor `POLLER-QUIRK-CATALOG-MISMATCH-001` surfaces the
            // diagnostic at the AST layer.
            _ => None,
        })
        .collect();

    Ok(ir::Poller {
        name: poller.name.clone(),
        source: poller.source.clone(),
        cursor,
        retry,
        states,
        resolve_handler: ir::HandlerRef {
            namespace: "fn".to_owned(),
            name: resolve_name.to_owned(),
            span_ref: Some(span_of(poller.span)),
        },
        terminal_status_field: poller.terminal_status_field.clone(),
        terminal_result_field: poller.terminal_result_field.clone(),
        tick,
        tenant_from,
        idempotency,
        audit,
        emits: poller.emits.clone(),
        retry_quirks,
        span_ref: Some(span_of(poller.span)),
    })
}

/// Phase L Tier 3 — lower a canonical-indent `webhook` block into
/// `ir::Webhook`. `verify: PathRef` falls back to a conventional path
/// derived from the webhook name (the legacy IR field is non-optional);
/// `structured_verify` carries the real structured spec lifted by
/// `parse_webhook_verify`.
pub fn lower_webhook(webhook: &syntax::Webhook) -> Result<ir::Webhook, AnalyzeError> {
    let structured_verify = Some(ir::VerifySpec {
        scheme: match webhook.verify.scheme.as_str() {
            "hmac" => ir::VerifyScheme::Hmac,
            other => {
                return Err(AnalyzeError::UnsupportedVerifyScheme {
                    scheme: other.to_owned(),
                });
            }
        },
        algorithm: webhook.verify.algorithm.clone(),
        secret_env: webhook
            .verify
            .secret_env
            .as_deref()
            .map(extract_env_binding)
            .unwrap_or_default(),
        header: webhook.verify.header.clone().unwrap_or_default(),
    });
    let tenant_from = webhook
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = webhook
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let policy = webhook
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));

    let (handler, returns) = match &webhook.handler {
        Some(h) => (
            ir::PathRef::authored(&h.path),
            h.returns.as_deref().map(|t| type_ref_from_text(t)),
        ),
        None => (
            ir::PathRef::convention(format!("./webhooks/{}.go", webhook.name)),
            None,
        ),
    };

    // Webhooks expanded cycle — typed payload reference (`payload from
    // webhook_events.<name>`). The parser stripped the catalog prefix
    // already, so the IR just keeps the suffix.
    let payload_from = webhook
        .payload_from
        .as_deref()
        .map(|name| ir::WebhookEventRef {
            name: name.to_owned(),
        });

    // `replay` short form (`replay allow within "..."`) and long form
    // (nested children) collapse onto the same `ReplaySpec`.
    let replay = webhook.replay.as_ref().map(|r| ir::ReplaySpec {
        mode: match r.mode.as_str() {
            "deny" => ir::ReplayMode::Deny,
            _ => ir::ReplayMode::Allow,
        },
        within: r.within.clone(),
        dedupe_by: r.dedupe_by.as_deref().map(lower_path_string),
    });

    // `dlq` discriminator (mutual exclusion enforced by the parser).
    let dlq = webhook.dlq.as_ref().map(|d| match d {
        syntax::WebhookDlq::Emit { event, .. } => ir::DlqSpec::Emit {
            event: event.clone(),
        },
        syntax::WebhookDlq::Handler { path, .. } => ir::DlqSpec::Handler {
            path: ir::PathRef::authored(path),
        },
        syntax::WebhookDlq::Drop { reason, .. } => ir::DlqSpec::Drop {
            reason: reason.clone(),
        },
    });

    // Inbound retry shares the jobs `RetryPolicy` shape (Atrito #5).
    let retry = webhook.retry.as_ref().map(lower_retry);

    let policy_expr = webhook.policy_expr.as_ref().map(lower_policy_expr);
    let scope_global = webhook
        .scope_global
        .as_ref()
        .map(|sg| ir::WebhookScopeGlobalSpec {
            reason: sg.reason.clone(),
        });
    // B5 framework gap 2 — lift per-branch emit predicates onto the
    // typed `EmitPredicate` shape. The AST carries the raw `when`
    // clauses; we promote `path = "literal"` and
    // `path in ("a", "b")` to typed variants and fall back to
    // `EmitPredicateKind::Other { raw }` for anything else. Length
    // matches `webhook.emits` when any predicate is authored; an
    // empty vec means "flat list, no per-branch dispatch".
    let emit_predicates = if webhook.emits_predicates.is_empty() {
        Vec::new()
    } else {
        webhook
            .emits_predicates
            .iter()
            .map(|raw| raw.as_deref().map(lower_emit_predicate))
            .collect::<Vec<_>>()
    };

    Ok(ir::Webhook {
        name: webhook.name.clone(),
        route: webhook.route.clone(),
        verify: ir::PathRef::convention(format!("./webhooks/{}_verify.go", webhook.name)),
        structured_verify,
        tenant_from,
        scope_global,
        idempotency,
        policy,
        policy_expr,
        policy_when_denied: None,
        handler,
        returns,
        emits: webhook.emits.clone(),
        emit_predicates,
        payload_from,
        replay,
        dlq,
        retry,
        previous_names: Vec::new(),
        span_ref: Some(span_of(webhook.span)),
    })
}

/// B5 framework gap 2 — lift a raw `when <predicate>` clause into the
/// typed `ir::EmitPredicate`. Recognised shapes:
///
/// * `path = "literal"` — equality.
/// * `path in ("a", "b")` — set membership.
/// * anything else — `EmitPredicateKind::Other { raw }`.
///
/// The lift is intentionally conservative: shapes that don't match
/// the typed catalog are preserved verbatim so codegen can emit a
/// runtime-evaluated stub without losing authoring intent.
/// Realtime bucket cycle MVP — lower a canonical-indent `channel`
/// block into `ir::Channel`. Mechanical projection: the parser
/// already enforces presence of all three required children, so the
/// lowering only wraps the verbatim strings into the typed shapes
/// (`TenantFromSpec`, `PolicyRef::Atom`, payload string verbatim).
/// Doctor `CHANNEL-PAYLOAD-001` resolves the payload reference
/// downstream.
pub fn lower_channel(channel: &syntax::Channel) -> ir::Channel {
    ir::Channel {
        name: channel.name.clone(),
        tenant_from: ir::TenantFromSpec {
            path: lower_path_string(&channel.tenant_from),
        },
        policy: lower_policy_atom(&channel.policy),
        policy_when_denied: None,
        payload: channel.payload.clone(),
        span_ref: Some(span_of(channel.span)),
    }
}

/// Phase L Tier 3 — lower a canonical-indent `notification` block into
/// `ir::Notification`. Reuses `JobTrigger`, `IdempotencyKey`,
/// `RetryPolicy`, `TenantFromSpec` from the job lowering helpers.
pub fn lower_notification(
    feature: &str,
    notification: &syntax::Notification,
) -> Result<ir::Notification, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &notification.trigger);
    let tenant_from = notification
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = notification
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = notification.retry.as_ref().map(lower_retry);
    let policy = notification
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));
    let digest = notification.digest.as_ref().map(lower_notification_digest);
    let throttle = notification
        .throttle
        .as_ref()
        .map(lower_notification_throttle);
    let policy_expr = notification.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Notification {
        name: notification.name.clone(),
        trigger,
        channels: notification.channels.clone(),
        recipient: notification.recipient.clone(),
        template: notification.template.clone(),
        policy,
        policy_expr,
        tenant_from,
        idempotency,
        retry,
        emits: notification.emits.clone(),
        digest,
        throttle,
        previous_names: Vec::new(),
        span_ref: Some(span_of(notification.span)),
    })
}

/// MCP bucket cycle — lower a canonical-indent `mcp_server` block into
/// `ir::MCPServerSpec`. Value-preserving except for the closed-catalog
/// `transport` mapping, which rejects unknown literals at lower-time.
pub fn lower_mcp_server(server: &syntax::McpServer) -> Result<ir::MCPServerSpec, AnalyzeError> {
    let transport = match server.transport.as_str() {
        "stdio" => ir::MCPTransport::Stdio,
        "http_sse" => ir::MCPTransport::HttpSse,
        "http_streamable" => ir::MCPTransport::HttpStreamable,
        other => {
            return Err(AnalyzeError::UnknownEnum {
                kind: format!("MCP-TRANSPORT-001 mcp_server `{}` transport", server.name),
                value: other.to_owned(),
            });
        }
    };
    let auth = server.auth.as_deref().and_then(parse_mcp_auth);
    let metadata = ir::MCPServerMetadata {
        name: server.metadata.name.clone(),
        description: server.metadata.description.clone(),
        version: server.metadata.version.clone(),
    };
    let tools = server.tools.iter().map(lower_mcp_tool).collect::<Vec<_>>();
    let resources = server
        .resources
        .iter()
        .map(lower_mcp_resource)
        .collect::<Vec<_>>();
    let prompts = server
        .prompts
        .iter()
        .map(lower_mcp_prompt)
        .collect::<Vec<_>>();
    Ok(ir::MCPServerSpec {
        name: server.name.clone(),
        transport,
        scope_feature: server.scope_feature.clone(),
        auth,
        metadata,
        tools,
        resources,
        prompts,
        span_ref: Some(span_of(server.span)),
    })
}

/// Parse `bearer env.<NAME>` into `ir::MCPAuth::BearerEnvVar`. Anything
/// else (future `oauth ...`, malformed line) returns `None`; doctor
/// `MCP-AUTH-001` (registered in proposal) catches malformed shapes.
fn parse_mcp_auth(raw: &str) -> Option<ir::MCPAuth> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("bearer env.") {
        let env = rest.trim().to_owned();
        if env.is_empty() {
            return None;
        }
        return Some(ir::MCPAuth::BearerEnvVar { env });
    }
    None
}

/// Phase L Tier 3 — lower a canonical-indent `event_group` into
/// `ir::EventGroup`. The payload bag and authored events stay as raw
/// strings; B5 framework gap 1 lifts the per-event typed payload
/// blocks into `variants`.
pub fn lower_event_group(group: &syntax::EventGroup) -> ir::EventGroup {
    // EVENT-OUTBOX §3.3 — lower the parallel bool vec into the typed
    // `OutboxMode` catalog. Index-paired with `events`; when the AST
    // emits an empty vec (legacy / pre-outbox payloads) we expand to
    // a same-length `None` vec so downstream code can read by index.
    let events_outbox: Vec<ir::OutboxMode> = if group.events_outbox_guaranteed.is_empty() {
        vec![ir::OutboxMode::None; group.events.len()]
    } else {
        group
            .events_outbox_guaranteed
            .iter()
            .map(|g| {
                if *g {
                    ir::OutboxMode::Guaranteed
                } else {
                    ir::OutboxMode::None
                }
            })
            .collect()
    };

    // B5 framework gap 1 — lift per-event field bodies into
    // `EventVariant` records. Each variant carries its `EventField`s
    // lifted via `type_ref_from_syntax`, the closed kind catalog
    // (committed vs trace), and the outbox flag mirrored from the
    // parallel slot above. Back-compat: variants whose body was
    // empty come through with an empty `fields` Vec; legacy fixtures
    // that didn't author `event_variants`/`event_variant_kinds` at
    // all leave `variants` empty.
    let variants: Vec<ir::EventVariant> =
        if group.event_variants.is_empty() && group.event_variant_kinds.is_empty() {
            Vec::new()
        } else {
            group
                .events
                .iter()
                .enumerate()
                .map(|(idx, short_name)| {
                    let kind = match group
                        .event_variant_kinds
                        .get(idx)
                        .copied()
                        .unwrap_or(syntax::EventVariantKindAst::Committed)
                    {
                        syntax::EventVariantKindAst::Committed => ir::EventVariantKind::Committed,
                        syntax::EventVariantKindAst::Trace => ir::EventVariantKind::Trace,
                    };
                    let fields = group
                        .event_variants
                        .get(idx)
                        .map(|rows| {
                            rows.iter()
                                .map(lower_event_variant_field)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let outbox = events_outbox
                        .get(idx)
                        .copied()
                        .unwrap_or(ir::OutboxMode::None);
                    ir::EventVariant {
                        name: short_name.clone(),
                        kind,
                        outbox,
                        fields,
                        span_ref: group
                            .event_variants
                            .get(idx)
                            .and_then(|rows| rows.first().map(|f| span_of(f.span))),
                    }
                })
                .collect()
        };

    ir::EventGroup {
        pattern: group.pattern.clone(),
        on_resource: group.on_resource.clone(),
        raw_payload: group.payload.clone(),
        raw_audit: group.audit.clone(),
        events: group.events.clone(),
        events_outbox,
        variants,
        span_ref: Some(span_of(group.span)),
    }
}

/// Migrations bucket cycle Route C — lower a canonical-indent
/// `tenant_migration` block into `ir::TenantMigration`. Mirrors
/// `lower_job` for the shared spine (idempotency / retry / timeout /
/// handler) and adds the `target tenants <axis>` slot. The lowering
/// does **not** enforce that `idempotency` is authored; that is
/// `TM-IDEMP-001`'s job downstream.
pub fn lower_tenant_migration(
    tm: &syntax::TenantMigration,
) -> Result<ir::TenantMigration, AnalyzeError> {
    let idempotency = tm
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path })
        .unwrap_or_else(|| ir::IdempotencyKey {
            by: ir::Path::from_segments(Vec::<String>::new()),
        });
    let retry = tm.retry.as_ref().map(lower_retry);
    Ok(ir::TenantMigration {
        name: tm.name.clone(),
        target: ir::TenantMigrationTarget {
            operation: tm.target_ref.as_deref().map(lower_tenant_migration_target),
            axis: tm.target_axis.clone(),
        },
        idempotency,
        retry,
        timeout: tm.timeout.clone(),
        handler: ir::PathRef::authored(&tm.handler),
        previous_names: Vec::new(),
        span_ref: Some(span_of(tm.span)),
    })
}

/// Analyzer-level resolution for `Command.invalidates`. This pass is
/// intentionally module-scoped: same-feature refs were normalized during
/// per-feature lowering, but cross-feature refs can only be validated once
/// all feature IR is present.
pub fn resolve_invalidates_targets(module: &mut ir::Module) -> Result<(), AnalyzeError> {
    normalize_legacy_invalidates_targets(&mut module.features);
    validate_invalidates_targets(&module.features)
}

pub fn validate_invalidates_targets(features: &[ir::Feature]) -> Result<(), AnalyzeError> {
    let index = InvalidatesQueryIndex::from_features(features);
    for feature in features {
        for command in &feature.commands {
            for invalidates in &command.invalidates {
                let target_feature = invalidates
                    .query
                    .feature
                    .as_deref()
                    .unwrap_or(feature.name.as_str());
                if !index.has_query(target_feature, &invalidates.query.name) {
                    return Err(AnalyzeError::UnknownInvalidateTarget {
                        cmd: command.name.clone(),
                        target: invalidates_target_display(&feature.name, &invalidates.query),
                        target_feature: target_feature.to_owned(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn normalize_legacy_invalidates_targets(features: &mut [ir::Feature]) {
    for feature in features {
        for command in &mut feature.commands {
            for invalidates in &mut command.invalidates {
                match invalidates.query.feature.as_deref() {
                    Some("query") | None => {
                        invalidates.query.feature = Some(feature.name.clone());
                    }
                    _ => {}
                }
            }
        }
    }
}

fn invalidates_target_display(current_feature: &str, query: &ir::QualifiedName) -> String {
    match query.feature.as_deref() {
        Some(feature) if feature == current_feature => format!("query.{}", query.name),
        Some(feature) => format!("{feature}.query.{}", query.name),
        None => format!("query.{}", query.name),
    }
}

struct InvalidatesQueryIndex {
    queries_by_feature: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
}

impl InvalidatesQueryIndex {
    fn from_features(features: &[ir::Feature]) -> Self {
        let queries_by_feature = features
            .iter()
            .map(|feature| {
                (
                    feature.name.clone(),
                    feature
                        .queries
                        .iter()
                        .map(|query| query.name().to_owned())
                        .collect(),
                )
            })
            .collect();
        Self { queries_by_feature }
    }

    fn has_query(&self, feature: &str, query: &str) -> bool {
        self.queries_by_feature
            .get(feature)
            .is_some_and(|queries| queries.contains(query))
    }
}

/// Extract the env binding name from `env.<NAME>` (`secret env.X`).
fn extract_env_binding(raw: &str) -> String {
    raw.trim()
        .strip_prefix("env.")
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|| raw.trim().to_owned())
}

/// Build a feature-local `QualifiedName` (no feature prefix).
pub(crate) fn qualified_name_local(name: &str) -> ir::QualifiedName {
    ir::QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

/// Treat the entire namespace literal as a single name (e.g.
/// `@llm.default`, `@validator.pii_email_scrub`, `@semantic.Email`).
/// Doctor + LSP enforce the closed-namespace catalog elsewhere; this
/// helper keeps the raw form so resolution stays uniform.
pub(crate) fn qualified_namespace(raw: &str) -> ir::QualifiedName {
    ir::QualifiedName {
        feature: None,
        name: raw.to_owned(),
    }
}

#[cfg(test)]
pub(crate) fn lower_policy_atom_with_args(text: &str) -> ir::PolicyAtom {
    let raw = text.trim().strip_prefix('@').unwrap_or(text.trim());
    let (ns_name, args) = match raw.split_once('(') {
        Some((head, tail)) => (head.trim(), Some(tail.trim_end_matches(')').to_owned())),
        None => (raw.trim(), None),
    };
    let (namespace, name) = ns_name
        .split_once('.')
        .map(|(namespace, name)| (namespace.to_owned(), name.to_owned()))
        .unwrap_or_else(|| ("".to_owned(), ns_name.to_owned()));
    ir::PolicyAtom {
        namespace,
        name,
        args,
    }
}

#[cfg(test)]
pub(crate) fn lower_audit_block(src: &str) -> ir::AuditSpec {
    let mut spec = ir::AuditSpec {
        subjects: Vec::new(),
        emit_to: None,
        data_subject: None,
        record_before: false,
        record_after: false,
        retain_for: None,
    };
    for line in src.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(rest) = line.strip_prefix("audit data_subject ") {
            spec.data_subject = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("data_subject ") {
            spec.data_subject = Some(rest.trim().to_owned());
        } else if line == "audit before" || line == "before" {
            spec.record_before = true;
        } else if line == "audit after" || line == "after" {
            spec.record_after = true;
        } else if let Some(rest) = line
            .strip_prefix("audit retain ")
            .or_else(|| line.strip_prefix("retain "))
        {
            spec.retain_for = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("audit ") {
            for part in rest
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                match part {
                    "before" => spec.record_before = true,
                    "after" => spec.record_after = true,
                    _ => spec.subjects.push(part.to_owned()),
                }
            }
        } else if let Some(rest) = line.strip_prefix("emit_to ") {
            spec.emit_to = Some(rest.trim().to_owned());
        }
    }
    spec
}

// =============================================================================
// L0 #3 §10 — inline field constraint analyzer tests (Cells D.1+D.2+D.3).
//
// Combination rules per §10.2 (length / between / in conflicts) plus

// =============================================================================
// `conventions [crud]` synthesis pass — Cell C3 tests
//
// Spec: `docs/proposals/ir-resource-conventions-crud.md` §5–§11.
//
// Tests build `ir::Feature` values programmatically because Cell C2's
// parser shim for `conventions [crud]` lands in parallel. The synth
// pass operates on the post-parse IR so direct construction is the

// =============================================================================
// `conventions [me]` synthesis pass — Cell M2 tests
//
// Spec: `docs/proposals/ir-resource-conventions-me.md` §§5–§11.
//
// Tests build `ir::Feature` values programmatically because M1's parser
// shim for `conventions [me]` lands in parallel. The synth pass operates
// on the post-parse IR so direct construction is the canonical surface
// to exercise here.
//
// Coverage:
// - 4 mode tests: user_keyed, user_keyed_no_org, org_keyed, self_keyed.
// - Override test: author wrote `lookup_my_customer` → synth skipped,
//   `synth_origins` records `AuthorOverride(Me)`.
// - Composition test: `conventions [crud, me]` → 6 entries, no collisions.
// - Diagnostic: `MeNoActorResolution` when resource has neither axis.

// =============================================================================
// Cell O2 — `@owner_axis(through: <col>)` synth-pass tests.
//
// Spec: `docs/proposals/ir-resource-conventions-owner-scope.md`
// §7.3 + §8 + §8.5.A + §11.1.
//
// Coverage matrix:
//   1. Mode: owner-scope `delete_*` emits chain WHERE.
//   2. Mode: owner-scope `update_*` / `lookup_*` / `list_*` emit chain WHERE.
//   3. CTE: owner-scope `create_*` emits CTE-INSERT shape via `cte_owner_check`.
//   4. Composition: `[crud, me]` + `@owner_axis` -> `lookup_my_*` ALSO carries scope.
//   5. Diagnostic: `owner_axis_unknown_through`.
//   6. Diagnostic: `owner_axis_through_not_user_keyed`.
//   7. Diagnostic: `owner_axis_collides_with_unique_user`.
//   8. Override: author's `command delete_<r>` skips synth; no diagnostic; scope
//      is NOT attached to the author's command.
//   9. Direct-call form: `build_owner_scope_where_for_test` round-trips the SQL.
//
// RULE-VOCAB-03 affirmation: each test asserts on the *single* SQL shape the

#[cfg(test)]
mod tests;
