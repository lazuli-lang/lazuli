package payments_test

import (
	"context"
	"errors"
	"testing"

	"lazuli.dev/runtime/lazuli/payments"
)

var _ payments.PaymentGateway = (*fakeGateway)(nil)

type fakeGateway struct{}

func (*fakeGateway) CreatePaymentIntent(
	_ context.Context,
	req payments.CreatePaymentIntentRequest,
) (payments.PaymentIntent, error) {
	return payments.PaymentIntent{
		ID:       req.TransactionID,
		Provider: req.Contract.Provider,
		Status:   payments.PaymentStatusCreated,
		Amount:   req.Amount,
	}, nil
}

func (*fakeGateway) ConfirmPayment(
	_ context.Context,
	req payments.ConfirmPaymentRequest,
) (payments.Payment, error) {
	return payments.Payment{
		ID:              req.ProviderID,
		Provider:        req.Contract.Provider,
		PaymentIntentID: req.PaymentIntentID,
		Status:          payments.PaymentStatusAuthorized,
		Amount:          req.Amount,
	}, nil
}

func (*fakeGateway) CapturePayment(
	_ context.Context,
	req payments.CapturePaymentRequest,
) (payments.Payment, error) {
	return payments.Payment{
		ID:             req.PaymentID,
		Provider:       req.Contract.Provider,
		Status:         payments.PaymentStatusCaptured,
		Amount:         req.Amount,
		CapturedAmount: req.Amount,
	}, nil
}

func (*fakeGateway) RefundPayment(
	_ context.Context,
	req payments.RefundPaymentRequest,
) (payments.Refund, error) {
	return payments.Refund{
		ID:        req.IdempotencyKey,
		Provider:  req.Contract.Provider,
		PaymentID: req.PaymentID,
		Status:    payments.RefundStatusRequested,
		Amount:    req.Amount,
	}, nil
}

func (*fakeGateway) ParseWebhookEvent(
	_ context.Context,
	req payments.WebhookRequest,
) (payments.WebhookEvent, error) {
	return payments.WebhookEvent{
		ID:       "evt_123",
		Provider: req.Provider,
		Type:     payments.WebhookEventPaymentUpdated,
	}, nil
}

func TestPaymentGatewayContractCompiles(t *testing.T) {
	t.Parallel()

	gateway := &fakeGateway{}
	contract := payments.PaymentContract{
		Feature:     "hostpoint",
		Name:        "checkout",
		Provider:    "test",
		Currency:    "BRL",
		CaptureMode: payments.CaptureModeManual,
	}
	amount := payments.Money{Amount: 2590, Currency: "BRL"}

	intent, err := gateway.CreatePaymentIntent(context.Background(), payments.CreatePaymentIntentRequest{
		Contract:      contract,
		Tenant:        "tenant-1",
		TransactionID: "txn_123",
		Amount:        amount,
	})
	if err != nil {
		t.Fatalf("CreatePaymentIntent failed: %v", err)
	}
	if intent.ID != "txn_123" || intent.Provider != "test" || intent.Amount != amount {
		t.Fatalf("intent = %+v", intent)
	}

	payment, err := gateway.CapturePayment(context.Background(), payments.CapturePaymentRequest{
		Contract:  contract,
		PaymentID: "pay_123",
		Amount:    amount,
	})
	if err != nil {
		t.Fatalf("CapturePayment failed: %v", err)
	}
	if payment.Status != payments.PaymentStatusCaptured {
		t.Fatalf("payment status = %q", payment.Status)
	}

	refund, err := gateway.RefundPayment(context.Background(), payments.RefundPaymentRequest{
		Contract:       contract,
		PaymentID:      payment.ID,
		Amount:         amount,
		IdempotencyKey: "refund-claim",
	})
	if err != nil {
		t.Fatalf("RefundPayment failed: %v", err)
	}
	if refund.Status != payments.RefundStatusRequested {
		t.Fatalf("refund status = %q", refund.Status)
	}
}

func TestStatusTerminalClassification(t *testing.T) {
	t.Parallel()

	if payments.PaymentStatusAuthorized.Terminal() {
		t.Fatal("authorized payment should not be terminal")
	}
	if !payments.PaymentStatusCaptured.Terminal() {
		t.Fatal("captured payment should be terminal")
	}
	if payments.RefundStatusPending.Terminal() {
		t.Fatal("pending refund should not be terminal")
	}
	if !payments.RefundStatusSucceeded.Terminal() {
		t.Fatal("succeeded refund should be terminal")
	}
}

func TestPaymentErrorsAreSentinels(t *testing.T) {
	t.Parallel()

	err := errors.Join(payments.ErrPaymentDeclined, payments.ErrGatewayUnavailable)
	if !errors.Is(err, payments.ErrPaymentDeclined) {
		t.Fatal("expected ErrPaymentDeclined sentinel")
	}
}
