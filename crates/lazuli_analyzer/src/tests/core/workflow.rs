use lazuli_ir as ir;

use lazuli_syntax::{parse_feature_skeletons, parse_lzx_document};

use crate::auth::lower_auth_identity;
use crate::query::parse_query_filter_line;
use crate::resource::lower_validate_line;
use crate::{
    AnalyzeError, lower_audit_block, lower_feature_skeleton, lower_lzx_document,
    lower_policy_atom_with_args, parse_cap_file_type, resolve_invalidates_targets,
    type_ref_from_syntax,
};

// -------------------------------------------------------------------------
// Phase L Tier 3 — job / webhook / notification / event_group lowering
// -------------------------------------------------------------------------

#[test]
fn lower_tier3_job_handler_full_block() {
    let source = r#"
feature customer
  job process_import
    trigger event customer_import_uploaded
    queue customer_imports
    tenant_from payload.org_id
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
      org_id = payload.org_id
    timeout "30s"
    handler "./jobs/process_import.go"
    emits customer_import_completed
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    assert_eq!(feature.jobs.len(), 1);
    let job = &feature.jobs[0];
    assert_eq!(job.name, "process_import");
    assert_eq!(job.queue.as_deref(), Some("customer_imports"));
    assert_eq!(job.timeout.as_deref(), Some("30s"));
    let tenant = job.tenant_from.as_ref().expect("tenant_from");
    assert_eq!(tenant.path.segments, vec!["payload", "org_id"]);
    let retry = job.retry.as_ref().expect("retry");
    assert_eq!(retry.count, 3);
    assert!(matches!(retry.backoff, ir::BackoffStrategy::Exponential));
    assert_eq!(job.external_calls.len(), 1);
    assert_eq!(job.external_calls[0].slot, "crm");
    assert_eq!(job.external_calls[0].op, "normalize_import_batch");
    assert_eq!(job.external_calls[0].args.len(), 2);
    match &job.body {
        ir::JobBody::Handler(h) => {
            assert_eq!(h.path.path, "./jobs/process_import.go");
        }
        other => panic!("expected Handler body, got {other:?}"),
    }
    assert_eq!(job.emits, vec!["customer_import_completed"]);
}

#[test]
fn lower_tier3_job_declarative_carve_out() {
    let source = r#"
feature customer
  job recompute_score_after_invoice
    trigger event billing.invoice_paid
    tenant_from payload.org_id
    idempotency by envelope.id
    target query.by_id(id: payload.customer_id)
    let new_score = @fn.risk_score(target)
    updates Customer
      score = new_score
    emits customer_score_recomputed
      score = new_score
      reason = "invoice_paid"
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    assert_eq!(feature.jobs.len(), 1);
    let job = &feature.jobs[0];
    match &job.body {
        ir::JobBody::Declarative(d) => {
            let target = d.target.as_ref().expect("target lifted");
            assert_eq!(target.query.name, "by_id");
            assert_eq!(d.lets.len(), 1);
            assert_eq!(d.lets[0].name, "new_score");
            match &d.effect {
                ir::CommandEffect::Updates(u) => {
                    assert_eq!(u.resource.name, "Customer");
                    assert_eq!(u.assignments.len(), 1);
                    assert_eq!(u.assignments[0].field, "score");
                }
                other => panic!("expected Updates effect, got {other:?}"),
            }
        }
        other => panic!("expected Declarative body, got {other:?}"),
    }
}

#[test]
fn lower_tier3_webhook_structured_verify() {
    let source = r#"
feature customer
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    handler "./integrations/upsert_customer_from_crm.go" returns Customer
    emits customer_webhook_received
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    assert_eq!(feature.webhooks.len(), 1);
    let webhook = &feature.webhooks[0];
    assert_eq!(webhook.route, "/webhooks/crm/customer-upsert");
    let verify = webhook
        .structured_verify
        .as_ref()
        .expect("structured verify");
    assert!(matches!(verify.scheme, ir::VerifyScheme::Hmac));
    assert_eq!(verify.algorithm, "sha256");
    assert_eq!(verify.secret_env, "CRM_WEBHOOK_SECRET");
    assert_eq!(verify.header, "X-CRM-Signature");
    let tenant = webhook.tenant_from.as_ref().expect("tenant_from");
    assert_eq!(tenant.path.segments, vec!["payload", "org_id"]);
    assert_eq!(
        webhook.handler.path,
        "./integrations/upsert_customer_from_crm.go"
    );
    assert_eq!(webhook.emits, vec!["customer_webhook_received"]);
}

#[test]
fn lower_tier3_notification_full_block() {
    let source = r#"
feature customer_outreach
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    tenant_from payload.org_id
    idempotency by envelope.id
    retry 3 backoff exponential
    template "./outreach/welcome_email.mjml"
    policy @policy.notify
    emits welcome_email_sent
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    assert_eq!(feature.notifications.len(), 1);
    let n = &feature.notifications[0];
    assert_eq!(n.name, "welcome_email");
    assert_eq!(n.channels, vec!["email"]);
    assert_eq!(n.recipient, "target.email");
    assert_eq!(n.template, "./outreach/welcome_email.mjml");
    match &n.trigger {
        ir::JobTrigger::Event { event } => {
            assert_eq!(event.feature.as_deref(), Some("customer"));
            assert_eq!(event.name, "customer_activated");
        }
        other => panic!("expected Event trigger, got {other:?}"),
    }
    assert_eq!(n.emits, vec!["welcome_email_sent"]);
}

