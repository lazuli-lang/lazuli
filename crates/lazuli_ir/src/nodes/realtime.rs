//! Realtime IR — `channel <name>` push-stream declarations.
//!
//! `channel` is the typed sibling of `event`. Where `event` declares a
//! durable bus contract (outbox + replay), `channel` declares a tenant-scoped
//! policy-gated push stream that the runtime materializes as a WebSocket /
//! SSE / equivalent transport. The language's job is to lock the contract;
//! the runtime owns transport mechanics, presence tracking, and the
//! authorize-then-fanout machinery.
//!
//! The MVP grammar is intentionally narrow: three required children
//! (`tenant_from <axis>`, `policy @policy.<name>`, `payload <RecordType>`)
//! and one optional ABI-future-proofing slot (`policy_when_denied`). Every
//! deferred ergonomic — `audit`, per-channel rate limits, presence /
//! subscription / broadcast wiring — waits on ≥3-app pilot evidence per
//! `docs/scope-discipline.md`. Adding an open-ended bag now would erode the
//! cold-readability of the construct exactly where it should be tightest.
//!
//! ## Doctor lattice
//!
//! - `CHANNEL-PAYLOAD-001` — `payload <T>` resolves against
//!   `Feature.records` / `Feature.resources`.
//! - Tenant axis + policy lattice — share the existing `tenant_axis` /
//!   `policy_lattice` diagnostic infrastructure.
//!
//! ## See also
//!
//! - `docs/proposals/bucket-realtime-cycle.md`
//! - `docs/proposals/bucket-realtime-scope.md`
//! - `docs/proposals/ir-error-messages-vocab.md` §3.3 — `policy_when_denied`
//!   IR shape (v1 codegen does not consume; v2-additive)

use serde::{Deserialize, Serialize};

use crate::{PolicyRef, SpanRef, TenantFromSpec, TranslationKeyRef};

/// Realtime bucket cycle MVP — `channel <name>` declaration.
///
/// Typed, tenant-scoped, policy-gated declaration of a push stream.
/// Sibling of `event` (durable bus) but on a push transport. The
/// runtime materializes WebSocket / SSE; the language declares the
/// contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    /// `tenant_from <axis>` — axis name verbatim (`org`, `team`, ...).
    /// Resolved against the feature's `defaults.tenancy` lattice via
    /// the same plumbing `Job.tenant_from` uses.
    pub tenant_from: TenantFromSpec,
    /// `policy @policy.<name>` — read policy gating subscribers.
    pub policy: PolicyRef,
    /// IR Error-Vocab — reserved-slot per-channel override for the
    /// `policy_denied` error message. v1 codegen does not consume this
    /// slot (subscriber rejections surface through realtime transport,
    /// not HTTP envelopes); the IR shape exists so v2 promotion is
    /// purely additive. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    /// `payload <RecordType>` — verbatim type name. Doctor
    /// `CHANNEL-PAYLOAD-001` resolves it against
    /// `Feature.records` / `Feature.resources`.
    pub payload: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Path;

    fn sample() -> Channel {
        Channel {
            name: "order_updates".to_owned(),
            tenant_from: TenantFromSpec {
                path: Path::from_segments(["payload", "org_id"]),
            },
            policy: PolicyRef::Atom("read".to_owned()),
            policy_when_denied: None,
            payload: "OrderEvent".to_owned(),
            span_ref: None,
        }
    }

    #[test]
    fn channel_round_trips_through_json() {
        let ch = sample();
        let json = serde_json::to_value(&ch).unwrap();
        let back: Channel = serde_json::from_value(json).unwrap();
        assert_eq!(back, ch);
    }

    #[test]
    fn channel_omits_optional_fields_when_unset() {
        let ch = sample();
        let value = serde_json::to_value(&ch).unwrap();
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("policy_when_denied"));
        assert!(!obj.contains_key("span_ref"));
    }

    #[test]
    fn channel_required_keys_present() {
        let ch = sample();
        let value = serde_json::to_value(&ch).unwrap();
        let obj = value.as_object().unwrap();
        for key in ["name", "tenant_from", "policy", "payload"] {
            assert!(obj.contains_key(key), "missing key {key}");
        }
    }

    #[test]
    fn channel_preserves_policy_when_denied_when_set() {
        let mut ch = sample();
        ch.policy_when_denied = Some(TranslationKeyRef {
            key: "errors.subscription_denied".to_owned(),
            span_ref: None,
        });
        let json = serde_json::to_value(&ch).unwrap();
        assert_eq!(
            json["policy_when_denied"]["key"],
            json!("errors.subscription_denied")
        );
    }
}
