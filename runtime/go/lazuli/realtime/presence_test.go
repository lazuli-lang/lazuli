package realtime_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/realtime"
)

var _ realtime.PresenceStore = (*realtime.MemoryPresenceStore)(nil)

func TestMemoryPresenceStoreJoinListAndLeave(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := realtime.NewMemoryPresenceStore()
	store.Clock = func() time.Time { return now }

	member, err := store.Join(context.Background(), "room-1", "user-b", time.Minute)
	if err != nil {
		t.Fatalf("Join() error = %v", err)
	}
	if member.Room != "room-1" || member.UserID != "user-b" {
		t.Fatalf("Join() member identity = %+v, want room-1/user-b", member)
	}
	if !member.JoinedAt.Equal(now) || !member.LastSeenAt.Equal(now) || !member.ExpiresAt.Equal(now.Add(time.Minute)) {
		t.Fatalf("Join() member times = %+v, want joined/seen %s expires %s", member, now, now.Add(time.Minute))
	}

	if _, err := store.Join(context.Background(), "room-1", "user-a", time.Minute); err != nil {
		t.Fatalf("second Join() error = %v", err)
	}

	members, err := store.ListByRoom(context.Background(), "room-1")
	if err != nil {
		t.Fatalf("ListByRoom() error = %v", err)
	}
	if got, want := presenceUsers(members), []string{"user-a", "user-b"}; !equalStrings(got, want) {
		t.Fatalf("ListByRoom() users = %v, want %v", got, want)
	}

	if err := store.Leave(context.Background(), "room-1", "user-a"); err != nil {
		t.Fatalf("Leave() error = %v", err)
	}
	members, err = store.ListByRoom(context.Background(), "room-1")
	if err != nil {
		t.Fatalf("ListByRoom() after Leave error = %v", err)
	}
	if got, want := presenceUsers(members), []string{"user-b"}; !equalStrings(got, want) {
		t.Fatalf("ListByRoom() after Leave users = %v, want %v", got, want)
	}
}

func TestMemoryPresenceStoreHeartbeatRenewsTTL(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := realtime.NewMemoryPresenceStore()
	store.Clock = func() time.Time { return now }

	joined, err := store.Join(context.Background(), "room-1", "user-1", 10*time.Second)
	if err != nil {
		t.Fatalf("Join() error = %v", err)
	}

	now = now.Add(5 * time.Second)
	renewed, err := store.Heartbeat(context.Background(), "room-1", "user-1", 20*time.Second)
	if err != nil {
		t.Fatalf("Heartbeat() error = %v", err)
	}
	if !renewed.JoinedAt.Equal(joined.JoinedAt) {
		t.Fatalf("Heartbeat() JoinedAt = %s, want %s", renewed.JoinedAt, joined.JoinedAt)
	}
	if !renewed.LastSeenAt.Equal(now) {
		t.Fatalf("Heartbeat() LastSeenAt = %s, want %s", renewed.LastSeenAt, now)
	}
	if want := now.Add(20 * time.Second); !renewed.ExpiresAt.Equal(want) {
		t.Fatalf("Heartbeat() ExpiresAt = %s, want %s", renewed.ExpiresAt, want)
	}

	now = renewed.ExpiresAt.Add(-time.Nanosecond)
	members, err := store.ListByRoom(context.Background(), "room-1")
	if err != nil {
		t.Fatalf("ListByRoom() before renewed expiry error = %v", err)
	}
	if got, want := len(members), 1; got != want {
		t.Fatalf("ListByRoom() before renewed expiry count = %d, want %d", got, want)
	}

	now = renewed.ExpiresAt
	members, err = store.ListByRoom(context.Background(), "room-1")
	if err != nil {
		t.Fatalf("ListByRoom() at renewed expiry error = %v", err)
	}
	if len(members) != 0 {
		t.Fatalf("ListByRoom() at renewed expiry = %+v, want empty", members)
	}
}

