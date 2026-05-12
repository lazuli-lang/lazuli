package notifications

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestReceiptStatusStateMachineMarksDeliveredAndRead(t *testing.T) {
	t.Parallel()

	deliveredAt := time.Date(2026, 5, 12, 18, 0, 0, 0, time.UTC)
	readAt := deliveredAt.Add(2 * time.Minute)
	status := ReceiptStatus{ReceiptKey: receiptTestKey()}

	if state := status.State(); state != ReceiptStatePending {
		t.Fatalf("initial State() = %q, want %q", state, ReceiptStatePending)
	}

	delivered, err := status.MarkDelivered(deliveredAt)
	if err != nil {
		t.Fatalf("MarkDelivered() error = %v", err)
	}
	if !delivered.Changed || delivered.Duplicate {
		t.Fatalf("MarkDelivered() result = %+v, want changed non-duplicate", delivered)
	}
	if delivered.State != ReceiptStateDelivered {
		t.Fatalf("MarkDelivered() state = %q, want %q", delivered.State, ReceiptStateDelivered)
	}
	if !delivered.Status.DeliveredAt.Equal(deliveredAt) {
		t.Fatalf("DeliveredAt = %v, want %v", delivered.Status.DeliveredAt, deliveredAt)
	}

	duplicateDelivery, err := delivered.Status.MarkDelivered(deliveredAt.Add(time.Minute))
	if err != nil {
		t.Fatalf("duplicate MarkDelivered() error = %v", err)
	}
	if duplicateDelivery.Changed || !duplicateDelivery.Duplicate {
		t.Fatalf("duplicate MarkDelivered() result = %+v, want duplicate without change", duplicateDelivery)
	}
	if !duplicateDelivery.Status.DeliveredAt.Equal(deliveredAt) {
		t.Fatalf("duplicate changed DeliveredAt to %v, want %v", duplicateDelivery.Status.DeliveredAt, deliveredAt)
	}

	read, err := delivered.Status.MarkRead(readAt)
	if err != nil {
		t.Fatalf("MarkRead() error = %v", err)
	}
	if !read.Changed || read.Duplicate {
		t.Fatalf("MarkRead() result = %+v, want changed non-duplicate", read)
	}
	if read.State != ReceiptStateRead {
		t.Fatalf("MarkRead() state = %q, want %q", read.State, ReceiptStateRead)
	}
	if !read.Status.ReadAt.Equal(readAt) {
		t.Fatalf("ReadAt = %v, want %v", read.Status.ReadAt, readAt)
	}

	duplicateRead, err := read.Status.MarkRead(readAt.Add(time.Minute))
	if err != nil {
		t.Fatalf("duplicate MarkRead() error = %v", err)
	}
	if duplicateRead.Changed || !duplicateRead.Duplicate {
		t.Fatalf("duplicate MarkRead() result = %+v, want duplicate without change", duplicateRead)
	}
	if !duplicateRead.Status.ReadAt.Equal(readAt) {
		t.Fatalf("duplicate changed ReadAt to %v, want %v", duplicateRead.Status.ReadAt, readAt)
	}
}

func TestReceiptStatusRejectsInvalidTimeline(t *testing.T) {
	t.Parallel()

	deliveredAt := time.Date(2026, 5, 12, 18, 30, 0, 0, time.UTC)
	readAt := deliveredAt.Add(time.Minute)
	delivered := ReceiptStatus{
		ReceiptKey:  receiptTestKey(),
		DeliveredAt: deliveredAt,
	}
	if _, err := delivered.MarkRead(deliveredAt.Add(-time.Second)); !errors.Is(err, ErrReceiptTimelineInvalid) {
		t.Fatalf("MarkRead(before delivery) error = %v, want ErrReceiptTimelineInvalid", err)
	}

	readOnly := ReceiptStatus{
		ReceiptKey: receiptTestKey(),
		ReadAt:     readAt,
	}
	if _, err := readOnly.MarkDelivered(readAt.Add(time.Second)); !errors.Is(err, ErrReceiptTimelineInvalid) {
		t.Fatalf("MarkDelivered(after read) error = %v, want ErrReceiptTimelineInvalid", err)
	}

	err := ValidateReceiptTimeline(ReceiptStatus{
		ReceiptKey:  receiptTestKey(),
		DeliveredAt: readAt,
		ReadAt:      deliveredAt,
	})
	if !errors.Is(err, ErrReceiptTimelineInvalid) {
		t.Fatalf("ValidateReceiptTimeline() error = %v, want ErrReceiptTimelineInvalid", err)
	}
}

