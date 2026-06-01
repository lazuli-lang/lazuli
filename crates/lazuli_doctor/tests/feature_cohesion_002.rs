//! Spec 0008 — `LZI-FEATURE-COHESION-002` + `LZI-FILE-SIZE-001` re-key.
//!
//! The 8 TDD cases named in the techspec, plus the fixtures it
//! specifies (`disconnected.lzi` modeled on hostpoint `platform.lzi`;
//! `connected.lzi` modeled on `account.lzi`).

use std::path::Path;

use lazuli_doctor::lzi_hygiene::feature_cohesion_002::{self, InfoKind, LoweredFeature};
use lazuli_doctor::lzi_hygiene::file_size_001;
use lazuli_ir::Feature;

fn lower(source: &str) -> Feature {
    let skeletons =
        lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
    lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
}

fn fixture_source(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cohesion")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

// ── LZI-FEATURE-COHESION-002 ────────────────────────────────────────────────

#[test]
fn disconnected_feature_fires() {
    let src = fixture_source("disconnected.lzi");
    let feature = lower(&src);
    let findings = feature_cohesion_002::check(&[LoweredFeature::new(
        "features/cohesion/disconnected.lzi",
        &feature,
        &src,
    )]);
    assert_eq!(findings.len(), 1, "platform-shaped fixture must fire");
    assert_eq!(
        findings[0].clusters.len(),
        3,
        "LegalDoc | PlatformConfig | DataRequest = 3 isolated clusters: {:?}",
        findings[0].clusters
    );
    let msg = findings[0].message();
    assert!(msg.contains("LegalDoc"));
    assert!(msg.contains("PlatformConfig"));
    assert!(msg.contains("DataRequest"));
    assert!(msg.contains("split the feature"));
}

#[test]
fn connected_feature_silent() {
    // account-shaped fixture: one connected graph (all FK to Account).
    let src = fixture_source("connected.lzi");
    let feature = lower(&src);
    assert!(
        feature_cohesion_002::check(&[LoweredFeature::new(
            "features/cohesion/connected.lzi",
            &feature,
            &src
        )])
        .is_empty(),
        "a single connected capability must not fire"
    );
}

#[test]
fn single_resource_silent() {
    let src = "feature solo\n  resource Widget\n    label: Text required\n";
    let feature = lower(src);
    assert!(
        feature_cohesion_002::check(&[LoweredFeature::new(
            "features/solo/solo.lzi",
            &feature,
            src
        )])
        .is_empty()
    );
}

#[test]
fn cross_feature_fk_is_not_an_edge() {
    // Two same-feature clusters; one resource carries a cross-feature FK
    // to a `uses`d other-feature resource. That cross-feature pointer
    // must NOT connect the two intra-feature clusters → still ≥2
    // components → still fires.
    let src = "feature mix\n  \
         uses billing\n  \
         resource Ticket\n    \
         subject: Text required\n    \
         invoice: ID target @feature.billing.Invoice\n  \
         resource Banner\n    \
         text: Text required\n";
    let feature = lower(src);
    let findings =
        feature_cohesion_002::check(&[LoweredFeature::new("features/mix/mix.lzi", &feature, src)]);
    assert_eq!(
        findings.len(),
        1,
        "cross-feature FK must not bridge intra-feature clusters: {findings:?}"
    );
    assert_eq!(findings[0].clusters.len(), 2);
}

// ── Event-group emit-coupling (spec 0008 follow-up) ──────────────────────────

/// Payments-shaped fixture: `Charge` + `WebhookEvent` are two FK-less
/// islands, but a `webhook` emits `charge_*` events declared under
/// `event_group charge_* on Charge`, and `WebhookEvent` is the webhook's
/// inbound-envelope log. The emit-coupling edge (webhook ↔ Charge via the
/// group's `on_resource`) plus the webhook-envelope-sink edge (WebhookEvent
/// ↔ Charge) must collapse them into ONE component so payments no longer
/// false-fires. `MercadoPagoAccount` stays its own island (touched only by
/// a separate command) — that residual is acceptable.
#[test]
fn payments_shaped_event_group_couples_charge_webhookevent() {
    let src = r#"
feature payments
  resource MercadoPagoAccount
    mp_user_id: Text required
  resource Charge
    amount: Text required
  resource WebhookEvent
    external_id: Text required
  event_group charge_* on Charge
    payload
      charge_id = id
    event confirmed
      provider_payment_id: Text
    event failed
      reason: Text
  command connect_mercadopago
    input
      authorization_code: Text required
    handler @fn.connect_mercadopago
  webhook mp_payment_event
    path "/webhooks/mp/payment"
    verify hmac sha256
      secret env.MERCADOPAGO_WEBHOOK_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_mp_payment_event
    emits charge_confirmed when payload.status == "approved"
    emits charge_failed when payload.status in ("rejected", "cancelled")
"#;
    let feature = lower(src);
    let findings = feature_cohesion_002::check(&[LoweredFeature::new(
        "features/payments/payments.lzi",
        &feature,
        src,
    )]);
    // Charge + WebhookEvent must now be in the SAME component. The only
    // possible residual island is MercadoPagoAccount, so either the
    // feature is silent (all three connected — not expected here) or it
    // fires with Charge and WebhookEvent sharing one cluster.
    if let Some(f) = findings.first() {
        let charge_cluster = f
            .clusters
            .iter()
            .find(|c| c.iter().any(|n| n == "Charge"))
            .expect("a cluster contains Charge");
        assert!(
            charge_cluster.iter().any(|n| n == "WebhookEvent"),
            "Charge and WebhookEvent must share a component: {:?}",
            f.clusters
        );
    }
}

/// Regression guard: the existing platform-shaped grab-bag fixture
/// (`disconnected.lzi`: 3 FK-less resources, NO event_group / webhook /
/// emits) MUST still fire with ≥2 components. The emit-coupling edge must
/// not mask genuine grab-bags.
#[test]
fn true_grabbag_still_fires() {
    let src = fixture_source("disconnected.lzi");
    let feature = lower(&src);
    let findings = feature_cohesion_002::check(&[LoweredFeature::new(
        "features/cohesion/disconnected.lzi",
        &feature,
        &src,
    )]);
    assert_eq!(
        findings.len(),
        1,
        "platform-shaped grab-bag must still fire after the emit-coupling fix"
    );
    assert!(
        findings[0].clusters.len() >= 2,
        "genuine grab-bag must keep ≥2 disconnected clusters: {:?}",
        findings[0].clusters
    );
}

/// The `charge_*` glob must prefix-match `charge_confirmed` (coupling the
/// emitter to the group's `on_resource`) but NOT `refund_started` (a name
/// outside the group). Modeled at the component level: two FK-less
/// resources `Charge` + `Audit`, an `event_group charge_* on Charge`, a
/// webhook that emits ONLY `refund_started` (no glob match) → the webhook
/// couples to nothing, so the two resources stay in separate components.
#[test]
fn event_group_glob_matches_prefix() {
    let src = r#"
feature billing
  resource Charge
    amount: Text required
  resource Audit
    note: Text required
  event_group charge_* on Charge
    payload
      charge_id = id
    event confirmed
      provider_payment_id: Text
  webhook stray
    path "/webhooks/stray"
    verify hmac sha256
      secret env.STRAY_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_stray
    emits refund_started
"#;
    let feature = lower(src);
    let findings = feature_cohesion_002::check(&[LoweredFeature::new(
        "features/billing/billing.lzi",
        &feature,
        src,
    )]);
    // `refund_started` does not match `charge_*`, so no emit-coupling
    // edge forms → Charge and Audit remain two components → still fires.
    assert_eq!(
        findings.len(),
        1,
        "a non-matching emit must not couple unrelated resources: {findings:?}"
    );
    assert_eq!(findings[0].clusters.len(), 2);
}

// ── LZI-FILE-SIZE-001 re-key ────────────────────────────────────────────────

#[test]
fn file_size_rekeyed_off_loc() {
    // High LOC, low (resource × effect) → silent.
    let mut low_surface = String::from("feature thin\n  resource Only\n    label: Text required\n");
    for _ in 0..700 {
        low_surface.push_str("  # padding comment line\n");
    }
    let thin = lower(&low_surface);
    assert!(
        file_size_001::check(&[LoweredFeature::new(
            "features/thin/thin.lzi",
            &thin,
            &low_surface
        )])
        .is_empty(),
        "700-LOC but low (resource × effect) must stay silent"
    );

    // High (resource × effect) → fires (LOC not the driver). Declarations
    // grouped (resources, then commands, then queries) per grammar.
    let mut high_surface = String::from("feature wide\n");
    for i in 0..8 {
        high_surface.push_str(&format!("  resource Res{i}\n    label: Text required\n"));
    }
    for i in 0..8 {
        high_surface.push_str(&format!("  command create_res{i}\n    creates Res{i}\n"));
    }
    for i in 0..8 {
        high_surface.push_str(&format!("  query.list list_res{i}\n"));
    }
    for i in 0..8 {
        high_surface.push_str(&format!("  query.lookup get_res{i} by id: ID\n"));
    }
    let wide = lower(&high_surface);
    let findings = file_size_001::check(&[LoweredFeature::new(
        "features/wide/wide.lzi",
        &wide,
        &high_surface,
    )]);
    assert_eq!(findings.len(), 1, "high (resource × effect) must fire");
    assert!(findings[0].resource_effect_pairs > file_size_001::RESOURCE_EFFECT_THRESHOLD);
}

#[test]
fn file_size_is_warn() {
    // The aggregator passes DoctorSeverity::Warning as the default for
    // LZI-FILE-SIZE-001 (demoted from preset-escalated). Assert the
    // contract via the preset module: outside a preset the rule default
    // is Warning, never Error.
    use lazuli_doctor::lzi_hygiene::preset::{LziHygienePreset, preset_rule_severity};
    // mature → no preset opinion, the per-rule default (Warning) applies.
    assert_eq!(
        preset_rule_severity(LziHygienePreset::TddMature, file_size_001::Finding::CODE),
        None,
        "mature defers to the per-rule default (Warning), not Error"
    );
    // iron-hand escalates to Error, proving it is NOT inherently Error
    // at default — only via preset.
    assert_eq!(
        preset_rule_severity(LziHygienePreset::TddIronHand, file_size_001::Finding::CODE),
        Some(lazuli_doctor::DoctorSeverity::Error),
    );
}

// ── Info companions ─────────────────────────────────────────────────────────

#[test]
fn uses_fanout_info() {
    let src = "feature hub\n  uses a\n  uses b\n  uses c\n  uses d\n  \
         resource Thing\n    label: Text required\n";
    let feature = lower(src);
    let infos = feature_cohesion_002::check_info(&[LoweredFeature::new(
        "features/hub/hub.lzi",
        &feature,
        src,
    )]);
    assert!(
        infos
            .iter()
            .any(|i| matches!(i.kind, InfoKind::UsesFanout { count } if count >= 4)),
        "uses fan-out ≥4 must emit info: {infos:?}"
    );
}

#[test]
fn name_collision_info() {
    // Near-identical names across two features (one differing token /
    // pluralization) cross the ≥0.7 similarity bar.
    let src_a = "feature account\n  resource WebhookEvent\n    kind: Text required\n";
    let src_b = "feature billing\n  resource WebhookEvents\n    kind: Text required\n";
    let fa = lower(src_a);
    let fb = lower(src_b);
    let infos = feature_cohesion_002::check_info(&[
        LoweredFeature::new("features/account/account.lzi", &fa, src_a),
        LoweredFeature::new("features/billing/billing.lzi", &fb, src_b),
    ]);
    assert!(
        infos
            .iter()
            .any(|i| matches!(&i.kind, InfoKind::NameCollision { .. })),
        "≥0.7 cross-feature name similarity must emit info: {infos:?}"
    );
}
