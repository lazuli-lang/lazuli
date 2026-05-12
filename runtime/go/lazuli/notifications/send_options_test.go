package notifications

import (
	"context"
	"errors"
	"testing"
	"time"
)

type recordingDispatcher struct {
	channel Channel
	errs    []error
	calls   []Envelope
}

func (d *recordingDispatcher) Channel() Channel { return d.channel }

func (d *recordingDispatcher) Dispatch(_ context.Context, env Envelope) error {
	d.calls = append(d.calls, env)
	if len(d.errs) == 0 {
		return nil
	}
	err := d.errs[0]
	d.errs = d.errs[1:]
	return err
}

func TestSendWithOptionsDispatchesAllChannels(t *testing.T) {
	t.Parallel()

	email := &recordingDispatcher{channel: ChannelEmail}
	slack := &recordingDispatcher{channel: ChannelSlack}
	registry := NewRegistry()
	if err := registry.Register(email); err != nil {
		t.Fatalf("Register email: %v", err)
	}
	if err := registry.Register(slack); err != nil {
		t.Fatalf("Register slack: %v", err)
	}

	payload := map[string]any{
		"user": map[string]any{"email": "a@example.com"},
		"tenant": map[string]any{
			"id": "acme",
		},
	}
	contract := NotificationContract{
		Feature:    "billing",
		Name:       "invoice_ready",
		Channels:   []Channel{ChannelEmail, ChannelSlack},
		Recipient:  "payload.user.email",
		TenantFrom: &TenantFromSpec{Path: "payload.tenant.id"},
	}

	if err := SendWithOptions(context.Background(), registry, contract, payload, SendOptions{}); err != nil {
		t.Fatalf("SendWithOptions: %v", err)
	}

	if len(email.calls) != 1 {
		t.Fatalf("email calls = %d, want 1", len(email.calls))
	}
	if len(slack.calls) != 1 {
		t.Fatalf("slack calls = %d, want 1", len(slack.calls))
	}
	if got := email.calls[0].Recipient; got != "a@example.com" {
		t.Fatalf("email recipient = %q, want a@example.com", got)
	}
	if got := slack.calls[0].Channel; got != ChannelSlack {
		t.Fatalf("slack channel = %q, want %q", got, ChannelSlack)
	}
	if got := email.calls[0].Tenant; got != "acme" {
		t.Fatalf("tenant = %q, want acme", got)
	}
}

func TestSendWithOptionsUnsupportedChannel(t *testing.T) {
	t.Parallel()

	registry := NewRegistry()
	contract := NotificationContract{
		Feature:   "billing",
		Name:      "invoice_ready",
		Channels:  []Channel{ChannelEmail},
		Recipient: "payload.email",
	}

	err := SendWithOptions(
		context.Background(),
		registry,
		contract,
		map[string]any{"email": "a@example.com"},
		SendOptions{},
	)
	if !errors.Is(err, ErrNotificationChannelUnsupported) {
		t.Fatalf("error = %v, want ErrNotificationChannelUnsupported", err)
	}
}

func TestSendWithOptionsRetriesDelivery(t *testing.T) {
	t.Parallel()

	sentinel := errors.New("temporary delivery error")
	email := &recordingDispatcher{
		channel: ChannelEmail,
		errs:    []error{sentinel, sentinel, nil},
	}
	registry := NewRegistry()
	if err := registry.Register(email); err != nil {
		t.Fatalf("Register email: %v", err)
	}

	var delays []time.Duration
	contract := NotificationContract{
		Feature:   "billing",
		Name:      "invoice_ready",
		Channels:  []Channel{ChannelEmail},
		Recipient: "payload.email",
		Retry:     &RetryPolicy{Count: 2, Backoff: "fixed"},
	}

	err := SendWithOptions(
		context.Background(),
		registry,
		contract,
		map[string]any{"email": "a@example.com"},
		SendOptions{
			RetrySleep: func(_ context.Context, delay time.Duration) error {
				delays = append(delays, delay)
				return nil
			},
		},
	)
	if err != nil {
		t.Fatalf("SendWithOptions: %v", err)
	}
	if len(email.calls) != 3 {
		t.Fatalf("calls = %d, want 3", len(email.calls))
	}
	if len(delays) != 2 {
		t.Fatalf("delays = %d, want 2", len(delays))
	}
	for _, delay := range delays {
		if delay != 5*time.Second {
			t.Fatalf("delay = %s, want 5s", delay)
		}
	}
}