func TestMarkDeliveredAndReadUseStoreAndSkipDuplicates(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	store := NewMemoryReceiptStore()
	deliveredAt := time.Date(2026, 5, 12, 19, 0, 0, 0, time.UTC)
	readAt := deliveredAt.Add(3 * time.Minute)

	delivery := DeliveryReceipt{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
		DeliveredAt:    deliveredAt,
	}
	firstDelivery, err := MarkDelivered(ctx, store, delivery)
	if err != nil {
		t.Fatalf("first MarkDelivered() error = %v", err)
	}
	if !firstDelivery.Changed || firstDelivery.Duplicate || firstDelivery.State != ReceiptStateDelivered {
		t.Fatalf("first MarkDelivered() result = %+v, want delivered change", firstDelivery)
	}

	secondDelivery, err := MarkDelivered(ctx, store, DeliveryReceipt{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
		DeliveredAt:    deliveredAt.Add(time.Minute),
	})
	if err != nil {
		t.Fatalf("duplicate MarkDelivered() error = %v", err)
	}
	if secondDelivery.Changed || !secondDelivery.Duplicate {
		t.Fatalf("duplicate MarkDelivered() result = %+v, want duplicate without change", secondDelivery)
	}

	deliveries, err := store.ListDeliveryReceipts(ctx, ReceiptFilter{NotificationID: "notif-1"})
	if err != nil {
		t.Fatalf("ListDeliveryReceipts() error = %v", err)
	}
	if len(deliveries) != 1 {
		t.Fatalf("stored deliveries = %d, want 1", len(deliveries))
	}
	if !deliveries[0].DeliveredAt.Equal(deliveredAt) {
		t.Fatalf("stored DeliveredAt = %v, want %v", deliveries[0].DeliveredAt, deliveredAt)
	}

	read := ReadReceipt{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
		ReadAt:         readAt,
	}
	firstRead, err := MarkRead(ctx, store, read)
	if err != nil {
		t.Fatalf("first MarkRead() error = %v", err)
	}
	if !firstRead.Changed || firstRead.Duplicate || firstRead.State != ReceiptStateRead {
		t.Fatalf("first MarkRead() result = %+v, want read change", firstRead)
	}

	secondRead, err := MarkRead(ctx, store, ReadReceipt{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
		ReadAt:         readAt.Add(time.Minute),
	})
	if err != nil {
		t.Fatalf("duplicate MarkRead() error = %v", err)
	}
	if secondRead.Changed || !secondRead.Duplicate {
		t.Fatalf("duplicate MarkRead() result = %+v, want duplicate without change", secondRead)
	}

	reads, err := store.ListReadReceipts(ctx, ReceiptFilter{NotificationID: "notif-1"})
	if err != nil {
		t.Fatalf("ListReadReceipts() error = %v", err)
	}
	if len(reads) != 1 {
		t.Fatalf("stored reads = %d, want 1", len(reads))
	}
	if !reads[0].ReadAt.Equal(readAt) {
		t.Fatalf("stored ReadAt = %v, want %v", reads[0].ReadAt, readAt)
	}

	status, err := LoadReceiptStatus(ctx, store, receiptTestKey())
	if err != nil {
		t.Fatalf("LoadReceiptStatus() error = %v", err)
	}
	if status.State() != ReceiptStateRead {
		t.Fatalf("loaded State() = %q, want %q", status.State(), ReceiptStateRead)
	}
	if !status.DeliveredAt.Equal(deliveredAt) || !status.ReadAt.Equal(readAt) {
		t.Fatalf("loaded status = %+v, want delivered/read timestamps", status)
	}
}

func TestMarkReadBeforeDeliveredAllowsEarlierDeliveryToFillTimeline(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	store := NewMemoryReceiptStore()
	readAt := time.Date(2026, 5, 12, 19, 30, 0, 0, time.UTC)
	deliveredAt := readAt.Add(-time.Minute)

	readResult, err := MarkRead(ctx, store, ReadReceipt{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
		ReadAt:         readAt,
	})
	if err != nil {
		t.Fatalf("MarkRead() error = %v", err)
	}
	if readResult.State != ReceiptStateRead {
		t.Fatalf("MarkRead() state = %q, want %q", readResult.State, ReceiptStateRead)
	}

	deliveryResult, err := MarkDelivered(ctx, store, DeliveryReceipt{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
		DeliveredAt:    deliveredAt,
	})
	if err != nil {
		t.Fatalf("MarkDelivered() after read error = %v", err)
	}
	if !deliveryResult.Changed || deliveryResult.Duplicate || deliveryResult.State != ReceiptStateRead {
		t.Fatalf("MarkDelivered() after read result = %+v, want changed read state", deliveryResult)
	}

	status, err := LoadReceiptStatus(ctx, store, receiptTestKey())
	if err != nil {
		t.Fatalf("LoadReceiptStatus() error = %v", err)
	}
	if !status.DeliveredAt.Equal(deliveredAt) || !status.ReadAt.Equal(readAt) {
		t.Fatalf("status = %+v, want delivered_at %v and read_at %v", status, deliveredAt, readAt)
	}
}

func TestReceiptHelpersValidateInputs(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	store := NewMemoryReceiptStore()
	validDelivery := DeliveryReceipt{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
		DeliveredAt:    time.Date(2026, 5, 12, 20, 0, 0, 0, time.UTC),
	}

	if _, err := MarkDelivered(ctx, nil, validDelivery); !errors.Is(err, ErrReceiptStoreNil) {
		t.Fatalf("MarkDelivered(nil store) error = %v, want ErrReceiptStoreNil", err)
	}

	withoutNotification := validDelivery
	withoutNotification.NotificationID = ""
	if _, err := MarkDelivered(ctx, store, withoutNotification); !errors.Is(err, ErrReceiptIdentityInvalid) {
		t.Fatalf("MarkDelivered(invalid identity) error = %v, want ErrReceiptIdentityInvalid", err)
	}

	withoutTimestamp := validDelivery
	withoutTimestamp.DeliveredAt = time.Time{}
	if _, err := MarkDelivered(ctx, store, withoutTimestamp); !errors.Is(err, ErrReceiptTimestampInvalid) {
		t.Fatalf("MarkDelivered(zero timestamp) error = %v, want ErrReceiptTimestampInvalid", err)
	}
	if _, err := MarkRead(ctx, store, ReadReceipt{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
	}); !errors.Is(err, ErrReceiptTimestampInvalid) {
		t.Fatalf("MarkRead(zero timestamp) error = %v, want ErrReceiptTimestampInvalid", err)
	}

	if _, err := LoadReceiptStatus(ctx, store, ReceiptKey{NotificationID: "notif-1"}); !errors.Is(err, ErrReceiptIdentityInvalid) {
		t.Fatalf("LoadReceiptStatus(invalid key) error = %v, want ErrReceiptIdentityInvalid", err)
	}
}

func receiptTestKey() ReceiptKey {
	return ReceiptKey{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
	}
}
