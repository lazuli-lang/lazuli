package notifications

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

var (
	// ErrDeliveryReceiptProviderInvalid reports a provider receipt without a
	// provider and at least one provider-supplied id.
	ErrDeliveryReceiptProviderInvalid = errors.New("notifications: delivery receipt provider invalid")
	// ErrDeliveryReceiptStateInvalid reports a provider delivery receipt with an
	// unknown outcome state.
	ErrDeliveryReceiptStateInvalid = errors.New("notifications: delivery receipt state invalid")
)

// DeliveryReceiptState is the provider delivery outcome for one outbound send.
// It is intentionally separate from ReceiptState, which tracks delivery/read
// timelines.
type DeliveryReceiptState string

const (
	DeliveryReceiptStateDelivered DeliveryReceiptState = "delivered"
	DeliveryReceiptStateFailed    DeliveryReceiptState = "failed"
	DeliveryReceiptStateBounced   DeliveryReceiptState = "bounced"
)

// Valid reports whether state is one of the supported delivery outcomes.
func (s DeliveryReceiptState) Valid() bool {
	switch s {
	case DeliveryReceiptStateDelivered,
		DeliveryReceiptStateFailed,
		DeliveryReceiptStateBounced:
		return true
	default:
		return false
	}
}

// Failed reports whether state represents unsuccessful provider delivery.
func (s DeliveryReceiptState) Failed() bool {
	switch s {
	case DeliveryReceiptStateFailed, DeliveryReceiptStateBounced:
		return true
	default:
		return false
	}
}

// DeliveryReceiptProviderKey identifies one provider-side delivery receipt.
type DeliveryReceiptProviderKey struct {
	Provider  string
	MessageID string
	ReceiptID string
}

// Empty reports whether key has no provider-side identity.
func (k DeliveryReceiptProviderKey) Empty() bool {
	return strings.TrimSpace(k.Provider) == "" &&
		strings.TrimSpace(k.MessageID) == "" &&
		strings.TrimSpace(k.ReceiptID) == ""
}

// Validate checks that key can identify a provider delivery receipt.
func (k DeliveryReceiptProviderKey) Validate() error {
	switch {
	case strings.TrimSpace(k.Provider) == "":
		return fmt.Errorf("%w: provider is required", ErrDeliveryReceiptProviderInvalid)
	case strings.TrimSpace(k.MessageID) == "" && strings.TrimSpace(k.ReceiptID) == "":
		return fmt.Errorf("%w: provider_message_id or provider_receipt_id is required", ErrDeliveryReceiptProviderInvalid)
	default:
		return nil
	}
}

// ProviderDeliveryReceipt records a provider-side delivery outcome. Provider
// adapters can keep their own message/receipt identifiers here without mixing
// failed or bounced outcomes into read-receipt timelines.
type ProviderDeliveryReceipt struct {
	NotificationID string
	Recipient      string
	Channel        Channel

	Provider          string
	ProviderMessageID string
	ProviderReceiptID string

	State      DeliveryReceiptState
	OccurredAt time.Time

	Retryable    bool
	ErrorCode    string
	ErrorMessage string
}

// ProviderKey returns the provider-side identity for receipt.
func (r ProviderDeliveryReceipt) ProviderKey() DeliveryReceiptProviderKey {
	return DeliveryReceiptProviderKey{
		Provider:  r.Provider,
		MessageID: r.ProviderMessageID,
		ReceiptID: r.ProviderReceiptID,
	}
}

// Validate checks that receipt has a notification identity, provider identity,
// known outcome state, and occurrence timestamp.
func (r ProviderDeliveryReceipt) Validate() error {
	return ValidateProviderDeliveryReceipt(r)
}

// CanRetry reports whether receipt describes a retryable delivery failure.
func (r ProviderDeliveryReceipt) CanRetry() bool {
	return r.Retryable && r.State.Failed()
}

// ValidateProviderDeliveryReceipt checks that receipt can be safely aggregated
// or stored by a provider delivery receipt adapter.
func ValidateProviderDeliveryReceipt(receipt ProviderDeliveryReceipt) error {
	switch {
	case receipt.NotificationID == "":
		return fmt.Errorf("%w: notification_id is required", ErrReceiptIdentityInvalid)
	case receipt.Recipient == "":
		return fmt.Errorf("%w: recipient is required", ErrReceiptIdentityInvalid)
	case receipt.Channel == "":
		return fmt.Errorf("%w: channel is required", ErrReceiptIdentityInvalid)
	}
	if err := receipt.ProviderKey().Validate(); err != nil {
		return err
	}
	if !receipt.State.Valid() {
		return fmt.Errorf("%w: state %q is unknown", ErrDeliveryReceiptStateInvalid, receipt.State)
	}
	if receipt.OccurredAt.IsZero() {
		return ErrReceiptTimestampInvalid
	}
	return nil
}

// DeliveryReceiptCounts is a compact count bucket for provider receipt states.
type DeliveryReceiptCounts struct {
	Total                int
	Delivered            int
	Failed               int
	Bounced              int
	RetryableFailures    int
	NonRetryableFailures int
}

// DeliveryReceiptSummary aggregates provider delivery receipt outcomes.
type DeliveryReceiptSummary struct {
	DeliveryReceiptCounts
	ByChannel  map[Channel]DeliveryReceiptCounts
	ByProvider map[string]DeliveryReceiptCounts
}

// SummarizeProviderDeliveryReceipts validates and aggregates provider delivery
// receipt outcomes by state, retryability, channel, and provider.
func SummarizeProviderDeliveryReceipts(receipts []ProviderDeliveryReceipt) (DeliveryReceiptSummary, error) {
	summary := DeliveryReceiptSummary{
		ByChannel:  make(map[Channel]DeliveryReceiptCounts),
		ByProvider: make(map[string]DeliveryReceiptCounts),
	}
	for i, receipt := range receipts {
		if err := receipt.Validate(); err != nil {
			return DeliveryReceiptSummary{}, fmt.Errorf("receipt %d: %w", i, err)
		}
		summary.add(receipt)
	}
	return summary, nil
}

func (s *DeliveryReceiptSummary) add(receipt ProviderDeliveryReceipt) {
	addDeliveryReceiptCounts(&s.DeliveryReceiptCounts, receipt)

	channelCounts := s.ByChannel[receipt.Channel]
	addDeliveryReceiptCounts(&channelCounts, receipt)
	s.ByChannel[receipt.Channel] = channelCounts

	provider := strings.TrimSpace(receipt.Provider)
	providerCounts := s.ByProvider[provider]
	addDeliveryReceiptCounts(&providerCounts, receipt)
	s.ByProvider[provider] = providerCounts
}

func addDeliveryReceiptCounts(counts *DeliveryReceiptCounts, receipt ProviderDeliveryReceipt) {
	counts.Total++
	switch receipt.State {
	case DeliveryReceiptStateDelivered:
		counts.Delivered++
	case DeliveryReceiptStateFailed:
		counts.Failed++
	case DeliveryReceiptStateBounced:
		counts.Bounced++
	}
	if receipt.State.Failed() {
		if receipt.CanRetry() {
			counts.RetryableFailures++
		} else {
			counts.NonRetryableFailures++
		}
	}
}
