//! Tests for the `verify none` webhook security opt-out — an inbound webhook
//! may intentionally skip signature verification (verified at a gateway, or
//! genuinely internal) but must justify it with a `reason "..."` child. Mirrors
//! the `scope global` escape hatch. The reason is required by the LSP security
//! rule but optional to the parser; exactly one of `verify` / `verify_none` is
//! set, and a webhook with neither is rejected.
//!
//! Co-located with `webhook/mod.rs` as a sibling per the ≤500-LOC rule.

#![cfg(test)]

use super::super::parse_feature_skeletons;

#[test]
fn verify_none_with_reason_parses_as_opt_out() {
    let source = r#"feature billing
  webhook payment_invoice_paid
    path "/webhooks/payments/invoice-paid"
    verify none
      reason "internal webhook, signature verified at the API gateway"
    tenant_from payload.org_id
    idempotency by payload.org_id, payload.provider_event_id
    handler "./integrations/record.go" returns BillingWebhook
    emits invoice_paid
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let webhook = &features[0].webhooks[0];
    assert!(
        webhook.verify.is_none(),
        "`verify none` yields no structured verify spec"
    );
    let opt_out = webhook
        .verify_none
        .as_ref()
        .expect("`verify_none` is populated for the opt-out");
    assert_eq!(
        opt_out.reason.as_deref(),
        Some("internal webhook, signature verified at the API gateway")
    );
}

#[test]
fn verify_none_without_reason_still_parses() {
    let source = r#"feature billing
  webhook payment_invoice_paid
    path "/webhooks/payments/invoice-paid"
    verify none
    tenant_from payload.org_id
    idempotency by payload.org_id, payload.provider_event_id
    handler "./integrations/record.go" returns BillingWebhook
    emits invoice_paid
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let webhook = &features[0].webhooks[0];
    assert!(webhook.verify.is_none());
    assert!(webhook.verify_none.is_some());
    assert!(webhook.verify_none.as_ref().unwrap().reason.is_none());
}

#[test]
fn verify_hmac_still_parses_as_structured() {
    let source = r#"feature billing
  webhook payment_invoice_paid
    path "/webhooks/payments/invoice-paid"
    verify hmac sha256
      secret env.PAYMENT_WEBHOOK_SECRET
      header "X-Signature"
    tenant_from payload.org_id
    idempotency by payload.org_id, payload.provider_event_id
    handler "./integrations/record.go" returns BillingWebhook
    emits invoice_paid
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let webhook = &features[0].webhooks[0];
    assert!(webhook.verify.is_some(), "hmac yields a structured verify");
    assert!(webhook.verify_none.is_none());
}

#[test]
fn webhook_without_any_verify_is_rejected() {
    let source = r#"feature billing
  webhook payment_invoice_paid
    path "/webhooks/payments/invoice-paid"
    tenant_from payload.org_id
    idempotency by payload.org_id, payload.provider_event_id
    handler "./integrations/record.go" returns BillingWebhook
    emits invoice_paid
"#;
    assert!(
        parse_feature_skeletons(source).is_err(),
        "a webhook must declare `verify hmac <alg>` or an explicit `verify none`"
    );
}