#[test]
fn lower_tier3_event_group_payload_and_events() {
    let source = r#"
feature customer
  event_group customer_* on Customer
    payload
      customer_id = id
      org_id = org.id
    event created
    event activated
    event archived
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    assert_eq!(feature.event_groups.len(), 1);
    let group = &feature.event_groups[0];
    assert_eq!(group.pattern, "customer_*");
    assert_eq!(group.on_resource.as_deref(), Some("Customer"));
    assert_eq!(group.raw_payload.len(), 2);
    assert_eq!(
        group.events,
        vec![
            "created".to_owned(),
            "activated".to_owned(),
            "archived".to_owned()
        ]
    );
}

/// B5 framework gap 1 — per-event typed payload field bodies are
/// lifted into `EventGroup.variants`. The legacy `events: Vec<String>`
/// slot still holds the name list (back-compat), and each variant
/// carries its `EventField`s, kind, and outbox flag.
#[test]
fn lower_event_group_lifts_per_event_typed_payload_fields() {
    let source = r#"
feature payments
  event_group charge_* on Charge
    payload
      charge_id = id
    event requested
      outbox guaranteed
      amount: @semantic.Money
      host_id: ID
    event confirmed
      outbox guaranteed
      amount: @semantic.Money
      provider_payment_id: Text
      paid_at: DateTime
    event.trace mp_status_received
      provider_status: Text
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let group = &feature.event_groups[0];
    assert_eq!(group.variants.len(), 3, "three variants under group");

    // Variant 0 — requested
    let requested = &group.variants[0];
    assert_eq!(requested.name, "requested");
    assert!(matches!(requested.kind, ir::EventVariantKind::Committed));
    assert!(requested.outbox.is_guaranteed());
    assert_eq!(requested.fields.len(), 2);
    assert_eq!(requested.fields[0].name, "amount");
    assert_eq!(requested.fields[1].name, "host_id");

    // Variant 1 — confirmed
    let confirmed = &group.variants[1];
    assert_eq!(confirmed.name, "confirmed");
    assert_eq!(confirmed.fields.len(), 3);
    let names: Vec<&str> = confirmed.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["amount", "provider_payment_id", "paid_at"]);

    // Variant 2 — trace
    let trace = &group.variants[2];
    assert_eq!(trace.name, "mp_status_received");
    assert!(matches!(trace.kind, ir::EventVariantKind::Trace));
    assert!(trace.outbox.is_none());
    assert_eq!(trace.fields.len(), 1);
    assert_eq!(trace.fields[0].name, "provider_status");
}

/// B5 framework gap 1 — `event foo` (no body) still parses and
/// lowers cleanly. The variant comes through with an empty
/// `fields` Vec so the legacy `Feature.events` lookup path stays
/// in charge of the typed projection.
#[test]
fn lower_event_group_back_compat_empty_event_bodies() {
    let source = r#"
feature customer
  event_group customer_* on Customer
    payload
      customer_id = id
    event created
    event archived
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let group = &feature.event_groups[0];
    assert_eq!(group.variants.len(), 2);
    for variant in &group.variants {
        assert!(variant.fields.is_empty());
        assert!(matches!(variant.kind, ir::EventVariantKind::Committed));
    }
}

/// B5 framework gap 2 — `webhook ... emits foo when <predicate>`
/// lifts the per-branch `when` clause into a typed `EmitPredicate`.
#[test]
fn lower_webhook_with_when_predicates_typed_lift() {
    let source = r#"
feature payments
  webhook mp_payment_event
    path "/webhooks/mp/payment"
    verify hmac sha256
      secret env.MERCADOPAGO_WEBHOOK_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_mp_payment_event
    emits charge_confirmed when payload.status == "approved"
    emits charge_failed when payload.status in ("rejected", "cancelled")
    emits mp_status_received
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let webhook = &feature.webhooks[0];
    assert_eq!(
        webhook.emits,
        vec![
            "charge_confirmed".to_owned(),
            "charge_failed".to_owned(),
            "mp_status_received".to_owned()
        ]
    );
    assert_eq!(webhook.emit_predicates.len(), 3);

    // [0] equals
    let approved = webhook.emit_predicates[0]
        .as_ref()
        .expect("first emit has predicate");
    match &approved.kind {
        ir::EmitPredicateKind::Equals { path, literal } => {
            assert_eq!(path, "payload.status");
            assert_eq!(literal, "approved");
        }
        other => panic!("expected Equals, got {:?}", other),
    }

    // [1] in
    let failed = webhook.emit_predicates[1]
        .as_ref()
        .expect("second emit has predicate");
    match &failed.kind {
        ir::EmitPredicateKind::In { path, literals } => {
            assert_eq!(path, "payload.status");
            assert_eq!(
                literals,
                &vec!["rejected".to_owned(), "cancelled".to_owned()]
            );
        }
        other => panic!("expected In, got {:?}", other),
    }

    // [2] no predicate (default branch)
    assert!(webhook.emit_predicates[2].is_none());
}

/// B5 framework gap 2 back-compat — the flat `emits foo` /
/// `emits bar` shape (no predicates) leaves `emit_predicates`
/// empty so the generated `WebhookContract` stays on the legacy
/// `Emits []string{}` shape.
#[test]
fn lower_webhook_without_when_predicates_keeps_legacy_emits_shape() {
    let source = r#"
feature payments
  webhook mp_payment_event
    path "/webhooks/mp/payment"
    verify hmac sha256
      secret env.MERCADOPAGO_WEBHOOK_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_mp_payment_event
    emits charge_confirmed
    emits charge_failed
"#;
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let webhook = &feature.webhooks[0];
    assert_eq!(webhook.emits.len(), 2);
    assert!(
        webhook.emit_predicates.is_empty(),
        "no `when` clauses means no per-branch dispatch"
    );
}
