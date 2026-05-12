package webhooks

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestHandleWithOptionsDispatchesVerifiedJSONEnvelope(t *testing.T) {
	t.Setenv("TEST_WEBHOOK_SECRET", "secret")

	body := `{"id":"evt_123","org":{"id":"org_1"},"external_id":"ext_9"}`
	contract := testReceiverContract()
	contract.TenantFrom = &TenantFromSpec{Path: "payload.org.id"}
	contract.IdempotencyBy = "payload.org.id, payload.external_id"

	var captured Envelope
	handler := func(_ context.Context, envelope Envelope) (any, error) {
		captured = envelope
		return map[string]string{"received": envelope.ID}, nil
	}

	rec := httptest.NewRecorder()
	HandleWithOptions(
		rec,
		newSignedReceiverRequest(http.MethodPost, body, contract.Verify.Header, "secret"),
		contract,
		handler,
		ReceiverOptions{IdempotencyStore: NewMemoryIdempotencyStore()},
	)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d: %s", rec.Code, http.StatusOK, rec.Body.String())
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var response map[string]string
	if err := json.Unmarshal(rec.Body.Bytes(), &response); err != nil {
		t.Fatalf("response JSON error: %v", err)
	}
	wantID := "org_1" + compoundReceiverKeySeparator + "ext_9"
	if response["received"] != wantID {
		t.Fatalf("response received = %q, want %q", response["received"], wantID)
	}

	if captured.ID != wantID {
		t.Fatalf("envelope ID = %q, want %q", captured.ID, wantID)
	}
	if captured.Tenant != "org_1" {
		t.Fatalf("envelope Tenant = %q, want org_1", captured.Tenant)
	}
	if string(captured.Body) != body {
		t.Fatalf("envelope Body = %q, want %q", string(captured.Body), body)
	}
	if captured.ParsedPayload["id"] != "evt_123" {
		t.Fatalf("ParsedPayload id = %v, want evt_123", captured.ParsedPayload["id"])
	}
	if captured.Header[contract.Verify.Header] == "" {
		t.Fatalf("envelope Header[%q] is empty", contract.Verify.Header)
	}
}

func TestHandleWithOptionsRejectsDuplicateEnvelope(t *testing.T) {
	t.Setenv("TEST_WEBHOOK_SECRET", "secret")

	body := `{"id":"evt_123"}`
	contract := testReceiverContract()
	store := NewMemoryIdempotencyStore()
	calls := 0
	handler := func(context.Context, Envelope) (any, error) {
		calls++
		return nil, nil
	}

	first := httptest.NewRecorder()
	HandleWithOptions(
		first,
		newSignedReceiverRequest(http.MethodPost, body, contract.Verify.Header, "secret"),
		contract,
		handler,
		ReceiverOptions{IdempotencyStore: store},
	)
	if first.Code != http.StatusNoContent {
		t.Fatalf("first status = %d, want %d: %s", first.Code, http.StatusNoContent, first.Body.String())
	}

	second := httptest.NewRecorder()
	HandleWithOptions(
		second,
		newSignedReceiverRequest(http.MethodPost, body, contract.Verify.Header, "secret"),
		contract,
		handler,
		ReceiverOptions{IdempotencyStore: store},
	)

	if second.Code != http.StatusConflict {
		t.Fatalf("second status = %d, want %d: %s", second.Code, http.StatusConflict, second.Body.String())
	}
	if calls != 1 {
		t.Fatalf("handler calls = %d, want 1", calls)
	}
	if got := decodeReceiverErrorCode(t, second); got != "duplicate_envelope" {
		t.Fatalf("error code = %q, want duplicate_envelope", got)
	}
}

func TestHandleWithOptionsRejectsInvalidHMAC(t *testing.T) {
	t.Setenv("TEST_WEBHOOK_SECRET", "secret")

	contract := testReceiverContract()
	called := false
	req := httptest.NewRequest(http.MethodPost, "/webhooks/test", strings.NewReader(`{"id":"evt_123"}`))
	req.Header.Set(contract.Verify.Header, "bad-signature")

	rec := httptest.NewRecorder()
	HandleWithOptions(rec, req, contract, func(context.Context, Envelope) (any, error) {
		called = true
		return nil, nil
	}, ReceiverOptions{})

	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want %d: %s", rec.Code, http.StatusUnauthorized, rec.Body.String())
	}
	if called {
		t.Fatal("handler was called")
	}
	if got := decodeReceiverErrorCode(t, rec); got != "hmac_invalid" {
		t.Fatalf("error code = %q, want hmac_invalid", got)
	}
}

