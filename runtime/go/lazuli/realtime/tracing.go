package realtime

import (
	"strings"
	"time"
)

// TraceEventName identifies a realtime trace event stream.
type TraceEventName string

const (
	TraceEventConnect      TraceEventName = "realtime.connect"
	TraceEventDisconnect   TraceEventName = "realtime.disconnect"
	TraceEventBroadcast    TraceEventName = "realtime.broadcast"
	TraceEventPresence     TraceEventName = "realtime.presence"
	TraceEventBackpressure TraceEventName = "realtime.backpressure"
)

const (
	TraceAttributeTransport          = "lazuli.realtime.transport"
	TraceAttributeConnectionState    = "lazuli.realtime.connection.state"
	TraceAttributeConnectionAccepted = "lazuli.realtime.connection.accepted"
	TraceAttributeReconnectAttempt   = "lazuli.realtime.connection.reconnect_attempt"
	TraceAttributeDisconnectReason   = "lazuli.realtime.disconnect.reason"
	TraceAttributeDisconnectClean    = "lazuli.realtime.disconnect.clean"
	TraceAttributeDurationMS         = "lazuli.realtime.duration_ms"
	TraceAttributeTopicPresent       = "lazuli.realtime.topic.present"
	TraceAttributeTopicLength        = "lazuli.realtime.topic.length"
	TraceAttributeTopicValid         = "lazuli.realtime.topic.valid"
	TraceAttributePayloadBytes       = "lazuli.realtime.payload_bytes"
	TraceAttributeSubscribers        = "lazuli.realtime.subscribers"
	TraceAttributeDelivered          = "lazuli.realtime.delivered"
	TraceAttributeDropped            = "lazuli.realtime.dropped"
	TraceAttributeErrorReports       = "lazuli.realtime.error_reports"
	TraceAttributePresenceAction     = "lazuli.realtime.presence.action"
	TraceAttributeRoomPresent        = "lazuli.realtime.room.present"
	TraceAttributeRoomLength         = "lazuli.realtime.room.length"
	TraceAttributeUserPresent        = "lazuli.realtime.user.present"
	TraceAttributeTTLMS              = "lazuli.realtime.ttl_ms"
	TraceAttributeMemberCount        = "lazuli.realtime.member_count"
	TraceAttributePrunedCount        = "lazuli.realtime.pruned_count"
	TraceAttributeDropPolicy         = "lazuli.realtime.drop_policy"
	TraceAttributeMaxQueuedMessages  = "lazuli.realtime.queue.max_queued_messages"
	TraceAttributeQueuedMessages     = "lazuli.realtime.queue.queued_messages"
	TraceAttributeEnqueuedMessages   = "lazuli.realtime.queue.enqueued_messages"
	TraceAttributeDequeuedMessages   = "lazuli.realtime.queue.dequeued_messages"
	TraceAttributeDroppedMessages    = "lazuli.realtime.queue.dropped_messages"
	TraceAttributeDroppedOldest      = "lazuli.realtime.queue.dropped_oldest_messages"
	TraceAttributeDroppedNewest      = "lazuli.realtime.queue.dropped_newest_messages"
)

// TraceAttribute is one adapter-neutral trace event attribute.
type TraceAttribute struct {
	Key   string
	Value any
}

// TraceEventMetadata is the SDK-neutral metadata a realtime adapter can attach
// to spans, structured logs, or Lazuli observability trace events.
//
// Helper-built attributes intentionally avoid raw topic, room, user, payload,
// connection ID, and error values. Sensitive identifiers are represented only
// through presence, length, validity, duration, and count metadata.
type TraceEventMetadata struct {
	Name       TraceEventName
	Attributes []TraceAttribute
}

// AttributeMap returns a copy of Attributes keyed by attribute name.
func (m TraceEventMetadata) AttributeMap() map[string]any {
	out := make(map[string]any, len(m.Attributes))
	for _, attr := range m.Attributes {
		out[attr.Key] = attr.Value
	}
	return out
}

