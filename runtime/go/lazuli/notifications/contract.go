// Package notifications implements the runtime side of the Lazuli
// `notification` block. The language declares the multi-channel
// outbound contract (channel, recipient, trigger, template, policy,
// tenant_from, idempotency, retry); this package owns the dispatcher
// and the typed errors. Concrete channel adapters (Sendgrid for email,
// FCM for push, Twilio for sms) sit in `@runtime/<adapter>` packages
// and bind via `@adapter.notification.*` resolution at boot.
//
// Phase L Tier 3 / row 33 stubs.
package notifications

import (
	"context"
	"errors"
)

// Channel names a closed-catalog notification channel. The DSL
// rejects values outside the catalog via doctor's `NOTIF-CHANNEL-001`.
type Channel string

const (
	ChannelEmail   Channel = "email"
	ChannelInApp   Channel = "in_app"
	ChannelPush    Channel = "push"
	ChannelSlack   Channel = "slack"
	ChannelDiscord Channel = "discord"
	ChannelWebhook Channel = "webhook"
)

// TenantFromSpec mirrors the jobs/webhooks shape.
type TenantFromSpec struct {
	Path string
}

// IdempotencyKeySpec mirrors the jobs-side shape.
type IdempotencyKeySpec struct {
	Path string
}

// RetryPolicy mirrors the jobs-side shape (closed-catalog backoff).
type RetryPolicy struct {
	Count   uint32
	Backoff string
}

// NotificationContract is the lowered `notification <name>` shape.
// Codegen emits `var <FeatureCamel>Notification<NameCamel>Contract = ...`.
type NotificationContract struct {
	Feature      string
	Name         string
	Channels     []Channel
	Recipient    string
	TriggerKind  string
	TriggerEvent string
	TriggerCron  string
	Template     string
	Policy       string
	TenantFrom   *TenantFromSpec
	Idempotency  *IdempotencyKeySpec
	Retry        *RetryPolicy
	Emits        []string
	// Notifications expanded bucket cycle — typed shape for the
	// `digest` sub-block on `notification`. nil when the DSL omits
	// digesting. Doctor (`NOTIF-DIGEST-001/002/003`) validates the
	// shape; this struct mirrors the IR one-to-one.
	Digest *NotificationDigest
	// Notifications expanded bucket cycle — typed shape for the
	// `throttle` sub-block. nil when no throttling is declared.
	// Distinct from any scalar `RateLimit` slot the runtime may add later;
	// `Throttle` always keys on recipient/channel/burst axes.
	Throttle *NotificationThrottle
}

// NotificationDigest mirrors `ir::NotificationDigest`. The
// dispatcher batches up to `MaxSize` triggers within `Every` per
// `GroupBy` key and emits a single rendered template with
// `TemplateStrategy` deciding how per-trigger payloads merge.
//
// `MaxSize == 0` means "no cap" at the runtime layer; doctor
// rejects 0 and `> 10_000` at the DSL layer so the runtime never
// sees a degenerate value.
type NotificationDigest struct {
	Every            string
	GroupBy          string
	MaxSize          uint32
	TemplateStrategy DigestStrategy
}

// DigestStrategy is the closed-catalog enum mirroring
// `ir::DigestStrategy`. Defaults to `DigestStrategyMerge` when the DSL omits
// `template_strategy`.
type DigestStrategy string

const (
	DigestStrategyMerge  DigestStrategy = "merge"
	DigestStrategyAppend DigestStrategy = "append"

	// Backwards-compatible aliases for the shorter names used by the
	// first runtime stub.
	DigestMerge  = DigestStrategyMerge
	DigestAppend = DigestStrategyAppend
)

// NotificationThrottle mirrors `ir::NotificationThrottle`. The
// dispatcher consults the throttle store before each dispatch; when
// the bucket is exhausted it returns `ErrThrottleExceeded` and the
// notification is skipped (this is intentional, not a delivery
// failure — same posture as `IdempotencyViolation` on jobs).
type NotificationThrottle struct {
	MaxPer       string
	PerRecipient bool
	PerChannel   bool
	Burst        uint32
}

// Envelope is the runtime payload threaded to a notification
// dispatch. `Recipient` is resolved before dispatch via the
// contract's `recipient <path>` expression.
type Envelope struct {
	ID           string
	Tenant       string
	Channel      Channel
	Recipient    string
	Payload      map[string]any
	TemplateData map[string]any
}

// Typed errors.
var (
	ErrNotificationChannelUnsupported = errors.New("notifications: channel not supported by any bound adapter")
	ErrNotificationDeliveryFailed     = errors.New("notifications: delivery failed after retries")
	ErrNotificationTenantUnresolved   = errors.New("notifications: tenant_from unresolved")
	// ErrNotificationIdempotent is returned when an active
	// idempotency claim already exists for a notification dispatch.
	ErrNotificationIdempotent = errors.New("notifications: duplicate idempotency key")
	// ErrDigestFull is returned by `DigestStore.Add` when the
	// in-window buffer for `<notification, group_by_value>` reaches
	// `NotificationDigest.MaxSize`. The dispatcher should flush
	// immediately and start a new window.
	ErrDigestFull = errors.New("notifications: digest window full, flush required")
	// ErrThrottleExceeded is returned by `ThrottleStore.Allow` when
	// the per-recipient/per-channel bucket is exhausted. The
	// dispatcher swallows the dispatch (intentional skip) and emits
	// a `slog.Info` event for ops; this is NOT a delivery failure.
	ErrThrottleExceeded = errors.New("notifications: throttle bucket exhausted")
	// ErrInvalidDuration surfaces when a digest/throttle duration
	// literal cannot be parsed at runtime, or when a runtime TTL is
	// non-positive. Doctor (`NOTIF-DIGEST-002` /
	// `NOTIF-THROTTLE-001`) rejects malformed literals at design time;
	// the runtime error is the defence-in-depth path.
	ErrInvalidDuration = errors.New("notifications: invalid duration literal")
)

// ChannelDispatcher is the per-channel adapter surface. Email
// dispatchers (`@runtime/sendgrid`, `@runtime/ses`) implement this for
// `ChannelEmail`; in-app dispatchers for `ChannelInApp`, and so on.
type ChannelDispatcher interface {
	Channel() Channel
	Dispatch(ctx context.Context, env Envelope) error
}
