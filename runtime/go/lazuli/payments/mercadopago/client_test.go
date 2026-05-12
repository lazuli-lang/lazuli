package mercadopago

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/payments"
)

func TestClientImplementsPaymentGateway(t *testing.T) {
	var _ payments.PaymentGateway = (*Client)(nil)
}

func TestCreatePaymentIntentPostsCheckoutPreference(t *testing.T) {
	expiresAt := time.Date(2026, 5, 12, 15, 30, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Fatalf("method = %s, want POST", r.Method)
		}
		if r.URL.Path != "/checkout/preferences" {
			t.Fatalf("path = %s, want /checkout/preferences", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer test-token" {
			t.Fatalf("Authorization = %q, want bearer token", got)
		}
		if got := r.Header.Get(HeaderIdempotencyKey); got != "pref-key" {
			t.Fatalf("%s = %q, want pref-key", HeaderIdempotencyKey, got)
		}

		var payload createPreferenceRequest
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			t.Fatalf("decode payload: %v", err)
		}
		if payload.ExternalReference != "txn-123" {
			t.Fatalf("external_reference = %q, want txn-123", payload.ExternalReference)
		}
		if payload.NotificationURL != "https://example.test/webhooks/mp" {
			t.Fatalf("notification_url = %q", payload.NotificationURL)
		}
		if payload.BackURLs.Success != "https://example.test/success" {
			t.Fatalf("success URL = %q", payload.BackURLs.Success)
		}
		if !payload.Expires || payload.ExpirationDateTo == "" {
			t.Fatalf("expiration fields = expires:%v expiration_date_to:%q", payload.Expires, payload.ExpirationDateTo)
		}
		if len(payload.Items) != 1 {
			t.Fatalf("items len = %d, want 1", len(payload.Items))
		}
		item := payload.Items[0]
		if item.ID != "sku-1" || item.Title != "Plan Pro" || item.Quantity != 2 || item.CurrencyID != "BRL" {
			t.Fatalf("item = %+v", item)
		}
		if item.UnitPrice != 12.34 {
			t.Fatalf("unit_price = %v, want 12.34", item.UnitPrice)
		}
		if payload.Payer.Email != "buyer@example.test" || payload.Payer.Identification.Number != "12345678900" {
			t.Fatalf("payer = %+v", payload.Payer)
		}

		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"id":                 "pref_123",
			"init_point":         "https://www.mercadopago.com/checkout/v1/redirect?pref_id=pref_123",
			"status":             "pending",
			"expiration_date_to": expiresAt.Format(time.RFC3339),
		})
	}))
	defer server.Close()

	client := NewClient("test-token", WithBaseURL(server.URL), WithHTTPClient(server.Client()))
	intent, err := client.CreatePaymentIntent(context.Background(), payments.CreatePaymentIntentRequest{
		Contract: payments.PaymentContract{
			Currency: "BRL",
		},
		TransactionID: "txn-123",
		Amount: payments.Money{
			Amount:   2468,
			Currency: "BRL",
		},
		Description: "Plan Pro subscription",
		Items: []payments.LineItem{{
			ID:         "sku-1",
			Title:      "Plan Pro",
			Quantity:   2,
			UnitAmount: payments.Money{Amount: 1234, Currency: "BRL"},
		}},
		Payer: payments.Payer{
			Email:    "buyer@example.test",
			Document: "12345678900",
		},
		SuccessURL:      "https://example.test/success",
		PendingURL:      "https://example.test/pending",
		FailureURL:      "https://example.test/failure",
		NotificationURL: "https://example.test/webhooks/mp",
		ExpiresAt:       expiresAt,
		IdempotencyKey:  "pref-key",
		Metadata:        map[string]string{"tenant": "tenant-1"},
	})
	if err != nil {
		t.Fatalf("CreatePaymentIntent returned error: %v", err)
	}
	if intent.Provider != ProviderName || intent.ProviderID != "pref_123" || intent.ID != "txn-123" {
		t.Fatalf("intent identity = %+v", intent)
	}
	if intent.Status != payments.PaymentStatusPending {
		t.Fatalf("intent status = %q, want pending", intent.Status)
	}
	if intent.CheckoutURL == "" || !strings.Contains(intent.CheckoutURL, "pref_123") {
		t.Fatalf("checkout URL = %q", intent.CheckoutURL)
	}
	if !intent.ExpiresAt.Equal(expiresAt) {
		t.Fatalf("ExpiresAt = %s, want %s", intent.ExpiresAt, expiresAt)
	}
	if intent.Metadata["tenant"] != "tenant-1" {
		t.Fatalf("metadata = %+v", intent.Metadata)
	}
}