// TraceTransport names a known realtime transport without exposing adapter
// details that may contain identifiers.
type TraceTransport string

const (
	TraceTransportUnknown     TraceTransport = "unknown"
	TraceTransportOther       TraceTransport = "other"
	TraceTransportWebSocket   TraceTransport = "websocket"
	TraceTransportSSE         TraceTransport = "sse"
	TraceTransportLongPolling TraceTransport = "long_polling"
	TraceTransportPubSub      TraceTransport = "pubsub"
)

// String returns a redaction-safe transport token.
func (t TraceTransport) String() string {
	return string(normalizeTraceTransport(t))
}

// TraceDisconnectReason names a redaction-safe disconnect category.
type TraceDisconnectReason string

const (
	TraceDisconnectReasonUnknown      TraceDisconnectReason = "unknown"
	TraceDisconnectReasonOther        TraceDisconnectReason = "other"
	TraceDisconnectReasonClientClosed TraceDisconnectReason = "client_closed"
	TraceDisconnectReasonServerClosed TraceDisconnectReason = "server_closed"
	TraceDisconnectReasonTimeout      TraceDisconnectReason = "timeout"
	TraceDisconnectReasonError        TraceDisconnectReason = "error"
	TraceDisconnectReasonUnauthorized TraceDisconnectReason = "unauthorized"
)

// String returns a redaction-safe disconnect reason token.
func (r TraceDisconnectReason) String() string {
	return string(normalizeDisconnectReason(r))
}

// TracePresenceAction names the presence operation represented by a trace
// event.
type TracePresenceAction string

const (
	TracePresenceActionUnknown   TracePresenceAction = "unknown"
	TracePresenceActionJoin      TracePresenceAction = "join"
	TracePresenceActionHeartbeat TracePresenceAction = "heartbeat"
	TracePresenceActionLeave     TracePresenceAction = "leave"
	TracePresenceActionList      TracePresenceAction = "list"
	TracePresenceActionPrune     TracePresenceAction = "prune"
)

// String returns a redaction-safe presence action token.
func (a TracePresenceAction) String() string {
	return string(normalizePresenceAction(a))
}

// ConnectTraceMetadata carries redaction-safe connection trace inputs.
type ConnectTraceMetadata struct {
	Transport        TraceTransport
	State            ConnectionState
	Accepted         bool
	ReconnectAttempt int
}

// DisconnectTraceMetadata carries redaction-safe disconnect trace inputs.
type DisconnectTraceMetadata struct {
	Transport TraceTransport
	State     ConnectionState
	Reason    TraceDisconnectReason
	Clean     bool
	Duration  time.Duration
}

// BroadcastTraceMetadata carries broadcast trace inputs. Topic and payload are
// never emitted raw; helpers only report topic shape, payload byte count, and
// fanout counters.
type BroadcastTraceMetadata struct {
	Topic        string
	PayloadBytes int
	Result       PublishResult
}

// PresenceTraceMetadata carries presence trace inputs. Room and UserID are
// never emitted raw.
type PresenceTraceMetadata struct {
	Action      TracePresenceAction
	Room        string
	UserID      string
	TTL         time.Duration
	MemberCount int
	PrunedCount int
}

// BackpressureTraceMetadata carries queue backpressure trace inputs.
type BackpressureTraceMetadata struct {
	DropPolicy DropPolicy
	Snapshot   BackpressureMetricsSnapshot
}

