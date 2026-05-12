// Package payments defines the provider-neutral runtime contract for
// Hostpoint payment transactions. Concrete gateway adapters translate
// these shapes to provider terms such as checkout preferences, payment
// intents, captures, confirmations, refunds, and webhook payloads.
package payments

import (
	"context"
	"errors"
	"time"
)

// Money stores an amount in the minor unit for Currency. For example,
// BRL 10.50 is Amount 1050 with Currency "BRL".
type Money struct {
	Amount   int64
	Currency string
}

// CaptureMode declares whether a payment should be captured by the
// provider immediately or authorized first and captured later.
type CaptureMode string

const (
	CaptureModeAutomatic CaptureMode = "automatic"
	CaptureModeManual    CaptureMode = "manual"
)

// PaymentStatus is the provider-neutral lifecycle for a payment intent
// or payment record.
type PaymentStatus string

const (
	PaymentStatusUnknown           PaymentStatus = "unknown"
	PaymentStatusCreated           PaymentStatus = "created"
	PaymentStatusPending           PaymentStatus = "pending"
	PaymentStatusRequiresAction    PaymentStatus = "requires_action"
	PaymentStatusAuthorized        PaymentStatus = "authorized"
	PaymentStatusCaptured          PaymentStatus = "captured"
	PaymentStatusSucceeded         PaymentStatus = "succeeded"
	PaymentStatusFailed            PaymentStatus = "failed"
	PaymentStatusCanceled          PaymentStatus = "canceled"
	PaymentStatusExpired           PaymentStatus = "expired"
	PaymentStatusRefunded          PaymentStatus = "refunded"
	PaymentStatusPartiallyRefunded PaymentStatus = "partially_refunded"
)

// Terminal reports whether no further provider-side progress is expected
// for this status without creating a new transaction.
func (s PaymentStatus) Terminal() bool {
	switch s {
	case PaymentStatusCaptured,
		PaymentStatusSucceeded,
		PaymentStatusFailed,
		PaymentStatusCanceled,
		PaymentStatusExpired,
		PaymentStatusRefunded:
		return true
	default:
		return false
	}
}

// RefundStatus is the provider-neutral lifecycle for a refund request.
type RefundStatus string

const (
	RefundStatusUnknown   RefundStatus = "unknown"
	RefundStatusRequested RefundStatus = "requested"
	RefundStatusPending   RefundStatus = "pending"
	RefundStatusSucceeded RefundStatus = "succeeded"
	RefundStatusFailed    RefundStatus = "failed"
	RefundStatusCanceled  RefundStatus = "canceled"
)

// Terminal reports whether no further provider-side progress is expected
// for this refund status.
func (s RefundStatus) Terminal() bool {
	switch s {
	case RefundStatusSucceeded, RefundStatusFailed, RefundStatusCanceled:
		return true
	default:
		return false
	}
}

// WebhookEventType names the normalized event category emitted by a
// payment gateway webhook.
type WebhookEventType string

const (
	WebhookEventPaymentCreated WebhookEventType = "payment.created"
	WebhookEventPaymentUpdated WebhookEventType = "payment.updated"
	WebhookEventPaymentFailed  WebhookEventType = "payment.failed"
	WebhookEventPaymentExpired WebhookEventType = "payment.expired"
	WebhookEventRefundCreated  WebhookEventType = "refund.created"
	WebhookEventRefundUpdated  WebhookEventType = "refund.updated"
)

// IdempotencyKeySpec is the lowered `idempotency by <path>` directive
// used by payment contracts that resolve request-specific keys.
type IdempotencyKeySpec struct {
	Path string
}

// PaymentContract is the generated contract for a payment gateway site.
// Provider is an adapter slot or registry key, not a concrete package name.
type PaymentContract struct {
	Feature     string
	Name        string
	Provider    string
	Currency    string
	CaptureMode CaptureMode
	Idempotency *IdempotencyKeySpec
	Metadata    map[string]string
}

// LineItem is an optional itemized amount sent to checkout-style gateways.
type LineItem struct {
	ID          string
	Title       string
	Description string
	Quantity    int64
	UnitAmount  Money
	Metadata    map[string]string
}

// Payer carries optional customer details accepted by most gateways.
type Payer struct {
	ID       string
	Email    string
	Name     string
	Document string
	Metadata map[string]string
}

