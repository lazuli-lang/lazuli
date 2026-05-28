//! Async work — jobs, webhooks, and notifications.
//!
//! All three primitives share the same operational backbone (trigger,
//! tenant_from, idempotency, retry, emits, policy) but expose distinct
//! authoring surfaces. Modeling them together in one IR module makes that
//! shared backbone visible: [`RetryPolicy`] is the same shape on a
//! [`Job`], a [`Webhook`], and a [`Notification`]; [`TenantFromSpec`] is
//! reused across all three; [`IdempotencyKey`] travels with every
//! trigger that could fire twice.
//!
//! ## Jobs
//!
//! See [`job`] — handler-backed or declaratively bound jobs (a job has
//! exactly one body style).
//!
//! ## Webhooks
//!
//! See [`webhook`] — inbound HTTP delivery contract with two verifier
//! shapes (legacy text-pattern + canonical-indent typed), structured
//! replay/dlq routing, optional retry.
//!
//! ## Notifications
//!
//! See [`notification`] — outbound multi-channel dispatch with
//! structured digest + throttle controls.
//!
//! ## Why "async_work" and not three modules
//!
//! Splitting jobs/webhooks/notifications would force [`RetryPolicy`],
//! [`IdempotencyKey`], [`TenantFromSpec`], [`JobTrigger`] (notifications
//! reuse this), and [`ExternalCallRef`] into a fourth common module —
//! adding indirection without payoff. The async-work boundary is real:
//! these three primitives differ in *direction* (inbound/outbound/scheduled)
//! but share the *operational discipline*. The module reflects that. The
//! shared backbone lives in [`shared`].
//!
//! ## See also
//!
//! - `docs/proposals/notifications-expanded-bucket-cycle.md` — digest +
//!   throttle design
//! - `docs/proposals/webhooks-expanded-cycle.md` — replay + dlq +
//!   typed `payload_from` design
//! - `docs/proposals/ir-error-messages-vocab.md` §3.3 — per-callable
//!   `policy_when_denied` slot rationale
//! - [`crate::Command`] — shares many of these shapes (retry,
//!   idempotency, external_calls) by intent

pub mod job;
pub mod notification;
pub mod shared;
pub mod webhook;

pub use job::{Job, JobBody, JobDeclarative, JobHandler, JobOperationalKind, JobTrigger};
pub use notification::{DigestStrategy, Notification, NotificationDigest, NotificationThrottle};
pub use shared::{
    BackoffStrategy, ExternalCallRef, FanoutScope, FanoutSpec, IdempotencyKey, RetryPolicy,
    TenantFromSpec,
};
pub use webhook::{
    DlqSpec, ReplayMode, ReplaySpec, VerifyScheme, VerifySpec, Webhook, WebhookEventRef,
    WebhookScopeGlobalSpec,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_round_trips_through_json() {
        let rp = RetryPolicy {
            count: 5,
            backoff: BackoffStrategy::Exponential,
        };
        let json = serde_json::to_string(&rp).unwrap();
        let back: RetryPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(rp, back);
    }

    #[test]
    fn job_trigger_event_round_trips_with_kind_value_envelope() {
        let trig = JobTrigger::Event {
            event: crate::QualifiedName {
                feature: Some("customer".into()),
                name: "customer_archived".into(),
            },
        };
        let json = serde_json::to_string(&trig).unwrap();
        let back: JobTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(trig, back);
        assert!(json.contains("\"kind\":\"Event\""), "got: {json}");
    }

    #[test]
    fn job_trigger_schedule_round_trip() {
        let trig = JobTrigger::Schedule {
            cron: "0 2 * * *".into(),
        };
        let json = serde_json::to_string(&trig).unwrap();
        let back: JobTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(trig, back);
        assert!(json.contains("\"kind\":\"Schedule\""), "got: {json}");
    }

    #[test]
    fn replay_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ReplayMode::Allow).unwrap(),
            "\"allow\""
        );
        assert_eq!(
            serde_json::to_string(&ReplayMode::Deny).unwrap(),
            "\"deny\""
        );
    }

    #[test]
    fn dlq_spec_variants_round_trip() {
        let cases = [
            DlqSpec::Emit {
                event: "delivery_failed".into(),
            },
            DlqSpec::Drop {
                reason: "provider sends 429 storms".into(),
            },
        ];
        for dlq in cases {
            let json = serde_json::to_string(&dlq).unwrap();
            let back: DlqSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(dlq, back);
        }
    }

    #[test]
    fn digest_strategy_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&DigestStrategy::Merge).unwrap(),
            "\"merge\""
        );
        assert_eq!(
            serde_json::to_string(&DigestStrategy::Append).unwrap(),
            "\"append\""
        );
    }

    #[test]
    fn throttle_default_bools_skipped_in_json() {
        let throttle = NotificationThrottle {
            max_per: "1 hour".into(),
            per_recipient: false,
            per_channel: false,
            burst: None,
        };
        let json = serde_json::to_string(&throttle).unwrap();
        // Both default-false bools must be skipped.
        assert!(!json.contains("per_recipient"));
        assert!(!json.contains("per_channel"));
        assert!(!json.contains("burst"));
    }
}
