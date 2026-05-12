// Package notifications — receipt store surface for notification
// delivery/read acknowledgements. Channel adapters record delivery
// receipts when a provider accepts or confirms delivery; product
// surfaces record read receipts when the recipient opens or reads the
// notification.
package notifications

import (
	"context"
	"sync"
	"time"
)

// DeliveryReceipt records a delivery acknowledgement for one outbound
// notification recipient/channel pair. DeliveredAt is supplied by the
// caller and is preserved exactly by ReceiptStore implementations.
type DeliveryReceipt struct {
	NotificationID string
	Recipient      string
	Channel        Channel
	DeliveredAt    time.Time
}

// ReadReceipt records a read acknowledgement for one outbound
// notification recipient/channel pair. ReadAt is supplied by the
// caller and is preserved exactly by ReceiptStore implementations.
type ReadReceipt struct {
	NotificationID string
	Recipient      string
	Channel        Channel
	ReadAt         time.Time
}

// ReceiptFilter selects receipts by notification id, recipient, and
// channel. Zero values are wildcards, so an empty filter lists all
// receipts of the requested type.
type ReceiptFilter struct {
	NotificationID string
	Recipient      string
	Channel        Channel
}

// ReceiptStore persists delivery and read receipts. Implementations
// MUST be safe for concurrent use and MUST preserve caller-supplied
// timestamps instead of replacing them with store time.
type ReceiptStore interface {
	RecordDelivery(ctx context.Context, receipt DeliveryReceipt) error
	RecordRead(ctx context.Context, receipt ReadReceipt) error
	ListDeliveryReceipts(ctx context.Context, filter ReceiptFilter) ([]DeliveryReceipt, error)
	ListReadReceipts(ctx context.Context, filter ReceiptFilter) ([]ReadReceipt, error)
}

// MemoryReceiptStore is the in-process reference implementation for
// notification receipt storage. It is intended for unit tests and
// single-instance deployments; production adapters can bind their own
// ReceiptStore implementations.
type MemoryReceiptStore struct {
	mu         sync.RWMutex
	deliveries []DeliveryReceipt
	reads      []ReadReceipt
}

// NewMemoryReceiptStore returns an empty in-process receipt store.
func NewMemoryReceiptStore() *MemoryReceiptStore {
	return &MemoryReceiptStore{}
}

// RecordDelivery implements ReceiptStore.
func (m *MemoryReceiptStore) RecordDelivery(_ context.Context, receipt DeliveryReceipt) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.deliveries = append(m.deliveries, receipt)
	return nil
}

// RecordRead implements ReceiptStore.
func (m *MemoryReceiptStore) RecordRead(_ context.Context, receipt ReadReceipt) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.reads = append(m.reads, receipt)
	return nil
}

// ListDeliveryReceipts implements ReceiptStore.
func (m *MemoryReceiptStore) ListDeliveryReceipts(
	_ context.Context,
	filter ReceiptFilter,
) ([]DeliveryReceipt, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	receipts := make([]DeliveryReceipt, 0, len(m.deliveries))
	for _, receipt := range m.deliveries {
		if filter.matches(receipt.NotificationID, receipt.Recipient, receipt.Channel) {
			receipts = append(receipts, receipt)
		}
	}
	return receipts, nil
}

// ListReadReceipts implements ReceiptStore.
func (m *MemoryReceiptStore) ListReadReceipts(
	_ context.Context,
	filter ReceiptFilter,
) ([]ReadReceipt, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	receipts := make([]ReadReceipt, 0, len(m.reads))
	for _, receipt := range m.reads {
		if filter.matches(receipt.NotificationID, receipt.Recipient, receipt.Channel) {
			receipts = append(receipts, receipt)
		}
	}
	return receipts, nil
}

func (f ReceiptFilter) matches(notificationID string, recipient string, channel Channel) bool {
	if f.NotificationID != "" && f.NotificationID != notificationID {
		return false
	}
	if f.Recipient != "" && f.Recipient != recipient {
		return false
	}
	if f.Channel != "" && f.Channel != channel {
		return false
	}
	return true
}
