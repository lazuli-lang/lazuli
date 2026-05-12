package notifications

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"
)

func TestInAppDispatcherStoresAndManagesMessages(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	dispatcher := NewInAppDispatcher()
	dispatcher.Clock = func() time.Time { return now }

	payload := map[string]any{
		"subject": "Welcome",
		"nested":  map[string]any{"count": 1},
		"items":   []any{"one", map[string]any{"two": 2}},
	}
	templateData := map[string]any{"name": "Ada"}

	if dispatcher.Channel() != ChannelInApp {
		t.Fatalf("Channel() = %q, want %q", dispatcher.Channel(), ChannelInApp)
	}
	if err := dispatcher.Dispatch(ctx, Envelope{
		ID:           "msg-1",
		Tenant:       "tenant-1",
		Channel:      ChannelInApp,
		Recipient:    "user-1",
		Payload:      payload,
		TemplateData: templateData,
	}); err != nil {
		t.Fatalf("Dispatch() error = %v", err)
	}

	payload["subject"] = "changed"
	payload["nested"].(map[string]any)["count"] = 99
	payload["items"].([]any)[1].(map[string]any)["two"] = 99
	templateData["name"] = "changed"

	messages, err := dispatcher.List(ctx, "user-1")
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(messages) != 1 {
		t.Fatalf("List() returned %d messages, want 1", len(messages))
	}
	msg := messages[0]
	if msg.ID != "msg-1" || msg.Tenant != "tenant-1" || msg.Recipient != "user-1" {
		t.Fatalf("message identity = (%q, %q, %q), want (msg-1, tenant-1, user-1)", msg.ID, msg.Tenant, msg.Recipient)
	}
	if msg.Payload["subject"] != "Welcome" {
		t.Fatalf("Payload[subject] = %q, want Welcome", msg.Payload["subject"])
	}
	if msg.Payload["nested"].(map[string]any)["count"] != 1 {
		t.Fatalf("nested payload was not defensively copied: %#v", msg.Payload["nested"])
	}
	if msg.Payload["items"].([]any)[1].(map[string]any)["two"] != 2 {
		t.Fatalf("slice payload was not defensively copied: %#v", msg.Payload["items"])
	}
	if msg.TemplateData["name"] != "Ada" {
		t.Fatalf("TemplateData[name] = %q, want Ada", msg.TemplateData["name"])
	}
	if !msg.CreatedAt.Equal(now) {
		t.Fatalf("CreatedAt = %v, want %v", msg.CreatedAt, now)
	}
	if msg.Acknowledged {
		t.Fatal("new message is acknowledged")
	}

	messages[0].Payload["subject"] = "mutated from list"
	messages, err = dispatcher.List(ctx, "user-1")
	if err != nil {
		t.Fatalf("List() after mutation error = %v", err)
	}
	if messages[0].Payload["subject"] != "Welcome" {
		t.Fatalf("List() returned shared payload map: %q", messages[0].Payload["subject"])
	}

	ackAt := now.Add(time.Minute)
	dispatcher.Clock = func() time.Time { return ackAt }
	if err := dispatcher.Ack(ctx, "user-1", "msg-1"); err != nil {
		t.Fatalf("Ack() error = %v", err)
	}
	messages, err = dispatcher.List(ctx, "user-1")
	if err != nil {
		t.Fatalf("List() after Ack error = %v", err)
	}
	if !messages[0].Acknowledged {
		t.Fatal("Ack() did not mark message acknowledged")
	}
	if !messages[0].AckedAt.Equal(ackAt) {
		t.Fatalf("AckedAt = %v, want %v", messages[0].AckedAt, ackAt)
	}

	if err := dispatcher.Delete(ctx, "user-1", "msg-1"); err != nil {
		t.Fatalf("Delete() error = %v", err)
	}
	messages, err = dispatcher.List(ctx, "user-1")
	if err != nil {
		t.Fatalf("List() after Delete error = %v", err)
	}
	if len(messages) != 0 {
		t.Fatalf("List() after Delete returned %d messages, want 0", len(messages))
	}
	if err := dispatcher.Delete(ctx, "user-1", "msg-1"); !errors.Is(err, ErrInAppMessageNotFound) {
		t.Fatalf("Delete() missing error = %v, want ErrInAppMessageNotFound", err)
	}
	if err := dispatcher.Ack(ctx, "user-1", "msg-1"); !errors.Is(err, ErrInAppMessageNotFound) {
		t.Fatalf("Ack() missing error = %v, want ErrInAppMessageNotFound", err)
	}
}

