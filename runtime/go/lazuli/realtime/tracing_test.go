package realtime

import (
	"strings"
	"testing"
	"time"
)

func TestConnectTraceEventNormalizesSafeAttributes(t *testing.T) {
	t.Parallel()

	event := NewConnectTraceEvent(ConnectTraceMetadata{
		Transport:        TraceTransport("Bearer secret-token"),
		State:            ConnectionState("tenant:user-1"),
		Accepted:         true,
		ReconnectAttempt: 2,
	})

	if event.Name != TraceEventConnect {
		t.Fatalf("Name = %q, want %q", event.Name, TraceEventConnect)
	}
	attrs := event.AttributeMap()
	if got := attrs[TraceAttributeTransport]; got != string(TraceTransportOther) {
		t.Fatalf("transport = %v, want other", got)
	}
	if got := attrs[TraceAttributeConnectionState]; got != "unknown" {
		t.Fatalf("state = %v, want unknown", got)
	}
	if got := attrs[TraceAttributeConnectionAccepted]; got != true {
		t.Fatalf("accepted = %v, want true", got)
	}
	if got := attrs[TraceAttributeReconnectAttempt]; got != 2 {
		t.Fatalf("reconnect_attempt = %v, want 2", got)
	}
	assertTraceAttrsDoNotContain(t, event, "secret-token", "tenant:user-1")
}

func TestDisconnectTraceEventUsesKnownReasonAndDuration(t *testing.T) {
	t.Parallel()

	event := NewDisconnectTraceEvent(DisconnectTraceMetadata{
		Transport: TraceTransportWebSocket,
		State:     ConnectionStateClosed,
		Reason:    TraceDisconnectReason("token expired for user@example.com"),
		Clean:     false,
		Duration:  1500 * time.Millisecond,
	})

	if event.Name != TraceEventDisconnect {
		t.Fatalf("Name = %q, want %q", event.Name, TraceEventDisconnect)
	}
	attrs := event.AttributeMap()
	if got := attrs[TraceAttributeTransport]; got != string(TraceTransportWebSocket) {
		t.Fatalf("transport = %v, want websocket", got)
	}
	if got := attrs[TraceAttributeConnectionState]; got != string(ConnectionStateClosed) {
		t.Fatalf("state = %v, want closed", got)
	}
	if got := attrs[TraceAttributeDisconnectReason]; got != string(TraceDisconnectReasonOther) {
		t.Fatalf("reason = %v, want other", got)
	}
	if got := attrs[TraceAttributeDisconnectClean]; got != false {
		t.Fatalf("clean = %v, want false", got)
	}
	if got := attrs[TraceAttributeDurationMS]; got != int64(1500) {
		t.Fatalf("duration_ms = %v, want 1500", got)
	}
	assertTraceAttrsDoNotContain(t, event, "user@example.com")
}

func TestBroadcastTraceEventReportsShapeAndFanoutOnly(t *testing.T) {
	t.Parallel()

	topic := "tenant:acme/orders.updated"
	event := NewBroadcastTraceEvent(BroadcastTraceMetadata{
		PayloadBytes: 24,
		Result: PublishResult{
			Topic:        topic,
			Subscribers:  3,
			Delivered:    2,
			Dropped:      1,
			ErrorReports: 1,
		},
	})

	if event.Name != TraceEventBroadcast {
		t.Fatalf("Name = %q, want %q", event.Name, TraceEventBroadcast)
	}
	attrs := event.AttributeMap()
	if got := attrs[TraceAttributeTopicPresent]; got != true {
		t.Fatalf("topic_present = %v, want true", got)
	}
	if got := attrs[TraceAttributeTopicLength]; got != len(topic) {
		t.Fatalf("topic_length = %v, want %d", got, len(topic))
	}
	if got := attrs[TraceAttributeTopicValid]; got != true {
		t.Fatalf("topic_valid = %v, want true", got)
	}
	if got := attrs[TraceAttributePayloadBytes]; got != 24 {
		t.Fatalf("payload_bytes = %v, want 24", got)
	}
	if got := attrs[TraceAttributeSubscribers]; got != 3 {
		t.Fatalf("subscribers = %v, want 3", got)
	}
	if got := attrs[TraceAttributeDelivered]; got != 2 {
		t.Fatalf("delivered = %v, want 2", got)
	}
	if got := attrs[TraceAttributeDropped]; got != 1 {
		t.Fatalf("dropped = %v, want 1", got)
	}
	if got := attrs[TraceAttributeErrorReports]; got != 1 {
		t.Fatalf("error_reports = %v, want 1", got)
	}
	assertTraceAttrsDoNotContain(t, event, topic, "orders.updated")
}

