package realtime

import (
	"sort"
	"sync"
	"time"
)

// RealtimeMetricsCollector records adapter-neutral realtime counters and
// gauges. It is safe for concurrent use, and the zero value is ready to use.
type RealtimeMetricsCollector struct {
	mu sync.RWMutex

	connections map[ConnectionState]uint64
	rooms       map[string]roomMetricsState
	channels    map[string]channelMetricsState
}

// NewRealtimeMetricsCollector returns an empty in-memory realtime metrics
// collector.
func NewRealtimeMetricsCollector() *RealtimeMetricsCollector {
	return &RealtimeMetricsCollector{}
}

// AddConnection increments the count for state. Empty state is recorded as
// disconnected.
func (c *RealtimeMetricsCollector) AddConnection(state ConnectionState) {
	if c == nil {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	c.connections[normalizeConnectionState(state)]++
}

// RemoveConnection decrements the count for state without allowing it to go
// negative.
func (c *RealtimeMetricsCollector) RemoveConnection(state ConnectionState) {
	if c == nil {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	state = normalizeConnectionState(state)
	if c.connections[state] > 0 {
		c.connections[state]--
	}
}

// SetConnectionCount sets the current connection count for state. Empty state
// is recorded as disconnected.
func (c *RealtimeMetricsCollector) SetConnectionCount(state ConnectionState, count uint64) {
	if c == nil {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	c.connections[normalizeConnectionState(state)] = count
}

// RecordRoomJoin records one room membership join and increments the current
// member count for room.
func (c *RealtimeMetricsCollector) RecordRoomJoin(room string) {
	if c == nil || room == "" {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	metrics := c.rooms[room]
	metrics.Joins++
	metrics.Members++
	c.rooms[room] = metrics
}

// RecordRoomLeave records one room membership leave and decrements the current
// member count for room when it is positive.
func (c *RealtimeMetricsCollector) RecordRoomLeave(room string) {
	if c == nil || room == "" {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	metrics := c.rooms[room]
	metrics.Leaves++
	if metrics.Members > 0 {
		metrics.Members--
	}
	c.rooms[room] = metrics
}

// SetRoomMembers sets the current member count for room.
func (c *RealtimeMetricsCollector) SetRoomMembers(room string, members uint64) {
	if c == nil || room == "" {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	metrics := c.rooms[room]
	metrics.Members = members
	c.rooms[room] = metrics
}

// SetChannelSubscribers sets the current subscriber count for channel.
func (c *RealtimeMetricsCollector) SetChannelSubscribers(channel string, subscribers uint64) {
	if c == nil || channel == "" {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	metrics := c.channels[channel]
	metrics.Subscribers = subscribers
	c.channels[channel] = metrics
}

// RecordFanout records one publish fanout result and the elapsed fanout
// latency. Negative latency is treated as zero.
func (c *RealtimeMetricsCollector) RecordFanout(result PublishResult, latency time.Duration) {
	if c == nil || result.Topic == "" {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	metrics := c.channels[result.Topic]
	metrics.Subscribers = nonNegativeUint64(result.Subscribers)
	metrics.Publishes++
	metrics.DeliveredMessages += nonNegativeUint64(result.Delivered)
	metrics.DroppedMessages += nonNegativeUint64(result.Dropped)
	metrics.ErrorReports += nonNegativeUint64(result.ErrorReports)
	metrics.FanoutLatency.observe(latency)
	c.channels[result.Topic] = metrics
}

// SetBackpressureDrops sets the current backpressure drop counters for channel.
func (c *RealtimeMetricsCollector) SetBackpressureDrops(channel string, drops BackpressureDropSnapshot) {
	if c == nil || channel == "" {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	metrics := c.channels[channel]
	metrics.BackpressureDrops = drops.Normalize()
	c.channels[channel] = metrics
}

// SetBackpressureSnapshot extracts drop counters from snapshot and stores them
// for channel.
func (c *RealtimeMetricsCollector) SetBackpressureSnapshot(channel string, snapshot BackpressureMetricsSnapshot) {
	c.SetBackpressureDrops(channel, BackpressureDrops(snapshot))
}

// AddBackpressureDrop records one dropped message for channel under policy.
// Empty or unknown policy values are treated as DropNewest.
func (c *RealtimeMetricsCollector) AddBackpressureDrop(channel string, policy DropPolicy) {
	if c == nil || channel == "" {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.initLocked()
	metrics := c.channels[channel]
	metrics.BackpressureDrops = metrics.BackpressureDrops.add(policy.Normalize(), 1)
	c.channels[channel] = metrics
}

// Snapshot returns a stable, sorted copy of all collected realtime metrics.
func (c *RealtimeMetricsCollector) Snapshot() RealtimeMetricsSnapshot {
	if c == nil {
		return RealtimeMetricsSnapshot{}
	}

	c.mu.RLock()
	defer c.mu.RUnlock()

	snapshot := RealtimeMetricsSnapshot{
		Connections: connectionMetricsSnapshot(c.connections),
		Rooms:       roomMetricsSnapshots(c.rooms),
		Channels:    channelMetricsSnapshots(c.channels),
	}
	snapshot.Summary = SummarizeRealtimeMetrics(snapshot)
	return snapshot
}

// Reset clears all collected metrics.
func (c *RealtimeMetricsCollector) Reset() {
	if c == nil {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	c.connections = nil
	c.rooms = nil
	c.channels = nil
}

func (c *RealtimeMetricsCollector) initLocked() {
	if c.connections == nil {
		c.connections = make(map[ConnectionState]uint64)
	}
	if c.rooms == nil {
		c.rooms = make(map[string]roomMetricsState)
	}
	if c.channels == nil {
		c.channels = make(map[string]channelMetricsState)
	}
}

// RealtimeMetricsSnapshot is a point-in-time view of realtime metrics.
type RealtimeMetricsSnapshot struct {
	Connections ConnectionMetricsSnapshot `json:"connections"`
	Rooms       []RoomMetricsSnapshot     `json:"rooms"`
	Channels    []ChannelMetricsSnapshot  `json:"channels"`
	Summary     RealtimeMetricsSummary    `json:"summary"`
}

// ConnectionMetricsSnapshot reports connection counts grouped by lifecycle
// state.
type ConnectionMetricsSnapshot struct {
	Total    uint64                 `json:"total"`
	Active   uint64                 `json:"active"`
	Terminal uint64                 `json:"terminal"`
	States   []ConnectionStateCount `json:"states"`
}

// ConnectionStateCount is the current count for one connection state.
type ConnectionStateCount struct {
	State ConnectionState `json:"state"`
	Count uint64          `json:"count"`
}

// RoomMetricsSnapshot reports current and cumulative metrics for one realtime
// room.
type RoomMetricsSnapshot struct {
	Room    string `json:"room"`
	Members uint64 `json:"members"`
	Joins   uint64 `json:"joins"`
	Leaves  uint64 `json:"leaves"`
}

// ChannelMetricsSnapshot reports current and cumulative metrics for one
// realtime channel or pub/sub topic.
type ChannelMetricsSnapshot struct {
	Channel           string                   `json:"channel"`
	Subscribers       uint64                   `json:"subscribers"`
	Publishes         uint64                   `json:"publishes"`
	DeliveredMessages uint64                   `json:"delivered_messages"`
	DroppedMessages   uint64                   `json:"dropped_messages"`
	ErrorReports      uint64                   `json:"error_reports"`
	FanoutLatency     FanoutLatencySnapshot    `json:"fanout_latency"`
	BackpressureDrops BackpressureDropSnapshot `json:"backpressure_drops"`
}

// FanoutLatencySnapshot is a point-in-time fanout latency summary.
type FanoutLatencySnapshot struct {
	Count   uint64        `json:"count"`
	Total   time.Duration `json:"total"`
	Average time.Duration `json:"average"`
	Min     time.Duration `json:"min"`
	Max     time.Duration `json:"max"`
}

// BackpressureDropSnapshot is the drop-only subset of backpressure metrics.
type BackpressureDropSnapshot struct {
	DroppedMessages       uint64 `json:"dropped_messages"`
	DroppedOldestMessages uint64 `json:"dropped_oldest_messages"`
	DroppedNewestMessages uint64 `json:"dropped_newest_messages"`
}

// Normalize returns drops with DroppedMessages at least as large as the sum of
// policy-specific drops.
func (d BackpressureDropSnapshot) Normalize() BackpressureDropSnapshot {
	policyDrops := addUint64(d.DroppedOldestMessages, d.DroppedNewestMessages)
	if d.DroppedMessages < policyDrops {
		d.DroppedMessages = policyDrops
	}
	return d
}

// BackpressureDrops extracts the drop-only counters from a full backpressure
// snapshot.
func BackpressureDrops(snapshot BackpressureMetricsSnapshot) BackpressureDropSnapshot {
	return (BackpressureDropSnapshot{
		DroppedMessages:       snapshot.DroppedMessages,
		DroppedOldestMessages: snapshot.DroppedOldestMessages,
		DroppedNewestMessages: snapshot.DroppedNewestMessages,
	}).Normalize()
}

// RealtimeMetricsSummary is a compact deterministic summary of a realtime
// metrics snapshot.
type RealtimeMetricsSummary struct {
	Connections                       uint64                `json:"connections"`
	ActiveConnections                 uint64                `json:"active_connections"`
	TerminalConnections               uint64                `json:"terminal_connections"`
	Rooms                             uint64                `json:"rooms"`
	RoomMembers                       uint64                `json:"room_members"`
	Channels                          uint64                `json:"channels"`
	ChannelSubscribers                uint64                `json:"channel_subscribers"`
	FanoutPublishes                   uint64                `json:"fanout_publishes"`
	FanoutDeliveredMessages           uint64                `json:"fanout_delivered_messages"`
	FanoutDroppedMessages             uint64                `json:"fanout_dropped_messages"`
	FanoutLatency                     FanoutLatencySnapshot `json:"fanout_latency"`
	BackpressureDroppedMessages       uint64                `json:"backpressure_dropped_messages"`
	BackpressureDroppedOldestMessages uint64                `json:"backpressure_dropped_oldest_messages"`
	BackpressureDroppedNewestMessages uint64                `json:"backpressure_dropped_newest_messages"`
	ErrorReports                      uint64                `json:"error_reports"`
}

// SummarizeRealtimeMetrics returns aggregate counters for snapshot.
func SummarizeRealtimeMetrics(snapshot RealtimeMetricsSnapshot) RealtimeMetricsSummary {
	summary := RealtimeMetricsSummary{
		Rooms:    uint64(len(snapshot.Rooms)),
		Channels: uint64(len(snapshot.Channels)),
	}
	summary.Connections, summary.ActiveConnections, summary.TerminalConnections = summarizeConnectionCounts(snapshot.Connections)

	var fanout fanoutLatencyState
	for _, room := range snapshot.Rooms {
		summary.RoomMembers += room.Members
	}
	for _, channel := range snapshot.Channels {
		drops := channel.BackpressureDrops.Normalize()
		summary.ChannelSubscribers += channel.Subscribers
		summary.FanoutPublishes += channel.Publishes
		summary.FanoutDeliveredMessages += channel.DeliveredMessages
		summary.FanoutDroppedMessages += channel.DroppedMessages
		summary.ErrorReports += channel.ErrorReports
		summary.BackpressureDroppedMessages += drops.DroppedMessages
		summary.BackpressureDroppedOldestMessages += drops.DroppedOldestMessages
		summary.BackpressureDroppedNewestMessages += drops.DroppedNewestMessages
		fanout.add(channel.FanoutLatency)
	}
	summary.FanoutLatency = fanout.snapshot()
	return summary
}

type roomMetricsState struct {
	Members uint64
	Joins   uint64
	Leaves  uint64
}

type channelMetricsState struct {
	Subscribers       uint64
	Publishes         uint64
	DeliveredMessages uint64
	DroppedMessages   uint64
	ErrorReports      uint64
	FanoutLatency     fanoutLatencyState
	BackpressureDrops BackpressureDropSnapshot
}

type fanoutLatencyState struct {
	count uint64
	total time.Duration
	min   time.Duration
	max   time.Duration
}

func (s *fanoutLatencyState) observe(latency time.Duration) {
	if latency < 0 {
		latency = 0
	}
	if s.count == 0 || latency < s.min {
		s.min = latency
	}
	if latency > s.max {
		s.max = latency
	}
	s.count++
	s.total += latency
}

func (s *fanoutLatencyState) add(snapshot FanoutLatencySnapshot) {
	if snapshot.Count == 0 {
		return
	}

	total := snapshot.Total
	min := snapshot.Min
	max := snapshot.Max
	if total < 0 {
		total = 0
	}
	if min < 0 {
		min = 0
	}
	if max < 0 {
		max = 0
	}

	if s.count == 0 || min < s.min {
		s.min = min
	}
	if max > s.max {
		s.max = max
	}
	s.count += snapshot.Count
	s.total += total
}

func (s fanoutLatencyState) snapshot() FanoutLatencySnapshot {
	return FanoutLatencySnapshot{
		Count:   s.count,
		Total:   s.total,
		Average: averageRealtimeDuration(s.total, s.count),
		Min:     s.min,
		Max:     s.max,
	}
}

func (d BackpressureDropSnapshot) add(policy DropPolicy, count uint64) BackpressureDropSnapshot {
	d.DroppedMessages = addUint64(d.DroppedMessages, count)
	switch policy {
	case DropOldest:
		d.DroppedOldestMessages = addUint64(d.DroppedOldestMessages, count)
	default:
		d.DroppedNewestMessages = addUint64(d.DroppedNewestMessages, count)
	}
	return d.Normalize()
}

func summarizeConnectionCounts(snapshot ConnectionMetricsSnapshot) (total, active, terminal uint64) {
	if len(snapshot.States) == 0 {
		return snapshot.Total, snapshot.Active, snapshot.Terminal
	}
	for _, state := range snapshot.States {
		total = addUint64(total, state.Count)
		if state.State.Active() {
			active = addUint64(active, state.Count)
		}
		if state.State.Terminal() {
			terminal = addUint64(terminal, state.Count)
		}
	}
	return total, active, terminal
}

func connectionMetricsSnapshot(counts map[ConnectionState]uint64) ConnectionMetricsSnapshot {
	snapshot := ConnectionMetricsSnapshot{
		States: make([]ConnectionStateCount, 0, len(counts)),
	}
	for state, count := range counts {
		if count == 0 {
			continue
		}
		snapshot.States = append(snapshot.States, ConnectionStateCount{
			State: state,
			Count: count,
		})
		snapshot.Total += count
		if state.Active() {
			snapshot.Active += count
		}
		if state.Terminal() {
			snapshot.Terminal += count
		}
	}
	sort.Slice(snapshot.States, func(i, j int) bool {
		return snapshot.States[i].State < snapshot.States[j].State
	})
	return snapshot
}

func roomMetricsSnapshots(metrics map[string]roomMetricsState) []RoomMetricsSnapshot {
	snapshots := make([]RoomMetricsSnapshot, 0, len(metrics))
	for room, state := range metrics {
		snapshots = append(snapshots, RoomMetricsSnapshot{
			Room:    room,
			Members: state.Members,
			Joins:   state.Joins,
			Leaves:  state.Leaves,
		})
	}
	sort.Slice(snapshots, func(i, j int) bool {
		return snapshots[i].Room < snapshots[j].Room
	})
	return snapshots
}

func channelMetricsSnapshots(metrics map[string]channelMetricsState) []ChannelMetricsSnapshot {
	snapshots := make([]ChannelMetricsSnapshot, 0, len(metrics))
	for channel, state := range metrics {
		snapshots = append(snapshots, ChannelMetricsSnapshot{
			Channel:           channel,
			Subscribers:       state.Subscribers,
			Publishes:         state.Publishes,
			DeliveredMessages: state.DeliveredMessages,
			DroppedMessages:   state.DroppedMessages,
			ErrorReports:      state.ErrorReports,
			FanoutLatency:     state.FanoutLatency.snapshot(),
			BackpressureDrops: state.BackpressureDrops.Normalize(),
		})
	}
	sort.Slice(snapshots, func(i, j int) bool {
		return snapshots[i].Channel < snapshots[j].Channel
	})
	return snapshots
}

func nonNegativeUint64(value int) uint64 {
	if value < 0 {
		return 0
	}
	return uint64(value)
}

func averageRealtimeDuration(total time.Duration, count uint64) time.Duration {
	if count == 0 {
		return 0
	}
	return time.Duration(int64(total) / int64(count))
}

func addUint64(a, b uint64) uint64 {
	sum := a + b
	if sum < a {
		return ^uint64(0)
	}
	return sum
}
