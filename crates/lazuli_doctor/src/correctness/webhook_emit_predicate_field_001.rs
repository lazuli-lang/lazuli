//! WEBHOOK-EMIT-PREDICATE-FIELD-001 — webhook `emits ... when <path> = ...`
//! references a path that does not resolve against the webhook's
//! payload contract.
//!
//! B5 framework gap 2 — when a webhook authors a per-branch emit
//! binding like:
//!
//! ```text
//! emits charge_confirmed when payload.status = "approved"
//! ```
//!
//! and `payload.status` is not declared on the webhook's payload
//! contract (`payload from webhook_events.<name>`), the runtime
//! dispatch table cannot evaluate the predicate. We fail fast on the
//! authoring side rather than letting the receiver no-op at runtime.
//!
//! Scope: only typed predicate shapes (`Equals` / `In`) are checked;
//! `EmitPredicateKind::Other` is opaque on purpose and skipped.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Webhook, WebhookEvent, WebhookEventField};

/// One WEBHOOK-EMIT-PREDICATE-FIELD-001 finding — a webhook's `emit
/// when` predicate references a payload field that isn't on the
/// webhook's declared typed-payload contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending webhook lives in.
    pub path: PathBuf,
    /// Feature owning the webhook.
    pub feature: String,
    /// Webhook name (`webhook <name>`).
    pub webhook: String,
    /// Emit clause's event name the predicate gates.
    pub emit_event: String,
    /// Dotted path the predicate referenced (e.g. `card.brand`).
    pub field_path: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "WEBHOOK-EMIT-PREDICATE-FIELD-001";

    /// Render the "field not declared on payload contract" message
    /// naming the webhook, emit event, and missing dotted path.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::webhook_emit_predicate_field_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     webhook: "stripe".into(),
    ///     emit_event: "payment.captured".into(),
    ///     field_path: "card.brand".into(),
    /// };
    /// assert!(f.message().contains("payload contract"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "Webhook `{}` emits `{}` when `{}` but `{}` is not declared on the webhook payload contract.",
            self.webhook, self.emit_event, self.field_path, self.field_path
        )
    }
}

/// Run WEBHOOK-EMIT-PREDICATE-FIELD-001 against a feature. Looks up
/// each webhook's `payload_from` against the registry-side
/// `webhook_events` index to resolve the typed field catalog. When the
/// webhook does not declare a typed payload contract the rule is a
/// no-op (the runtime dispatch table degrades to opaque envelope
/// access; doctor doesn't gate on that until the typed catalog
/// exists).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::webhook_emit_predicate_field_001::check;
/// use lazuli_ir::{Feature, WebhookEvent};
///
/// let feature: Feature = unimplemented!("lower a feature with webhooks");
/// let events: Vec<WebhookEvent> = vec![];
/// let _ = check(&feature, &events, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, webhook_events: &[WebhookEvent], file_path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for webhook in &feature.webhooks {
        check_webhook(feature, webhook, webhook_events, file_path, &mut out);
    }
    out
}

fn check_webhook(
    feature: &Feature,
    webhook: &Webhook,
    webhook_events: &[WebhookEvent],
    file_path: &Path,
    out: &mut Vec<Finding>,
) {
    if webhook.emit_predicates.is_empty() {
        return;
    }
    // Resolve the webhook's typed payload contract once per webhook.
    let payload_fields: Option<&[WebhookEventField]> =
        webhook.payload_from.as_ref().and_then(|payload_ref| {
            webhook_events
                .iter()
                .find(|we| we.name == payload_ref.name)
                .map(|we| we.payload.as_slice())
        });

    let Some(payload_fields) = payload_fields else {
        // Webhook did not declare a typed payload contract; cannot
        // resolve predicate paths against an opaque envelope. Skip.
        return;
    };

    for (idx, predicate) in webhook.emit_predicates.iter().enumerate() {
        let Some(predicate) = predicate.as_ref() else {
            continue;
        };
        let Some(path) = predicate.payload_path() else {
            // Opaque `Other` predicate — runtime evaluator is the
            // author's responsibility; doctor stays silent.
            continue;
        };
        let head = leading_segment(path);
        if !payload_field_exists(payload_fields, head) {
            let emit_event = webhook
                .emits
                .get(idx)
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_owned());
            out.push(Finding {
                path: file_path.to_path_buf(),
                feature: feature.name.clone(),
                webhook: webhook.name.clone(),
                emit_event,
                field_path: path.to_owned(),
            });
        }
        // Touch `predicate` so the doc-comment example compiles even
        // when this branch is the only consumer.
        let _ = predicate;
    }
}

