package notifications

import (
	"context"
	"errors"
	"fmt"
	"time"
)

var (
	// ErrReceiptStoreNil is returned when a receipt helper is called without storage.
	ErrReceiptStoreNil = errors.New("notifications: receipt store is nil")
	// ErrReceiptIdentityInvalid reports a receipt without an exact notification,
	// recipient, and channel identity.
	ErrReceiptIdentityInvalid = errors.New("notifications: receipt identity invalid")
	// ErrReceiptTimestampInvalid reports a delivery/read mark without a timestamp.
	ErrReceiptTimestampInvalid = errors.New("notifications: receipt timestamp invalid")
	// ErrReceiptTimelineInvalid reports receipt timestamps that cannot describe a
	// valid lifecycle, such as a read timestamp before delivery.
	ErrReceiptTimelineInvalid = errors.New("notifications: receipt timeline invalid")
)

// ReceiptState is the recipient/channel receipt lifecycle for one notification.
type ReceiptState string

const (
	ReceiptStatePending   ReceiptState = "pending"
	ReceiptStateDelivered ReceiptState = "delivered"
	ReceiptStateRead      ReceiptState = "read"
)

// ReceiptKey identifies one notification receipt timeline.
type ReceiptKey struct {
	NotificationID string
	Recipient      string
	Channel        Channel
}

// ReceiptStatus is the collapsed delivery/read timeline for one receipt key.
type ReceiptStatus struct {
	ReceiptKey
	DeliveredAt time.Time
	ReadAt      time.Time
}

// ReceiptMarkResult describes the effect of applying a delivered/read mark.
type ReceiptMarkResult struct {
	Status    ReceiptStatus
	State     ReceiptState
	Changed   bool
	Duplicate bool
}

// State returns the lifecycle state implied by the receipt timestamps.
func (s ReceiptStatus) State() ReceiptState {
	if !s.ReadAt.IsZero() {
		return ReceiptStateRead
	}
	if !s.DeliveredAt.IsZero() {
		return ReceiptStateDelivered
	}
	return ReceiptStatePending
}

// ValidateTimeline reports invalid timestamp ordering.
func (s ReceiptStatus) ValidateTimeline() error {
	return ValidateReceiptTimeline(s)
}

// MarkDelivered applies a delivery mark to an in-memory receipt status.
func (s ReceiptStatus) MarkDelivered(deliveredAt time.Time) (ReceiptMarkResult, error) {
	if deliveredAt.IsZero() {
		return ReceiptMarkResult{}, ErrReceiptTimestampInvalid
	}
	if err := ValidateReceiptTimeline(s); err != nil {
		return ReceiptMarkResult{}, err
	}
	if !s.DeliveredAt.IsZero() {
		return receiptMarkResult(s, false, true), nil
	}
	if !s.ReadAt.IsZero() && deliveredAt.After(s.ReadAt) {
		return ReceiptMarkResult{}, fmt.Errorf(
			"%w: delivered_at %s after read_at %s",
			ErrReceiptTimelineInvalid,
			formatReceiptTime(deliveredAt),
			formatReceiptTime(s.ReadAt),
		)
	}

	s.DeliveredAt = deliveredAt
	return receiptMarkResult(s, true, false), nil
}

// MarkRead applies a read mark to an in-memory receipt status.
func (s ReceiptStatus) MarkRead(readAt time.Time) (ReceiptMarkResult, error) {
	if readAt.IsZero() {
		return ReceiptMarkResult{}, ErrReceiptTimestampInvalid
	}
	if err := ValidateReceiptTimeline(s); err != nil {
		return ReceiptMarkResult{}, err
	}
	if !s.ReadAt.IsZero() {
		return receiptMarkResult(s, false, true), nil
	}
	if !s.DeliveredAt.IsZero() && readAt.Before(s.DeliveredAt) {
		return ReceiptMarkResult{}, fmt.Errorf(
			"%w: read_at %s before delivered_at %s",
			ErrReceiptTimelineInvalid,
			formatReceiptTime(readAt),
			formatReceiptTime(s.DeliveredAt),
		)
	}

	s.ReadAt = readAt
	return receiptMarkResult(s, true, false), nil
}

// ValidateReceiptTimeline reports invalid delivery/read timestamp ordering.
func ValidateReceiptTimeline(status ReceiptStatus) error {
	if !status.DeliveredAt.IsZero() && !status.ReadAt.IsZero() && status.ReadAt.Before(status.DeliveredAt) {
		return fmt.Errorf(
			"%w: read_at %s before delivered_at %s",
			ErrReceiptTimelineInvalid,
			formatReceiptTime(status.ReadAt),
			formatReceiptTime(status.DeliveredAt),
		)
	}
	return nil
}