func TestRefundPaymentPostsRefundWithIdempotency(t *testing.T) {
	createdAt := time.Date(2026, 5, 12, 16, 0, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Fatalf("method = %s, want POST", r.Method)
		}
		if r.URL.Path != "/v1/payments/pay_123/refunds" {
			t.Fatalf("path = %s, want /v1/payments/pay_123/refunds", r.URL.Path)
		}
		if got := r.Header.Get(HeaderIdempotencyKey); got != "refund-key" {
			t.Fatalf("%s = %q, want refund-key", HeaderIdempotencyKey, got)
		}
		var payload refundRequest
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			t.Fatalf("decode payload: %v", err)
		}
		if payload.Amount != 5.5 {
			t.Fatalf("amount = %v, want 5.5", payload.Amount)
		}

		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"id":           987654,
			"payment_id":   "pay_123",
			"status":       "approved",
			"amount":       5.5,
			"date_created": createdAt.Format(time.RFC3339),
		})
	}))
	defer server.Close()

	client := NewClient("test-token", WithBaseURL(server.URL), WithHTTPClient(server.Client()))
	refund, err := client.RefundPayment(context.Background(), payments.RefundPaymentRequest{
		PaymentID:      "pay_123",
		Amount:         payments.Money{Amount: 550, Currency: "BRL"},
		Reason:         "customer_request",
		IdempotencyKey: "refund-key",
	})
	if err != nil {
		t.Fatalf("RefundPayment returned error: %v", err)
	}
	if refund.Provider != ProviderName || refund.ProviderID != "987654" || refund.ID != "987654" {
		t.Fatalf("refund identity = %+v", refund)
	}
	if refund.PaymentID != "pay_123" {
		t.Fatalf("PaymentID = %q, want pay_123", refund.PaymentID)
	}
	if refund.Status != payments.RefundStatusSucceeded {
		t.Fatalf("status = %q, want succeeded", refund.Status)
	}
	if refund.Amount != (payments.Money{Amount: 550, Currency: "BRL"}) {
		t.Fatalf("amount = %+v", refund.Amount)
	}
	if !refund.CreatedAt.Equal(createdAt) {
		t.Fatalf("CreatedAt = %s, want %s", refund.CreatedAt, createdAt)
	}
}

func TestCapturePaymentPutsCaptureTrue(t *testing.T) {
	approvedAt := time.Date(2026, 5, 12, 17, 0, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPut {
			t.Fatalf("method = %s, want PUT", r.Method)
		}
		if r.URL.Path != "/v1/payments/provider_pay_123" {
			t.Fatalf("path = %s, want /v1/payments/provider_pay_123", r.URL.Path)
		}
		var payload captureRequest
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			t.Fatalf("decode payload: %v", err)
		}
		if !payload.Capture {
			t.Fatal("capture = false, want true")
		}
		if payload.TransactionAmount != 12.34 {
			t.Fatalf("transaction_amount = %v, want 12.34", payload.TransactionAmount)
		}

		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"id":                 "provider_pay_123",
			"status":             "approved",
			"transaction_amount": 12.34,
			"currency_id":        "BRL",
			"date_approved":      approvedAt.Format(time.RFC3339),
			"payment_method_id":  "pix",
		})
	}))
	defer server.Close()

	client := NewClient("test-token", WithBaseURL(server.URL), WithHTTPClient(server.Client()))
	payment, err := client.CapturePayment(context.Background(), payments.CapturePaymentRequest{
		PaymentID:      "local-pay-123",
		ProviderID:     "provider_pay_123",
		Amount:         payments.Money{Amount: 1234, Currency: "BRL"},
		IdempotencyKey: "capture-key",
	})
	if err != nil {
		t.Fatalf("CapturePayment returned error: %v", err)
	}
	if payment.ID != "local-pay-123" || payment.ProviderID != "provider_pay_123" {
		t.Fatalf("payment identity = %+v", payment)
	}
	if payment.Status != payments.PaymentStatusCaptured {
		t.Fatalf("status = %q, want captured", payment.Status)
	}
	if payment.CapturedAmount != (payments.Money{Amount: 1234, Currency: "BRL"}) {
		t.Fatalf("captured amount = %+v", payment.CapturedAmount)
	}
	if payment.PaymentMethod != "pix" {
		t.Fatalf("payment method = %q, want pix", payment.PaymentMethod)
	}
	if !payment.PaidAt.Equal(approvedAt) {
		t.Fatalf("PaidAt = %s, want %s", payment.PaidAt, approvedAt)
	}
}

