package notifications

import (
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestProviderDeliveryReceiptValidatesProviderIDsAndState(t *testing.T) {
	t.Parallel()

	valid := providerDeliveryReceiptTestValue(DeliveryReceiptStateDelivered, false)
	if err := valid.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	key := valid.ProviderKey()
	if key.Provider != "sendgrid" || key.MessageID != "msg-123" || key.ReceiptID != "evt-123" {
		t.Fatalf("ProviderKey() = %#v, want provider/message/receipt ids", key)
	}

	withoutNotification := valid
	withoutNotification.NotificationID = ""
	if err := withoutNotification.Validate(); !errors.Is(err, ErrReceiptIdentityInvalid) {
		t.Fatalf("Validate(missing notification) error = %v, want ErrReceiptIdentityInvalid", err)
	}

	withoutProvider := valid
	withoutProvider.Provider = ""
	if err := withoutProvider.Validate(); !errors.Is(err, ErrDeliveryReceiptProviderInvalid) {
		t.Fatalf("Validate(missing provider) error = %v, want ErrDeliveryReceiptProviderInvalid", err)
	}

	withoutProviderIDs := valid
	withoutProviderIDs.ProviderMessageID = ""
	withoutProviderIDs.ProviderReceiptID = ""
	if err := withoutProviderIDs.Validate(); !errors.Is(err, ErrDeliveryReceiptProviderInvalid) {
		t.Fatalf("Validate(missing provider ids) error = %v, want ErrDeliveryReceiptProviderInvalid", err)
	}

	unknownState := valid
	unknownState.State = DeliveryReceiptState("queued")
	if err := unknownState.Validate(); !errors.Is(err, ErrDeliveryReceiptStateInvalid) {
		t.Fatalf("Validate(unknown state) error = %v, want ErrDeliveryReceiptStateInvalid", err)
	}

	withoutTimestamp := valid
	withoutTimestamp.OccurredAt = time.Time{}
	if err := withoutTimestamp.Validate(); !errors.Is(err, ErrReceiptTimestampInvalid) {
		t.Fatalf("Validate(missing timestamp) error = %v, want ErrReceiptTimestampInvalid", err)
	}
}

func TestProviderDeliveryReceiptRetryabilityFollowsFailureState(t *testing.T) {
	t.Parallel()

	delivered := providerDeliveryReceiptTestValue(DeliveryReceiptStateDelivered, true)
	if delivered.CanRetry() {
		t.Fatalf("delivered CanRetry() = true, want false")
	}

	retryableFailure := providerDeliveryReceiptTestValue(DeliveryReceiptStateFailed, true)
	if !retryableFailure.CanRetry() {
		t.Fatalf("retryable failed CanRetry() = false, want true")
	}

	permanentFailure := providerDeliveryReceiptTestValue(DeliveryReceiptStateFailed, false)
	if permanentFailure.CanRetry() {
		t.Fatalf("permanent failed CanRetry() = true, want false")
	}

	retryableBounce := providerDeliveryReceiptTestValue(DeliveryReceiptStateBounced, true)
	if !retryableBounce.CanRetry() {
		t.Fatalf("retryable bounced CanRetry() = false, want true")
	}
}

func TestSummarizeProviderDeliveryReceiptsAggregatesByStateRetryabilityChannelAndProvider(t *testing.T) {
	t.Parallel()

	receipts := []ProviderDeliveryReceipt{
		providerDeliveryReceiptTestValue(DeliveryReceiptStateDelivered, false),
		providerDeliveryReceiptTestValue(DeliveryReceiptStateFailed, true),
		{
			NotificationID:    "notif-2",
			Recipient:         "user-2",
			Channel:           ChannelEmail,
			Provider:          "sendgrid",
			ProviderMessageID: "msg-456",
			State:             DeliveryReceiptStateBounced,
			OccurredAt:        providerDeliveryReceiptTestTime().Add(2 * time.Minute),
			Retryable:         false,
			ErrorCode:         "bounce",
		},
		{
			NotificationID:    "notif-3",
			Recipient:         "user-3",
			Channel:           ChannelSlack,
			Provider:          "slack",
			ProviderReceiptID: "evt-789",
			State:             DeliveryReceiptStateFailed,
			OccurredAt:        providerDeliveryReceiptTestTime().Add(3 * time.Minute),
			Retryable:         false,
			ErrorCode:         "rate_limited",
		},
	}

	summary, err := SummarizeProviderDeliveryReceipts(receipts)
	if err != nil {
		t.Fatalf("SummarizeProviderDeliveryReceipts() error = %v", err)
	}

	wantTotal := DeliveryReceiptCounts{
		Total:                4,
		Delivered:            1,
		Failed:               2,
		Bounced:              1,
		RetryableFailures:    1,
		NonRetryableFailures: 2,
	}
	if summary.DeliveryReceiptCounts != wantTotal {
		t.Fatalf("summary counts = %#v, want %#v", summary.DeliveryReceiptCounts, wantTotal)
	}

	wantByChannel := map[Channel]DeliveryReceiptCounts{
		ChannelEmail: {
			Total:                3,
			Delivered:            1,
			Failed:               1,
			Bounced:              1,
			RetryableFailures:    1,
			NonRetryableFailures: 1,
		},
		ChannelSlack: {
			Total:                1,
			Failed:               1,
			NonRetryableFailures: 1,
		},
	}
	if !reflect.DeepEqual(summary.ByChannel, wantByChannel) {
		t.Fatalf("ByChannel = %#v, want %#v", summary.ByChannel, wantByChannel)
	}

	wantByProvider := map[string]DeliveryReceiptCounts{
		"sendgrid": {
			Total:                3,
			Delivered:            1,
			Failed:               1,
			Bounced:              1,
			RetryableFailures:    1,
			NonRetryableFailures: 1,
		},
		"slack": {
			Total:                1,
			Failed:               1,
			NonRetryableFailures: 1,
		},
	}
	if !reflect.DeepEqual(summary.ByProvider, wantByProvider) {
		t.Fatalf("ByProvider = %#v, want %#v", summary.ByProvider, wantByProvider)
	}
}

func TestSummarizeProviderDeliveryReceiptsRejectsInvalidReceipt(t *testing.T) {
	t.Parallel()

	receipt := providerDeliveryReceiptTestValue(DeliveryReceiptState("queued"), false)
	_, err := SummarizeProviderDeliveryReceipts([]ProviderDeliveryReceipt{receipt})
	if !errors.Is(err, ErrDeliveryReceiptStateInvalid) {
		t.Fatalf("SummarizeProviderDeliveryReceipts() error = %v, want ErrDeliveryReceiptStateInvalid", err)
	}
}

func providerDeliveryReceiptTestValue(state DeliveryReceiptState, retryable bool) ProviderDeliveryReceipt {
	return ProviderDeliveryReceipt{
		NotificationID:    "notif-1",
		Recipient:         "user-1",
		Channel:           ChannelEmail,
		Provider:          "sendgrid",
		ProviderMessageID: "msg-123",
		ProviderReceiptID: "evt-123",
		State:             state,
		OccurredAt:        providerDeliveryReceiptTestTime(),
		Retryable:         retryable,
	}
}

func providerDeliveryReceiptTestTime() time.Time {
	return time.Date(2026, 5, 12, 21, 0, 0, 0, time.UTC)
}