func TestPresenceTraceEventRedactsRoomAndUser(t *testing.T) {
	t.Parallel()

	room := "support/private-room"
	userID := "user@example.com"
	event := NewPresenceTraceEvent(PresenceTraceMetadata{
		Action:      TracePresenceActionJoin,
		Room:        room,
		UserID:      userID,
		TTL:         30 * time.Second,
		MemberCount: 4,
		PrunedCount: -1,
	})

	if event.Name != TraceEventPresence {
		t.Fatalf("Name = %q, want %q", event.Name, TraceEventPresence)
	}
	attrs := event.AttributeMap()
	if got := attrs[TraceAttributePresenceAction]; got != string(TracePresenceActionJoin) {
		t.Fatalf("action = %v, want join", got)
	}
	if got := attrs[TraceAttributeRoomPresent]; got != true {
		t.Fatalf("room_present = %v, want true", got)
	}
	if got := attrs[TraceAttributeRoomLength]; got != len(room) {
		t.Fatalf("room_length = %v, want %d", got, len(room))
	}
	if got := attrs[TraceAttributeUserPresent]; got != true {
		t.Fatalf("user_present = %v, want true", got)
	}
	if got := attrs[TraceAttributeTTLMS]; got != int64(30000) {
		t.Fatalf("ttl_ms = %v, want 30000", got)
	}
	if got := attrs[TraceAttributeMemberCount]; got != 4 {
		t.Fatalf("member_count = %v, want 4", got)
	}
	if got := attrs[TraceAttributePrunedCount]; got != 0 {
		t.Fatalf("pruned_count = %v, want negative clamped to 0", got)
	}
	assertTraceAttrsDoNotContain(t, event, room, userID)
}

func TestBackpressureTraceEventUsesQueueSnapshot(t *testing.T) {
	t.Parallel()

	event := NewBackpressureTraceEvent(BackpressureTraceMetadata{
		DropPolicy: DropOldest,
		Snapshot: BackpressureMetricsSnapshot{
			MaxQueuedMessages:     8,
			QueuedMessages:        7,
			EnqueuedMessages:      12,
			DequeuedMessages:      5,
			DroppedMessages:       3,
			DroppedOldestMessages: 2,
			DroppedNewestMessages: 1,
		},
	})

	if event.Name != TraceEventBackpressure {
		t.Fatalf("Name = %q, want %q", event.Name, TraceEventBackpressure)
	}
	attrs := event.AttributeMap()
	if got := attrs[TraceAttributeDropPolicy]; got != string(DropOldest) {
		t.Fatalf("drop_policy = %v, want drop_oldest", got)
	}
	if got := attrs[TraceAttributeMaxQueuedMessages]; got != 8 {
		t.Fatalf("max_queued_messages = %v, want 8", got)
	}
	if got := attrs[TraceAttributeQueuedMessages]; got != 7 {
		t.Fatalf("queued_messages = %v, want 7", got)
	}
	if got := attrs[TraceAttributeEnqueuedMessages]; got != uint64(12) {
		t.Fatalf("enqueued_messages = %v, want 12", got)
	}
	if got := attrs[TraceAttributeDequeuedMessages]; got != uint64(5) {
		t.Fatalf("dequeued_messages = %v, want 5", got)
	}
	if got := attrs[TraceAttributeDroppedMessages]; got != uint64(3) {
		t.Fatalf("dropped_messages = %v, want 3", got)
	}
	if got := attrs[TraceAttributeDroppedOldest]; got != uint64(2) {
		t.Fatalf("dropped_oldest = %v, want 2", got)
	}
	if got := attrs[TraceAttributeDroppedNewest]; got != uint64(1) {
		t.Fatalf("dropped_newest = %v, want 1", got)
	}
}

func TestTraceEventAttributeMapReturnsCopy(t *testing.T) {
	t.Parallel()

	event := NewPresenceTraceEvent(PresenceTraceMetadata{Action: TracePresenceActionLeave})

	attrs := event.AttributeMap()
	attrs[TraceAttributePresenceAction] = "mutated"

	if got := event.AttributeMap()[TraceAttributePresenceAction]; got != string(TracePresenceActionLeave) {
		t.Fatalf("AttributeMap mutation changed event action = %v, want leave", got)
	}
}

func assertTraceAttrsDoNotContain(t *testing.T, event TraceEventMetadata, blocked ...string) {
	t.Helper()

	for _, attr := range event.Attributes {
		value, ok := attr.Value.(string)
		if !ok {
			continue
		}
		for _, text := range blocked {
			if text != "" && strings.Contains(value, text) {
				t.Fatalf("attribute %s leaked %q in value %q", attr.Key, text, value)
			}
		}
	}
}