func TestUnsupportedOperationsReturnTypedErrors(t *testing.T) {
	client := NewClient("test-token")

	_, err := client.ConfirmPayment(context.Background(), payments.ConfirmPaymentRequest{})
	assertUnsupportedOperation(t, err, "confirm_payment")

	_, err = client.ParseWebhookEvent(context.Background(), payments.WebhookRequest{})
	assertUnsupportedOperation(t, err, "parse_webhook_event")
}

func TestClientStatusErrorWrapsPaymentSentinel(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "missing payment", http.StatusNotFound)
	}))
	defer server.Close()

	client := NewClient("test-token", WithBaseURL(server.URL), WithHTTPClient(server.Client()))
	_, err := client.CapturePayment(context.Background(), payments.CapturePaymentRequest{
		PaymentID: "missing",
	})
	if !errors.Is(err, payments.ErrPaymentNotFound) {
		t.Fatalf("CapturePayment error = %v, want ErrPaymentNotFound", err)
	}
	var apiErr *APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("CapturePayment error = %T, want *APIError", err)
	}
	if apiErr.StatusCode != http.StatusNotFound {
		t.Fatalf("status = %d, want 404", apiErr.StatusCode)
	}
}

func TestRefundPaymentRequiresIdempotencyKey(t *testing.T) {
	client := NewClient("test-token")

	_, err := client.RefundPayment(context.Background(), payments.RefundPaymentRequest{
		PaymentID: "pay_123",
	})
	if !errors.Is(err, payments.ErrInvalidPaymentRequest) {
		t.Fatalf("RefundPayment error = %v, want ErrInvalidPaymentRequest", err)
	}
	if !errors.Is(err, ErrIdempotencyKeyMissing) {
		t.Fatalf("RefundPayment error = %v, want ErrIdempotencyKeyMissing", err)
	}
}

func assertUnsupportedOperation(t *testing.T, err error, operation string) {
	t.Helper()

	if !errors.Is(err, payments.ErrGatewayUnsupported) {
		t.Fatalf("error = %v, want ErrGatewayUnsupported", err)
	}
	var unsupported UnsupportedOperationError
	if !errors.As(err, &unsupported) {
		t.Fatalf("error = %T, want UnsupportedOperationError", err)
	}
	if unsupported.Operation != operation {
		t.Fatalf("operation = %q, want %q", unsupported.Operation, operation)
	}
}

type createPreferenceRequest struct {
	Items             []createPreferenceItem `json:"items"`
	Payer             createPreferencePayer  `json:"payer"`
	ExternalReference string                 `json:"external_reference"`
	BackURLs          createPreferenceURLs   `json:"back_urls"`
	NotificationURL   string                 `json:"notification_url"`
	Expires           bool                   `json:"expires"`
	ExpirationDateTo  string                 `json:"expiration_date_to"`
}

type createPreferenceItem struct {
	ID         string  `json:"id"`
	Title      string  `json:"title"`
	Quantity   int64   `json:"quantity"`
	CurrencyID string  `json:"currency_id"`
	UnitPrice  float64 `json:"unit_price"`
}

type createPreferencePayer struct {
	Email          string `json:"email"`
	Identification struct {
		Number string `json:"number"`
	} `json:"identification"`
}

type createPreferenceURLs struct {
	Success string `json:"success"`
	Pending string `json:"pending"`
	Failure string `json:"failure"`
}

type refundRequest struct {
	Amount float64 `json:"amount"`
}

type captureRequest struct {
	Capture           bool    `json:"capture"`
	TransactionAmount float64 `json:"transaction_amount"`
}
