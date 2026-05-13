package realtime

import (
	"reflect"
	"sync"
	"testing"
	"time"
)

func TestRealtimeMetricsCollectorSnapshotSummarizesMetrics(t *testing.T) {
	t.Parallel()

	collector := NewRealtimeMetricsCollector()
	collector.SetConnectionCount(ConnectionStateReconnecting, 2)
	collector.AddConnection(ConnectionStateConnected)
	collector.AddConnection(ConnectionStateConnected)
	collector.RemoveConnection(ConnectionStateConnected)
	collector.SetConnectionCount(ConnectionStateClosed, 1)
	collector.SetConnectionCount("", 3)

	collector.SetRoomMembers("room-a", 3)
	collector.RecordRoomJoin("room-b")
	collector.RecordRoomJoin("room-b")
	collector.RecordRoomLeave("room-b")

	collector.SetChannelSubscribers("channel-a", 2)
	collector.SetBackpressureDrops("channel-a", BackpressureDropSnapshot{
		DroppedOldestMessages: 1,
	})
	collector.RecordFanout(PublishResult{
		Topic:        "channel-b",
		Subscribers:  4,
		Delivered:    3,
		Dropped:      1,
		ErrorReports: 1,
	}, 10*time.Millisecond)
	collector.RecordFanout(PublishResult{
		Topic:       "channel-b",
		Subscribers: 4,
		Delivered:   2,
	}, 30*time.Millisecond)
	collector.SetBackpressureSnapshot("channel-b", BackpressureMetricsSnapshot{
		DroppedMessages:       4,
		DroppedOldestMessages: 1,
		DroppedNewestMessages: 2,
	})

	snapshot := collector.Snapshot()
	wantStates := []ConnectionStateCount{
		{State: ConnectionStateClosed, Count: 1},
		{State: ConnectionStateConnected, Count: 1},
		{State: ConnectionStateDisconnected, Count: 3},
		{State: ConnectionStateReconnecting, Count: 2},
	}
	if !reflect.DeepEqual(snapshot.Connections.States, wantStates) {
		t.Fatalf("connection states = %#v, want %#v", snapshot.Connections.States, wantStates)
	}
	if snapshot.Connections.Total != 7 || snapshot.Connections.Active != 1 || snapshot.Connections.Terminal != 1 {
		t.Fatalf("connections = %+v, want total=7 active=1 terminal=1", snapshot.Connections)
	}

	wantRooms := []RoomMetricsSnapshot{
		{Room: "room-a", Members: 3},
		{Room: "room-b", Members: 1, Joins: 2, Leaves: 1},
	}
	if !reflect.DeepEqual(snapshot.Rooms, wantRooms) {
		t.Fatalf("rooms = %#v, want %#v", snapshot.Rooms, wantRooms)
	}

	if got, want := len(snapshot.Channels), 2; got != want {
		t.Fatalf("channels len = %d, want %d", got, want)
	}
	channelA := snapshot.Channels[0]
	if channelA.Channel != "channel-a" || channelA.Subscribers != 2 {
		t.Fatalf("channel-a identity = %+v, want channel-a subscribers=2", channelA)
	}
	if channelA.BackpressureDrops != (BackpressureDropSnapshot{DroppedMessages: 1, DroppedOldestMessages: 1}) {
		t.Fatalf("channel-a backpressure = %+v, want one oldest drop", channelA.BackpressureDrops)
	}

	channelB := snapshot.Channels[1]
	if channelB.Channel != "channel-b" {
		t.Fatalf("second channel = %q, want channel-b", channelB.Channel)
	}
	if channelB.Subscribers != 4 || channelB.Publishes != 2 || channelB.DeliveredMessages != 5 || channelB.DroppedMessages != 1 || channelB.ErrorReports != 1 {
		t.Fatalf("channel-b counters = %+v, want subscribers=4 publishes=2 delivered=5 dropped=1 reports=1", channelB)
	}
	if channelB.FanoutLatency != (FanoutLatencySnapshot{
		Count:   2,
		Total:   40 * time.Millisecond,
		Average: 20 * time.Millisecond,
		Min:     10 * time.Millisecond,
		Max:     30 * time.Millisecond,
	}) {
		t.Fatalf("channel-b fanout latency = %+v, want 2 samples avg 20ms", channelB.FanoutLatency)
	}
	if channelB.BackpressureDrops != (BackpressureDropSnapshot{
		DroppedMessages:       4,
		DroppedOldestMessages: 1,
		DroppedNewestMessages: 2,
	}) {
		t.Fatalf("channel-b backpressure = %+v, want snapshot drops", channelB.BackpressureDrops)
	}

	wantSummary := RealtimeMetricsSummary{
		Connections:                       7,
		ActiveConnections:                 1,
		TerminalConnections:               1,
		Rooms:                             2,
		RoomMembers:                       4,
		Channels:                          2,
		ChannelSubscribers:                6,
		FanoutPublishes:                   2,
		FanoutDeliveredMessages:           5,
		FanoutDroppedMessages:             1,
		FanoutLatency:                     channelB.FanoutLatency,
		BackpressureDroppedMessages:       5,
		BackpressureDroppedOldestMessages: 2,
		BackpressureDroppedNewestMessages: 2,
		ErrorReports:                      1,
	}
	if !reflect.DeepEqual(snapshot.Summary, wantSummary) {
		t.Fatalf("summary = %#v, want %#v", snapshot.Summary, wantSummary)
	}
	if got := SummarizeRealtimeMetrics(snapshot); !reflect.DeepEqual(got, wantSummary) {
		t.Fatalf("SummarizeRealtimeMetrics() = %#v, want %#v", got, wantSummary)
	}
}