// NewConnectTraceEvent returns metadata for a realtime connection attempt.
func NewConnectTraceEvent(metadata ConnectTraceMetadata) TraceEventMetadata {
	attrs := []TraceAttribute{
		{Key: TraceAttributeTransport, Value: metadata.Transport.String()},
		{Key: TraceAttributeConnectionState, Value: traceConnectionState(metadata.State)},
		{Key: TraceAttributeConnectionAccepted, Value: metadata.Accepted},
	}
	if metadata.ReconnectAttempt > 0 {
		attrs = append(attrs, TraceAttribute{
			Key:   TraceAttributeReconnectAttempt,
			Value: metadata.ReconnectAttempt,
		})
	}
	return TraceEventMetadata{Name: TraceEventConnect, Attributes: attrs}
}

// NewDisconnectTraceEvent returns metadata for a realtime disconnect.
func NewDisconnectTraceEvent(metadata DisconnectTraceMetadata) TraceEventMetadata {
	return TraceEventMetadata{
		Name: TraceEventDisconnect,
		Attributes: []TraceAttribute{
			{Key: TraceAttributeTransport, Value: metadata.Transport.String()},
			{Key: TraceAttributeConnectionState, Value: traceConnectionState(metadata.State)},
			{Key: TraceAttributeDisconnectReason, Value: metadata.Reason.String()},
			{Key: TraceAttributeDisconnectClean, Value: metadata.Clean},
			{Key: TraceAttributeDurationMS, Value: durationMillis(metadata.Duration)},
		},
	}
}

// NewBroadcastTraceEvent returns metadata for a realtime broadcast fanout.
func NewBroadcastTraceEvent(metadata BroadcastTraceMetadata) TraceEventMetadata {
	topic := metadata.Topic
	if topic == "" {
		topic = metadata.Result.Topic
	}

	attrs := traceValueShape(nil, TraceAttributeTopicPresent, TraceAttributeTopicLength, topic)
	attrs = append(attrs,
		TraceAttribute{Key: TraceAttributeTopicValid, Value: ValidateTopic(topic) == nil},
		TraceAttribute{Key: TraceAttributePayloadBytes, Value: nonNegativeInt(metadata.PayloadBytes)},
		TraceAttribute{Key: TraceAttributeSubscribers, Value: nonNegativeInt(metadata.Result.Subscribers)},
		TraceAttribute{Key: TraceAttributeDelivered, Value: nonNegativeInt(metadata.Result.Delivered)},
		TraceAttribute{Key: TraceAttributeDropped, Value: nonNegativeInt(metadata.Result.Dropped)},
		TraceAttribute{Key: TraceAttributeErrorReports, Value: nonNegativeInt(metadata.Result.ErrorReports)},
	)
	return TraceEventMetadata{Name: TraceEventBroadcast, Attributes: attrs}
}

// NewPresenceTraceEvent returns metadata for a presence operation.
func NewPresenceTraceEvent(metadata PresenceTraceMetadata) TraceEventMetadata {
	attrs := []TraceAttribute{
		{Key: TraceAttributePresenceAction, Value: metadata.Action.String()},
	}
	attrs = traceValueShape(attrs, TraceAttributeRoomPresent, TraceAttributeRoomLength, metadata.Room)
	attrs = traceValuePresence(attrs, TraceAttributeUserPresent, metadata.UserID)
	attrs = append(attrs,
		TraceAttribute{Key: TraceAttributeTTLMS, Value: durationMillis(metadata.TTL)},
		TraceAttribute{Key: TraceAttributeMemberCount, Value: nonNegativeInt(metadata.MemberCount)},
		TraceAttribute{Key: TraceAttributePrunedCount, Value: nonNegativeInt(metadata.PrunedCount)},
	)
	return TraceEventMetadata{Name: TraceEventPresence, Attributes: attrs}
}

