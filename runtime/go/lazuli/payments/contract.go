// Package payments declares the runtime contract for `@lazuli/plugin-<name>`
// adapters that provide payment-gateway services (preference / checkout
// creation, webhook signature verification, refund operations).
//
// The contract is intentionally minimal — wire-thin. Concrete adapters
// (MercadoPago, Stripe, Pagar.me, PayPal, Pix-direct, etc.) live in
// separate `@lazuli/plugin-<name>` repos and register via
// `lazuli.RegisterAdapter` against this package's `PaymentGateway`
// interface.
//
// EXPERIMENTAL: shape may grow additive fields before 1.0. Stable
// promotion gated on first pilot consumer in production.
package payments

import "context"

// PreferenceRequest creates a checkout / payment intent on the provider.
//
// `Reference` is the consumer's idempotency token — adapter callers
// MUST supply a unique value per logical transaction. The webhook
// callback echoes this back via `WebhookEvent.ExternalReference` so
// the consumer can correlate.
type PreferenceRequest struct {
	Reference   string            // external reference / idempotency token
	AmountCents int64             // amount in minor units (cents, satoshis, etc.)
	Currency    string            // ISO 4217 (e.g. "BRL", "USD")
	Description string            // human-readable; surfaces on provider UI
	CallbackURL string            // success / pending / failure return URL
	NotifyURL   string            // webhook URL for async status updates
	BuyerEmail  string            // optional; provider-specific UX impact
	Metadata    map[string]string // free-form provider-passthrough
}

// Preference is the provider's response after a successful create.
type Preference struct {
	ID          string // provider's opaque preference / payment intent ID
	InitPoint   string // checkout URL the consumer redirects buyer to
	ExternalRef string // echo of request's Reference for double-check
}

// WebhookEvent is the parsed inbound webhook payload (provider-shaped
// decode is the adapter's job; this is the canonicalized form the
// consumer sees).
type WebhookEvent struct {
	// PaymentID is the provider's opaque payment identifier (independent
	// from the preference ID — one preference may have many payment
	// attempts).
	PaymentID string
	// ExternalReference echoes the consumer's `PreferenceRequest.Reference`.
	ExternalReference string
	// Status is the closed status enum.
	Status WebhookStatus
	// AmountCents + Currency reflect what was actually charged (may
	// differ from request when partial captures / installments apply).
	AmountCents int64
	Currency    string
	// WhatsAppQRPayload carries the Pix copy-paste payload when a provider
	// webhook includes one for WhatsApp checkout flows.
	WhatsAppQRPayload string
	// Raw is the original payload bytes for audit / replay.
	Raw []byte
}

// WebhookStatus is the closed catalog of payment lifecycle states the
// runtime understands. Adapters MUST map provider-specific states to
// one of these.
type WebhookStatus uint8

const (
	WebhookStatusPending WebhookStatus = iota
	WebhookStatusApproved
	WebhookStatusRejected
	WebhookStatusCancelled
	WebhookStatusRefunded
)

// PaymentGateway is the closed adapter contract. Implementations live
// in `@lazuli/plugin-<name>` repos.
//
// Implementations MUST be safe for concurrent use.
type PaymentGateway interface {
	// CreatePreference initiates a payment / checkout flow.
	CreatePreference(ctx context.Context, req PreferenceRequest) (Preference, error)

	// VerifyWebhook validates the inbound payload's HMAC / signature
	// against the configured shared secret. Returns nil on valid,
	// non-nil error on tamper or malformed.
	//
	// `signature` is the raw header value the provider supplies
	// (e.g. MercadoPago's `X-Signature`, Stripe's `Stripe-Signature`).
	VerifyWebhook(payload []byte, signature string) error

	// ParseWebhook decodes a verified payload into the canonical
	// `WebhookEvent` shape. Called only after `VerifyWebhook` succeeded.
	ParseWebhook(payload []byte) (WebhookEvent, error)
}
