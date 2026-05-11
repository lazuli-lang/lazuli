// Package notifications implements the runtime side of the Lazuli
// `notification` block. The language declares the multi-channel
// outbound contract (channel, recipient, trigger, template, policy,
// tenant_from, idempotency, retry); this package owns the dispatcher
// and the typed errors. Concrete channel adapters (Sendgrid for email,
// FCM for push, Twilio for sms) sit in `@drusa/<adapter>` packages
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
	ChannelEmail    Channel = "email"
	ChannelInApp    Channel = "in_app"
	ChannelSlack    Channel = "slack"
	ChannelDiscord  Channel = "discord"
	ChannelWebhook  Channel = "webhook"
	// ChannelPush / ChannelSms are SPECULATIVE — gated on
	// `@adapter.notification.push` / `.sms` adapter bindings landing.
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
}

// Envelope is the runtime payload threaded to a notification
// dispatch. `Recipient` is resolved before dispatch via the
// contract's `recipient <path>` expression.
type Envelope struct {
	ID            string
	Tenant        string
	Channel       Channel
	Recipient     string
	Payload       map[string]any
	TemplateData  map[string]any
}

// Typed errors.
var (
	ErrNotificationChannelUnsupported = errors.New("notifications: channel not supported by any bound adapter")
	ErrNotificationDeliveryFailed     = errors.New("notifications: delivery failed after retries")
	ErrNotificationTenantUnresolved   = errors.New("notifications: tenant_from unresolved")
)

// ChannelDispatcher is the per-channel adapter surface. Email
// dispatchers (`@drusa/sendgrid`, `@drusa/ses`) implement this for
// `ChannelEmail`; in-app dispatchers for `ChannelInApp`, and so on.
type ChannelDispatcher interface {
	Channel() Channel
	Dispatch(ctx context.Context, env Envelope) error
}
