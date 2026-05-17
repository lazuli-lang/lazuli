// Dispatcher tests — exercise the wire-thin Send orchestration:
// throttle/digest skip paths, retry policy, emit publishing, content-
// addressed envelope IDs. Uses fake ChannelDispatcher to avoid real
// channel adapter wiring.
package notifications_test

import (
	"context"
	"errors"
	"strings"
	"sync"
	"sync/atomic"
	"testing"

	"lazuli.dev/runtime/lazuli/notifications"
)

type fakeDispatcher struct {
	channel  notifications.Channel
	calls    atomic.Int32
	failN    int32 // fail the first N dispatches (then succeed)
	err      error
	mu       sync.Mutex
	envIDs   []string
	received []string
}

func (f *fakeDispatcher) Channel() notifications.Channel { return f.channel }

func (f *fakeDispatcher) Dispatch(_ context.Context, env notifications.Envelope) error {
	n := f.calls.Add(1)
	f.mu.Lock()
	f.envIDs = append(f.envIDs, env.ID)
	f.received = append(f.received, env.Recipient)
	f.mu.Unlock()
	if n <= f.failN {
		if f.err != nil {
			return f.err
		}
		return errors.New("simulated dispatch failure")
	}
	return nil
}

func contract() notifications.NotificationContract {
	return notifications.NotificationContract{
		Feature:   "customer_outreach",
		Name:      "welcome_email",
		Channels:  []notifications.Channel{notifications.ChannelEmail},
		Recipient: "target.email",
	}
}

func TestSendHappyPath(t *testing.T) {
	t.Parallel()

	reg := notifications.NewRegistry()
	disp := &fakeDispatcher{channel: notifications.ChannelEmail}
	_ = reg.Register(disp)

	err := notifications.Send(context.Background(), reg, contract(), map[string]any{
		"target": map[string]any{"email": "alice@example.com"},
	})
	if err != nil {
		t.Fatalf("Send returned %v, want nil", err)
	}
	if disp.calls.Load() != 1 {
		t.Fatalf("dispatcher calls = %d, want 1", disp.calls.Load())
	}
	if disp.received[0] != "alice@example.com" {
		t.Fatalf("recipient = %q, want alice@example.com", disp.received[0])
	}
	if disp.envIDs[0] == "" {
		t.Fatalf("envelope ID empty; expected content-addressed hash")
	}
}

func TestSendSkipOnUnresolvedRecipient(t *testing.T) {
	t.Parallel()

	reg := notifications.NewRegistry()
	disp := &fakeDispatcher{channel: notifications.ChannelEmail}
	_ = reg.Register(disp)

	// Path resolves to empty (no `target.email` in payload).
	err := notifications.Send(context.Background(), reg, contract(), map[string]any{})
	if err != nil {
		t.Fatalf("Send returned %v, want nil (skip)", err)
	}
	if disp.calls.Load() != 0 {
		t.Fatalf("dispatcher called %d times, want 0", disp.calls.Load())
	}
}

func TestSendThrottleSkipReturnsNilNotError(t *testing.T) {
	t.Parallel()

	reg := notifications.NewRegistry()
	disp := &fakeDispatcher{channel: notifications.ChannelEmail}
	_ = reg.Register(disp)
	reg.RegisterThrottleStore(notifications.NewMemoryThrottleStore())

	c := contract()
	c.Throttle = &notifications.NotificationThrottle{MaxPer: "1m", Burst: 1}
	payload := map[string]any{"target": map[string]any{"email": "alice@example.com"}}

	if err := notifications.Send(context.Background(), reg, c, payload); err != nil {
		t.Fatalf("first Send returned %v", err)
	}
	// Second call within window — throttle blocks; Send returns nil.
	if err := notifications.Send(context.Background(), reg, c, payload); err != nil {
		t.Fatalf("throttled Send returned %v, want nil (intentional skip)", err)
	}
	if disp.calls.Load() != 1 {
		t.Fatalf("dispatcher called %d times, want 1 (throttled)", disp.calls.Load())
	}
}

func TestSendRetryThenSucceed(t *testing.T) {
	t.Parallel()

	reg := notifications.NewRegistry()
	disp := &fakeDispatcher{channel: notifications.ChannelEmail, failN: 1}
	_ = reg.Register(disp)

	c := contract()
	c.Retry = &notifications.RetryPolicy{Count: 1, Backoff: "fixed"}
	payload := map[string]any{"target": map[string]any{"email": "alice@example.com"}}

	// NOTE: The retry backoff uses `jobs.NextDelay` which returns
	// 5s for "fixed". Inside `time.NewTimer` this blocks real time
	// unless wrapped in synctest. For the smoke test we just verify
	// the first attempt fails and the second succeeds — accept the
	// ~5s wait once. Skip the test in short mode.
	if testing.Short() {
		t.Skip("skipping retry timing test in short mode")
	}
	if err := notifications.Send(context.Background(), reg, c, payload); err != nil {
		t.Fatalf("Send returned %v after retry, want nil", err)
	}
	if disp.calls.Load() != 2 {
		t.Fatalf("dispatcher calls = %d, want 2 (1 fail + 1 succeed)", disp.calls.Load())
	}
}

// Meta-regression: assert the stub 501-shape error string never
// reappears from Send. Mirrors webhooks.TestHandleOneNotImplementedStatusGone
// in receive_test.go. The package-level wire_smoke test in
// internal/wiresmoke also grep-guards this literal across all impl
// files; this functional assertion is the symmetric per-call guard.
//
// Synth Wave 1 cell 02.
func TestSendNotYetImplementedErrorGone(t *testing.T) {
	t.Parallel()

	reg := notifications.NewRegistry()
	disp := &fakeDispatcher{channel: notifications.ChannelEmail}
	_ = reg.Register(disp)

	err := notifications.Send(context.Background(), reg, contract(), map[string]any{
		"target": map[string]any{"email": "alice@example.com"},
	})
	if err != nil && strings.Contains(err.Error(), "not yet implemented") {
		t.Fatalf("stub 501 error reappeared: %v", err)
	}
}

func TestSendDeterministicEnvelopeID(t *testing.T) {
	t.Parallel()

	reg := notifications.NewRegistry()
	disp := &fakeDispatcher{channel: notifications.ChannelEmail}
	_ = reg.Register(disp)

	c := contract()
	payload := map[string]any{
		"target":  map[string]any{"email": "alice@example.com"},
		"org_id":  "org_42",
		"trigger": "welcome",
	}

	_ = notifications.Send(context.Background(), reg, c, payload)
	_ = notifications.Send(context.Background(), reg, c, payload)

	if len(disp.envIDs) != 2 {
		t.Fatalf("expected 2 envelope IDs, got %d", len(disp.envIDs))
	}
	if disp.envIDs[0] != disp.envIDs[1] {
		t.Fatalf("envelope IDs differ across identical payloads: %q vs %q",
			disp.envIDs[0], disp.envIDs[1])
	}
}
