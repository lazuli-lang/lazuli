// Package webhooks implements the runtime side of the Lazuli
// `webhook` block. The language declares the inbound contract (path,
// verify hmac alg, secret env, header, tenant_from, idempotency,
// handler, emits); this package owns the chi-mounted receiver, the
// HMAC verifier, and the typed error contract. Concrete adapters
// (Sendgrid inbound parser, Stripe-flavored sigs) sit in
// `@runtime/...` packages and bind via `@adapter.webhook.*` resolution
// when applicable.
//
// Phase L Tier 3 / row 33 stubs.
//
// Webhooks expanded cycle (rows 38–39, 2026-05-11) extends the language
// surface with `payload from webhook_events.<name>`, `replay`, `dlq`,
// and an inbound `retry` decorator that reuses the jobs RetryPolicy
// shape. This contract carries those lowered fields so codegen can wire
// `WebhookContract` field-for-field; lifecycle behavior remains owned by
// the receiver implementation.
package webhooks

import (
	"context"
	"errors"

	"lazuli.dev/runtime/lazuli/jobs"
)

// VerifyScheme is the closed-catalog scheme for inbound webhook
// verification today. v0 ships only `hmac`; future schemes (JWT-bearer,
// mTLS) gate on pilot evidence and adapter bindings.
type VerifyScheme string

const (
	VerifyHmac VerifyScheme = "hmac"
)

// VerifySpec is the lowered `verify hmac <alg>` declaration plus its
// nested `secret env.<NAME>` and `header "X-..."`. The Lazuli runtime
// receiver reads this to instantiate the right HMAC verifier.
type VerifySpec struct {
	Scheme    VerifyScheme
	Algorithm string
	SecretEnv string
	Header    string
}

// TenantFromSpec mirrors the jobs-side spec. Webhooks may scope their
// effect to a single tenant by extracting the axis id from the
// payload.
type TenantFromSpec struct {
	Path string
}

// WebhookEventRef is the lowered `payload from webhook_events.<name>`
// reference. The receiver uses it to select the generated payload
// envelope for typed decode.
type WebhookEventRef struct {
	Name string
}

// ReplayMode names whether a webhook allows replayed envelopes.
type ReplayMode int

const (
	ReplayAllow ReplayMode = iota
	ReplayDeny
)

// ReplaySpec is the lowered `replay` block. Window is the accepted
// freshness bound as authored in the DSL (for example, `"24h"`).
type ReplaySpec struct {
	Mode   ReplayMode
	Window string
}

// DlqKind names how exhausted webhook deliveries are handled.
type DlqKind int

const (
	DlqEmit DlqKind = iota
	DlqHandler
	DlqDrop
)

// DlqSpec is the lowered `dlq` block. Handler is used when Kind is
// DlqHandler; Topic is used when Kind is DlqEmit.
type DlqSpec struct {
	Kind    DlqKind
	Handler string
	Topic   string
}

// WebhookContract is the lowered `webhook <name>` shape. The codegen
// emits `var <FeatureCamel>Webhook<NameCamel>Contract = webhooks.WebhookContract{...}`
// per webhook.
type WebhookContract struct {
	Feature       string
	Name          string
	WithSource    func(context.Context) context.Context
	Route         string
	Verify        VerifySpec
	TenantFrom    *TenantFromSpec
	IdempotencyBy string
	Policy        string
	HandlerPath   string
	ReturnsType   string
	Emits         []string
	PayloadFrom   *WebhookEventRef
	Replay        *ReplaySpec
	DLQ           *DlqSpec
	Retry         *jobs.RetryPolicy
}

// Envelope is the runtime payload threaded through a webhook handler.
// `Body` carries the raw bytes (post-verify); `ParsedPayload` is the
// adapter-decoded map (often JSON).
type Envelope struct {
	ID            string
	Tenant        string
	Header        map[string]string
	Body          []byte
	ParsedPayload map[string]any
}

// HandlerFunc is the signature the codegen wires for webhook
// handlers. Returns nil on success; any returned `value` is published
// downstream when the contract declared `handler "./..." returns <Type>`.
type HandlerFunc func(ctx context.Context, envelope Envelope) (any, error)

// Typed errors. Surfaced through the observability / audit pipeline.
var (
	ErrWebhookHmacInvalid    = errors.New("webhooks: hmac verification failed")
	ErrWebhookIdempotent     = errors.New("webhooks: duplicate envelope")
	ErrWebhookTenantUnscoped = errors.New("webhooks: tenant_from unresolved")
)
