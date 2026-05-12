package notifications

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"
)

func TestMemoryReceiptStoreDeliveryReceipts(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	store := NewMemoryReceiptStore()
	deliveredAt := time.Date(2026, 5, 12, 14, 30, 0, 123, time.FixedZone("receipt", -3*60*60))

	receipts := []DeliveryReceipt{
		{
			NotificationID: "notif-1",
			Recipient:      "user-1",
			Channel:        ChannelEmail,
			DeliveredAt:    deliveredAt,
		},
		{
			NotificationID: "notif-1",
			Recipient:      "user-2",
			Channel:        ChannelSlack,
			DeliveredAt:    deliveredAt.Add(time.Minute),
		},
		{
			NotificationID: "notif-2",
			Recipient:      "user-1",
			Channel:        ChannelEmail,
			DeliveredAt:    deliveredAt.Add(2 * time.Minute),
		},
	}
	for _, receipt := range receipts {
		if err := store.RecordDelivery(ctx, receipt); err != nil {
			t.Fatalf("RecordDelivery: %v", err)
		}
	}

	byNotification, err := store.ListDeliveryReceipts(ctx, ReceiptFilter{NotificationID: "notif-1"})
	if err != nil {
		t.Fatalf("ListDeliveryReceipts notification: %v", err)
	}
	if len(byNotification) != 2 {
		t.Fatalf("notification filter returned %d receipts, want 2", len(byNotification))
	}

	byRecipient, err := store.ListDeliveryReceipts(ctx, ReceiptFilter{Recipient: "user-1"})
	if err != nil {
		t.Fatalf("ListDeliveryReceipts recipient: %v", err)
	}
	if len(byRecipient) != 2 {
		t.Fatalf("recipient filter returned %d receipts, want 2", len(byRecipient))
	}

	byChannel, err := store.ListDeliveryReceipts(ctx, ReceiptFilter{Channel: ChannelEmail})
	if err != nil {
		t.Fatalf("ListDeliveryReceipts channel: %v", err)
	}
	if len(byChannel) != 2 {
		t.Fatalf("channel filter returned %d receipts, want 2", len(byChannel))
	}

	exact, err := store.ListDeliveryReceipts(ctx, ReceiptFilter{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
	})
	if err != nil {
		t.Fatalf("ListDeliveryReceipts exact: %v", err)
	}
	if len(exact) != 1 {
		t.Fatalf("exact filter returned %d receipts, want 1", len(exact))
	}
	if exact[0].DeliveredAt != deliveredAt {
		t.Fatalf("DeliveredAt = %v, want %v", exact[0].DeliveredAt, deliveredAt)
	}

	exact[0].DeliveredAt = time.Time{}
	again, err := store.ListDeliveryReceipts(ctx, ReceiptFilter{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelEmail,
	})
	if err != nil {
		t.Fatalf("ListDeliveryReceipts after mutation: %v", err)
	}
	if again[0].DeliveredAt != deliveredAt {
		t.Fatalf("stored DeliveredAt mutated to %v, want %v", again[0].DeliveredAt, deliveredAt)
	}
}

func TestMemoryReceiptStoreReadReceipts(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	store := NewMemoryReceiptStore()
	readAt := time.Date(2026, 5, 12, 15, 45, 0, 456, time.UTC)

	receipts := []ReadReceipt{
		{
			NotificationID: "notif-1",
			Recipient:      "user-1",
			Channel:        ChannelInApp,
			ReadAt:         readAt,
		},
		{
			NotificationID: "notif-1",
			Recipient:      "user-2",
			Channel:        ChannelEmail,
			ReadAt:         readAt.Add(time.Minute),
		},
		{
			NotificationID: "notif-2",
			Recipient:      "user-1",
			Channel:        ChannelInApp,
			ReadAt:         readAt.Add(2 * time.Minute),
		},
	}
	for _, receipt := range receipts {
		if err := store.RecordRead(ctx, receipt); err != nil {
			t.Fatalf("RecordRead: %v", err)
		}
	}

	all, err := store.ListReadReceipts(ctx, ReceiptFilter{})
	if err != nil {
		t.Fatalf("ListReadReceipts all: %v", err)
	}
	if len(all) != len(receipts) {
		t.Fatalf("empty filter returned %d receipts, want %d", len(all), len(receipts))
	}

	exact, err := store.ListReadReceipts(ctx, ReceiptFilter{
		NotificationID: "notif-1",
		Recipient:      "user-1",
		Channel:        ChannelInApp,
	})
	if err != nil {
		t.Fatalf("ListReadReceipts exact: %v", err)
	}
	if len(exact) != 1 {
		t.Fatalf("exact filter returned %d receipts, want 1", len(exact))
	}
	if exact[0].ReadAt != readAt {
		t.Fatalf("ReadAt = %v, want %v", exact[0].ReadAt, readAt)
	}
}

func TestMemoryReceiptStoreConcurrentAccess(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	store := NewMemoryReceiptStore()
	base := time.Date(2026, 5, 12, 16, 0, 0, 0, time.UTC)
	const records = 128

	var wg sync.WaitGroup
	for i := 0; i < records; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			channel := ChannelEmail
			if i%2 == 0 {
				channel = ChannelSlack
			}
			recipient := fmt.Sprintf("user-%d", i%8)
			deliveredAt := base.Add(time.Duration(i) * time.Second)
			readAt := deliveredAt.Add(time.Minute)
			if err := store.RecordDelivery(ctx, DeliveryReceipt{
				NotificationID: "notif-concurrent",
				Recipient:      recipient,
				Channel:        channel,
				DeliveredAt:    deliveredAt,
			}); err != nil {
				t.Errorf("RecordDelivery: %v", err)
			}
			if err := store.RecordRead(ctx, ReadReceipt{
				NotificationID: "notif-concurrent",
				Recipient:      recipient,
				Channel:        channel,
				ReadAt:         readAt,
			}); err != nil {
				t.Errorf("RecordRead: %v", err)
			}
		}(i)
	}
	for i := 0; i < 16; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := store.ListDeliveryReceipts(ctx, ReceiptFilter{NotificationID: "notif-concurrent"}); err != nil {
				t.Errorf("ListDeliveryReceipts: %v", err)
			}
			if _, err := store.ListReadReceipts(ctx, ReceiptFilter{Channel: ChannelEmail}); err != nil {
				t.Errorf("ListReadReceipts: %v", err)
			}
		}()
	}
	wg.Wait()

	deliveries, err := store.ListDeliveryReceipts(ctx, ReceiptFilter{NotificationID: "notif-concurrent"})
	if err != nil {
		t.Fatalf("ListDeliveryReceipts final: %v", err)
	}
	if len(deliveries) != records {
		t.Fatalf("deliveries = %d, want %d", len(deliveries), records)
	}

	reads, err := store.ListReadReceipts(ctx, ReceiptFilter{NotificationID: "notif-concurrent"})
	if err != nil {
		t.Fatalf("ListReadReceipts final: %v", err)
	}
	if len(reads) != records {
		t.Fatalf("reads = %d, want %d", len(reads), records)
	}
}
