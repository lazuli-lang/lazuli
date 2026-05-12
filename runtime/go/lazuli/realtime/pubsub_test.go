package realtime

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestValidateTopic(t *testing.T) {
	t.Parallel()

	valid := []string{
		"orders.created",
		"tenant:acme/orders.updated",
		"feature_name/event-1",
	}
	for _, topic := range valid {
		if err := ValidateTopic(topic); err != nil {
			t.Fatalf("ValidateTopic(%q) error = %v, want nil", topic, err)
		}
	}

	invalid := []string{
		"",
		"orders created",
		"orders\ncreated",
		"orders*",
		"café",
		strings.Repeat("a", MaxTopicLength+1),
	}
	for _, topic := range invalid {
		if err := ValidateTopic(topic); !errors.Is(err, ErrInvalidTopic) {
			t.Fatalf("ValidateTopic(%q) error = %v, want ErrInvalidTopic", topic, err)
		}
	}
}

func TestHubPublishFansOutAndCopiesMessages(t *testing.T) {
	t.Parallel()

	hub := NewHub(WithSubscriberBuffer(2))
	first := mustSubscribe(t, hub, "tenant:acme/orders.updated")
	defer first.Unsubscribe()
	second := mustSubscribe(t, hub, "tenant:acme/orders.updated")
	defer second.Unsubscribe()

	payload := []byte("ready")
	result, err := hub.Publish(context.Background(), "tenant:acme/orders.updated", payload)
	if err != nil {
		t.Fatalf("Publish() error = %v", err)
	}
	if result.Subscribers != 2 || result.Delivered != 2 || result.Dropped != 0 || result.ErrorReports != 0 {
		t.Fatalf("Publish() result = %+v, want subscribers=2 delivered=2 dropped=0 reports=0", result)
	}
	payload[0] = 'R'

	firstMessage := receiveMessage(t, first)
	secondMessage := receiveMessage(t, second)
	if firstMessage.Topic != "tenant:acme/orders.updated" {
		t.Fatalf("first message topic = %q, want tenant:acme/orders.updated", firstMessage.Topic)
	}
	if string(firstMessage.Data) != "ready" {
		t.Fatalf("first message data = %q, want ready", firstMessage.Data)
	}
	if string(secondMessage.Data) != "ready" {
		t.Fatalf("second message data = %q, want ready", secondMessage.Data)
	}

	firstMessage.Data[0] = 'R'
	if string(secondMessage.Data) != "ready" {
		t.Fatalf("second message data after first mutation = %q, want ready", secondMessage.Data)
	}
}

func TestHubUnsubscribeStopsDeliveryAndClosesChannels(t *testing.T) {
	t.Parallel()

	hub := NewHub(WithSubscriberBuffer(1))
	sub := mustSubscribe(t, hub, "orders.created")
	sub.Unsubscribe()
	waitDone(t, sub)

	if _, ok := <-sub.Messages(); ok {
		t.Fatal("Messages() channel is open after Unsubscribe")
	}
	if _, ok := <-sub.Errors(); ok {
		t.Fatal("Errors() channel is open after Unsubscribe")
	}

	result, err := hub.Publish(context.Background(), "orders.created", []byte("late"))
	if err != nil {
		t.Fatalf("Publish() error = %v", err)
	}
	if result.Subscribers != 0 || result.Delivered != 0 || result.Dropped != 0 {
		t.Fatalf("Publish() after unsubscribe result = %+v, want no subscribers or delivery", result)
	}
}

func TestHubContextCancellationUnsubscribes(t *testing.T) {
	t.Parallel()

	hub := NewHub(WithSubscriberBuffer(1))
	ctx, cancel := context.WithCancel(context.Background())
	sub, err := hub.Subscribe(ctx, "orders.created")
	if err != nil {
		t.Fatalf("Subscribe() error = %v", err)
	}

	cancel()
	waitDone(t, sub)

	result, err := hub.Publish(context.Background(), "orders.created", []byte("late"))
	if err != nil {
		t.Fatalf("Publish() error = %v", err)
	}
	if result.Subscribers != 0 {
		t.Fatalf("Publish() subscribers after context cancellation = %d, want 0", result.Subscribers)
	}
}

