package payments_test

import (
	"testing"

	"lazuli.dev/runtime/lazuli/payments"
)

func TestIdempotencyKeyHelpers(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name string
		key  payments.IdempotencyKey
		want string
	}{
		{
			name: "create intent",
			key:  payments.CreateIntentKey("tenant-1", "txn-1").WithProvider("gateway"),
			want: "payments:create_intent:provider=gateway:tenant=tenant-1:transaction=txn-1:subject=txn-1",
		},
		{
			name: "confirm",
			key:  payments.ConfirmKey("tenant-1", "txn-1", "pay-1"),
			want: "payments:confirm:tenant=tenant-1:transaction=txn-1:subject=pay-1",
		},
		{
			name: "capture",
			key:  payments.CaptureKey("tenant-1", "txn-1", "pay-1"),
			want: "payments:capture:tenant=tenant-1:transaction=txn-1:subject=pay-1",
		},
		{
			name: "refund",
			key:  payments.RefundKey("tenant-1", "txn-1", "refund-1"),
			want: "payments:refund:tenant=tenant-1:transaction=txn-1:subject=refund-1",
		},
		{
			name: "webhook",
			key:  payments.WebhookKey("gateway", "evt-1"),
			want: "payments:webhook:provider=gateway:subject=evt-1",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			if got := tc.key.String(); got != tc.want {
				t.Fatalf("String() = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestIdempotencyKeyEscapesSegments(t *testing.T) {
	t.Parallel()

	key := payments.CaptureKey(`tenant:one`, `txn\one`, `pay:one`).WithProvider(`gw\br`)
	want := `payments:capture:provider=gw\\br:tenant=tenant\:one:transaction=txn\\one:subject=pay\:one`

	if got := key.String(); got != want {
		t.Fatalf("String() = %q, want %q", got, want)
	}
}

func TestIdempotencyKeyIsZero(t *testing.T) {
	t.Parallel()

	var key payments.IdempotencyKey
	if !key.IsZero() {
		t.Fatal("zero-value key should be zero")
	}
	if payments.CreateIntentKey("", "").IsZero() {
		t.Fatal("key with operation should not be zero")
	}
}