func TestRealtimeMetricsCollectorZeroValueNilSafeAndDetached(t *testing.T) {
	t.Parallel()

	var collector RealtimeMetricsCollector
	collector.AddConnection("")
	collector.RecordRoomLeave("room")
	collector.RecordFanout(PublishResult{
		Topic:        "topic",
		Subscribers:  -1,
		Delivered:    -1,
		Dropped:      -1,
		ErrorReports: -1,
	}, -time.Second)
	collector.AddBackpressureDrop("topic", DropPolicy("unexpected"))

	snapshot := collector.Snapshot()
	if got := snapshot.Connections.States; !reflect.DeepEqual(got, []ConnectionStateCount{{State: ConnectionStateDisconnected, Count: 1}}) {
		t.Fatalf("connection states = %#v, want one disconnected", got)
	}
	if got := snapshot.Rooms; !reflect.DeepEqual(got, []RoomMetricsSnapshot{{Room: "room", Leaves: 1}}) {
		t.Fatalf("rooms = %#v, want leave-only room", got)
	}
	if got := snapshot.Channels[0].FanoutLatency; got != (FanoutLatencySnapshot{Count: 1}) {
		t.Fatalf("negative fanout latency snapshot = %+v, want one zero-duration sample", got)
	}
	if got := snapshot.Channels[0].BackpressureDrops; got != (BackpressureDropSnapshot{DroppedMessages: 1, DroppedNewestMessages: 1}) {
		t.Fatalf("unknown backpressure policy drops = %+v, want newest drop", got)
	}

	snapshot.Connections.States[0].Count = 99
	snapshot.Rooms[0].Room = "changed"
	snapshot.Channels[0].Channel = "changed"

	next := collector.Snapshot()
	if next.Connections.States[0].Count != 1 {
		t.Fatalf("snapshot mutation changed stored connection count to %d, want 1", next.Connections.States[0].Count)
	}
	if next.Rooms[0].Room != "room" {
		t.Fatalf("snapshot mutation changed stored room to %q, want room", next.Rooms[0].Room)
	}
	if next.Channels[0].Channel != "topic" {
		t.Fatalf("snapshot mutation changed stored channel to %q, want topic", next.Channels[0].Channel)
	}

	var nilCollector *RealtimeMetricsCollector
	nilCollector.AddConnection(ConnectionStateConnected)
	nilCollector.RecordRoomJoin("room")
	nilCollector.RecordFanout(PublishResult{Topic: "topic"}, time.Second)
	if got := nilCollector.Snapshot(); len(got.Connections.States) != 0 || len(got.Rooms) != 0 || len(got.Channels) != 0 {
		t.Fatalf("nil Snapshot() = %+v, want empty", got)
	}
}

func TestRealtimeMetricsCollectorConcurrentUse(t *testing.T) {
	t.Parallel()

	const workers = 16
	const iterations = 100

	var collector RealtimeMetricsCollector
	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < iterations; j++ {
				collector.AddConnection(ConnectionStateConnected)
				collector.RecordRoomJoin("room")
				collector.RecordFanout(PublishResult{
					Topic:       "topic",
					Subscribers: 1,
					Delivered:   1,
				}, time.Millisecond)
				collector.AddBackpressureDrop("topic", DropNewest)
				_ = collector.Snapshot()
			}
		}()
	}
	wg.Wait()

	summary := collector.Snapshot().Summary
	want := uint64(workers * iterations)
	if summary.Connections != want {
		t.Fatalf("Connections = %d, want %d", summary.Connections, want)
	}
	if summary.RoomMembers != want {
		t.Fatalf("RoomMembers = %d, want %d", summary.RoomMembers, want)
	}
	if summary.FanoutPublishes != want || summary.FanoutDeliveredMessages != want {
		t.Fatalf("fanout summary = %+v, want publishes and delivered %d", summary, want)
	}
	if summary.BackpressureDroppedNewestMessages != want {
		t.Fatalf("BackpressureDroppedNewestMessages = %d, want %d", summary.BackpressureDroppedNewestMessages, want)
	}
}

func TestBackpressureDropHelpersNormalizeDrops(t *testing.T) {
	t.Parallel()

	drops := BackpressureDrops(BackpressureMetricsSnapshot{
		DroppedOldestMessages: 2,
		DroppedNewestMessages: 3,
	})
	if drops != (BackpressureDropSnapshot{
		DroppedMessages:       5,
		DroppedOldestMessages: 2,
		DroppedNewestMessages: 3,
	}) {
		t.Fatalf("BackpressureDrops() = %+v, want normalized policy total", drops)
	}

	var collector RealtimeMetricsCollector
	collector.SetBackpressureDrops("topic", BackpressureDropSnapshot{
		DroppedMessages:       1,
		DroppedOldestMessages: 2,
		DroppedNewestMessages: 3,
	})
	collector.AddBackpressureDrop("topic", DropOldest)
	collector.AddBackpressureDrop("topic", DropPolicy("unexpected"))

	got := collector.Snapshot().Channels[0].BackpressureDrops
	want := BackpressureDropSnapshot{
		DroppedMessages:       7,
		DroppedOldestMessages: 3,
		DroppedNewestMessages: 4,
	}
	if got != want {
		t.Fatalf("collector backpressure drops = %+v, want %+v", got, want)
	}
}