func TestHubReturnsContextErrors(t *testing.T) {
	t.Parallel()

	hub := NewHub()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if _, err := hub.Subscribe(ctx, "orders.created"); !errors.Is(err, context.Canceled) {
		t.Fatalf("Subscribe() error = %v, want context.Canceled", err)
	}
	if _, err := hub.Publish(ctx, "orders.created", []byte("payload")); !errors.Is(err, context.Canceled) {
		t.Fatalf("Publish() error = %v, want context.Canceled", err)
	}
}

func TestHubRejectsInvalidTopics(t *testing.T) {
	t.Parallel()

	hub := NewHub()
	if _, err := hub.Subscribe(context.Background(), "orders created"); !errors.Is(err, ErrInvalidTopic) {
		t.Fatalf("Subscribe() error = %v, want ErrInvalidTopic", err)
	}
	if _, err := hub.Publish(context.Background(), "orders created", []byte("payload")); !errors.Is(err, ErrInvalidTopic) {
		t.Fatalf("Publish() error = %v, want ErrInvalidTopic", err)
	}
}

func TestHubDropsWhenSubscriberBufferIsFullAndReportsError(t *testing.T) {
	t.Parallel()

	hub := NewHub(WithSubscriberBuffer(1))
	sub := mustSubscribe(t, hub, "orders.created")
	defer sub.Unsubscribe()

	result, err := hub.Publish(context.Background(), "orders.created", []byte("first"))
	if err != nil {
		t.Fatalf("first Publish() error = %v", err)
	}
	if result.Delivered != 1 || result.Dropped != 0 || result.ErrorReports != 0 {
		t.Fatalf("first Publish() result = %+v, want delivered=1 dropped=0 reports=0", result)
	}

	result, err = hub.Publish(context.Background(), "orders.created", []byte("second"))
	if err != nil {
		t.Fatalf("second Publish() error = %v", err)
	}
	if result.Delivered != 0 || result.Dropped != 1 || result.ErrorReports != 1 {
		t.Fatalf("second Publish() result = %+v, want delivered=0 dropped=1 reports=1", result)
	}

	result, err = hub.Publish(context.Background(), "orders.created", []byte("third"))
	if err != nil {
		t.Fatalf("third Publish() error = %v", err)
	}
	if result.Delivered != 0 || result.Dropped != 1 || result.ErrorReports != 0 {
		t.Fatalf("third Publish() result = %+v, want delivered=0 dropped=1 reports=0", result)
	}

	message := receiveMessage(t, sub)
	if string(message.Data) != "first" {
		t.Fatalf("buffered message = %q, want first", message.Data)
	}

	report := receiveError(t, sub)
	if !errors.Is(report, ErrSubscriberBufferFull) {
		t.Fatalf("reported error = %v, want ErrSubscriberBufferFull", report)
	}
	var deliveryErr DeliveryError
	if !errors.As(report, &deliveryErr) {
		t.Fatalf("reported error type = %T, want DeliveryError", report)
	}
	if deliveryErr.Topic != "orders.created" {
		t.Fatalf("DeliveryError topic = %q, want orders.created", deliveryErr.Topic)
	}
}

func mustSubscribe(t *testing.T, hub *Hub, topic string) *Subscription {
	t.Helper()
	sub, err := hub.Subscribe(context.Background(), topic)
	if err != nil {
		t.Fatalf("Subscribe(%q) error = %v", topic, err)
	}
	return sub
}

func receiveMessage(t *testing.T, sub *Subscription) Message {
	t.Helper()
	select {
	case message := <-sub.Messages():
		return message
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for message")
		return Message{}
	}
}

func receiveError(t *testing.T, sub *Subscription) error {
	t.Helper()
	select {
	case err := <-sub.Errors():
		return err
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for error report")
		return nil
	}
}

func waitDone(t *testing.T, sub *Subscription) {
	t.Helper()
	select {
	case <-sub.Done():
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for subscription to close")
	}
}
