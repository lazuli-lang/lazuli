// Receiver tests — exercise the wire-thin handleOne orchestration:
// HMAC verify, idempotency dedupe hook, tenant resolution, handler
// dispatch, emit publishing. Critical: status code 501 must NEVER
// surface from any path.
package webhooks_test

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"testing"

	"lazuli.dev/runtime/lazuli/webhooks"
)

func sign(secret, body []byte) string {
	mac := hmac.New(sha256.New, secret)
	mac.Write(body)
	return hex.EncodeToString(mac.Sum(nil))
}

func contract() webhooks.WebhookContract {
	return webhooks.WebhookContract{
		Feature:       "customer",
		Name:          "inbound",
		Route:         "/webhooks/inbound",
		IdempotencyBy: "id",
		Verify: webhooks.VerifySpec{
			Scheme:    webhooks.VerifyHmac,
			Algorithm: "sha256",
			SecretEnv: "INBOUND_SECRET",
			Header:    "X-Signature",
		},
	}
}

func newRequest(t *testing.T, body []byte, sig string) *http.Request {
	t.Helper()
	req := httptest.NewRequest("POST", "/webhooks/inbound", bytes.NewReader(body))
	req.Header.Set("X-Signature", sig)
	return req
}

// invokeHandleOne mounts the contract on a stub router and invokes
// the registered handler with a single request. The stub router
// captures the http.HandlerFunc so we can call it directly.
type captureRouter struct {
	handler http.HandlerFunc
}

func (c *captureRouter) Method(_, _ string, handler http.HandlerFunc) {
	c.handler = handler
}

func invoke(t *testing.T, c webhooks.WebhookContract, h webhooks.HandlerFunc, req *http.Request) *httptest.ResponseRecorder {
	t.Helper()
	router := &captureRouter{}
	if err := webhooks.Mount(router, []webhooks.WebhookContract{c}, []webhooks.HandlerFunc{h}); err != nil {
		t.Fatalf("Mount: %v", err)
	}
	rec := httptest.NewRecorder()
	router.handler(rec, req)
	return rec
}

func TestHandleOneNotImplementedStatusGone(t *testing.T) {
	t.Setenv("INBOUND_SECRET", "shh")
	body := []byte(`{"id":"evt_1","org_id":"org_42"}`)
	sig := sign([]byte("shh"), body)
	c := contract()
	called := false
	h := func(_ context.Context, _ webhooks.Envelope) (any, error) {
		called = true
		return nil, nil
	}
	rec := invoke(t, c, h, newRequest(t, body, sig))
	if rec.Code == http.StatusNotImplemented {
		t.Fatalf("status code 501 should never appear; got it")
	}
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	if !called {
		t.Fatalf("handler was not invoked")
	}
}

func TestHandleOneRejectsBadHmac(t *testing.T) {
	t.Setenv("INBOUND_SECRET", "shh")
	body := []byte(`{"id":"evt_1"}`)
	c := contract()
	h := func(_ context.Context, _ webhooks.Envelope) (any, error) {
		return nil, nil
	}
	rec := invoke(t, c, h, newRequest(t, body, "wrong-signature"))
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want 401", rec.Code)
	}
}

func TestHandleOneSurfacesTenantFromUnresolved(t *testing.T) {
	t.Setenv("INBOUND_SECRET", "shh")
	body := []byte(`{"id":"evt_1"}`)
	sig := sign([]byte("shh"), body)
	c := contract()
	c.TenantFrom = &webhooks.TenantFromSpec{Path: "org_id"}
	h := func(_ context.Context, _ webhooks.Envelope) (any, error) {
		return nil, nil
	}
	rec := invoke(t, c, h, newRequest(t, body, sig))
	if rec.Code != http.StatusUnprocessableEntity {
		t.Fatalf("status = %d, want 422", rec.Code)
	}
}

func TestHandleOneDedupeHookShortCircuits(t *testing.T) {
	t.Setenv("INBOUND_SECRET", "shh")
	body := []byte(`{"id":"evt_1","org_id":"org_42"}`)
	sig := sign([]byte("shh"), body)
	c := contract()
	called := false
	h := func(_ context.Context, _ webhooks.Envelope) (any, error) {
		called = true
		return nil, nil
	}

	webhooks.RegisterIdempotencyChecker(func(_ context.Context, _, _, key string) (bool, error) {
		return key == "evt_1", nil
	})
	defer webhooks.RegisterIdempotencyChecker(nil)

	rec := invoke(t, c, h, newRequest(t, body, sig))
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 on dedupe-hit", rec.Code)
	}
	if called {
		t.Fatalf("handler should NOT be invoked on dedupe-hit")
	}
}

func TestHandleOneHandlerErrorReturns500(t *testing.T) {
	t.Setenv("INBOUND_SECRET", "shh")
	body := []byte(`{"id":"evt_1"}`)
	sig := sign([]byte("shh"), body)
	c := contract()
	h := func(_ context.Context, _ webhooks.Envelope) (any, error) {
		return nil, errSimulated
	}
	rec := invoke(t, c, h, newRequest(t, body, sig))
	if rec.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500", rec.Code)
	}
}

var errSimulated = errSentinel("simulated handler failure")

type errSentinel string

func (e errSentinel) Error() string { return string(e) }