/// Leading segment of a dotted payload path. `payload.status` -> `payload`,
/// `status` -> `status`. The receiver-side dispatch table resolves the
/// remainder against the parsed JSON envelope; the typed lift only
/// gates the root attribute.
fn leading_segment(path: &str) -> &str {
    path.split_once('.').map(|(head, _)| head).unwrap_or(path)
}

fn payload_field_exists(fields: &[WebhookEventField], name: &str) -> bool {
    // Accept either a literal field match or the synthetic `payload`
    // root prefix (the receiver wraps every parsed envelope under
    // `payload.*` so `payload.status` is valid when `status` is on
    // the contract).
    if name == "payload" {
        return true;
    }
    fields.iter().any(|f| f.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        EmitPredicate, EmitPredicateKind, PathRef, VerifyScheme, VerifySpec, Webhook, WebhookEvent,
        WebhookEventField, WebhookEventRef,
    };
    use std::path::PathBuf;

    fn mk_predicate_equals(path: &str, literal: &str) -> EmitPredicate {
        EmitPredicate {
            raw: format!("{} = \"{}\"", path, literal),
            kind: EmitPredicateKind::Equals {
                path: path.to_owned(),
                literal: literal.to_owned(),
            },
            span_ref: None,
        }
    }

    fn mk_webhook(name: &str) -> Webhook {
        Webhook {
            name: name.to_owned(),
            route: format!("/webhooks/{}", name),
            verify: PathRef::convention(format!("./webhooks/{}_verify.go", name)),
            structured_verify: Some(VerifySpec {
                scheme: VerifyScheme::Hmac,
                algorithm: "sha256".to_owned(),
                secret_env: "SECRET".to_owned(),
                header: "x-signature".to_owned(),
            }),
            tenant_from: None,
            scope_global: None,
            idempotency: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            handler: PathRef::authored(format!("./webhooks/{}.go", name)),
            returns: None,
            emits: Vec::new(),
            emit_predicates: Vec::new(),
            payload_from: None,
            replay: None,
            dlq: None,
            retry: None,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn mk_feature(webhooks: Vec<Webhook>) -> Feature {
        lazuli_ir::Feature {
            name: "payments".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            knowledge: None,
            defaults: lazuli_ir::Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: lazuli_ir::Policies::default(),
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks,
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn webhook_event_with_status() -> WebhookEvent {
        WebhookEvent {
            name: "mp_payment".to_owned(),
            payload: vec![WebhookEventField {
                name: "status".to_owned(),
                type_text: "Text".to_owned(),
                required: true,
                capabilities: Vec::new(),
            }],
            version: 1,
            previous_version: None,
            deprecated: false,
            span_ref: None,
        }
    }

    #[test]
    fn predicate_field_resolved_against_payload_contract_passes() {
        let mut webhook = mk_webhook("mp_payment_event");
        webhook.payload_from = Some(WebhookEventRef {
            name: "mp_payment".to_owned(),
        });
        webhook.emits = vec!["charge_confirmed".to_owned()];
        webhook.emit_predicates = vec![Some(mk_predicate_equals("status", "approved"))];
        let feature = mk_feature(vec![webhook]);
        let webhook_events = vec![webhook_event_with_status()];
        let findings = check(&feature, &webhook_events, &PathBuf::from("payments.lzi"));
        assert!(findings.is_empty(), "no findings expected: {:?}", findings);
    }

    #[test]
    fn predicate_field_not_on_payload_contract_fires() {
        let mut webhook = mk_webhook("mp_payment_event");
        webhook.payload_from = Some(WebhookEventRef {
            name: "mp_payment".to_owned(),
        });
        webhook.emits = vec!["charge_confirmed".to_owned()];
        webhook.emit_predicates = vec![Some(mk_predicate_equals("not_a_field", "approved"))];
        let feature = mk_feature(vec![webhook]);
        let webhook_events = vec![webhook_event_with_status()];
        let findings = check(&feature, &webhook_events, &PathBuf::from("payments.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].emit_event, "charge_confirmed");
        assert_eq!(findings[0].field_path, "not_a_field");
    }

    #[test]
    fn webhook_without_typed_payload_contract_is_skipped() {
        let mut webhook = mk_webhook("mp_payment_event");
        webhook.emits = vec!["charge_confirmed".to_owned()];
        webhook.emit_predicates = vec![Some(mk_predicate_equals("status", "approved"))];
        let feature = mk_feature(vec![webhook]);
        let webhook_events: Vec<WebhookEvent> = Vec::new();
        let findings = check(&feature, &webhook_events, &PathBuf::from("payments.lzi"));
        assert!(findings.is_empty());
    }
}