// CreatePaymentIntentRequest creates a provider checkout preference or
// payment intent for a Hostpoint transaction.
type CreatePaymentIntentRequest struct {
	Contract        PaymentContract
	Tenant          string
	TransactionID   string
	Amount          Money
	Description     string
	Items           []LineItem
	Payer           Payer
	SuccessURL      string
	PendingURL      string
	FailureURL      string
	NotificationURL string
	ExpiresAt       time.Time
	IdempotencyKey  string
	Metadata        map[string]string
}

// PaymentIntent is the provider-neutral result of creating a checkout
// preference or payment intent.
type PaymentIntent struct {
	ID           string
	Provider     string
	ProviderID   string
	Status       PaymentStatus
	Amount       Money
	CheckoutURL  string
	ClientSecret string
	ExpiresAt    time.Time
	Metadata     map[string]string
}

// ConfirmPaymentRequest confirms an existing provider payment intent when
// the provider separates creation from confirmation.
type ConfirmPaymentRequest struct {
	Contract        PaymentContract
	Tenant          string
	PaymentIntentID string
	ProviderID      string
	Amount          Money
	IdempotencyKey  string
	Metadata        map[string]string
}

// CapturePaymentRequest captures an authorized payment.
type CapturePaymentRequest struct {
	Contract       PaymentContract
	Tenant         string
	PaymentID      string
	ProviderID     string
	Amount         Money
	IdempotencyKey string
	Metadata       map[string]string
}

// Payment is the provider-neutral payment record returned by confirm and
// capture operations or decoded from webhook events.
type Payment struct {
	ID               string
	Provider         string
	ProviderID       string
	PaymentIntentID  string
	Status           PaymentStatus
	Amount           Money
	AuthorizedAmount Money
	CapturedAmount   Money
	PaymentMethod    string
	PaidAt           time.Time
	Metadata         map[string]string
}

// RefundPaymentRequest refunds all or part of a payment.
type RefundPaymentRequest struct {
	Contract       PaymentContract
	Tenant         string
	PaymentID      string
	ProviderID     string
	Amount         Money
	Reason         string
	IdempotencyKey string
	Metadata       map[string]string
}

// Refund is the provider-neutral refund record.
type Refund struct {
	ID         string
	Provider   string
	ProviderID string
	PaymentID  string
	Status     RefundStatus
	Amount     Money
	Reason     string
	CreatedAt  time.Time
	Metadata   map[string]string
}

// WebhookRequest carries the raw provider webhook frame for verification
// and normalization by the bound adapter.
type WebhookRequest struct {
	Provider   string
	Headers    map[string]string
	Body       []byte
	ReceivedAt time.Time
}

// WebhookEvent is the normalized payment gateway event shape published to
// generated handlers and downstream Hostpoint transaction state machines.
type WebhookEvent struct {
	ID              string
	Provider        string
	Type            WebhookEventType
	OccurredAt      time.Time
	PaymentIntentID string
	PaymentID       string
	RefundID        string
	PaymentStatus   PaymentStatus
	RefundStatus    RefundStatus
	Amount          Money
	RawPayload      []byte
	Headers         map[string]string
	Metadata        map[string]string
}

// PaymentGateway is the adapter contract implemented by concrete payment
// providers. Implementations may map CreatePaymentIntent to a checkout
// preference, hosted checkout session, or native provider payment intent.
type PaymentGateway interface {
	CreatePaymentIntent(ctx context.Context, req CreatePaymentIntentRequest) (PaymentIntent, error)
	ConfirmPayment(ctx context.Context, req ConfirmPaymentRequest) (Payment, error)
	CapturePayment(ctx context.Context, req CapturePaymentRequest) (Payment, error)
	RefundPayment(ctx context.Context, req RefundPaymentRequest) (Refund, error)
	ParseWebhookEvent(ctx context.Context, req WebhookRequest) (WebhookEvent, error)
}

var (
	ErrGatewayUnavailable       = errors.New("payments: gateway unavailable")
	ErrGatewayUnsupported       = errors.New("payments: gateway operation unsupported")
	ErrPaymentDeclined          = errors.New("payments: payment declined")
	ErrPaymentNotFound          = errors.New("payments: payment not found")
	ErrRefundNotFound           = errors.New("payments: refund not found")
	ErrInvalidPaymentRequest    = errors.New("payments: invalid request")
	ErrPaymentIdempotent        = errors.New("payments: duplicate idempotency key")
	ErrWebhookVerification      = errors.New("payments: webhook verification failed")
	ErrWebhookEventUnsupported  = errors.New("payments: webhook event unsupported")
	ErrWebhookEventUnidentified = errors.New("payments: webhook event unidentified")
)
