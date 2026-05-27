//! Top-level `command <name>` block lowering + the `deprecated` decorator.
//!
//! ## What lives here
//!
//! The orchestrator that turns a whole `syntax::CommandDecl` envelope
//! into an `ir::Command`: the kind inference (`creates|updates|deletes`
//! → `Create|Update|Delete`, no-effect with `returns` → `Returns`), the
//! typed input slot lifter (including the four `validate_constraint_*`
//! gates), the policy / audit / approval / invalidates / external_calls
//! / handler projections.
//!
//! `lower_deprecated` ships with it because `command` and `api` are the
//! two declaration sites that can authored `deprecated since v<N>
//! replacement <ref> [sunset <date>]`. The `DeprecationTarget` discriminator
//! (Command vs Api) controls how unqualified `<name>` replacements
//! disambiguate.
//!
//! ## What does NOT live here
//!
//! The leaf builders that lift a command's body parts — effect, target,
//! let, named-arg, assignment, invalidates-query-ref — stay in
//! `command.rs`. This module is the envelope; that module is the
//! body-internals. Splitting on that line keeps the most "thinking"
//! lowering (the discrimination of kind from body shape) auditable in
//! isolation.

use crate::command::{
    lower_command_effect, lower_invalidates_query_ref, lower_let_binding, lower_named_arg,
    lower_target_expr,
};
use crate::errors::AnalyzeError;
use crate::expr::{
    lower_path_string, lower_policy_atom, lower_policy_expr, lower_translation_key_ref,
};
use crate::feature_meta::{lower_public_contract, lower_route_slot_kind};
use crate::helpers::span_of;
use crate::query::strip_validate_skip;
use crate::resource::{
    lift_field_constraints, lower_rate_limit_spec, validate_constraint_combinations,
    validate_constraint_pattern_compile, validate_constraint_range_invariant,
    validate_constraint_type_compatibility,
};
use crate::types::type_ref_from_text;
use crate::workflow::{lower_external_call, lower_retry};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Phase L Tier 4b — lower a canonical-indent `command` block into
/// `ir::Command`. The kind is inferred from the body shape: `creates`
/// → Create, `updates` → Update, `deletes` → Delete, `returns` → Returns,
/// `handler`-only → Returns (the escape hatch case).
pub(crate) fn lower_command_decl(
    feature: &str,
    c: &syntax::CommandDecl,
) -> Result<ir::Command, AnalyzeError> {
    let kind = match c.effect.as_ref().map(|e| e.kind) {
        Some(syntax::CommandEffectKindDecl::Creates) => ir::CommandKind::Create,
        Some(syntax::CommandEffectKindDecl::Updates) => ir::CommandKind::Update,
        Some(syntax::CommandEffectKindDecl::Deletes) => ir::CommandKind::Delete,
        None => ir::CommandKind::Returns,
    };
    let route = c
        .route
        .iter()
        .map(|r| ir::RouteSlot {
            name: r.name.clone(),
            type_ref: type_ref_from_text(&r.type_text),
            from: r.from.clone(),
            kind: lower_route_slot_kind(r.kind),
        })
        .collect();
    let input = match &c.input {
        syntax::CommandInputDecl::Empty => ir::CommandInput::Empty,
        syntax::CommandInputDecl::Short(name) => ir::CommandInput::Short(vec![name.clone()]),
        syntax::CommandInputDecl::Typed(slots) => {
            // L0 #3 §10.2 — apply combination + default-compat checks
            // to each typed input slot too. Wave-B-CL4 — also run the
            // range / type-compatibility / pattern-compile checks so
            // command inputs aren't a back door past the resource-side
            // diagnostics.
            let mut lifted = Vec::with_capacity(slots.len());
            for s in slots {
                validate_constraint_combinations(&s.name, &s.constraints)?;
                validate_constraint_range_invariant(&s.name, &s.constraints)?;
                let (cleaned, vskip) = strip_validate_skip(&s.type_text);
                validate_constraint_type_compatibility(&s.name, &cleaned, &s.constraints)?;
                validate_constraint_pattern_compile(&s.name, &s.constraints)?;
                lifted.push(ir::TypedSlot {
                    name: s.name.clone(),
                    type_ref: type_ref_from_text(&cleaned),
                    required: s.required,
                    constraints: lift_field_constraints(&s.name, &s.constraints)?,
                    validate_skip: vskip,
                });
            }
            ir::CommandInput::Typed(lifted)
        }
    };
    let target = c.target.as_ref().map(lower_target_expr);
    let lets = c.lets.iter().map(lower_let_binding).collect();
    let effect = if let Some(e) = c.effect.as_ref() {
        lower_command_effect(e)
    } else if let Some(returns) = c.returns.as_deref() {
        ir::CommandEffect::Returns(ir::ReturnsEffect {
            return_type: type_ref_from_text(returns),
        })
    } else {
        ir::CommandEffect::None
    };
    let policy = c
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let emits = c.emits.iter().map(|e| e.name.clone()).collect();
    let audit = c.audit.as_ref().map(|a| ir::AuditSpec {
        subjects: a.subjects.clone(),
        emit_to: a.emit_to.clone(),
        data_subject: a.data_subject.clone(),
        record_before: a.record_before,
        record_after: a.record_after,
        retain_for: a.retain_for.clone(),
    });
    let approval = c.approval.as_ref().map(|a| ir::ApprovalSpec {
        required_when: a.required_when.clone(),
        by: a.by.clone(),
        timeout: a.timeout.clone(),
        then: match a.then {
            syntax::ApprovalThenDecl::Deny => ir::ApprovalThen::Deny,
            syntax::ApprovalThenDecl::Allow => ir::ApprovalThen::Allow,
            syntax::ApprovalThenDecl::Escalate => ir::ApprovalThen::Escalate,
        },
    });
    let invalidates = c
        .invalidates
        .iter()
        .map(|inv| ir::InvalidatesSpec {
            query: lower_invalidates_query_ref(feature, &inv.query),
            args: inv.args.iter().map(lower_named_arg).collect(),
        })
        .collect();
    let external_calls = c.external_calls.iter().map(lower_external_call).collect();
    let deprecated = c
        .deprecated
        .as_ref()
        .map(|dep| lower_deprecated(dep, DeprecationTarget::Command));
    // Phase L Tier 4 follow-up — lift `timeout`/`retry`/`idempotency by`
    // mirrors of `parse_job`. Doctor cross-checks against
    // `external_calls` for the `INT-CALL-*` integration coverage rules.
    let timeout = c.timeout.clone();
    let retry = c.retry.as_ref().map(lower_retry);
    let idempotency = c
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let write_window = c.write_window.as_ref().map(|w| ir::CommandWriteWindow {
        by: lower_path_string(&w.by),
        within: w.within.clone(),
        span_ref: Some(span_of(w.span)),
    });
    let policy_expr = c.policy_expr.as_ref().map(lower_policy_expr);
    // WAR-RUNTIME-COMMAND-01 (Effect half): lift `handler @fn.<name>`
    // into the typed `HandlerRef`. `handler "./path.go"` (file escape
    // hatch) lifts as namespace=`path`, name=verbatim path. Codegen uses
    // the `fn` form to auto-wire `Effect: lazuli.Returns(...)` when the
    // declarative body has no other effect.
    let handler = c.handler.as_ref().map(|h| {
        let path = h.path.trim();
        if let Some(name) = path.strip_prefix("@fn.") {
            ir::HandlerRef {
                namespace: "fn".to_owned(),
                name: name.to_owned(),
                span_ref: Some(span_of(c.span)),
            }
        } else {
            ir::HandlerRef {
                namespace: "path".to_owned(),
                name: path.to_owned(),
                span_ref: Some(span_of(c.span)),
            }
        }
    });
    // IR Error-Vocab (Cell PARSE-1) — lift the optional `when_denied
    // @translation.<key>` child captured by the parser under `policy`.
    // Resolution-chain step 1 (proposal §2.A).
    let policy_when_denied = c.policy_when_denied.as_ref().map(lower_translation_key_ref);
    Ok(ir::Command {
        name: c.name.clone(),
        public_contract: lower_public_contract(&c.public_contract),
        kind,
        route,
        input,
        target,
        lets,
        effect,
        policy,
        policy_expr,
        policy_when_denied,
        emits,
        rate_limit: c.rate_limit.as_ref().map(lower_rate_limit_spec),
        audit,
        approval,
        invalidates,
        external_calls,
        timeout,
        retry,
        idempotency,
        write_window,
        deprecated,
        handler,
        tests: None,
        triggers: c.triggers.clone(),
        synthesized_from_cap_file: None,
        // owner-scope §7.3 — author-written commands default to
        // tenant-only. The synth pass mutates the slot post-build for
        // crud / me bundle outputs when the resource carries
        // `@owner_axis`.
        owner_scope_sql: None,
        previous_names: c.previously.clone(),
        span_ref: Some(span_of(c.span)),
        derived_from: None,
    })
}

