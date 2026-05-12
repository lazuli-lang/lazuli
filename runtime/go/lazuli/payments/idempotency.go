package payments

import "strings"

// Operation names the gateway call that an idempotency key protects.
type Operation string

const (
	OperationCreateIntent Operation = "create_intent"
	OperationConfirm      Operation = "confirm"
	OperationCapture      Operation = "capture"
	OperationRefund       Operation = "refund"
	OperationWebhook      Operation = "webhook"
)

// IdempotencyKey is the canonical scope for a payment idempotency claim.
// Subject is the operation-specific value: transaction id for create,
// payment id for confirm/capture, refund id for refund, and event id for
// webhook processing.
type IdempotencyKey struct {
	Operation     Operation
	Provider      string
	Tenant        string
	TransactionID string
	Subject       string
}

// CreateIntentKey builds a key for creating a checkout preference or
// payment intent for transactionID.
func CreateIntentKey(tenant, transactionID string) IdempotencyKey {
	return IdempotencyKey{
		Operation:     OperationCreateIntent,
		Tenant:        tenant,
		TransactionID: transactionID,
		Subject:       transactionID,
	}
}

// ConfirmKey builds a key for confirming a provider payment.
func ConfirmKey(tenant, transactionID, paymentID string) IdempotencyKey {
	return IdempotencyKey{
		Operation:     OperationConfirm,
		Tenant:        tenant,
		TransactionID: transactionID,
		Subject:       paymentID,
	}
}

// CaptureKey builds a key for capturing an authorized provider payment.
func CaptureKey(tenant, transactionID, paymentID string) IdempotencyKey {
	return IdempotencyKey{
		Operation:     OperationCapture,
		Tenant:        tenant,
		TransactionID: transactionID,
		Subject:       paymentID,
	}
}

// RefundKey builds a key for refunding a provider payment. refundID can be
// an application-level refund id before the provider creates its own id.
func RefundKey(tenant, transactionID, refundID string) IdempotencyKey {
	return IdempotencyKey{
		Operation:     OperationRefund,
		Tenant:        tenant,
		TransactionID: transactionID,
		Subject:       refundID,
	}
}

// WebhookKey builds a key for processing a provider webhook event.
func WebhookKey(provider, eventID string) IdempotencyKey {
	return IdempotencyKey{
		Operation: OperationWebhook,
		Provider:  provider,
		Subject:   eventID,
	}
}

// WithProvider returns a copy scoped to provider.
func (k IdempotencyKey) WithProvider(provider string) IdempotencyKey {
	k.Provider = provider
	return k
}

// String returns a stable textual key suitable for adapter requests or
// idempotency stores. Values are escaped so colons and backslashes cannot
// collapse adjacent segments.
func (k IdempotencyKey) String() string {
	parts := []string{"payments", string(k.Operation)}
	if k.Provider != "" {
		parts = append(parts, "provider="+escapeKeySegment(k.Provider))
	}
	if k.Tenant != "" {
		parts = append(parts, "tenant="+escapeKeySegment(k.Tenant))
	}
	if k.TransactionID != "" {
		parts = append(parts, "transaction="+escapeKeySegment(k.TransactionID))
	}
	if k.Subject != "" {
		parts = append(parts, "subject="+escapeKeySegment(k.Subject))
	}
	return strings.Join(parts, ":")
}

// IsZero reports whether k carries no operation or scoped subject.
func (k IdempotencyKey) IsZero() bool {
	return k.Operation == "" &&
		k.Provider == "" &&
		k.Tenant == "" &&
		k.TransactionID == "" &&
		k.Subject == ""
}

func escapeKeySegment(value string) string {
	value = strings.ReplaceAll(value, `\`, `\\`)
	return strings.ReplaceAll(value, ":", `\:`)
}