func TestInAppDispatcherListsByRecipientAndGeneratesIDs(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	dispatcher := NewInAppDispatcher()

	for _, recipient := range []string{"user-1", "user-1", "user-2"} {
		if err := dispatcher.Dispatch(ctx, Envelope{Recipient: recipient}); err != nil {
			t.Fatalf("Dispatch(%q) error = %v", recipient, err)
		}
	}

	userOne, err := dispatcher.List(ctx, "user-1")
	if err != nil {
		t.Fatalf("List(user-1) error = %v", err)
	}
	if len(userOne) != 2 {
		t.Fatalf("List(user-1) returned %d messages, want 2", len(userOne))
	}
	if userOne[0].ID == "" || userOne[1].ID == "" || userOne[0].ID == userOne[1].ID {
		t.Fatalf("generated IDs = %q, %q; want unique non-empty IDs", userOne[0].ID, userOne[1].ID)
	}
	secondID := userOne[1].ID
	if err := dispatcher.Delete(ctx, "user-1", userOne[0].ID); err != nil {
		t.Fatalf("Delete(first user-1 message) error = %v", err)
	}
	userOne, err = dispatcher.List(ctx, "user-1")
	if err != nil {
		t.Fatalf("List(user-1) after Delete error = %v", err)
	}
	if len(userOne) != 1 || userOne[0].ID != secondID {
		t.Fatalf("List(user-1) after Delete = %#v, want remaining message %q", userOne, secondID)
	}

	userTwo, err := dispatcher.List(ctx, "user-2")
	if err != nil {
		t.Fatalf("List(user-2) error = %v", err)
	}
	if len(userTwo) != 1 {
		t.Fatalf("List(user-2) returned %d messages, want 1", len(userTwo))
	}
}

func TestInAppDispatcherZeroValueIsUsable(t *testing.T) {
	t.Parallel()

	var dispatcher InAppDispatcher
	if err := dispatcher.Dispatch(context.Background(), Envelope{Recipient: "user-1"}); err != nil {
		t.Fatalf("Dispatch() error = %v", err)
	}
	messages, err := dispatcher.List(context.Background(), "user-1")
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(messages) != 1 {
		t.Fatalf("List() returned %d messages, want 1", len(messages))
	}
}

func TestInAppDispatcherConcurrentDispatch(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	dispatcher := NewInAppDispatcher()

	const count = 100
	var wg sync.WaitGroup
	wg.Add(count)
	for range count {
		go func() {
			defer wg.Done()
			if err := dispatcher.Dispatch(ctx, Envelope{Recipient: "user-1"}); err != nil {
				t.Errorf("Dispatch() error = %v", err)
			}
		}()
	}
	wg.Wait()

	messages, err := dispatcher.List(ctx, "user-1")
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(messages) != count {
		t.Fatalf("List() returned %d messages, want %d", len(messages), count)
	}
}

func TestInAppDispatcherHonorsCanceledContext(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	dispatcher := NewInAppDispatcher()
	if err := dispatcher.Dispatch(ctx, Envelope{Recipient: "user-1"}); !errors.Is(err, context.Canceled) {
		t.Fatalf("Dispatch() error = %v, want context.Canceled", err)
	}
	if _, err := dispatcher.List(ctx, "user-1"); !errors.Is(err, context.Canceled) {
		t.Fatalf("List() error = %v, want context.Canceled", err)
	}
	if err := dispatcher.Ack(ctx, "user-1", "msg-1"); !errors.Is(err, context.Canceled) {
		t.Fatalf("Ack() error = %v, want context.Canceled", err)
	}
	if err := dispatcher.Delete(ctx, "user-1", "msg-1"); !errors.Is(err, context.Canceled) {
		t.Fatalf("Delete() error = %v, want context.Canceled", err)
	}
}