func TestSendWithOptionsDeliveryFailureWrapsLastError(t *testing.T) {
	t.Parallel()

	sentinel := errors.New("permanent delivery error")
	email := &recordingDispatcher{
		channel: ChannelEmail,
		errs:    []error{sentinel, sentinel},
	}
	registry := NewRegistry()
	if err := registry.Register(email); err != nil {
		t.Fatalf("Register email: %v", err)
	}

	contract := NotificationContract{
		Feature:   "billing",
		Name:      "invoice_ready",
		Channels:  []Channel{ChannelEmail},
		Recipient: "payload.email",
		Retry:     &RetryPolicy{Count: 1, Backoff: "fixed"},
	}

	err := SendWithOptions(
		context.Background(),
		registry,
		contract,
		map[string]any{"email": "a@example.com"},
		SendOptions{
			RetrySleep: func(context.Context, time.Duration) error { return nil },
		},
	)
	if !errors.Is(err, ErrNotificationDeliveryFailed) {
		t.Fatalf("error = %v, want ErrNotificationDeliveryFailed", err)
	}
	if !errors.Is(err, sentinel) {
		t.Fatalf("error = %v, want sentinel", err)
	}
}

func TestSendWithOptionsIdempotencySkipsDuplicate(t *testing.T) {
	t.Parallel()

	email := &recordingDispatcher{channel: ChannelEmail}
	registry := NewRegistry()
	if err := registry.Register(email); err != nil {
		t.Fatalf("Register email: %v", err)
	}

	contract := NotificationContract{
		Feature:     "billing",
		Name:        "invoice_ready",
		Channels:    []Channel{ChannelEmail},
		Recipient:   "payload.email",
		Idempotency: &IdempotencyKeySpec{Path: "payload.request_id"},
	}
	payload := map[string]any{
		"email":      "a@example.com",
		"request_id": "req-1",
	}
	opts := SendOptions{IdempotencyStore: NewMemoryIdempotencyStore()}

	if err := SendWithOptions(context.Background(), registry, contract, payload, opts); err != nil {
		t.Fatalf("first SendWithOptions: %v", err)
	}
	if err := SendWithOptions(context.Background(), registry, contract, payload, opts); err != nil {
		t.Fatalf("second SendWithOptions: %v", err)
	}
	if len(email.calls) != 1 {
		t.Fatalf("calls = %d, want 1", len(email.calls))
	}
	if got := email.calls[0].ID; got != "req-1" {
		t.Fatalf("envelope ID = %q, want req-1", got)
	}
}

func TestSendWithOptionsThrottleSkipsExhaustedBucket(t *testing.T) {
	t.Parallel()

	email := &recordingDispatcher{channel: ChannelEmail}
	registry := NewRegistry()
	if err := registry.Register(email); err != nil {
		t.Fatalf("Register email: %v", err)
	}

	contract := NotificationContract{
		Feature:   "billing",
		Name:      "invoice_ready",
		Channels:  []Channel{ChannelEmail},
		Recipient: "payload.email",
		Throttle: &NotificationThrottle{
			MaxPer:       "1m",
			PerRecipient: true,
			PerChannel:   true,
			Burst:        1,
		},
	}
	payload := map[string]any{"email": "a@example.com"}
	opts := SendOptions{ThrottleStore: NewMemoryThrottleStore()}

	if err := SendWithOptions(context.Background(), registry, contract, payload, opts); err != nil {
		t.Fatalf("first SendWithOptions: %v", err)
	}
	if err := SendWithOptions(context.Background(), registry, contract, payload, opts); err != nil {
		t.Fatalf("second SendWithOptions: %v", err)
	}
	if len(email.calls) != 1 {
		t.Fatalf("calls = %d, want 1", len(email.calls))
	}
}

func TestSendWithOptionsUnresolvedRecipient(t *testing.T) {
	t.Parallel()

	email := &recordingDispatcher{channel: ChannelEmail}
	registry := NewRegistry()
	if err := registry.Register(email); err != nil {
		t.Fatalf("Register email: %v", err)
	}

	contract := NotificationContract{
		Feature:   "billing",
		Name:      "invoice_ready",
		Channels:  []Channel{ChannelEmail},
		Recipient: "payload.user.email",
	}

	err := SendWithOptions(context.Background(), registry, contract, map[string]any{}, SendOptions{})
	if !errors.Is(err, ErrNotificationRecipientUnresolved) {
		t.Fatalf("error = %v, want ErrNotificationRecipientUnresolved", err)
	}
}
