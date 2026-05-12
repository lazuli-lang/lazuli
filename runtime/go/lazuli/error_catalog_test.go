package lazuli_test

import (
	"errors"
	"fmt"
	"net/http"
	"testing"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/auth"
	"lazuli.dev/runtime/lazuli/notifications"
	"lazuli.dev/runtime/lazuli/storage"
	"lazuli.dev/runtime/lazuli/webhooks"
)

func TestC122ClassifyLazuliErrorDefaultsByCode(t *testing.T) {
	got := lazuli.ClassifyLazuliError(&lazuli.Error{
		Code:    lazuli.CodeValidationFailed,
		Message: "invalid email",
	})

	c122AssertClassification(t, got, lazuli.ErrorClassification{
		Code:   lazuli.CodeValidationFailed,
		Status: http.StatusBadRequest,
		Origin: lazuli.ErrorOriginUserDSL,
	})
}

func TestC122ClassifyLazuliErrorPreservesCustomStatusAndCode(t *testing.T) {
	got := lazuli.ClassifyError(fmt.Errorf("wrapped: %w", &lazuli.Error{
		Status:  http.StatusTeapot,
		Code:    "custom_teapot",
		Message: "short and stout",
	}))

	c122AssertClassification(t, got, lazuli.ErrorClassification{
		Code:   "custom_teapot",
		Status: http.StatusTeapot,
		Origin: lazuli.ErrorOriginUserDSL,
	})
}

func TestC122ClassifyRuntimeSentinels(t *testing.T) {
	_, replayErr := webhooks.ParseWebhookTimestamp("not-a-time")

	tests := []struct {
		name string
		err  error
		want lazuli.ErrorClassification
	}{
		{
			name: "auth",
			err:  auth.ErrPasswordMismatch,
			want: lazuli.ErrorClassification{
				Code:   "auth.password_mismatch",
				Status: http.StatusUnauthorized,
				Origin: lazuli.ErrorOriginUserDSL,
			},
		},
		{
			name: "wrapped storage",
			err:  fmt.Errorf("upload failed: %w", storage.ErrFileSizeExceeded),
			want: lazuli.ErrorClassification{
				Code:   "storage.file_size_exceeded",
				Status: http.StatusRequestEntityTooLarge,
				Origin: lazuli.ErrorOriginUserDSL,
			},
		},
		{
			name: "adapter runtime",
			err:  notifications.ErrNotificationDeliveryFailed,
			want: lazuli.ErrorClassification{
				Code:   "notifications.delivery_failed",
				Status: http.StatusBadGateway,
				Origin: lazuli.ErrorOriginAdapterRuntime,
			},
		},
		{
			name: "structured replay error",
			err:  replayErr,
			want: lazuli.ErrorClassification{
				Code:   "webhooks.replay_timestamp_invalid",
				Status: http.StatusBadRequest,
				Origin: lazuli.ErrorOriginUserDSL,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := lazuli.ClassifyError(tt.err)
			c122AssertClassification(t, got, tt.want)
		})
	}
}

func TestC122ClassifyUnknownErrorUsesUncataloguedFallback(t *testing.T) {
	got := lazuli.ClassifyError(fmt.Errorf("handler returned: %w", errors.New("custom: sentinel")))

	c122AssertClassification(t, got, lazuli.ErrorClassification{
		Code:   lazuli.CodeUncataloguedSentinel,
		Status: http.StatusInternalServerError,
		Origin: lazuli.ErrorOriginLibInternal,
	})
}

func TestC122ClassifyUnknownErrorDoesNotGuessByPrefix(t *testing.T) {
	got := lazuli.ClassifyError(errors.New("auth: password mismatch: fabricated"))

	c122AssertClassification(t, got, lazuli.ErrorClassification{
		Code:   lazuli.CodeUncataloguedSentinel,
		Status: http.StatusInternalServerError,
		Origin: lazuli.ErrorOriginLibInternal,
	})
}

func TestC122RuntimeErrorCatalogIsStableAndUnique(t *testing.T) {
	entries := lazuli.RuntimeErrorCatalog()
	if len(entries) == 0 {
		t.Fatalf("RuntimeErrorCatalog returned no entries")
	}

	seenSentinels := make(map[string]struct{}, len(entries))
	seenCodes := make(map[string]struct{}, len(entries))
	for _, entry := range entries {
		if entry.Sentinel == "" {
			t.Fatalf("catalog entry has empty sentinel: %#v", entry)
		}
		if entry.Code == "" {
			t.Fatalf("catalog entry has empty code: %#v", entry)
		}
		if entry.Status < 100 || entry.Status > 599 {
			t.Fatalf("catalog entry status = %d, want HTTP status: %#v", entry.Status, entry)
		}
		if entry.Origin == "" {
			t.Fatalf("catalog entry has empty origin: %#v", entry)
		}
		if _, ok := seenSentinels[entry.Sentinel]; ok {
			t.Fatalf("duplicate sentinel %q", entry.Sentinel)
		}
		if _, ok := seenCodes[entry.Code]; ok {
			t.Fatalf("duplicate code %q", entry.Code)
		}
		seenSentinels[entry.Sentinel] = struct{}{}
		seenCodes[entry.Code] = struct{}{}
	}
}

func c122AssertClassification(t *testing.T, got, want lazuli.ErrorClassification) {
	t.Helper()

	if got != want {
		t.Fatalf("classification = %#v, want %#v", got, want)
	}
}
