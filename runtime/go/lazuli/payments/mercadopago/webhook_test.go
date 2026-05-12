package mercadopago

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/webhooks"
)

func TestVerifyRequestAcceptsMercadoPagoSignature(t *testing.T) {
	const (
		dataID    = "123456"
		requestID = "bb56a2f1-6aae-46ac-982e-9dcd3581d08e"
		timestamp = "1742505638683"
		secret    = "secret"
	)
	req := signedRequest(t, secret, dataID, "payment", requestID, timestamp)

	if err := VerifyRequest(req, []byte(secret)); err != nil {
		t.Fatalf("VerifyRequest returned error: %v", err)
	}
}

func TestVerifyRequestRejectsTamperedManifestValue(t *testing.T) {
	req := signedRequest(t, "secret", "123456", "payment", "request-1", "1742505638683")
	req.URL.RawQuery = "data.id=999999&type=payment"

	err := VerifyRequest(req, []byte("secret"))
	if !errors.Is(err, webhooks.ErrWebhookHmacInvalid) {
		t.Fatalf("VerifyRequest error = %v, want ErrWebhookHmacInvalid", err)
	}
	if !errors.Is(err, ErrSignatureInvalid) {
		t.Fatalf("VerifyRequest error = %v, want ErrSignatureInvalid", err)
	}
}

func TestVerifySignatureOmitsMissingManifestValues(t *testing.T) {
	signature, err := Sign([]byte("secret"), "", "request-1", "1742505638683")
	if err != nil {
		t.Fatalf("Sign returned error: %v", err)
	}

	header := "ts=1742505638683,v1=" + strings.ToUpper(signature)
	if err := VerifySignature([]byte("secret"), "", "request-1", header); err != nil {
		t.Fatalf("VerifySignature returned error: %v", err)
	}
}

func TestParseSignatureHeaderRejectsMissingV1(t *testing.T) {
	_, err := ParseSignatureHeader("ts=1742505638683")

	if !errors.Is(err, ErrSignatureMalformed) {
		t.Fatalf("ParseSignatureHeader error = %v, want ErrSignatureMalformed", err)
	}
}

func TestManifestUsesMercadoPagoTemplateOrder(t *testing.T) {
	got := Manifest("123456", "request-1", "1742505638683")
	want := "id:123456;request-id:request-1;ts:1742505638683;"

	if got != want {
		t.Fatalf("Manifest = %q, want %q", got, want)
	}
}

func TestIdempotencyKeyFromRequestScopesByType(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/webhooks/mercadopago?data.id=123456&type=payment", nil)
	req.Header.Set(HeaderRequestID, "request-1")

	got, err := IdempotencyKeyFromRequest(req)
	if err != nil {
		t.Fatalf("IdempotencyKeyFromRequest returned error: %v", err)
	}
	if got != "payment:123456" {
		t.Fatalf("IdempotencyKeyFromRequest = %q, want payment:123456", got)
	}
}

func TestIdempotencyKeyFromRequestAcceptsTopicFallback(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/webhooks/mercadopago?data.id=123456&topic=merchant_order", nil)

	got, err := IdempotencyKeyFromRequest(req)
	if err != nil {
		t.Fatalf("IdempotencyKeyFromRequest returned error: %v", err)
	}
	if got != "merchant_order:123456" {
		t.Fatalf("IdempotencyKeyFromRequest = %q, want merchant_order:123456", got)
	}
}

func TestIdempotencyKeyFromRequestRequiresDataID(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/webhooks/mercadopago?type=payment", nil)

	_, err := IdempotencyKeyFromRequest(req)
	if !errors.Is(err, ErrIdempotencyKeyMissing) {
		t.Fatalf("IdempotencyKeyFromRequest error = %v, want ErrIdempotencyKeyMissing", err)
	}
}

func TestTimestampFromRequestParsesMilliseconds(t *testing.T) {
	req := signedRequest(t, "secret", "123456", "payment", "request-1", "1742505638683")

	got, err := TimestampFromRequest(req)
	if err != nil {
		t.Fatalf("TimestampFromRequest returned error: %v", err)
	}
	want := time.UnixMilli(1742505638683).UTC()
	if !got.Equal(want) {
		t.Fatalf("TimestampFromRequest = %s, want %s", got, want)
	}
}

func signedRequest(t *testing.T, secret, dataID, kind, requestID, timestamp string) *http.Request {
	t.Helper()

	signature, err := Sign([]byte(secret), dataID, requestID, timestamp)
	if err != nil {
		t.Fatalf("Sign returned error: %v", err)
	}

	req := httptest.NewRequest(http.MethodPost, "/webhooks/mercadopago?data.id="+dataID+"&type="+kind, nil)
	req.Header.Set(HeaderRequestID, requestID)
	req.Header.Set(HeaderSignature, "ts="+timestamp+",v1="+signature)
	return req
}