// LoadReceiptStatus returns the collapsed delivery/read status for key.
func LoadReceiptStatus(ctx context.Context, store ReceiptStore, key ReceiptKey) (ReceiptStatus, error) {
	if store == nil {
		return ReceiptStatus{}, ErrReceiptStoreNil
	}
	if err := key.validate(); err != nil {
		return ReceiptStatus{}, err
	}
	if err := ctx.Err(); err != nil {
		return ReceiptStatus{}, err
	}

	status := ReceiptStatus{ReceiptKey: key}
	filter := key.filter()

	deliveries, err := store.ListDeliveryReceipts(ctx, filter)
	if err != nil {
		return ReceiptStatus{}, err
	}
	for _, receipt := range deliveries {
		if !key.matches(receipt.NotificationID, receipt.Recipient, receipt.Channel) {
			continue
		}
		if receipt.DeliveredAt.IsZero() {
			return ReceiptStatus{}, ErrReceiptTimestampInvalid
		}
		if status.DeliveredAt.IsZero() || receipt.DeliveredAt.Before(status.DeliveredAt) {
			status.DeliveredAt = receipt.DeliveredAt
		}
	}

	reads, err := store.ListReadReceipts(ctx, filter)
	if err != nil {
		return ReceiptStatus{}, err
	}
	for _, receipt := range reads {
		if !key.matches(receipt.NotificationID, receipt.Recipient, receipt.Channel) {
			continue
		}
		if receipt.ReadAt.IsZero() {
			return ReceiptStatus{}, ErrReceiptTimestampInvalid
		}
		if status.ReadAt.IsZero() || receipt.ReadAt.Before(status.ReadAt) {
			status.ReadAt = receipt.ReadAt
		}
	}

	if err := ValidateReceiptTimeline(status); err != nil {
		return ReceiptStatus{}, err
	}
	return status, nil
}

// MarkDelivered records a delivery receipt unless the timeline is already delivered.
func MarkDelivered(ctx context.Context, store ReceiptStore, receipt DeliveryReceipt) (ReceiptMarkResult, error) {
	key := deliveryReceiptKey(receipt)
	if err := key.validate(); err != nil {
		return ReceiptMarkResult{}, err
	}
	if receipt.DeliveredAt.IsZero() {
		return ReceiptMarkResult{}, ErrReceiptTimestampInvalid
	}

	status, err := LoadReceiptStatus(ctx, store, key)
	if err != nil {
		return ReceiptMarkResult{}, err
	}

	result, err := status.MarkDelivered(receipt.DeliveredAt)
	if err != nil {
		return ReceiptMarkResult{}, err
	}
	if result.Duplicate {
		return result, nil
	}
	if err := ctx.Err(); err != nil {
		return ReceiptMarkResult{}, err
	}
	if err := store.RecordDelivery(ctx, receipt); err != nil {
		return ReceiptMarkResult{}, err
	}
	return result, nil
}

// MarkRead records a read receipt unless the timeline is already read.
func MarkRead(ctx context.Context, store ReceiptStore, receipt ReadReceipt) (ReceiptMarkResult, error) {
	key := readReceiptKey(receipt)
	if err := key.validate(); err != nil {
		return ReceiptMarkResult{}, err
	}
	if receipt.ReadAt.IsZero() {
		return ReceiptMarkResult{}, ErrReceiptTimestampInvalid
	}

	status, err := LoadReceiptStatus(ctx, store, key)
	if err != nil {
		return ReceiptMarkResult{}, err
	}

	result, err := status.MarkRead(receipt.ReadAt)
	if err != nil {
		return ReceiptMarkResult{}, err
	}
	if result.Duplicate {
		return result, nil
	}
	if err := ctx.Err(); err != nil {
		return ReceiptMarkResult{}, err
	}
	if err := store.RecordRead(ctx, receipt); err != nil {
		return ReceiptMarkResult{}, err
	}
	return result, nil
}

func receiptMarkResult(status ReceiptStatus, changed bool, duplicate bool) ReceiptMarkResult {
	return ReceiptMarkResult{
		Status:    status,
		State:     status.State(),
		Changed:   changed,
		Duplicate: duplicate,
	}
}

func deliveryReceiptKey(receipt DeliveryReceipt) ReceiptKey {
	return ReceiptKey{
		NotificationID: receipt.NotificationID,
		Recipient:      receipt.Recipient,
		Channel:        receipt.Channel,
	}
}

func readReceiptKey(receipt ReadReceipt) ReceiptKey {
	return ReceiptKey{
		NotificationID: receipt.NotificationID,
		Recipient:      receipt.Recipient,
		Channel:        receipt.Channel,
	}
}

func (k ReceiptKey) filter() ReceiptFilter {
	return ReceiptFilter{
		NotificationID: k.NotificationID,
		Recipient:      k.Recipient,
		Channel:        k.Channel,
	}
}

func (k ReceiptKey) matches(notificationID string, recipient string, channel Channel) bool {
	return k.NotificationID == notificationID && k.Recipient == recipient && k.Channel == channel
}

func (k ReceiptKey) validate() error {
	switch {
	case k.NotificationID == "":
		return fmt.Errorf("%w: notification_id is required", ErrReceiptIdentityInvalid)
	case k.Recipient == "":
		return fmt.Errorf("%w: recipient is required", ErrReceiptIdentityInvalid)
	case k.Channel == "":
		return fmt.Errorf("%w: channel is required", ErrReceiptIdentityInvalid)
	default:
		return nil
	}
}

func formatReceiptTime(t time.Time) string {
	return t.UTC().Format(time.RFC3339Nano)
}