// NewBackpressureTraceEvent returns metadata for queue backpressure counters.
func NewBackpressureTraceEvent(metadata BackpressureTraceMetadata) TraceEventMetadata {
	snapshot := metadata.Snapshot
	return TraceEventMetadata{
		Name: TraceEventBackpressure,
		Attributes: []TraceAttribute{
			{Key: TraceAttributeDropPolicy, Value: string(metadata.DropPolicy.Normalize())},
			{Key: TraceAttributeMaxQueuedMessages, Value: nonNegativeInt(snapshot.MaxQueuedMessages)},
			{Key: TraceAttributeQueuedMessages, Value: nonNegativeInt(snapshot.QueuedMessages)},
			{Key: TraceAttributeEnqueuedMessages, Value: snapshot.EnqueuedMessages},
			{Key: TraceAttributeDequeuedMessages, Value: snapshot.DequeuedMessages},
			{Key: TraceAttributeDroppedMessages, Value: snapshot.DroppedMessages},
			{Key: TraceAttributeDroppedOldest, Value: snapshot.DroppedOldestMessages},
			{Key: TraceAttributeDroppedNewest, Value: snapshot.DroppedNewestMessages},
		},
	}
}

func normalizeTraceTransport(transport TraceTransport) TraceTransport {
	switch strings.ToLower(strings.TrimSpace(string(transport))) {
	case "":
		return TraceTransportUnknown
	case "unknown":
		return TraceTransportUnknown
	case "websocket", "web_socket", "ws":
		return TraceTransportWebSocket
	case "sse", "server_sent_events", "server-sent-events":
		return TraceTransportSSE
	case "long_polling", "long-polling", "longpoll", "long_poll":
		return TraceTransportLongPolling
	case "pubsub", "pub_sub", "pub-sub", "pub/sub":
		return TraceTransportPubSub
	default:
		return TraceTransportOther
	}
}

func normalizeDisconnectReason(reason TraceDisconnectReason) TraceDisconnectReason {
	switch strings.ToLower(strings.TrimSpace(string(reason))) {
	case "":
		return TraceDisconnectReasonUnknown
	case "unknown":
		return TraceDisconnectReasonUnknown
	case "client_closed", "client-closed", "client":
		return TraceDisconnectReasonClientClosed
	case "server_closed", "server-closed", "server", "closed":
		return TraceDisconnectReasonServerClosed
	case "timeout", "timed_out", "timed-out":
		return TraceDisconnectReasonTimeout
	case "error", "failed", "failure":
		return TraceDisconnectReasonError
	case "unauthorized", "auth", "auth_failed", "auth-failed":
		return TraceDisconnectReasonUnauthorized
	default:
		return TraceDisconnectReasonOther
	}
}

func normalizePresenceAction(action TracePresenceAction) TracePresenceAction {
	switch strings.ToLower(strings.TrimSpace(string(action))) {
	case "join":
		return TracePresenceActionJoin
	case "heartbeat":
		return TracePresenceActionHeartbeat
	case "leave":
		return TracePresenceActionLeave
	case "list":
		return TracePresenceActionList
	case "prune":
		return TracePresenceActionPrune
	default:
		return TracePresenceActionUnknown
	}
}

func traceConnectionState(state ConnectionState) string {
	switch state {
	case ConnectionStateDisconnected,
		ConnectionStateConnecting,
		ConnectionStateConnected,
		ConnectionStateReconnecting,
		ConnectionStateClosed:
		return string(state)
	case "":
		return string(ConnectionStateDisconnected)
	default:
		return "unknown"
	}
}

func traceValueShape(attrs []TraceAttribute, presentKey, lengthKey, value string) []TraceAttribute {
	return append(attrs,
		TraceAttribute{Key: presentKey, Value: value != ""},
		TraceAttribute{Key: lengthKey, Value: len(value)},
	)
}

func traceValuePresence(attrs []TraceAttribute, presentKey, value string) []TraceAttribute {
	return append(attrs, TraceAttribute{Key: presentKey, Value: value != ""})
}

func durationMillis(duration time.Duration) int64 {
	if duration < 0 {
		return 0
	}
	return duration.Milliseconds()
}

func nonNegativeInt(value int) int {
	if value < 0 {
		return 0
	}
	return value
}