func TestHandleWithOptionsRejectsInvalidJSONPayload(t *testing.T) {
	t.Setenv("TEST_WEBHOOK_SECRET", "secret")

	body := `{`
	contract := testReceiverContract()

	rec := httptest.NewRecorder()
	HandleWithOptions(
		rec,
		newSignedReceiverRequest(http.MethodPost, body, contract.Verify.Header, "secret"),
		contract,
		func(context.Context, Envelope) (any, error) { return nil, nil },
		ReceiverOptions{},
	)

	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d: %s", rec.Code, http.StatusBadRequest, rec.Body.String())
	}
	if got := decodeReceiverErrorCode(t, rec); got != "invalid_json" {
		t.Fatalf("error code = %q, want invalid_json", got)
	}
}

func TestHandleWithOptionsHonorsMaxBodyBytes(t *testing.T) {
	contract := testReceiverContract()

	rec := httptest.NewRecorder()
	HandleWithOptions(
		rec,
		httptest.NewRequest(http.MethodPost, "/webhooks/test", strings.NewReader(`{}`)),
		contract,
		func(context.Context, Envelope) (any, error) { return nil, nil },
		ReceiverOptions{MaxBodyBytes: 1},
	)

	if rec.Code != http.StatusRequestEntityTooLarge {
		t.Fatalf("status = %d, want %d: %s", rec.Code, http.StatusRequestEntityTooLarge, rec.Body.String())
	}
	if got := decodeReceiverErrorCode(t, rec); got != "body_too_large" {
		t.Fatalf("error code = %q, want body_too_large", got)
	}
}

func TestHandleWithOptionsRejectsReplayOutsideWindow(t *testing.T) {
	t.Setenv("TEST_WEBHOOK_SECRET", "secret")

	body := `{"id":"evt_123"}`
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	contract := testReceiverContract()
	contract.Replay = &ReplaySpec{Mode: ReplayAllow, Window: "1h"}
	req := newSignedReceiverRequest(http.MethodPost, body, contract.Verify.Header, "secret")
	req.Header.Set(defaultReceiverReplayHeader, now.Add(-2*time.Hour).Format(time.RFC3339Nano))

	called := false
	rec := httptest.NewRecorder()
	HandleWithOptions(rec, req, contract, func(context.Context, Envelope) (any, error) {
		called = true
		return nil, nil
	}, ReceiverOptions{Now: func() time.Time { return now }})

	if rec.Code != http.StatusConflict {
		t.Fatalf("status = %d, want %d: %s", rec.Code, http.StatusConflict, rec.Body.String())
	}
	if called {
		t.Fatal("handler was called")
	}
	if got := decodeReceiverErrorCode(t, rec); got != "replay_window_expired" {
		t.Fatalf("error code = %q, want replay_window_expired", got)
	}
}

func TestHandleWithOptionsRunsReplayCheckHook(t *testing.T) {
	t.Setenv("TEST_WEBHOOK_SECRET", "secret")

	body := `{"id":"evt_123"}`
	contract := testReceiverContract()
	var replayEnvelope Envelope

	rec := httptest.NewRecorder()
	HandleWithOptions(
		rec,
		newSignedReceiverRequest(http.MethodPost, body, contract.Verify.Header, "secret"),
		contract,
		func(context.Context, Envelope) (any, error) {
			t.Fatal("handler should not run after replay hook error")
			return nil, nil
		},
		ReceiverOptions{
			ReplayCheck: func(_ context.Context, _ WebhookContract, _ *http.Request, envelope Envelope) error {
				replayEnvelope = envelope
				return ErrWebhookReplayDenied
			},
		},
	)

	if rec.Code != http.StatusConflict {
		t.Fatalf("status = %d, want %d: %s", rec.Code, http.StatusConflict, rec.Body.String())
	}
	if replayEnvelope.ID != "evt_123" {
		t.Fatalf("replay envelope ID = %q, want evt_123", replayEnvelope.ID)
	}
	if got := decodeReceiverErrorCode(t, rec); got != "replay_denied" {
		t.Fatalf("error code = %q, want replay_denied", got)
	}
}

func testReceiverContract() WebhookContract {
	return WebhookContract{
		Feature: "test",
		Name:    "inbound",
		Route:   "/webhooks/test",
		Verify: VerifySpec{
			Scheme:    VerifyHmac,
			Algorithm: "sha256",
			SecretEnv: "TEST_WEBHOOK_SECRET",
			Header:    "X-Test-Signature",
		},
		IdempotencyBy: "payload.id",
	}
}

func newSignedReceiverRequest(method, body, header, secret string) *http.Request {
	req := httptest.NewRequest(method, "/webhooks/test", strings.NewReader(body))
	req.Header.Set(header, signReceiverBody(secret, body))
	return req
}

func signReceiverBody(secret, body string) string {
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(body))
	return hex.EncodeToString(mac.Sum(nil))
}

func decodeReceiverErrorCode(t *testing.T, rec *httptest.ResponseRecorder) string {
	t.Helper()

	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var response receiverErrorResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &response); err != nil {
		t.Fatalf("error response JSON error: %v", err)
	}
	return response.Code
}
