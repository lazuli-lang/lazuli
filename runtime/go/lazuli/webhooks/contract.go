// Package webhooks implements the runtime side of the Lazuli
// `webhook` block. The language declares the inbound contract (path,
// verify hmac alg, secret env, header, tenant_from, idempotency,
// handler, emits); this package owns the chi-mounted receiver, the
// HMAC verifier, and the typed error contract. Concrete adapters
// (Sendgrid inbound parser, Stripe-flavored sigs) sit in
// `@drusa/...` packages and bind via `@adapter.webhook.*` resolution
// when applicable.
//
// Phase L Tier 3 / row 33 stubs.
package webhooks

import (
	"context"
	"errors"
)

// VerifyScheme is the closed-catalog scheme for inbound webhook
// verification today. v0 ships only `hmac`; future schemes (JWT-bearer,
// mTLS) gate on pilot evidence and adapter bindings.
type VerifyScheme string

const (
	VerifyHmac VerifyScheme = "hmac"
)

// VerifySpec is the lowered `verify hmac <alg>` declaration plus its
// nested `secret env.<NAME>` and `header "X-..."`. The Drusa
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

// WebhookContract is the lowered `webhook <name>` shape. The codegen
// emits `var <FeatureCamel>Webhook<NameCamel>Contract = webhooks.WebhookContract{...}`
// per webhook.
type WebhookContract struct {
	Feature        string
	Name           string
	Route          string
	Verify         VerifySpec
	TenantFrom     *TenantFromSpec
	IdempotencyBy  string
	Policy         string
	HandlerPath    string
	ReturnsType    string
	Emits          []string
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
	ErrWebhookHmacInvalid  = errors.New("webhooks: hmac verification failed")
	ErrWebhookIdempotent   = errors.New("webhooks: duplicate envelope")
	ErrWebhookTenantUnscoped = errors.New("webhooks: tenant_from unresolved")
)