func TestMemoryPresenceStoreHeartbeatRequiresActiveMember(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := realtime.NewMemoryPresenceStore()
	store.Clock = func() time.Time { return now }

	if _, err := store.Heartbeat(context.Background(), "room-1", "user-1", time.Minute); !errors.Is(err, realtime.ErrPresenceNotFound) {
		t.Fatalf("Heartbeat() absent error = %v, want ErrPresenceNotFound", err)
	}

	if _, err := store.Join(context.Background(), "room-1", "user-1", time.Minute); err != nil {
		t.Fatalf("Join() error = %v", err)
	}
	now = now.Add(time.Minute)
	if _, err := store.Heartbeat(context.Background(), "room-1", "user-1", time.Minute); !errors.Is(err, realtime.ErrPresenceNotFound) {
		t.Fatalf("Heartbeat() expired error = %v, want ErrPresenceNotFound", err)
	}

	members, err := store.ListByRoom(context.Background(), "room-1")
	if err != nil {
		t.Fatalf("ListByRoom() after expired heartbeat error = %v", err)
	}
	if len(members) != 0 {
		t.Fatalf("ListByRoom() after expired heartbeat = %+v, want empty", members)
	}
}

func TestMemoryPresenceStorePruneExpired(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := realtime.NewMemoryPresenceStore()
	store.Clock = func() time.Time { return now }

	if _, err := store.Join(context.Background(), "room-1", "user-1", 10*time.Second); err != nil {
		t.Fatalf("Join() short ttl error = %v", err)
	}
	if _, err := store.Join(context.Background(), "room-2", "user-2", time.Minute); err != nil {
		t.Fatalf("Join() long ttl error = %v", err)
	}

	now = now.Add(10 * time.Second)
	deleted, err := store.PruneExpired(context.Background())
	if err != nil {
		t.Fatalf("PruneExpired() error = %v", err)
	}
	if deleted != 1 {
		t.Fatalf("PruneExpired() deleted = %d, want 1", deleted)
	}

	members, err := store.ListByRoom(context.Background(), "room-2")
	if err != nil {
		t.Fatalf("ListByRoom() room-2 error = %v", err)
	}
	if got, want := presenceUsers(members), []string{"user-2"}; !equalStrings(got, want) {
		t.Fatalf("ListByRoom() room-2 users = %v, want %v", got, want)
	}

	deleted, err = store.PruneExpired(context.Background())
	if err != nil {
		t.Fatalf("second PruneExpired() error = %v", err)
	}
	if deleted != 0 {
		t.Fatalf("second PruneExpired() deleted = %d, want 0", deleted)
	}
}

func TestMemoryPresenceStoreValidatesInputsAndContext(t *testing.T) {
	t.Parallel()

	var store realtime.MemoryPresenceStore

	if _, err := store.Join(context.Background(), "", "user-1", time.Minute); !errors.Is(err, realtime.ErrPresenceRoomRequired) {
		t.Fatalf("Join() empty room error = %v, want ErrPresenceRoomRequired", err)
	}
	if _, err := store.Join(context.Background(), "room-1", "", time.Minute); !errors.Is(err, realtime.ErrPresenceUserRequired) {
		t.Fatalf("Join() empty user error = %v, want ErrPresenceUserRequired", err)
	}
	if _, err := store.Join(context.Background(), "room-1", "user-1", 0); !errors.Is(err, realtime.ErrPresenceTTLRequired) {
		t.Fatalf("Join() zero ttl error = %v, want ErrPresenceTTLRequired", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := store.ListByRoom(ctx, "room-1"); !errors.Is(err, context.Canceled) {
		t.Fatalf("ListByRoom() canceled context error = %v, want context.Canceled", err)
	}
}

func presenceUsers(members []realtime.PresenceMember) []string {
	users := make([]string, 0, len(members))
	for _, member := range members {
		users = append(users, member.UserID)
	}
	return users
}

func equalStrings(a []string, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