#[derive(Clone, Copy)]
pub(crate) enum DeprecationTarget {
    Command,
    Api,
}

/// OpenAPI bucket cycle — lower an authored `deprecated` decorator into
/// the typed IR shape. `replacement` is classified by syntactic shape:
/// `https?://` → Url, `[<feature>.]command.<name>` / `[<feature>.]api.<name>`
/// → typed callable ref, otherwise → same-kind local ref.
pub(crate) fn lower_deprecated(
    decl: &syntax::CommandDeprecatedDecl,
    target: DeprecationTarget,
) -> ir::Deprecation {
    let replacement = decl.replacement.as_ref().map(|raw| {
        let trimmed = raw.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            ir::DeprecationReplacement::Url(trimmed.to_owned())
        } else if let Some(stripped) = trimmed.strip_prefix("@") {
            // `@adapter.command.<name>` or similar — store as Url-style
            // verbatim escape hatch.
            ir::DeprecationReplacement::Url(format!("@{}", stripped))
        } else {
            let parts: Vec<&str> = trimmed.split('.').collect();
            if parts.len() == 2 && parts[0] == "command" {
                ir::DeprecationReplacement::LocalCommand(parts[1].to_owned())
            } else if parts.len() == 2 && parts[0] == "api" {
                ir::DeprecationReplacement::LocalApi(parts[1].to_owned())
            } else if parts.len() == 3 && parts[1] == "command" {
                ir::DeprecationReplacement::Qualified(ir::QualifiedName {
                    feature: Some(parts[0].to_owned()),
                    name: parts[2].to_owned(),
                })
            } else if parts.len() == 3 && parts[1] == "api" {
                ir::DeprecationReplacement::QualifiedApi(ir::QualifiedName {
                    feature: Some(parts[0].to_owned()),
                    name: parts[2].to_owned(),
                })
            } else {
                match target {
                    DeprecationTarget::Command => {
                        ir::DeprecationReplacement::LocalCommand(trimmed.to_owned())
                    }
                    DeprecationTarget::Api => {
                        ir::DeprecationReplacement::LocalApi(trimmed.to_owned())
                    }
                }
            }
        }
    });
    ir::Deprecation {
        since: decl.since.clone(),
        replacement,
        sunset: decl.sunset.clone(),
    }
}
